import test from 'node:test';
import assert from 'node:assert/strict';
import { parseRoute, docAddress, fileAddress, agendaAddress, routeTab, activeTab, tabAddress, findAgenda, calendarView } from '../src/navigation/routes.ts';
import { TabModel, readTabs } from '../src/navigation/tabs.ts';
import { NoteScrollMemory, rememberNoteScroll } from '../src/navigation/scroll.ts';
import { WorkspaceController } from '../src/workspace/controller.ts';
import { sampleWorkspace } from '../src/workspace/sample.ts';
import { KEYS } from '../src/workspace/storage.ts';

const workspace = (ids = ['a', 'b', 'c']) => ({ ...sampleWorkspace(), sample: false, path: '/fixture',
  notes: ids.map((id, index) => ({ ...sampleWorkspace().notes[0], id, title: id, body: `# ${id}\n${index}` })) });
const persisted = (openIds, workspaceRoot = '/fixture', version = 1) => JSON.stringify({ state: { workspaceRoot, openIds }, version });
function storage(tabs) {
  const values = new Map(tabs === undefined ? [] : [[KEYS.tabs, tabs]]), writes = [];
  return { values, writes, getItem: key => values.get(key) ?? null, setItem: (key, value) => { values.set(key, value); writes.push(key); } };
}
function model(ids) { const disk = storage(ids === undefined ? undefined : persisted(ids)); const tabs = new TabModel(disk, true); tabs.attach(workspace()); return { disk, tabs }; }

test('recognizes all thirteen screen families and encoded identifiers', () => {
  const cases = [['/launcher?mode=create','launcher'], ['/doc/n%2Fa','doc'], ['/file?path=%2Fdocs%2Fa%20b.md','file'], ['/calendar?mode=week','calendar'],
    ['/favorites','favorites'], ['/agenda/%ED%95%9C%EA%B8%80?open=a,b','agenda'], ['/connections','connections'], ['/review','review'],
    ['/templates?open=t1','templates'], ['/trash','trash'], ['/guide','guide'], ['/session/a%20b','session'], ['/terminal','terminal'], ['/terminal/a%20b','terminal']];
  for (const [address, kind] of cases) assert.equal(parseRoute(address).kind, kind, address);
  assert.equal(parseRoute('/doc/n%2Fa').id, 'n/a');
  assert.equal(parseRoute('/file?path=%2Fdocs%2Fa%20b.md').path, '/docs/a b.md');
  assert.deepEqual(parseRoute('/agenda/demo?open=a%2Cb').open, ['a', 'b']);
});

test('malformed and extra path segments cannot accidentally become valid routes', () => {
  for (const path of ['/missing', '/doc/', '/doc/a/b', '/doc/%ZZ', '/note-window/a', '//example.com/doc/a', 'https://example.com/doc/a']) assert.equal(parseRoute(path).kind, 'unknown', path);
  assert.equal(parseRoute('/').kind, 'root');
});

test('address builders preserve spaces, slashes, query characters and Unicode', () => {
  assert.equal(parseRoute(docAddress('한글/a ?#')).id, '한글/a ?#');
  assert.equal(parseRoute(agendaAddress('안건 / ?')).id, '안건 / ?');
  assert.equal(parseRoute(fileAddress('/a b/한글?.md')).path, '/a b/한글?.md');
  assert.equal(tabAddress('term:a/b ?'), '/terminal/a%2Fb%20%3F');
});

test('agenda resolution gives ids priority over colliding slugs', () => {
  const data = workspace(); data.agendas = [{ id: 'id-first', slug: 'demo', title: 'id' }, { id: 'other', slug: 'id-first', title: 'slug' }];
  assert.equal(findAgenda(data, 'id-first').title, 'id'); assert.equal(findAgenda(data, 'demo').title, 'id'); assert.equal(findAgenda(data, 'missing'), undefined);
});

test('tab ids and active selection preserve the session-terminal compatibility rule', () => {
  assert.equal(routeTab(parseRoute('/file')), null);
  assert.equal(routeTab(parseRoute('/file?path=%2Fnote.md')), 'ext:/note.md');
  assert.equal(routeTab(parseRoute('/terminal/s1')), 'term:s1');
  assert.equal(activeTab('/terminal/s1'), null); assert.equal(activeTab('/terminal'), 'term:main');
  assert.equal(activeTab('/session/s1'), null); assert.equal(activeTab('/doc/a'), 'a');
});

test('stored values reject wrong versions, objects, empty ids, duplicates and transient prefixes', () => {
  const disk = storage(persisted(['a', 'a', '', 3, {}, 'term:main', 'ext:/x.md', 'sess:s1', 'other:note']));
  assert.deepEqual(readTabs(disk), { workspaceRoot: '/fixture', openIds: ['a', 'other:note'] });
  for (const value of ['{broken', persisted(['a'], '/fixture', 2), 'null']) assert.deepEqual(readTabs(storage(value)), { workspaceRoot: null, openIds: [] });
  assert.deepEqual(readTabs(storage(persisted('a', 3))), { workspaceRoot: null, openIds: [] });
});

test('address entry opens a tab once and preserves its position on repeated visits', () => {
  const { tabs } = model();
  for (const route of ['/doc/a','/doc/b','/doc/a',fileAddress('/x.md'),'/terminal','/terminal/s1']) tabs.visit(route, workspace());
  assert.deepEqual(tabs.snapshot().openIds, ['a','b','ext:/x.md','term:main','term:s1']);
});

test('persistence contains only string note ids and does not restore active or main-route state', () => {
  const { tabs, disk } = model();
  tabs.visit('/guide', workspace()); tabs.visit('/doc/a', workspace()); tabs.visit('/terminal', workspace()); tabs.visit(fileAddress('/x.md'), workspace());
  assert.equal(disk.values.get(KEYS.tabs), '{"state":{"workspaceRoot":"/fixture","openIds":["a"]},"version":1}');
  const again = new TabModel(disk, true); again.attach(workspace()); again.visit('/', workspace());
  assert.deepEqual(again.snapshot().openIds, ['a']); assert.equal(again.snapshot().lastMain, '/calendar'); assert.equal(again.snapshot().calendar.mode, 'day');
});

test('closing the active tab uses right, then left, then the last main address', () => {
  const { tabs } = model(); tabs.visit('/favorites?query=keep', workspace());
  for (const id of ['a','b','c']) tabs.visit(docAddress(id), workspace());
  assert.equal(tabs.close('b', '/doc/b'), '/doc/c'); assert.equal(tabs.close('c', '/doc/c'), '/doc/a');
  assert.equal(tabs.close('a', '/doc/a'), '/favorites?query=keep');
});

test('closing a nonactive tab leaves the address intact and reordered note ids persist', () => {
  const { tabs, disk } = model(); for (const id of ['a','b','c']) tabs.visit(docAddress(id), workspace());
  tabs.reorder('c', 0); assert.deepEqual(JSON.parse(disk.values.get(KEYS.tabs)).state.openIds, ['c','a','b']);
  assert.equal(tabs.close('a', '/doc/b'), '/doc/b'); assert.deepEqual(tabs.snapshot().openIds, ['c','b']);
});

test('main navigation remembers query strings and session routes without persisting them', () => {
  const { tabs } = model(); tabs.visit('/calendar?mode=month&date=2026-02-03', workspace()); tabs.visit('/doc/a', workspace());
  assert.equal(tabs.snapshot().lastMain, '/calendar?mode=month&date=2026-02-03');
  tabs.visit('/session/s1', workspace()); tabs.visit('/doc/b', workspace()); assert.equal(tabs.snapshot().lastMain, '/session/s1');
});

test('missing documents select the first live note tab, independently of adjacent external tabs', () => {
  const { tabs } = model(['b', 'a']); tabs.visit(fileAddress('/x.md'), workspace()); tabs.visit('/guide', workspace());
  assert.equal(tabs.visit('/doc/missing', workspace()), '/doc/b');
  tabs.attach(workspace([])); assert.equal(tabs.visit('/doc/missing', workspace([])), '/calendar');
});

test('root and unknown paths redirect without removing the workspace', () => {
  const { tabs } = model(); const data = workspace();
  assert.equal(tabs.visit('/', data), '/calendar'); assert.equal(tabs.visit('/typo', data), '/launcher');
  assert.equal(data.path, '/fixture'); assert.equal(tabs.visit('/doc/a', null), '/launcher');
  assert.equal(tabs.visit('/launcher?mode=create', null), '/launcher?mode=create');
});

test('workspace changes clear all tabs while pruning preserves transient tabs', () => {
  const { tabs } = model(['a','b']); tabs.visit('/terminal', workspace());
  tabs.attach(workspace(['a'])); assert.deepEqual(tabs.snapshot().openIds, ['a','term:main']);
  tabs.attach({ ...workspace(), path: '/different' }); assert.deepEqual(tabs.snapshot().openIds, []); assert.equal(tabs.snapshot().workspaceRoot, '/different');
});

test('frozen failed-read and sample behavior prunes saved notes while retaining the real root', () => {
  const { tabs, disk } = model(['a','b']); tabs.attach(sampleWorkspace());
  assert.deepEqual(JSON.parse(disk.values.get(KEYS.tabs)), { state: { workspaceRoot: '/fixture', openIds: [] }, version: 1 });
  tabs.attach(workspace()); assert.deepEqual(tabs.snapshot().openIds, []);
  tabs.visit('/doc/a', workspace()); tabs.attach({ ...workspace([]), noteReadFailed: true }); assert.deepEqual(tabs.snapshot().openIds, []);
});

test('a read-only note-window model never writes main tabs', () => {
  const disk = storage(persisted(['a'])), tabs = new TabModel(disk, false);
  tabs.attach(workspace()); tabs.visit('/doc/b', workspace()); tabs.reorder('a', 2); tabs.close('a', '/doc/a');
  assert.deepEqual(disk.writes, []); assert.equal(disk.values.get(KEYS.tabs), persisted(['a']));
});

test('calendar queries override memory and missing queries restore the last view', () => {
  const first = calendarView(new URLSearchParams(), null, '2026-11-01'); assert.deepEqual(first, { mode: 'day', date: '2026-11-01', week: '2026-10-26' });
  const second = calendarView(new URLSearchParams('mode=month&date=2026-02-20'), first, '2026-11-01');
  assert.deepEqual(calendarView(new URLSearchParams(), second, '2026-11-02'), second);
  assert.equal(calendarView(new URLSearchParams('date=2026-02-30'), second, '2026-11-01').date, '2026-02-20');
  assert.equal(calendarView(new URLSearchParams('week=2026-11-01'), second, '2026-11-01').week, '2026-10-26');
});

function scrollFixture() {
  let now = 0, next = 0;
  const listeners = new Map(), frames = new Map();
  const surface = { scrollTop: 0, addEventListener: (type, callback) => listeners.set(type, callback), removeEventListener: type => listeners.delete(type) };
  const clock = { now: () => now, frame: callback => { frames.set(++next, callback); return next; }, cancel: id => frames.delete(id) };
  const event = type => listeners.get(type)?.(new Event(type));
  const advance = ms => { now += ms; const pending = [...frames.values()]; frames.clear(); pending.forEach(callback => callback(now)); };
  return { surface, clock, event, advance, frames, listeners };
}

test('unknown scroll positions do not move content and user scroll is recorded', () => {
  const h = scrollFixture(), memory = new NoteScrollMemory(); h.surface.scrollTop = 37;
  const stop = rememberNoteScroll(h.surface, 'a', memory, h.clock); assert.equal(h.surface.scrollTop, 37); assert.equal(h.frames.size, 0);
  h.surface.scrollTop = 91; h.event('scroll'); assert.equal(memory.get('a'), 91); stop(); assert.equal(h.listeners.size, 0);
});

test('remembered zero is restored and layout changes are held for 1500 ms without overwriting memory', () => {
  const h = scrollFixture(), memory = new NoteScrollMemory(); memory.set('a', 0); h.surface.scrollTop = 80;
  const stop = rememberNoteScroll(h.surface, 'a', memory, h.clock); assert.equal(h.surface.scrollTop, 0);
  h.surface.scrollTop = 200; h.event('scroll'); assert.equal(memory.get('a'), 0); h.advance(1400); assert.equal(h.surface.scrollTop, 0);
  h.advance(101); h.surface.scrollTop = 60; h.event('scroll'); assert.equal(memory.get('a'), 60); stop();
});

test('wheel, pointer and keyboard input release restoration immediately', () => {
  for (const type of ['wheel','pointerdown','keydown']) {
    const h = scrollFixture(), memory = new NoteScrollMemory(); memory.set('a', 300);
    const stop = rememberNoteScroll(h.surface, 'a', memory, h.clock); h.event(type); h.surface.scrollTop = 500; h.event('scroll'); h.advance(200);
    assert.equal(h.surface.scrollTop, 500); assert.equal(memory.get('a'), 500); assert.equal(h.frames.size, 0); stop();
  }
});

test('scroll memories are isolated per window and cleanup cancels pending frames', () => {
  const h = scrollFixture(), a = new NoteScrollMemory(), b = new NoteScrollMemory(); a.set('doc', 200);
  assert.equal(b.get('doc'), undefined); const stop = rememberNoteScroll(h.surface, 'doc', a, h.clock); assert.equal(h.frames.size, 1); stop(); assert.equal(h.frames.size, 0);
});

async function controllerFixture(t, context = { kind: 'main' }) {
  const disk = storage(persisted(['a','b'])); disk.values.set(KEYS.last, JSON.stringify({ path: '/fixture', name: 'fixture' }));
  const data = workspace(), calls = [], locations = [];
  const services = { storage: disk, host: {
    invoke: async (command, args) => { calls.push({ command, args });
      if (command === 'vault_root') return '/fixture';
      if (command === 'note_windows' || command === 'saved_note_windows') return [];
      if (command === 'sidecar_call') { const value = args.argv[0] === 'list-docs' ? data.notes : args.argv[0] === 'list-agendas' ? data.agendas : [];
        return { status: 0, stdout: JSON.stringify(value), stderr: '' }; }
    }, documentDir: async () => '/docs', confirmLeave: async () => false,
  }, files: { exists: async () => false }, watch: async () => () => {}, navigate: (path, replace) => locations.push({ path, replace }),
    pending: { flushReports: async () => {}, flushEditors: async () => {}, waitForWrites: async () => true, unsavedNotes: () => 0 },
    now: () => new Date('2026-11-01T00:00:00Z'), uuid: () => 'uuid', sleep: async () => {}, warn: () => {} };
  const controller = new WorkspaceController(services, context, '/'); t.after(() => controller.dispose()); await controller.boot();
  return { controller, disk, calls, locations, services };
}

test('controller follows external popstate routes and replacement guards preserve the last workspace', async t => {
  const h = await controllerFixture(t), last = h.disk.values.get(KEYS.last);
  h.controller.locationChanged('/doc/a'); assert.equal(activeTab(h.controller.getSnapshot().route), 'a');
  h.controller.locationChanged('/unknown'); assert.equal(h.controller.getSnapshot().route, '/launcher');
  assert.deepEqual(h.locations.at(-1), { path: '/launcher', replace: true }); assert.equal(h.disk.values.get(KEYS.last), last);
  h.controller.locationChanged('/doc/missing'); assert.equal(h.controller.getSnapshot().route, '/doc/a');
});

test('moving a note to a native window closes its tab and keeps browser location synchronized', async t => {
  const h = await controllerFixture(t); h.controller.navigate('/doc/a'); h.controller.setNoteWindows(['a']);
  assert.equal(h.controller.getSnapshot().route, '/doc/b'); assert.equal(h.locations.at(-1).path, '/doc/b');
  assert.ok(!h.controller.getSnapshot().tabs.openIds.includes('a'));
  h.controller.navigate('/doc/a'); assert.equal(h.controller.getSnapshot().route, '/doc/b');
  assert.ok(h.calls.some(call => call.command === 'open_note_window' && call.args.docId === 'a'));
});

test('terminal close detaches the exact key and a closed session tab is not immediately reopened', async t => {
  const h = await controllerFixture(t); h.controller.navigate('/terminal'); h.controller.closeActiveTab();
  assert.ok(h.calls.some(call => call.command === 'pty_detach' && call.args.key === 'term:main'));
  h.controller.navigate('/terminal/session'); h.controller.closeActiveTab(); assert.ok(h.controller.getSnapshot().tabs.openIds.includes('term:session'));
  h.controller.closeTab('term:session'); assert.ok(!h.controller.getSnapshot().tabs.openIds.includes('term:session'));
  assert.equal(h.controller.getSnapshot().route, '/terminal/session');
});

test('focus refresh updates the real current day but leaves sample dates fixed', async t => {
  const h = await controllerFixture(t); h.services.now = () => new Date('2026-11-02T00:00:00Z'); h.controller.refresh();
  assert.equal(h.controller.getSnapshot().workspace.today, '2026-11-02');
  await h.controller.sample(); h.controller.refresh(); assert.equal(h.controller.getSnapshot().workspace.today, '2026-06-04');
});
