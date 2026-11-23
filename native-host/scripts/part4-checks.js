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
const fillText = (element, value) => {
  const proto = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, 'value').set.call(element, value);
  element.dispatchEvent(new Event('input', { bubbles: true }));
};
const pollEngine = async (argv, ok, label) => {
  let out = '';
  for (let n = 0; n < 200; n++) {
    const r = await invoke('sidecar_call', { argv });
    out = r.status === 0 ? r.stdout : `status ${r.status}: ${r.stderr}`;
    if (ok(out)) return out;
    await wait(50);
  }
  throw Error(`${label}: ${out.slice(0, 300)}`);
};
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
  // 세션 화면의 입력창: 가짜 claude가 이어 쓰기에 답하면 입력칸이 비고 오류가 안 남는다. 입력창이 막히는 경우(다른 터미널이 쥔 세션 따위)는 순수 함수 시험이 본다.
  if (cfg.part4Check) await test('enabled-session-composer-sends-through-the-host', async () => {
    go(`/session/${cfg.sessionId}`);
    await until(() => document.querySelector('.sess-page-turns')?.textContent.includes('세션 점검용 첫 답'));
    await until(() => document.querySelector('textarea[aria-label="세션으로 보내기"]'));
    assert(!document.querySelector('.sess-composer-blocked'), `입력창이 막혀 있다: ${document.querySelector('.sess-composer-blocked')?.textContent}`);
    const box = document.querySelector('textarea[aria-label="세션으로 보내기"]');
    const send = () => document.querySelector('.sess-composer-send');
    assert(send().disabled, '빈 입력칸에서는 보내기가 닫혀 있다');
    fillText(box, '이어서 한 가지만 더 물어볼게요');
    await until(() => !send().disabled);
    send().click();
    // 누르면 입력칸은 곧바로 비므로 그것만으로는 성공을 모른다. 보내는 중(멈추기 단추)이 끝난 뒤에도 오류가 없어야 하고, 실패하면 글이 되돌아온다.
    await until(() => document.querySelector('textarea[aria-label="세션으로 보내기"]')?.value === '', '누르면 입력칸이 빈다');
    await wait(150);
    const sendEnd = Date.now() + 25000;
    while (Date.now() < sendEnd && document.querySelector('.sess-composer-stop')) await wait(50);
    assert(!document.querySelector('.sess-composer-stop'), '보내기가 끝나지 않았다');
    await wait(400);
    assert(!document.querySelector('.sess-composer-error'), `오류가 남았다: ${document.querySelector('.sess-composer-error')?.textContent}`);
    assert(document.querySelector('textarea[aria-label="세션으로 보내기"]').value === '', '실패했다면 글이 되돌아온다: 입력칸이 비어 있어야 한다');
    assert(document.querySelector('.sess-composer-send') && document.querySelector('textarea[aria-label="세션으로 보내기"]'), '입력창이 다시 쓸 수 있다');
  });
  // 「노트로 굳히기」: 세션 대화를 읽어 노트로 만들고 그 노트로 간다. 만든 노트는 엔진에서 읽힌다.
  if (cfg.part4Check) await test('enabled-session-pin-creates-a-note-real-engine', async () => {
    go(`/session/${cfg.sessionId}`);
    await until(() => document.querySelector('.sess-page-turns')?.textContent.includes('세션 점검용 첫 답'));
    await until(() => document.querySelector('.sess-page-pin') && !document.querySelector('.sess-page-pin').disabled);
    document.querySelector('.sess-page-pin').click();
    await until(() => document.querySelector('.session-pin-modal .pin-hint'));
    assert(document.querySelector('.session-pin-modal .pin-shapes')?.textContent.includes('0 건'), '비밀 모양 검사 결과가 보인다');
    const title = document.querySelector('.session-pin-modal input[type="text"]');
    fillText(title, '굳힌 세션 점검');
    await wait(60);
    const save = [...document.querySelectorAll('.session-pin-modal footer .btn-primary')].find(b => b.textContent.trim() === '굳히기');
    assert(save && !save.disabled, '굳히기 단추');
    save.click();
    await until(() => location.pathname.startsWith('/doc/') && document.querySelector('.note-reading'), '굳힌 노트로 이동');
    const id = decodeURIComponent(location.pathname.split('/')[2]);
    const doc = JSON.parse(await pollEngine(['get-doc', id], out => out.includes('세션 점검용 첫 답'), '굳힌 노트 본문'));
    assert(doc.title === '굳힌 세션 점검', `제목: ${doc.title}`);
    assert(doc.body.includes('세션 점검용 첫 물음') && doc.body.includes('세션 점검용 첫 답'), '대화가 본문에 담겼다');
    assert(Array.isArray(doc.tags) && doc.tags.includes('세션전사'), `태그: ${JSON.stringify(doc.tags)}`);
  });
  // 설정의 「Claude 연결」: 실제 창에서 눌러 보고, 진짜 ais-cli(claude-mcp)가 가짜 claude에 등록하는 왕복을 잰다
  if (cfg.part4Check) await test('enabled-claude-connect-real-host', async () => {
    go('/');
    await until(() => document.querySelector('button[aria-label="설정"]'));
    document.querySelector('button[aria-label="설정"]').click();
    await until(() => document.querySelector('#set-tab-ai'));
    document.querySelector('#set-tab-ai').click();
    await until(() => document.querySelector('.settings-modal .set-connect'));
    const status = () => document.querySelector('.set-connect-status')?.textContent || '';
    await until(() => status().includes('연결되어 있지 않아요'));
    assert(!document.querySelector('.set-connect-actions button:not(.button-primary)'), '연결 전에는 끊기 단추가 없어야 한다');
    // 데스크톱용 조각은 등록 없이도 보인다
    await until(() => document.querySelector('.set-connect-desktop pre')?.textContent.includes('mcpServers'));
    const snippet = () => document.querySelector('.set-connect-desktop pre').textContent;
    assert(snippet().includes('"mcp"') && !snippet().includes('--write'), '읽기 전용 조각이어야 한다');
    // 읽기 전용으로 연결
    document.querySelector('.set-connect-actions .button-primary').click();
    await until(() => status().includes('노트를 읽기만 해요'));
    // 쓰기를 켜고 다시 연결
    document.querySelector('.set-connect-write input').click();
    await until(() => snippet().includes('--write'));
    document.querySelector('.set-connect-actions .button-primary').click();
    await until(() => status().includes('노트를 읽고 쓸 수 있어요'));
    // 끊기
    [...document.querySelectorAll('.set-connect-actions button')].find(b => b.textContent.includes('연결 끊기')).click();
    await until(() => status().includes('연결되어 있지 않아요'));
    assert(!document.querySelector('.set-connect-error'), `오류 글이 남았다: ${document.querySelector('.set-connect-error')?.textContent}`);
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await until(() => !document.querySelector('.settings-modal'));
  });
  // 세션 칩의 「열기」: 레일은 본문 아래가 아니라 오른쪽에, 본문과 따로 스크롤되는 칸으로 떠야 한다.
  if (cfg.part4Check) await test('enabled-note-rail-beside-body-not-below', async () => {
    go(`/doc/${cfg.id}`);
    await until(() => document.querySelector('.doc-scroll .note-reading .lex-editor'));
    await until(() => document.querySelector('.doc-meta-id-open'));
    document.querySelector('.doc-meta-id-open').click();
    await until(() => document.querySelector('.nsrail'));
    await wait(200);
    const rail = document.querySelector('.nsrail'), body = document.querySelector('.doc-scroll');
    assert(!body.contains(rail), '레일이 본문 스크롤 상자 안에 들어 있다');
    const r = rail.getBoundingClientRect(), b = body.getBoundingClientRect();
    assert(r.left >= b.right - 1, `레일이 본문 오른쪽이 아니다: 레일 left=${r.left} 본문 right=${b.right}`);
    assert(Math.abs(r.top - b.top) < 2 && Math.abs(r.height - b.height) < 2, `레일이 본문과 같은 높이가 아니다: ${r.top}/${r.height} vs ${b.top}/${b.height}`);
    assert(b.width > 300, `본문 칸이 너무 좁다: ${b.width}`);
    document.querySelector('.nsrail-close').click();
    await until(() => !document.querySelector('.nsrail'));
    assert(document.querySelector('.doc-scroll').getBoundingClientRect().width >= b.width + r.width - 2, '레일을 닫아도 본문 칸이 넓어지지 않는다');
  });
  // 세션 레일의 대화: 노트 곁 레일에서 새 대화로 묻고, 가짜 claude의 답이 화면에 나오고 엔진(chats)에 적힌다.
  if (cfg.part4Check) await test('enabled-rail-chat-asks-and-persists', async () => {
    go(`/doc/${cfg.id}`);
    await until(() => document.querySelector('.doc-meta-id-open'));
    document.querySelector('.doc-meta-id-open').click();
    await until(() => document.querySelector('.nsrail'));
    await until(() => document.querySelector('textarea[aria-label="새 대화로 묻기"]'));
    assert(document.querySelector('.rchat-empty') || document.querySelector('.rchat'), '레일 대화 구역');
    const box = document.querySelector('textarea[aria-label="새 대화로 묻기"]');
    fillText(box, '레일에서 묻는 말');
    const send = () => document.querySelector('.nsrail-composer .sess-composer-send');
    await until(() => send() && !send().disabled, '보내기가 열림');
    send().click();
    await until(() => document.querySelector('.nsrail .rchat-turn-pair')?.textContent.includes('fixture answer'), '답이 레일에 나옴');
    assert(document.querySelector('.nsrail .rchat-turn-pair').textContent.includes('레일에서 묻는 말'), '내 질문도 보인다');
    const chats = await pollEngine(['list-chats', cfg.id], out => out.includes('fixture answer'), '대화가 엔진에 적힘');
    assert(chats.includes('레일에서 묻는 말'), '엔진의 대화 기록에 질문이 있다');
    assert(document.querySelector('.rchat-new'), '새 대화 열기 단추');
    document.querySelector('.nsrail-close').click();
    await until(() => !document.querySelector('.nsrail'));
  });
  // 후속 노트: 메인 창에서는 창이 아니라 탭으로 열리고, 일반 노트와 같은 본문 칸(.doc-scroll)·레일 칸(.doc-route) 구조를 쓴다.
  // (하위 노트는 list-sub-notes가 sessionId를 주지 않아 칩의 「열기」가 없다. 그래서 레일 대신 이 구조만 잰다.)
  await test('subnote-opens-as-tab-in-doc-layout', async () => {
    go(`/doc/${cfg.id}`);
    await until(() => document.querySelector('.doc-scroll .child-page-card'));
    document.querySelector('.doc-scroll .child-page-card').click();
    await until(() => location.pathname.startsWith('/sub/') && document.querySelector('.doc-route > .doc-scroll .note-reading'));
    assert(!document.querySelector('.subnote-page'), '메인 창에서 후속 노트가 창으로 떴다');
    assert(document.querySelector('.tab-item.tab-selected .tab-sub-mark'), '후속 노트 탭에 ↳ 표시가 없다');
    assert(document.querySelector(`.tab-item[data-tab-id="${cfg.id}"]`), '상위 노트 탭이 열려 있지 않다');
    const scroll = document.querySelector('.doc-route > .doc-scroll');
    assert(getComputedStyle(scroll).overflowY === 'auto', `후속 본문 칸이 스크롤 상자가 아니다: ${getComputedStyle(scroll).overflowY}`);
    assert(!document.querySelector('.nsrail'), '레일을 열지 않았는데 레일이 있다');
  });
} catch { /* failing check is already in results */ }
await post('/result', results);
