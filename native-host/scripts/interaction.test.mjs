import test from 'node:test';
import assert from 'node:assert/strict';
import { parseAisurada, fileURLPath, latestInput, connectLinkSource } from '../src/interaction/links.ts';
import { shortcutFor } from '../src/interaction/shortcuts.ts';
import { WorkspaceController } from '../src/workspace/controller.ts';
import { sampleWorkspace } from '../src/workspace/sample.ts';
import { KEYS } from '../src/workspace/storage.ts';

const tick = () => new Promise(resolve => setImmediate(resolve));
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
const key = options => ({ key: '', code: '', metaKey: true, ctrlKey: false, shiftKey: false, altKey: false, defaultPrevented: false,
  preventDefault() { this.defaultPrevented = true; }, ...options });
const url = (id, ws = '/fixture', extra = '') => `aisurada://doc/${encodeURIComponent(id)}${ws === null ? '' : '?ws=' + encodeURIComponent(ws)}${extra}`;
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

test('aisurada parsing preserves raw ws, case-insensitive kinds, encoded targets and exact window flag', () => {
  assert.deepEqual(parseAisurada('  AISURADA://DOC/a%20b?ws=%2Fv%2F.%2F&window=1#ignored  '), { kind:'doc', id:'a b', ws:'/v/./', window:true, chain:[] });
  assert.deepEqual(parseAisurada('aisurada://AGENDA/a%2Fb?ws=&window=1'), { kind:'agenda', slug:'a/b', ws:null });
  assert.equal(parseAisurada('aisurada://doc/a?window=true').window, false);
  assert.equal(parseAisurada('aisurada://doc/a?ws=/a+b').ws, '/a b');
});

test('malformed targets and trailing path separators are rejected', () => {
  for (const value of ['aisurada://doc/', 'aisurada://doc/a/', 'aisurada://doc/a/b', 'aisurada://doc/%ZZ', 'aisurada://other/a', 'https://doc/a']) assert.equal(parseAisurada(value), null, value);
});

test('file parsing accepts local Markdown only and latest input ignores unsupported trailing URLs', () => {
  assert.equal(fileURLPath('file://localhost/a%20b.MARKDOWN#heading'), '/a b.MARKDOWN');
  for (const value of ['file://remote/a.md', 'file:///a.txt', 'file:///%ZZ.md', 'https://example.com/a.md']) assert.equal(fileURLPath(value), null, value);
  assert.equal(latestInput([url('a'), 'invalid', url('b'), 'https://ignored']).link.id, 'b');
});

test('C194: the palette shortcut ignores a key whose default the editor already handled', () => {
  assert.equal(shortcutFor(key({ key:'k', defaultPrevented:true }), true, true), null);
  assert.equal(shortcutFor(key({ key:'k', metaKey:false, ctrlKey:true, defaultPrevented:true }), false, true), null);
  assert.deepEqual(shortcutFor(key({ key:'k' }), true, true), { kind:'search' });
});
test('shortcuts use physical codes for digits and brackets and ignore Alt, Shift-digits and wrong modifiers', () => {
  assert.deepEqual(shortcutFor(key({ code:'Digit2', key:'é' }), true, true), {kind:'index',index:1});
  assert.deepEqual(shortcutFor(key({ code:'Numpad9' }), true, true), {kind:'index',index:-1});
  assert.deepEqual(shortcutFor(key({ code:'BracketLeft', key:'{', shiftKey:true }), true, true), {kind:'cycle',direction:-1});
  for (const event of [key({code:'Digit1',shiftKey:true}),key({key:'k',altKey:true}),key({key:'k',metaKey:false,ctrlKey:false})]) assert.equal(shortcutFor(event,true,true),null);
  // 목록 30번(4.3): Ctrl works like Cmd on macOS too.
  assert.deepEqual(shortcutFor(key({key:'k',metaKey:false,ctrlKey:true}),true,true),{kind:'search'});
  assert.deepEqual(shortcutFor(key({code:'Digit2',key:'2',metaKey:false,ctrlKey:true}),true,true),{kind:'index',index:1});
  assert.deepEqual(shortcutFor(key({key:'K',metaKey:false,ctrlKey:true}),false,true),{kind:'search'});
  assert.equal(shortcutFor(key({key:'w'}),true,true),null); assert.deepEqual(shortcutFor(key({key:'w'}),true,false),{kind:'close'});
});

test('Option+1 and Option+2 switch rail tabs between notes and agents', async t => {
  assert.deepEqual(shortcutFor(key({ altKey: true, metaKey: false, ctrlKey: false, code: 'Digit1', key: '¡' }), true, true), { kind: 'railTab', tab: 'notes' });
  assert.deepEqual(shortcutFor(key({ altKey: true, metaKey: false, ctrlKey: false, code: 'Digit2', key: '™' }), true, true), { kind: 'railTab', tab: 'agents' });
  assert.deepEqual(shortcutFor(key({ altKey: true, metaKey: false, ctrlKey: false, code: 'Numpad1', key: '1' }), true, true), { kind: 'railTab', tab: 'notes' });
  assert.deepEqual(shortcutFor(key({ altKey: true, metaKey: false, ctrlKey: false, code: 'Numpad2', key: '2' }), true, true), { kind: 'railTab', tab: 'agents' });
  // Shift나 Cmd/Ctrl과 함께 누르면 railTab이 아니다
  assert.equal(shortcutFor(key({ altKey: true, shiftKey: true, code: 'Digit1' }), true, true), null);
  assert.equal(shortcutFor(key({ altKey: true, metaKey: true, code: 'Digit1' }), true, true), null);

  const h = await fixture(t);
  h.controller.navigate('/calendar');
  // Option + 2 누르면 에이전트 탭의 첫 화면(/tasks)으로 전환된다
  assert.equal(h.controller.handleKey(key({ altKey: true, metaKey: false, ctrlKey: false, code: 'Digit2', key: '2' })), true);
  assert.equal(h.controller.getSnapshot().route, '/tasks');
  // Option + 1 누르면 직전 노트 화면(/calendar)으로 돌아온다
  assert.equal(h.controller.handleKey(key({ altKey: true, metaKey: false, ctrlKey: false, code: 'Digit1', key: '1' })), true);
  assert.equal(h.controller.getSnapshot().route, '/calendar');
});

test('new-workspace menu retains the vault and launcher ignores terminal, close and search shortcuts', async t => {
  const h = await fixture(t), last = h.values.get(KEYS.last);
  await h.controller.handleMenu('ais_new_workspace');
  assert.equal(h.controller.getSnapshot().route,'/launcher?mode=create'); assert.equal(h.values.get(KEYS.last),last); assert.ok(h.controller.getSnapshot().workspace);
  const count = h.calls.length;
  assert.equal(await h.controller.handleMenu('ais_open_terminal'),false); assert.equal(await h.controller.handleMenu('ais_close_tab'),false);
  assert.equal(h.controller.handleKey(key({key:'k'})),false); assert.equal(h.calls.length,count);
});

test('menu open cancellation is silent while a selected folder reuses common opening', async t => {
  const h = await fixture(t); const count=h.calls.length;
  assert.equal(await h.controller.handleMenu('ais_open_workspace'),false); assert.equal(h.controller.getSnapshot().notice,null); assert.equal(h.calls.length,count);
  h.services.host.chooseDirectory = async () => '/selected';
  assert.equal(await h.controller.handleMenu('ais_open_workspace'),true);
  assert.equal(h.controller.getSnapshot().workspace.path,'/selected'); assert.equal(h.controller.getSnapshot().route,'/calendar');
});

test('menu open failure returns to launcher but unsaved-data cancellation retains the current route', async t => {
  let reject=false;
  const h=await fixture(t,{handler:async cmd=>{if(reject&&cmd==='allow_workspace')throw 'refused';}});
  h.controller.navigate('/doc/a'); h.services.host.chooseDirectory=async()=>'/new'; h.services.pending.unsavedNotes=()=>1;
  assert.equal(await h.controller.handleMenu('ais_open_workspace'),false); assert.equal(h.controller.getSnapshot().route,'/doc/a');
  h.services.pending.unsavedNotes=()=>0; reject=true;
  assert.equal(await h.controller.handleMenu('ais_open_workspace'),false); assert.equal(h.controller.getSnapshot().route,'/launcher'); assert.match(h.controller.getSnapshot().notice,/폴더를 열 수/);
});

test('welcome closes note windows, clears last workspace and leaves the host root untouched', async t => {
  const h=await fixture(t); await h.controller.handleMenu('ais_welcome');
  assert.equal(h.controller.getSnapshot().workspace,null); assert.equal(h.values.get(KEYS.last),'null'); assert.equal(commands(h,'close_note_windows').length,1);
  // 시작 화면으로 가는 길은 앱을 끄는 길이 아니다. 「종료 중」 표시를 켜면 이후 종료 확인이 건너뛰어진다.
  assert.equal(commands(h,'begin_quit').length,0);
  assert.equal(await h.services.host.invoke('vault_root'),'/fixture');
});

test('terminal and close menus respect active tabs and intentionally denied main-window close', async t => {
  const h=await fixture(t); await h.controller.handleMenu('ais_open_terminal'); assert.equal(h.controller.getSnapshot().route,'/terminal');
  await h.controller.handleMenu('ais_close_tab'); assert.ok(!h.controller.getSnapshot().tabs.openIds.includes('term:main'));
  assert.equal(commands(h,'pty_detach')[0].args.key,'term:main');
  await h.controller.handleMenu('ais_close_tab'); assert.equal(commands(h,'window.close').length,1);
  h.controller.navigate('/terminal/session'); await h.controller.handleMenu('ais_close_tab'); assert.equal(commands(h,'window.close').length,2);
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

test('목록 14번: 노트 창은 옮기거나 크기를 바꾸면 마지막 바뀜에서 400ms 뒤 소수점 그대로 자리를 적는다',async t=>{
  const h=await fixture(t,{context:{kind:'note',docId:'a'}});let report;
  h.services.noteGeometryChanged=async changed=>{report=changed;return()=>{}};
  t.mock.timers.enable({apis:['setTimeout']});await h.controller.listenNoteGeometry();
  report({width:800.5,height:600.25,x:10.75,y:20});t.mock.timers.tick(399);assert.equal(h.values.get(KEYS.noteWindow),undefined);
  report({width:812.5,height:600.25,x:30.5,y:20.125});t.mock.timers.tick(399);assert.equal(h.values.get(KEYS.noteWindow),undefined);
  t.mock.timers.tick(1);assert.deepEqual(JSON.parse(h.values.get(KEYS.noteWindow)),{width:812.5,height:600.25,x:30.5,y:20.125});
  const saved=h.calls.filter(call=>call.command==='remember_note_window');assert.equal(saved.length,1);assert.deepEqual(saved[0].args.geometry,{width:812.5,height:600.25,x:30.5,y:20.125});
});
test('목록 15번(C149): 노트 창이 vault에 붙기 전 화면에서는 Cmd+W가 아무 일도 안 하고 창 자리도 안 적는다',async t=>{
  const h=await fixture(t,{context:{kind:'note',docId:'a'},inputs:true,noBoot:true});
  h.services.noteGeometryChanged=async changed=>{h.report=changed;return()=>{}};
  assert.equal(await h.controller.handleMenu('ais_close_tab'),false);assert.equal(h.calls.filter(call=>call.command==='destroy_current_window').length,0);
  h.controller.saveNoteGeometry({width:700,height:500,x:1,y:2});assert.equal(h.values.get(KEYS.noteWindow),undefined);
  assert.equal(h.calls.filter(call=>call.command==='remember_note_window').length,0);
});
test('목록 16번(C153): 노트 창에서 바꾼 설정은 그 창 안에서만 살고 공유 저장소에는 안 쓴다',async t=>{
  const h=await fixture(t,{context:{kind:'note',docId:'a'},inputs:true});
  const before=h.values.get(KEYS.prefs);
  h.controller.preference({previewWidth:900,previewHeight:700});
  assert.deepEqual(h.controller.getSnapshot().prefs.previewSize,{width:900,height:700});
  h.controller.refreshTheme();assert.deepEqual(h.controller.getSnapshot().prefs.previewSize,{width:900,height:700});
  assert.equal(h.values.get(KEYS.prefs),before);assert.ok(!h.writes.includes(KEYS.prefs));
  const main=await fixture(t,{});main.controller.preference({previewWidth:640,previewHeight:480});assert.ok(main.writes.includes(KEYS.prefs));
});
test('목록 17번(C156): 노트 탭의 「새 창으로」는 open_note_window를 2번 보내고, 탭이 아니면 1번이다',async t=>{
  const h=await fixture(t,{});h.controller.navigate('/doc/a');
  const opens=()=>h.calls.filter(call=>call.command==='open_note_window');
  assert.equal(await h.controller.moveNoteWindow('a'),'opened');assert.equal(opens().length,2);
  assert.deepEqual(opens()[1].args,{docId:'a',title:null,geometry:null});
  assert.equal(await h.controller.moveNoteWindow('b'),'opened');assert.equal(opens().length,3);
});
// 후속 노트 창(모달)은 탭 바가 없는 노트 창에서만 남았다. 11.7과 4.8의 주소 규칙은 노트 창 기준으로 시험한다.
test('목록 28번(11.7): 노트 창의 후속 페이지는 같은 주소로 옮겨도 닫히고, 도착할 주소에 닿는 바뀜 1번만 봐준다',async t=>{
  const h=await fixture(t,{context:{kind:'note',docId:'a'}});h.controller.navigate('/doc/a');h.controller.openSubNotes('a',['child']);
  assert.equal(h.controller.getSnapshot().subNotes.root,'a');assert.equal(h.controller.getSnapshot().subNotes.arrival,null);
  h.controller.navigate('/doc/a');assert.equal(h.controller.getSnapshot().subNotes,null,'moving to the same address did not close it');
  h.controller.navigate('/note-window/a');h.controller.openSubNotes('a',['child']);assert.equal(h.controller.getSnapshot().subNotes.arrival,'/doc/a');
  h.controller.navigate('/doc/a');assert.equal(h.controller.getSnapshot().subNotes.root,'a');assert.equal(h.controller.getSnapshot().subNotes.arrival,null);
  h.controller.navigate('/doc/a');assert.equal(h.controller.getSnapshot().subNotes,null);
});
test('목록 28번과 4.8: 노트 창에서 링크로 후속 페이지를 열며 같은 주소로 가는 이동 1번은 봐주고 그다음 이동은 센다',async t=>{
  const h=await fixture(t,{context:{kind:'note',docId:'a'}});h.controller.navigate('/doc/a');
  h.controller.openSubNotes('a',['child'],true);assert.equal(h.controller.getSnapshot().subNotes.arrival,'/doc/a');
  h.controller.navigate('/doc/a');assert.equal(h.controller.getSnapshot().subNotes.root,'a','the link flow own navigation closed it');
  h.controller.navigate('/doc/a');assert.equal(h.controller.getSnapshot().subNotes,null);
});
test('후속 노트 탭: 메인 창은 후속 노트를 창이 아니라 상위 노트 탭 바로 뒤의 탭으로 연다',async t=>{
  const h=await fixture(t,{});h.controller.navigate('/doc/a');h.controller.navigate('/doc/b');h.controller.navigate('/doc/a');
  h.controller.openSubNotes('a',['child']);
  assert.equal(h.controller.getSnapshot().route,'/sub/a/child');assert.equal(h.controller.getSnapshot().subNotes,null,'the main window still opened a modal');
  assert.deepEqual(h.controller.getSnapshot().tabs.openIds,['a','sub:a/child','b']);
  h.controller.openSubNotes('a',['child','grandchild']);h.controller.openSubNotes('a',['other']);
  assert.deepEqual(h.controller.getSnapshot().tabs.openIds,['a','sub:a/child','sub:a/child/grandchild','sub:a/other','b']);
  assert.equal(h.controller.getSnapshot().route,'/sub/a/other');
  h.controller.openSubNotes('a',['child']);assert.equal(h.controller.getSnapshot().tabs.openIds.length,5,'an open follow-up tab was opened twice');
  assert.equal(h.controller.getSnapshot().route,'/sub/a/child');
});
test('후속 노트 탭: 열린 탭은 다시 켜도 남고, 노트 id 아래에 저장된다',async t=>{
  const h=await fixture(t,{});h.controller.navigate('/doc/a');h.controller.openSubNotes('a',['child','grandchild']);
  assert.deepEqual(JSON.parse(h.values.get(KEYS.tabs)).state.openIds,['a','sub:a/child/grandchild']);
  const again=await fixture(t,{values:{[KEYS.tabs]:h.values.get(KEYS.tabs)}});
  assert.deepEqual(again.controller.getSnapshot().tabs.openIds,['a','sub:a/child/grandchild']);
});
test('후속 노트 탭: 노트를 지우면 그 탭과 아래 후속 탭이 닫히고 상위 탭으로 돌아간다',async t=>{
  const h=await fixture(t,{});h.controller.navigate('/doc/a');h.controller.openSubNotes('a',['child']);h.controller.openSubNotes('a',['child','grandchild']);
  h.controller.popSubNote();
  assert.deepEqual(h.controller.getSnapshot().tabs.openIds,['a','sub:a/child']);assert.equal(h.controller.getSnapshot().route,'/sub/a/child');
  h.controller.openSubNotes('a',['child','grandchild']);h.controller.navigate('/sub/a/child');h.controller.popSubNote();
  assert.deepEqual(h.controller.getSnapshot().tabs.openIds,['a'],'the follow-up under the deleted one stayed open');assert.equal(h.controller.getSnapshot().route,'/doc/a');
  h.controller.popSubNote();assert.deepEqual(h.controller.getSnapshot().tabs.openIds,['a'],'popSubNote closed a tab that was not a follow-up');
});
test('후속 노트 탭: 노트를 새 창으로 옮기면 그 노트의 후속 탭도 메인 창에서 닫히고, 후속 노트를 열려 하면 그 창을 앞으로 가져온다',async t=>{
  const h=await fixture(t,{});h.controller.navigate('/doc/a');h.controller.openSubNotes('a',['child']);h.controller.navigate('/doc/b');
  h.controller.setNoteWindows(['a']);
  assert.deepEqual(h.controller.getSnapshot().tabs.openIds,['b']);assert.equal(h.controller.getSnapshot().route,'/doc/b');
  const before=commands(h,'open_note_window').length;h.controller.openSubNotes('a',['child']);
  assert.equal(commands(h,'open_note_window').length,before+1);assert.equal(h.controller.getSnapshot().route,'/doc/b');assert.deepEqual(h.controller.getSnapshot().tabs.openIds,['b']);
  h.controller.navigate('/sub/a/child');assert.deepEqual(h.controller.getSnapshot().tabs.openIds,['b'],'an address of a popped-out note opened a main tab');
});
test('note window receives only close, flushes before destroy, and never subscribes to deep links', async t => {
  const h=await fixture(t,{context:{kind:'note',docId:'a'},inputs:true});
  assert.ok(h.listeners.menu); assert.equal(h.listeners.links,undefined); assert.equal(h.listeners.development,undefined);
  assert.equal(await h.controller.handleMenu('ais_new_workspace'),false); assert.equal(h.controller.handleKey(key({key:'k'})),false);
  let flushed=false; h.services.pending.flushEditors=async()=>{flushed=true;};
  assert.equal(await h.controller.handleMenu('ais_close_tab'),true); assert.equal(flushed,true); assert.equal(commands(h,'destroy_current_window').length,1); assert.deepEqual(h.writes,[]);
});

test('note close can be cancelled when unsaved data remains', async t => {
  const h=await fixture(t,{context:{kind:'note',docId:'a'}}); h.services.pending.unsavedNotes=()=>1;
  assert.equal(await h.controller.handleMenu('ais_close_tab'),false); assert.equal(commands(h,'destroy_current_window').length,0);
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

test('host-classified child chains open the follow-up tab together with the parent note tab', async t => {
  const h=await fixture(t,{handler:async cmd=>cmd==='resolve_md_target'?{kind:'note',ws:'/fixture',chain:['a','child','grandchild']}:undefined});
  await h.controller.receiveURLs(['file:///child.md']); assert.equal(h.controller.getSnapshot().route,'/sub/a/child/grandchild');
  assert.equal(h.controller.getSnapshot().subNotes,null); assert.deepEqual(h.controller.getSnapshot().tabs.openIds,['a','sub:a/child/grandchild']);
  assert.equal(h.controller.handleKey(key({key:'Escape'})),false,'Escape closed a follow-up tab');
  h.controller.navigate('/guide'); assert.deepEqual(h.controller.getSnapshot().tabs.openIds,['a','sub:a/child/grandchild']);
});

test('window links reuse existing windows with null title and validated remembered geometry', async t => {
  const h=await fixture(t,{values:{'aisurada.noteWindow.v1':JSON.stringify({width:900,height:800,x:10,y:20})}}); h.controller.setNoteWindows(['a']);
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
