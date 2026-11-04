/* Actual WKWebView, real Java sidecar and isolated vault. Claude listing uses a local fake executable. */
const cfg = window.__AIS_FIXTURE__;
const results = [];
const invoke = (...args) => window.__TAURI_INTERNALS__.invoke(...args);
const assert = (condition, why) => { if (!condition) throw Error(why); };
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(predicate) {
  const end = Date.now() + 25000;
  while (Date.now() < end) { if (predicate()) return; await wait(30); }
  throw Error(`timed out: ${location.pathname}: ${document.body.textContent.slice(-1000) + '; markers=' + document.querySelectorAll('.lex-ask').length + '; dots=' + document.querySelectorAll('.lex-rownote-dot').length}`);
}
const go = path => { history.pushState({}, '', path); dispatchEvent(new PopStateEvent('popstate')); };
const post = (path, value) => fetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(value) });
async function test(id, work) {
  try { await work(); results.push({ id, ok: true }); }
  catch (error) { results.push({ id, ok: false, error: String(error) }); }
  await post('/progress', results.at(-1));
  if (!results.at(-1).ok) throw Error(id);
}
try {
  await until(() => document.querySelector('.tab-bar'));
  if (!cfg.part4Check) await test('disabled-host-has-no-claude-command', async () => {
    let error = '';
    try { await invoke('ask_runtime'); } catch (cause) { error = String(cause); }
    assert(error.includes('not found'), `off host still exposes ask_runtime: ${error}`);
  });
  await test('note-client-effects', async () => {
    go(`/doc/${cfg.id}`);
    await until(() => document.querySelector('.note-reading .lex-editor tr'));
    await wait(200);
    assert(document.querySelector('.lex-editor'), 'note screen crashed after effects');
    assert(!document.body.textContent.includes('이 화면을 여는 중 문제가'), 'route boundary caught render loop');
  });
  await test(cfg.part4Check ? 'enabled-table-ask-dot' : 'disabled-table-ask-mark-kept', async () => {
    await until(() => document.querySelector('.lex-ask'));
    if (cfg.part4Check) await until(() => document.querySelector('.lex-rownote-dot.is-ask'));
    else assert(!document.querySelector('.lex-rownote-dot.is-ask'), 'off build drew an ask dot');
    assert(document.querySelector('.lex-ask').textContent === '표 문답', 'marker text was damaged');
  });
  await test('sub-note-client-effects', async () => {
    document.querySelector('.child-page-card').click();
    await until(() => document.body.textContent.includes('하위 Markdown 파일의 본문이에요.'));
    await wait(150);
    assert(!document.body.textContent.includes('앱을 여는 중 문제가'), 'sub-note crashed');
  });
  if (cfg.part4Check) await test('enabled-session-route-real-host', async () => {
    go('/session/11111111-2222-4333-8444-555555555555');
    await until(() => document.querySelector('.sess-screen'));
    await until(() => document.body.textContent.includes('이 기계에 그 세션의 기록이 없어요.'));
    assert(document.querySelector('.prevcol .is-cli'), 'CLI pane was not mounted');
    const rail = document.querySelector('.prevcol');
    const toggle = document.querySelector('.prev-toggle');
    if (toggle) { toggle.click(); await wait(50); assert(document.querySelector('.prevcol') === rail, 'collapse unmounted rail'); }
  });
} catch { /* failing check is already in results */ }
await post('/result', results);
