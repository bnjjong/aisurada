import test from 'node:test';
import assert from 'node:assert/strict';
import { WorkspaceController } from '../src/workspace/controller.ts';
import { windowContext } from '../src/workspace/types.ts';
import { KEYS, addRecent, lastWorkspace, homePath, writeJSON } from '../src/workspace/storage.ts';

const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { promise, resolve }; };
const tick = () => new Promise(resolve => setImmediate(resolve));
const fixtureNote = { id: 'ndfixture-00000001', title: '기록', body: '# 기록\n\n본문\n', agenda: 'demo', created: '2026-06-04T09:30:00+09:00', format: 'markdown', precondition: 'f1-token' };

function harness(t, options = {}) {
  const values = new Map(Object.entries(options.values || {}));
  const writes = [], calls = [], events = [], watches = [];
  let current = '/fixture';
  const storage = { getItem: key => values.get(key) ?? null, setItem: (key, value) => { writes.push(key); values.set(key, value); } };
  const host = {
    invoke: async (command, args) => {
      calls.push({ command, args }); events.push(command === 'sidecar_call' ? args.argv[0] : command);
      if (options.invoke) { const result = await options.invoke(command, args); if (result !== undefined) return result; }
      if (command === 'allow_workspace') { current = args.path; return; }
      if (command === 'vault_root') return current;
      if (command === 'create_workspace_dir') return `${args.parent}/${args.name}`;
      if (['note_windows', 'saved_note_windows'].includes(command)) return [];
      if (command === 'sidecar_call') {
        const cmd = args.argv[0];
        const value = cmd === 'list-docs' ? [fixtureNote] : cmd === 'list-agendas' ? [{ id: 'ag-demo', slug: 'demo', title: 'Demo' }] : cmd === 'reindex' ? { skipped: [], conflicts: [] } : [];
        return { status: 0, stdout: JSON.stringify(value), stderr: '' };
      }
    },
    documentDir: options.documentDir || (async () => '/documents/'),
    chooseDirectory: async () => '/chosen/',
    confirmLeave: async () => false,
  };
  const services = {
    host, storage,
    files: { exists: async () => { events.push('migration'); return false; } },
    watch: async (root, changed) => { const watch = { root, changed, stopped: false }; watches.push(watch); return () => { watch.stopped = true; }; },
    navigate: (path, replace) => events.push(`route:${path}:${replace}`),
    pending: { flushReports: async () => {}, flushEditors: async () => {}, waitForWrites: async () => true, unsavedNotes: () => 0 },
    now: () => new Date('2026-06-04T00:00:00Z'), uuid: () => 'test-uuid', sleep: async () => {}, warn: () => {},
  };
  const controller = new WorkspaceController(services, options.context || { kind: 'main' }, options.route || '/');
  t.after(() => controller.dispose());
  return { controller, services, values, writes, calls, events, watches };
}

const setLast = (h, path = '/fixture') => writeJSON(h.services.storage, KEYS.last, { path, name: '테스트' });
const argv = h => h.calls.filter(c => c.command === 'sidecar_call').map(c => c.args.argv);

test('window classification accepts one encoded segment and rejects malformed paths', () => {
  assert.deepEqual(windowContext('/note-window/a%20b'), { kind: 'note', docId: 'a b' });
  for (const path of ['/note-window/', '/note-window/a/b', '/note-window/%ZZ', '/doc/a']) assert.deepEqual(windowContext(path), { kind: 'main' });
});

test('invalid last workspace and explicit creation address still show the launcher', async t => {
  const h = harness(t, { values: { [KEYS.last]: '{broken' }, route: '/launcher?mode=create' });
  await h.controller.boot();
  assert.equal(h.controller.getSnapshot().route, '/launcher?mode=create');
  assert.equal(h.controller.getSnapshot().home, '/documents');
  assert.equal(h.calls.length, 0);
  assert.equal(lastWorkspace(h.services.storage), null);
});

test('boot preserves raw path and enforces allow, init, migration and two read batches', async t => {
  const h = harness(t); setLast(h, '/fixture/./');
  const first = h.controller.boot(); assert.equal(h.controller.boot(), first); await first;
  assert.deepEqual(h.calls.find(c => c.command === 'allow_workspace').args, { path: '/fixture/./' });
  assert.deepEqual(argv(h), [ ['init-workspace', 'name=테스트', 'templates=[]'], ['reindex'], ['list-reports', 'bodies=true'],
    ['list-templates', 'bodies=true'], ['list-trash'], ['list-agendas'], ['list-projects'], ['list-docs', 'limit=0', 'bodies=true'] ]);
  assert.ok(h.events.indexOf('allow_workspace') < h.events.indexOf('init-workspace'));
  assert.ok(h.events.indexOf('init-workspace') < h.events.indexOf('migration'));
  assert.ok(h.events.indexOf('migration') < h.events.indexOf('reindex'));
  const snapshot = h.controller.getSnapshot();
  assert.equal(snapshot.workspace.notes[0].body, fixtureNote.body);
  assert.equal(snapshot.workspace.notes[0].precondition, 'f1-token');
  assert.equal(snapshot.workspace.notes[0].agendaId, 'ag-demo');
  assert.equal(snapshot.workspace.notes[0].time, '09:30');
  assert.equal(snapshot.route, '/calendar');
  assert.equal(lastWorkspace(h.services.storage).path, '/fixture/./');
});

test('second batch waits for every first-batch command, including a rejected command', async t => {
  const barrier = deferred();
  const h = harness(t, { invoke: async (cmd, args) => {
    if (cmd === 'sidecar_call' && args.argv[0] === 'list-trash') { await barrier.promise; throw new Error('trash failed'); }
  } }); setLast(h);
  const boot = h.controller.boot(); await tick();
  assert.ok(argv(h).some(a => a[0] === 'reindex'));
  assert.ok(!argv(h).some(a => a[0] === 'list-docs'));
  barrier.resolve(); await boot;
  assert.ok(argv(h).some(a => a[0] === 'list-docs'));
  assert.equal(h.controller.getSnapshot().workspace.trash.length, 0);
});

test('host string rejection clears the last workspace without initializing or reading', async t => {
  const h = harness(t, { invoke: async cmd => { if (cmd === 'allow_workspace') throw '거절'; } }); setLast(h);
  await h.controller.boot();
  assert.equal(h.controller.getSnapshot().workspace, null);
  assert.equal(h.controller.getSnapshot().route, '/launcher');
  assert.equal(lastWorkspace(h.services.storage), null);
  assert.equal(argv(h).length, 0);
  assert.match(h.controller.getSnapshot().notice, /이전 워크스페이스/);
});

test('missing engine opens an empty workspace with a persistent read failure flag', async t => {
  const h = harness(t, { invoke: async cmd => { if (cmd === 'sidecar_call') return { status: 4, stdout: '', stderr: 'no engine' }; } }); setLast(h, '/missing');
  await h.controller.boot();
  const workspace = h.controller.getSnapshot().workspace;
  assert.equal(workspace.path, '/missing'); assert.equal(workspace.notes.length, 0); assert.equal(workspace.noteReadFailed, true);
  assert.equal(lastWorkspace(h.services.storage).path, '/missing');
});

test('creation initializes twice but fails visibly if its first initialization fails', async t => {
  const h = harness(t); await h.controller.boot(); h.controller.setHome('/parent');
  assert.equal(await h.controller.create('  새 공간  '), true);
  assert.equal(h.calls.filter(c => c.command === 'allow_workspace').length, 2);
  assert.equal(argv(h).filter(a => a[0] === 'init-workspace').length, 2);
  assert.equal(h.controller.getSnapshot().recent[0].created, true);
  assert.deepEqual(lastWorkspace(h.services.storage), { path: '/parent/새 공간', name: '새 공간' });
  const broken = harness(t, { invoke: async (cmd, args) => {
    if (cmd === 'sidecar_call' && args.argv[0] === 'init-workspace') return { status: 3, stdout: '', stderr: 'WL-400' };
  } }); await broken.controller.boot(); broken.controller.setHome('/parent');
  assert.equal(await broken.controller.create('broken'), false);
  assert.equal(broken.controller.getSnapshot().workspace, null);
  assert.equal(lastWorkspace(broken.services.storage), null);
  assert.equal(broken.controller.getSnapshot().recent.length, 0);
  assert.match(broken.controller.getSnapshot().notice, /생성 실패/);
});

test('sample stops watching, preserves last real workspace and makes no invoke calls', async t => {
  const h = harness(t); setLast(h); await h.controller.boot(); await tick();
  const count = h.calls.length, last = h.values.get(KEYS.last);
  assert.equal(await h.controller.sample(), true);
  assert.equal(h.calls.length, count);
  assert.equal(h.values.get(KEYS.last), last);
  assert.equal(h.watches[0].stopped, true);
  const sample = h.controller.getSnapshot().workspace;
  assert.equal(sample.sample, true); assert.equal(sample.today, '2026-06-04');
  assert.equal(sample.notes.find(n => n.id === 'md1').format, 'markdown');
  assert.ok(sample.notes.find(n => n.id === 'md1').body.includes('\n\n'));
  assert.equal(h.controller.getSnapshot().recent[0].path, '/documents/withlog-sample');
});

test('C202: a failed initialization of a new vault puts the host back on the old vault and keeps the old vault on screen', async t => {
  let fail = false;
  const h = harness(t, { invoke: async (cmd, args) => {
    if (fail && cmd === 'sidecar_call' && args.argv[0] === 'init-workspace') return { status: 3, stdout: '', stderr: 'init failed' };
  } }); setLast(h); await h.controller.boot(); await tick();
  const oldLast = h.values.get(KEYS.last), shown = h.controller.getSnapshot().workspace, route = h.controller.getSnapshot().route; fail = true;
  h.controller.setHome('/parent');
  assert.equal(await h.controller.create('new'), false);
  const allows = h.calls.filter(c => c.command === 'allow_workspace').map(c => c.args.path);
  assert.deepEqual(allows.slice(-2), ['/parent/new', shown.path]);
  assert.equal(await h.services.host.invoke('vault_root'), shown.path);
  assert.equal(h.controller.getSnapshot().workspace.path, shown.path);
  assert.equal(h.controller.getSnapshot().route, route);
  assert.match(h.controller.getSnapshot().notice, /생성 실패/);
  assert.equal(h.values.get(KEYS.last), oldLast);
  assert.equal(h.watches[0].stopped, true); assert.equal(h.watches.at(-1).root, shown.path); assert.equal(h.watches.at(-1).stopped, false);
});

test('목록 25번: 샘플로 바꿀 때도 400ms 안에 모아 둔 회고 쓰기를 먼저 보낸다',async t=>{
  for(const change of [h=>h.controller.sample(),h=>h.controller.openRecent({path:'/documents/withlog-sample',name:'샘플',sample:true})]){
    const h=harness(t);setLast(h);await h.controller.boot();await tick();
    h.controller.data.saveReport('rv-morning-2026-06-04','아침 회고','');
    await change(h);await tick();
    assert.ok(h.calls.some(call=>call.command==='sidecar_call'&&call.args.argv[0]==='save-report'),'the collected report was dropped');
  }
});
test('C174: retrospective edits waiting to be written go to the old vault before opening or creating another vault', async t => {
  for (const change of [h => h.controller.create('new'), h => h.controller.open('/other')]) {
    const h = harness(t); setLast(h); await h.controller.boot(); await tick(); h.controller.setHome('/parent');
    h.controller.data.saveReport('rv-morning-2026-06-04', '아침 회고', '');
    await change(h);
    const save = h.calls.findIndex(c => c.command === 'sidecar_call' && c.args.argv[0] === 'save-report');
    const leave = h.calls.findIndex(c => c.command === 'allow_workspace' && c.args.path !== h.calls.find(d => d.command === 'allow_workspace').args.path);
    assert.ok(save >= 0 && leave >= 0 && save < leave, 'report written after the host left the vault');
  }
});

test('recent opening does not reorder entries and removal re-reads shared storage', async t => {
  const h = harness(t); await h.controller.boot();
  addRecent(h.services.storage, { path: '/older', name: 'old' }); addRecent(h.services.storage, { path: '/newer', name: 'new' });
  await h.controller.openRecent({ path: '/older', name: 'old', opened: '방금 전' });
  assert.equal(JSON.parse(h.values.get(KEYS.recent))[0].path, '/newer');
  addRecent(h.services.storage, { path: '/elsewhere', name: 'external update' });
  h.controller.forgetRecent('/older');
  assert.deepEqual(h.controller.getSnapshot().recent.map(r => r.path), ['/elsewhere', '/newer']);
});

test('note boot performs only six reads and never writes main storage or authorizes a vault', async t => {
  const h = harness(t, { context: { kind: 'note', docId: fixtureNote.id }, values: { [KEYS.last]: JSON.stringify({ path: '/wrong', name: 'wrong' }) } });
  await h.controller.boot();
  assert.equal(h.calls[0].command, 'vault_root');
  assert.equal(argv(h).length, 6);
  assert.ok(!argv(h).some(a => ['reindex', 'init-workspace'].includes(a[0])));
  assert.ok(!h.calls.some(c => ['allow_workspace', 'saved_note_windows', 'create_workspace_dir'].includes(c.command)));
  assert.equal(h.writes.length, 0);
  assert.equal(await h.controller.open('/wrong'), false); assert.equal(await h.controller.create('wrong'), false);
  assert.equal(await h.controller.sample(), false); assert.equal(await h.controller.launcher(), false);
  assert.equal(await h.controller.chooseDirectory(true), null);
  assert.equal(h.writes.length, 0);
});

test('note window without a main vault stops before engine reads', async t => {
  const h = harness(t, { context: { kind: 'note', docId: 'missing' }, invoke: async cmd => cmd === 'vault_root' ? null : undefined });
  await h.controller.boot();
  assert.match(h.controller.getSnapshot().fatal, /main 창/);
  assert.equal(argv(h).length, 0);
});

test('restoration filters missing notes and main tabs and runs once per launch', async t => {
  const second = { ...fixtureNote, id: 'ndfixture-00000002', title: 'second' };
  const h = harness(t, { values: { [KEYS.tabs]: JSON.stringify({ state: { workspaceRoot: '/fixture', openIds: [fixtureNote.id] }, version: 1 }) }, invoke: async (cmd, args) => {
    if (cmd === 'saved_note_windows') return [fixtureNote.id, second.id, 'missing'];
    if (cmd === 'sidecar_call' && args.argv[0] === 'list-docs') return { status: 0, stdout: JSON.stringify([fixtureNote, second]), stderr: '' };
  } }); setLast(h);
  await h.controller.boot();
  assert.deepEqual(h.calls.find(c => c.command === 'restore_note_windows').args, { docs: [{ docId: second.id, title: 'second' }] });
  await h.controller.open('/other');
  assert.equal(h.calls.filter(c => c.command === 'saved_note_windows').length, 1);
});

test('a late watch installation is stopped after switching to the sample', async t => {
  const watch = deferred(); let stopped = 0;
  const h = harness(t); h.services.watch = () => watch.promise; setLast(h);
  await h.controller.boot(); await h.controller.sample();
  watch.resolve(() => { stopped++; }); await tick();
  assert.equal(stopped, 1);
});

test('dirty-work cancellation prevents host changes and last-workspace mutations', async t => {
  const h = harness(t); setLast(h); await h.controller.boot();
  h.services.pending.unsavedNotes = () => 1;
  h.services.host.confirmLeave = async () => false;
  const count = h.calls.length, last = h.values.get(KEYS.last);
  assert.equal(await h.controller.open('/other'), false);
  assert.equal(h.calls.length, count); assert.equal(h.values.get(KEYS.last), last);
  assert.equal(h.controller.getSnapshot().workspace.path, '/fixture');
});

test('note windows are closed and polled before authorizing a different root', async t => {
  let closed = false;
  const h = harness(t, { invoke: async cmd => {
    if (cmd === 'note_windows') return closed ? [] : ['note1'];
    if (cmd === 'vault_root') return '/old';
    if (cmd === 'close_note_windows') { closed = true; return null; }
  } }); await h.controller.boot(); await h.controller.open('/next');
  const commands = h.calls.map(c => c.command);
  assert.deepEqual(commands.slice(0, 5), ['note_windows', 'vault_root', 'close_note_windows', 'note_windows', 'allow_workspace']);
});

test('a delayed default document directory cannot overwrite an edited parent', async t => {
  const directory = deferred();
  const h = harness(t, { documentDir: () => directory.promise });
  await h.controller.boot(); h.controller.setHome('/my-parent');
  directory.resolve('/default/'); await tick();
  assert.equal(h.controller.getSnapshot().home, '/my-parent');
  assert.equal(homePath('~/Documents'), null); assert.equal(homePath('/documents///'), '/documents');
});

test('queued opens cannot interleave host roots and leave mixed workspace data', async t => {
  const barrier = deferred(); let root;
  const h = harness(t, { invoke: async (cmd, args) => {
    if (cmd === 'allow_workspace') root = args.path;
    if (cmd === 'sidecar_call' && args.argv[0] === 'list-docs') {
      if (root === '/first') await barrier.promise;
      return { status: 0, stdout: JSON.stringify([{ ...fixtureNote, title: root }]), stderr: '' };
    }
  } }); await h.controller.boot();
  const first = h.controller.open('/first'), second = h.controller.open('/second'); await tick();
  assert.equal(root, '/first'); barrier.resolve(); await Promise.all([first, second]);
  assert.equal(h.controller.getSnapshot().workspace.path, '/second');
  assert.equal(h.controller.getSnapshot().workspace.notes[0].title, '/second');
});

test('sample transition finishes an in-flight read without launching its stale second batch', async t => {
  const h = harness(t); setLast(h); await h.controller.boot();
  const gate = deferred(), invoke = h.services.host.invoke;
  h.services.host.invoke = async (command, args) => {
    const value = await invoke(command, args);
    if (command === 'sidecar_call' && args.argv[0] === 'list-reports') await gate.promise;
    return value;
  };
  const read = h.controller.refresh(); await tick(); const before = h.calls.filter(call => call.command === 'sidecar_call').length;
  const sample = h.controller.sample(); await tick(); gate.resolve(); await Promise.all([sample, read]);
  assert.equal(h.controller.getSnapshot().workspace.sample, true);
  assert.equal(h.calls.filter(call => call.command === 'sidecar_call').length, before);
});

test('sample report edits and memos survive reopening without calling the engine', async t => {
  const h = harness(t); await h.controller.boot(); await h.controller.sample(); const before = h.calls.length;
  h.controller.data.saveReport('rv-morning-2026-06-04', 'sample body', 'memo');
  await h.controller.launcher(); await h.controller.sample();
  assert.deepEqual(h.controller.getSnapshot().workspace.reports, [{id:'rv-morning-2026-06-04',body:'sample body',memo:'memo'}]);
  assert.ok(!h.calls.slice(before).some(call=>call.command==='sidecar_call'));
});
