/* Acceptance-only driver; never included in the application bundle. */
const cfg = window.__AIS_FIXTURE__;
const invoke = (...args) => window.__TAURI_INTERNALS__.invoke(...args);
const assert = (condition, message) => { if (!condition) throw Error(message || 'assertion failed'); };
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(predicate, timeout = 30000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) { if (await predicate()) return; await wait(40); }
  throw Error('timed out waiting for workspace state');
}
const post = (path, value) => fetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(value) });
const calls = async () => (await fetch('/calls')).json();
const text = () => document.body.textContent;
const click = selector => { const element = document.querySelector(selector); assert(element && !element.disabled, `Cannot click ${selector}`); element.click(); };
const fill = (selector, value) => {
  const element = document.querySelector(selector); assert(element, `Missing ${selector}`);
  Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(element, value);
  element.dispatchEvent(new Event('input', { bubbles: true }));
};
const navigate = path => { history.pushState({}, '', path); dispatchEvent(new PopStateEvent('popstate')); };

if (location.pathname.startsWith('/note-window/')) {
  try {
    await until(() => document.querySelector('.note-body'));
    assert(text().includes('격리된 vault의 본문이에요.'), 'note body');
    assert(!window.__AIS_WRITES__.length, 'note window wrote browser storage');
    assert(document.documentElement.dataset.theme === 'dark', 'note theme');
    await post('/note-result', { ok: true });
  } catch (error) { await post('/note-result', { ok: false, error: String(error) }); }
} else {
  const results = [];
  let sampleCallCount;
  const test = async (id, work) => {
    try { await work(); results.push({ id, ok: true }); }
    catch (error) { results.push({ id, ok: false, error: String(error) }); }
    await post('/progress', results.at(-1));
    if (!results.at(-1).ok) throw Error(id);
  };
  try {
    await test('react-vault-boot', async () => {
      await until(() => document.querySelector('[data-rail-section="nav"]'));
      assert(typeof window.__TAURI__ === 'undefined', 'global Tauri object must be absent');
      assert(text().includes('실제 엔진 기록') && text().includes('옛 노트'), 'engine notes missing');
      assert(!document.querySelector('.read-failure'), 'engine read failed');
      assert(document.documentElement.dataset.theme === 'dark', 'main theme');
    });
    await test('raw-vault-path-and-batches', async () => {
      assert(await invoke('vault_root') === cfg.raw, 'raw path changed');
      const initial = (await calls()).slice(0, 7);
      const names = initial.map(c => c.args.argv[0]);
      assert(names[0] === 'init-workspace' && JSON.stringify(names.slice(1,4).sort()) === JSON.stringify(['list-templates','list-trash','reindex']) && JSON.stringify(names.slice(4).sort()) === JSON.stringify(['list-agendas','list-docs','list-projects']), 'initial command groups: ' + names.join(', '));
      assert(JSON.stringify(initial.find(c => c.args.argv[0] === 'list-docs').args.argv) === JSON.stringify(['list-docs', 'limit=0', 'bodies=true']), 'bodies or unlimited flag missing');
    });
    await test('legacy-note-readable', async () => {
      const result = await invoke('sidecar_call', { argv: ['list-docs', 'limit=0', 'bodies=true'] });
      const notes = JSON.parse(result.stdout), legacy = notes.find(note => note.id === cfg.legacyId);
      assert(legacy?.body.includes('이관 전 본문') && !legacy.path.endsWith('legacy.md'), 'migration did not preserve body: ' + JSON.stringify(legacy));
    });
    await test('note-window-read-only-boot', async () => {
      await invoke('open_note_window', { docId: cfg.id, title: '검증 노트' });
      let note;
      await until(async () => { note = await (await fetch('/note-result')).json(); return note; });
      assert(note.ok, note.error);
      await invoke('close_note_windows');
      await until(async () => !(await invoke('note_windows')).length);
    });
    await test('sample-preserves-real-vault', async () => {
      const last = localStorage.getItem('aisurada.lastWorkspace.v1');
      navigate('/launcher'); await until(() => document.querySelector('.sample-button'));
      click('.sample-button'); await until(() => document.querySelector('.sample-badge'));
      assert(localStorage.getItem('aisurada.lastWorkspace.v1') === last, 'sample replaced last real vault');
      // An earlier focus refresh may finish while the transition waits in the
      // queue. From the moment the sample opens, no engine read may follow.
      sampleCallCount = (await calls()).length;
      assert(await invoke('vault_root') === cfg.raw, 'sample changed native vault');
    });
    await test('sample-md1-and-launcher', async () => {
      navigate('/doc/md1'); await until(() => document.querySelector('.note-body'));
      assert(text().includes('생각이 떠오른 순간'), 'sample markdown paragraph missing');
      assert((await calls()).length === sampleCallCount, 'sample note called engine');
      click('.tab-main .tab-button'); await until(() => document.querySelector('.rail-back'));
      click('.rail-back'); await until(() => document.querySelector('.launcher'));
      assert(localStorage.getItem('aisurada.lastWorkspace.v1') === 'null', 'launcher did not clear last workspace');
    });
    await test('folder-open-ui-and-recent', async () => {
      click('.workspace-option:nth-child(2)'); await until(() => document.querySelector('input[name="path"]'));
      fill('input[name="path"]', cfg.raw); await wait(0); click('.workspace-form button[type="submit"]');
      await until(() => document.querySelector('[data-rail-section="nav"]') && document.querySelector('.workspace-shell')?.getAttribute('aria-busy') === 'false');
      assert(JSON.parse(localStorage.getItem('aisurada.workspaces.v1'))[0].path === cfg.raw, 'recent real path missing');
      click('.rail-back'); await until(() => document.querySelector('.recent-open'));
      const order = localStorage.getItem('aisurada.workspaces.v1');
      click('.recent-open'); await until(() => document.querySelector('[data-rail-section="nav"]'));
      assert(localStorage.getItem('aisurada.workspaces.v1') === order, 'recent click reordered list');
    });
    await test('creation-ui-initializes-twice', async () => {
      click('.rail-back'); await until(() => document.querySelector('.launcher'));
      click('.workspace-option:first-child'); await until(() => document.querySelector('input[name="name"]'));
      fill('input[name="name"]', 'vault'); fill('input[name="parent"]', cfg.parent); await wait(0);
      const before = (await calls()).filter(c => c.args.argv[0] === 'init-workspace').length;
      click('.workspace-form button[type="submit"]');
      await until(() => document.querySelector('.workspace-shell')?.getAttribute('aria-busy') === 'false');
      assert((await calls()).filter(c => c.args.argv[0] === 'init-workspace').length === before + 2, 'creation init count');
      assert(JSON.parse(localStorage.getItem('aisurada.workspaces.v1'))[0].created === true, 'created flag missing');
      assert(JSON.parse(localStorage.getItem('aisurada.lastWorkspace.v1')).path === cfg.vault, 'created path missing');
    });
  } catch { /* Stop dependent checks after their first failure. */ }
  await post('/result', results);
}
