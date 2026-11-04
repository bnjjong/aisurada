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
  // 선택 → 「AI에게 묻기」 → 묻는 칸 → 가짜 claude의 답 → 노트 아래 「AI 문답」 칸 → 엔진이 쓴 ask.json까지 이어지는 실제 왕복
  if (cfg.part4Check) await test('enabled-ask-roundtrip-real-host', async () => {
    const paragraph = [...document.querySelectorAll('.note-reading .lex-editor p')].find(p => p.textContent.includes('격리된 vault의 본문이에요.'));
    assert(paragraph, 'target paragraph missing');
    const root = paragraph.closest('[contenteditable]') ?? document.querySelector('.note-reading .lex-editor');
    const text = document.createTreeWalker(paragraph, NodeFilter.SHOW_TEXT).nextNode();
    root.focus({ preventScroll: true });
    const range = document.createRange(); range.setStart(text, 0); range.setEnd(text, text.textContent.length);
    getSelection().removeAllRanges(); getSelection().addRange(range);
    await until(() => document.querySelector('.sel-bar .ask-add-btn'));
    document.querySelector('.sel-bar .ask-add-btn').click();
    await until(() => document.querySelector('textarea[aria-label="묻는 내용"]'));
    const box = document.querySelector('textarea[aria-label="묻는 내용"]');
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value').set.call(box, '이 문장은 무슨 뜻이야?');
    box.dispatchEvent(new Event('input', { bubbles: true }));
    const submit = () => [...document.querySelectorAll('.link-popover-btn.primary')].find(button => button.textContent.trim() === '묻기');
    await until(() => submit() && !submit().disabled);
    submit().click();
    await until(() => document.querySelector('.ask-panel')?.textContent.includes('fixture answer'));
    const poll = async (argv, ok, label) => {
      let out = '';
      for (let n = 0; n < 200; n++) {
        const r = await invoke('sidecar_call', { argv });
        out = r.status === 0 ? r.stdout : `status ${r.status}: ${r.stderr}`;
        if (ok(out)) return out;
        await wait(50);
      }
      throw Error(`${label}: ${out.slice(0, 300)}`);
    };
    const saved = await poll(['get-ask', cfg.id], out => out.includes('fixture answer'), 'ask.json has no answer');
    assert(saved.includes('이 문장은 무슨 뜻이야?'), 'ask.json has no question');
    await poll(['get-doc', cfg.id], out => /aisa:ak(?!fixture)[a-z0-9]+-[a-z0-9]+/.test(out), 'note body has no new ask marker');
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
  // 전사가 있는 세션: 엔진이 격리 HOME의 ~/.claude/projects를 읽고 화면이 대화를 그린다
  if (cfg.part4Check) await test('enabled-session-transcript-real-engine', async () => {
    const recent = await invoke('sidecar_call', { argv: ['recent-sessions', 'days=7'] });
    assert(recent.status === 0 && recent.stdout.includes(cfg.sessionId), `recent-sessions missed the fixture session: ${recent.stderr || recent.stdout.slice(0, 200)}`);
    go(`/session/${cfg.sessionId}`);
    await until(() => document.querySelector('.sess-screen'));
    await until(() => document.querySelector('.sess-page-turns')?.textContent.includes('세션 점검용 첫 답'));
    const turns = [...document.querySelectorAll('.sess-turn-text')].map(node => node.textContent);
    assert(turns.some(turn => turn.includes('세션 점검용 첫 물음')), 'user turn missing');
    assert(turns.some(turn => turn.includes('세션 점검용 첫 답')), 'assistant turn missing');
  });
} catch { /* failing check is already in results */ }
await post('/result', results);
