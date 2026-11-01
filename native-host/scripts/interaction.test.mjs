import test from 'node:test';
import assert from 'node:assert/strict';
import { parseWithlog, fileURLPath, latestInput, connectLinkSource } from '../src/interaction/links.ts';
import { shortcutFor } from '../src/interaction/shortcuts.ts';
import { WorkspaceController } from '../src/workspace/controller.ts';
import { sampleWorkspace } from '../src/workspace/sample.ts';
import { KEYS } from '../src/workspace/storage.ts';

const tick = () => new Promise(resolve => setImmediate(resolve));
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
const key = options => ({ key: '', code: '', metaKey: true, ctrlKey: false, shiftKey: false, altKey: false, defaultPrevented: false,
  preventDefault() { this.defaultPrevented = true; }, ...options });
const url = (id, ws = '/fixture', extra = '') => `withlog://doc/${encodeURIComponent(id)}${ws === null ? '' : '?ws=' + encodeURIComponent(ws)}${extra}`;
const commands = (h, command) => h.calls.filter(call => call.command === command);

async function fixture(t, options = {}) {
  const values = new Map(Object.entries(options.values || {}));
  if (options.last !== null) values.set(KEYS.last, JSON.stringify({ path: options.last || '/fixture', name: 'fixture' }));
  const calls = [], writes = [], warnings = [], locations = [], listeners = {};
  let root = '/fixture', disposed = 0;
  const notes = ['a','b','c'].map(id => ({ ...sampleWorkspace().notes[0], id, title: id }));
  const services = {
    native: true, mac: true,
    storage: { getItem: k => values.get(k) ?? null, setItem: (k,v) => { writes.push(k); values.set(k,v); } },
    host: {
      invoke: async (command, args) => {
        calls.push({ command, args });
        if (options.handler) { const result = await options.handler(command, args); if (result !== undefined) return result; }
        if (command === 'allow_workspace') root = args.path;
        if (command === 'vault_root') return root;
        if (command === 'note_windows') return options.windows || [];
        if (command === 'saved_note_windows') return [];
        if (command === 'is_workspace') return true;
        if (command === 'resolve_md_target') return { kind: 'external', path: args.path };
        if (command === 'sidecar_call') return { status: 0, stderr: '', stdout: JSON.stringify(args.argv[0] === 'list-docs' ? notes : []) };
      }, documentDir: async () => '/docs', chooseDirectory: async () => null, confirmLeave: async () => false,
    },
    files: { exists: async () => false }, watch: async () => () => {},
    closeWindow: async () => { calls.push({ command: 'window.close' }); throw 'denied'; },
    navigate: (path, replace) => locations.push({ path, replace }),
    pending: { flushReports: async () => {}, flushEditors: async () => {}, waitForWrites: async () => true, unsavedNotes: () => 0 },
    now: () => new Date('2026-11-01T00:00:00Z'), uuid: () => 'uuid', sleep: async () => {}, warn: (...args) => warnings.push(args),
    ...(options.inputs ? {
      menu: async receive => { listeners.menu = receive; return () => { disposed++; }; },
      deepLinks: { listen: async receive => { listeners.links = receive; return () => { disposed++; }; }, current: async () => options.cold || null },
      developmentLinks: receive => { listeners.development = receive; return () => { disposed++; }; },
    } : {}),
  };
  const controller = new WorkspaceController(services, options.context || { kind: 'main' }, '/');
  t.after(() => controller.dispose());
  if (!options.noBoot) { await controller.boot(); await tick(); await controller.waitForIdle(); }
  return { controller, services, calls, writes, values, warnings, locations, listeners, disposed: () => disposed };
}

test('withlog parsing preserves raw ws, case-insensitive kinds, encoded targets and exact window flag', () => {
  assert.deepEqual(parseWithlog('  WITHLOG://DOC/a%20b?ws=%2Fv%2F.%2F&window=1#ignored  '), { kind:'doc', id:'a b', ws:'/v/./', window:true, chain:[] });
  assert.deepEqual(parseWithlog('withlog://AGENDA/a%2Fb?ws=&window=1'), { kind:'agenda', slug:'a/b', ws:null });
  assert.equal(parseWithlog('withlog://doc/a?window=true').window, false);
  assert.equal(parseWithlog('withlog://doc/a?ws=/a+b').ws, '/a b');
});

test('malformed targets and trailing path separators are rejected', () => {
  for (const value of ['withlog://doc/', 'withlog://doc/a/', 'withlog://doc/a/b', 'withlog://doc/%ZZ', 'withlog://other/a', 'https://doc/a']) assert.equal(parseWithlog(value), null, value);
});

test('file parsing accepts local Markdown only and latest input ignores unsupported trailing URLs', () => {
  assert.equal(fileURLPath('file://localhost/a%20b.MARKDOWN#heading'), '/a b.MARKDOWN');
  for (const value of ['file://remote/a.md', 'file:///a.txt', 'file:///%ZZ.md', 'https://example.com/a.md']) assert.equal(fileURLPath(value), null, value);
  assert.equal(latestInput([url('a'), 'invalid', url('b'), 'https://ignored']).link.id, 'b');
});

test('shortcuts use physical codes for digits and brackets and ignore Alt, Shift-digits and wrong modifiers', () => {
  assert.deepEqual(shortcutFor(key({ code:'Digit2', key:'é' }), true, true), {kind:'index',index:1});
  assert.deepEqual(shortcutFor(key({ code:'Numpad9' }), true, true), {kind:'index',index:-1});
  assert.deepEqual(shortcutFor(key({ code:'BracketLeft', key:'{', shiftKey:true }), true, true), {kind:'cycle',direction:-1});
  for (const event of [key({code:'Digit1',shiftKey:true}),key({key:'k',altKey:true}),key({key:'k',metaKey:false,ctrlKey:true})]) assert.equal(shortcutFor(event,true,true),null);
  assert.deepEqual(shortcutFor(key({key:'K',metaKey:false,ctrlKey:true}),false,true),{kind:'search'});
  assert.equal(shortcutFor(key({key:'w'}),true,true),null); assert.deepEqual(shortcutFor(key({key:'w'}),true,false),{kind:'close'});
});

test('new-workspace menu retains the vault and launcher ignores terminal, close and search shortcuts', async t => {
  const h = await fixture(t), last = h.values.get(KEYS.last);
  await h.controller.handleMenu('wl_new_workspace');
  assert.equal(h.controller.getSnapshot().route,'/launcher?mode=create'); assert.equal(h.values.get(KEYS.last),last); assert.ok(h.controller.getSnapshot().workspace);
  const count = h.calls.length;
  assert.equal(await h.controller.handleMenu('wl_open_terminal'),false); assert.equal(await h.controller.handleMenu('wl_close_tab'),false);
  assert.equal(h.controller.handleKey(key({key:'k'})),false); assert.equal(h.calls.length,count);
});

test('menu open cancellation is silent while a selected folder reuses common opening', async t => {
  const h = await fixture(t); const count=h.calls.length;
  assert.equal(await h.controller.handleMenu('wl_open_workspace'),false); assert.equal(h.controller.getSnapshot().notice,null); assert.equal(h.calls.length,count);
  h.services.host.chooseDirectory = async () => '/selected';
  assert.equal(await h.controller.handleMenu('wl_open_workspace'),true);
  assert.equal(h.controller.getSnapshot().workspace.path,'/selected'); assert.equal(h.controller.getSnapshot().route,'/calendar');
});

test('menu open failure returns to launcher but unsaved-data cancellation retains the current route', async t => {
  let reject=false;
  const h=await fixture(t,{handler:async cmd=>{if(reject&&cmd==='allow_workspace')throw 'refused';}});
  h.controller.navigate('/doc/a'); h.services.host.chooseDirectory=async()=>'/new'; h.services.pending.unsavedNotes=()=>1;
  assert.equal(await h.controller.handleMenu('wl_open_workspace'),false); assert.equal(h.controller.getSnapshot().route,'/doc/a');
  h.services.pending.unsavedNotes=()=>0; reject=true;
  assert.equal(await h.controller.handleMenu('wl_open_workspace'),false); assert.equal(h.controller.getSnapshot().route,'/launcher'); assert.match(h.controller.getSnapshot().notice,/폴더를 열 수/);
});

test('welcome closes note windows, clears last workspace and leaves the host root untouched', async t => {
  const h=await fixture(t); await h.controller.handleMenu('wl_welcome');
  assert.equal(h.controller.getSnapshot().workspace,null); assert.equal(h.values.get(KEYS.last),'null'); assert.equal(commands(h,'close_note_windows').length,1);
  assert.equal(await h.services.host.invoke('vault_root'),'/fixture');
});

test('terminal and close menus respect active tabs and intentionally denied main-window close', async t => {
  const h=await fixture(t); await h.controller.handleMenu('wl_open_terminal'); assert.equal(h.controller.getSnapshot().route,'/terminal');
  await h.controller.handleMenu('wl_close_tab'); assert.ok(!h.controller.getSnapshot().tabs.openIds.includes('term:main'));
  assert.equal(commands(h,'pty_detach')[0].args.key,'term:main');
  await h.controller.handleMenu('wl_close_tab'); assert.equal(commands(h,'window.close').length,1);
  h.controller.navigate('/terminal/session'); await h.controller.handleMenu('wl_close_tab'); assert.equal(commands(h,'window.close').length,2);
});

test('global shortcuts wrap tabs, skip redundant history entries, select last, and distinguish native Cmd+W', async t => {
  const h=await fixture(t); for(const id of ['a','b','c'])h.controller.navigate('/doc/'+id);
  const count=h.locations.length; h.controller.handleKey(key({code:'Digit4'})); assert.equal(h.locations.length,count);
  h.controller.handleKey(key({code:'Digit1'})); assert.equal(h.controller.getSnapshot().route,'/calendar');
  h.controller.handleKey(key({code:'BracketLeft',shiftKey:true})); assert.equal(h.controller.getSnapshot().route,'/doc/c');
  h.controller.handleKey(key({code:'BracketRight',shiftKey:true})); assert.equal(h.controller.getSnapshot().route,'/calendar');
  h.controller.handleKey(key({code:'Digit9'})); assert.equal(h.controller.getSnapshot().route,'/doc/c');
  assert.equal(h.controller.handleKey(key({key:'w'})),false); assert.ok(h.controller.getSnapshot().tabs.openIds.includes('c'));
  h.services.native=false; assert.equal(h.controller.handleKey(key({key:'w'})),true); assert.ok(!h.controller.getSnapshot().tabs.openIds.includes('c'));
});

test('search toggles over editable focus and Escape honors already-consumed inner events', async t => {
  const h=await fixture(t); h.controller.handleKey(key({key:'k',target:{isContentEditable:true}})); assert.equal(h.controller.getSnapshot().searchOpen,true);
  assert.equal(h.controller.handleKey(key({key:'Escape',defaultPrevented:true})),false); assert.equal(h.controller.getSnapshot().searchOpen,true);
  h.controller.handleKey(key({key:'Escape'})); assert.equal(h.controller.getSnapshot().searchOpen,false);
});

test('note window receives only close, flushes before destroy, and never subscribes to deep links', async t => {
  const h=await fixture(t,{context:{kind:'note',docId:'a'},inputs:true});
  assert.ok(h.listeners.menu); assert.equal(h.listeners.links,undefined); assert.equal(h.listeners.development,undefined);
  assert.equal(await h.controller.handleMenu('wl_new_workspace'),false); assert.equal(h.controller.handleKey(key({key:'k'})),false);
  let flushed=false; h.services.pending.flushEditors=async()=>{flushed=true;};
  assert.equal(await h.controller.handleMenu('wl_close_tab'),true); assert.equal(flushed,true); assert.equal(commands(h,'destroy_current_window').length,1); assert.deepEqual(h.writes,[]);
});

test('note close can be cancelled when unsaved data remains', async t => {
  const h=await fixture(t,{context:{kind:'note',docId:'a'}}); h.services.pending.unsavedNotes=()=>1;
  assert.equal(await h.controller.handleMenu('wl_close_tab'),false); assert.equal(commands(h,'destroy_current_window').length,0);
});

test('links wait for boot and the newest accepted link replaces the pending one', async t => {
  const barrier=deferred();
  const h=await fixture(t,{noBoot:true,handler:async(cmd,args)=>{if(cmd==='sidecar_call'&&args.argv[0]==='list-docs')await barrier.promise;}});
  const boot=h.controller.boot(); await tick();
  const first=h.controller.receiveURLs([url('a')]), second=h.controller.receiveURLs([url('b')]);
  assert.notEqual(h.controller.getSnapshot().route,'/doc/a'); barrier.resolve(); await Promise.all([boot,first,second]); await h.controller.waitForIdle();
  assert.equal(h.controller.getSnapshot().route,'/doc/b'); assert.equal(commands(h,'allow_workspace').length,1);
});

test('different raw vault paths are validated before two allows and one initialization', async t => {
  const h=await fixture(t), before=h.calls.length;
  await h.controller.receiveURLs([url('a','/fixture/')]); await h.controller.waitForIdle();
  const calls=h.calls.slice(before); assert.equal(calls[0].command,'is_workspace'); assert.deepEqual(calls[0].args,{root:'/fixture/'});
  assert.equal(calls.filter(call=>call.command==='allow_workspace').length,2);
  assert.equal(calls.filter(call=>call.command==='sidecar_call'&&call.args.argv[0]==='init-workspace').length,1);
  assert.equal(h.controller.getSnapshot().workspace.path,'/fixture/'); assert.equal(h.controller.getSnapshot().route,'/doc/a');
});

test('non-workspace deep links never grant a new scope or initialize a folder', async t => {
  const h=await fixture(t,{handler:async cmd=>cmd==='is_workspace'?false:undefined}), before=h.calls.length;
  await h.controller.receiveURLs([url('a','/invalid')]);
  assert.ok(!h.calls.slice(before).some(call=>call.command==='allow_workspace')); assert.equal(h.controller.getSnapshot().route,'/launcher'); assert.match(h.controller.getSnapshot().notice,/워크스페이스가 아니/);
});

test('unsaved-data cancellation discards a vault-changing link without leaving the current note', async t => {
  const h=await fixture(t); h.controller.navigate('/doc/b'); h.services.pending.unsavedNotes=()=>1;
  const before=commands(h,'allow_workspace').length; await h.controller.receiveURLs([url('a','/other')]);
  assert.equal(h.controller.getSnapshot().route,'/doc/b'); assert.equal(commands(h,'allow_workspace').length,before);
});

test('a newer link waits for a workspace switch to finish and prevents the old target opening', async t => {
  const barrier=deferred();
  const h=await fixture(t,{handler:async(cmd,args)=>{if(cmd==='allow_workspace'&&args.path==='/other')await barrier.promise;}});
  const old=h.controller.receiveURLs([url('a','/other')]); await tick();
  const next=h.controller.receiveURLs([url('b','/other')]); barrier.resolve(); await Promise.all([old,next]); await h.controller.waitForIdle();
  assert.equal(h.controller.getSnapshot().workspace.path,'/other'); assert.equal(h.controller.getSnapshot().route,'/doc/b');
  assert.ok(!h.locations.some(item=>item.path==='/doc/a'));
});

test('file resolution gets current then freshly read recent roots and stays silent on rejection', async t => {
  const h=await fixture(t); h.values.set(KEYS.recent,JSON.stringify([{path:'/recent',name:'r',opened:'방금 전'}]));
  await h.controller.receiveURLs(['file:///external%20note.md']);
  assert.deepEqual(commands(h,'resolve_md_target').at(-1).args,{path:'/external note.md',knownRoots:['/fixture','/recent']});
  assert.equal(h.controller.getSnapshot().route,'/file?path=%2Fexternal%20note.md');
  const old=h.services.host.invoke; h.services.host.invoke=async(cmd,args)=>{if(cmd==='resolve_md_target')throw 'no';return old(cmd,args);};
  const route=h.controller.getSnapshot().route; await h.controller.receiveURLs(['file:///unreadable.md']); assert.equal(h.controller.getSnapshot().route,route); assert.equal(h.controller.getSnapshot().notice,null);
});

test('external files without an open vault hit the same gate and show the launcher', async t => {
  const h=await fixture(t,{last:null}); await h.controller.receiveURLs(['file:///note.md']);
  assert.equal(h.controller.getSnapshot().route,'/launcher'); assert.match(h.controller.getSnapshot().notice,/워크스페이스가 필요/);
});

test('late asynchronous file lookup cannot overwrite a newer doc link', async t => {
  const barrier=deferred(); const h=await fixture(t,{handler:async cmd=>cmd==='resolve_md_target'?barrier.promise:undefined});
  const old=h.controller.receiveURLs(['file:///old.md']); await h.controller.receiveURLs([url('b')]);
  barrier.resolve({kind:'external',path:'/old.md'}); await old; await h.controller.waitForIdle(); assert.equal(h.controller.getSnapshot().route,'/doc/b');
});

test('host-classified child chains survive the one arrival and close on the next address change', async t => {
  const h=await fixture(t,{handler:async cmd=>cmd==='resolve_md_target'?{kind:'note',ws:'/fixture',chain:['a','child','grandchild']}:undefined});
  await h.controller.receiveURLs(['file:///child.md']); assert.equal(h.controller.getSnapshot().route,'/doc/a');
  assert.deepEqual(h.controller.getSnapshot().subNotes,{root:'a',ids:['child','grandchild'],arrival:null});
  h.controller.handleKey(key({key:'Escape'})); assert.deepEqual(h.controller.getSnapshot().subNotes.ids,['child']);
  h.controller.navigate('/guide'); assert.equal(h.controller.getSnapshot().subNotes,null);
});

test('window links reuse existing windows with null title and validated remembered geometry', async t => {
  const h=await fixture(t,{values:{'withlog.noteWindow.v1':JSON.stringify({width:900,height:800,x:10,y:20})}}); h.controller.setNoteWindows(['a']);
  await h.controller.receiveURLs([url('a','/fixture','&window=1')]);
  assert.deepEqual(commands(h,'open_note_window').at(-1).args,{docId:'a',title:null,geometry:{width:900,height:800,x:10,y:20}});
});

test('window links flush before moving and fall back to tabs when dirty or host-rejected', async t => {
  const h=await fixture(t); const flushed=[]; h.services.pending.flushNote=async id=>{flushed.push(id);};
  await h.controller.receiveURLs([url('a','/fixture','&window=1')]); assert.deepEqual(flushed,['a']); assert.equal(commands(h,'open_note_window').at(-1).args.title,'a');
  h.services.pending.unsavedForNote=()=>1; await h.controller.receiveURLs([url('b','/fixture','&window=1')]);
  assert.equal(h.controller.getSnapshot().route,'/doc/b'); assert.equal(commands(h,'open_note_window').length,1);
  h.services.pending.unsavedForNote=()=>0;
  const invoke=h.services.host.invoke; h.services.host.invoke=async(cmd,args)=>{if(cmd==='open_note_window')throw 'refused';return invoke(cmd,args);};
  await h.controller.receiveURLs([url('c','/fixture','&window=1')]); assert.equal(h.controller.getSnapshot().route,'/doc/c');
  assert.deepEqual(flushed,['a','b','c']); assert.equal(h.warnings.length,2);
});

test('cold-start listener is installed first and stale current URLs never override live delivery', async () => {
  const cold=deferred(), received=[]; let live, reads=0, revision=0, disposed=0;
  const stop=await connectLinkSource({listen:async cb=>{live=cb;return()=>{disposed++;};},current:()=>{reads++;return cold.promise;}}, urls=>{revision++;received.push(urls);},()=>revision,()=>{});
  live([url('new')]); cold.resolve([url('old')]); await tick(); assert.deepEqual(received,[[url('new')]]); assert.equal(reads,1); stop(); assert.equal(disposed,1);
});

test('cold-start output is ignored after another entry point or disposal wins', async () => {
  for(const disposeEarly of [false,true]) {
    const cold=deferred(), received=[]; let revision=0;
    const stop=await connectLinkSource({listen:async()=>()=>{},current:()=>cold.promise}, values=>received.push(values),()=>revision,()=>{});
    if(disposeEarly)stop();else revision++;
    cold.resolve([url('old')]); await tick(); assert.deepEqual(received,[]); if(!disposeEarly)stop();
  }
});

test('all main entry points share the gate and subscribe only once', async t => {
  const h=await fixture(t,{inputs:true,cold:[url('a')]}); assert.equal(h.controller.getSnapshot().route,'/doc/a');
  h.listeners.links([url('b')]); await h.controller.waitForIdle(); assert.equal(h.controller.getSnapshot().route,'/doc/b');
  h.listeners.development([url('c')]); await h.controller.waitForIdle(); assert.equal(h.controller.getSnapshot().route,'/doc/c');
  await h.controller.boot(); h.controller.dispose(); assert.equal(h.disposed(),3);
});
