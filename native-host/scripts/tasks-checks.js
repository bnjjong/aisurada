/* 실제 WKWebView와 앱 안의 Rust 엔진, 격리된 vault로 태스크 화면, 알림 센터, 러너 상태 화면을 점검한다. 이슈 #73.
   러너의 저장소 받기는 GitHub 앱 자격이 있어야 해서 격리 환경에서는 늘 실패한다. 그래서 「지금 실행」을 눌러 실패가 기록과 알림으로
   화면에 나오는 데까지를 본다(가짜 claude는 실행 파일이 있다는 사실만 쓴다). */
const cfg = window.__AIS_FIXTURE__;
const T = cfg.tasks;
const invoke = (...args) => window.__TAURI_INTERNALS__.invoke(...args);
const assert = (condition, message) => { if (!condition) throw Error(message || 'assertion failed'); };
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(predicate, what = '화면 상태', timeout = 30000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) { if (await predicate()) return; await wait(40); }
  throw Error(`timed out: ${what} (${location.pathname}) ${document.body.textContent.slice(-300)}`);
}
const post = (path, value) => fetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(value) });
const calls = async () => (await fetch('/calls')).json();
const callsOf = async name => (await calls()).filter(c => c.args.argv[0] === name);
const navigate = path => { history.pushState({}, '', path); dispatchEvent(new PopStateEvent('popstate')); };
const all = (selector, root = document) => [...root.querySelectorAll(selector)];
const byText = (selector, text, root = document) => all(selector, root).find(e => e.textContent.trim().includes(text));
const fillField = (element, value) => {
  assert(element, '채울 칸이 없다');
  const proto = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : element instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, 'value').set.call(element, value);
  element.dispatchEvent(new Event(element instanceof HTMLSelectElement ? 'change' : 'input', { bubbles: true }));
};
const engine = async argv => {
  const r = await invoke('sidecar_call', { argv });
  assert(r.status === 0, `${argv[0]} 실패: ${r.stderr}`);
  return JSON.parse(r.stdout);
};
const column = title => all('.task-col').find(c => c.getAttribute('aria-label') === title);
const card = key => document.querySelector(`.task-card[data-task-key="${key}"]`);
const bell = () => all('button').find(b => /^알림( \d+개)?$/.test(b.getAttribute('aria-label') || ''));

const results = [];
async function test(id, work) {
  try { await work(); results.push({ id, ok: true }); }
  catch (error) { results.push({ id, ok: false, error: String(error) }); }
  await post('/progress', results.at(-1));
  if (!results.at(-1).ok) throw Error(id);
}

let created;
try {
  await until(() => document.querySelector('[data-rail-section="nav"]'), '레일');

  await test('tasks-board-shows-the-seeded-draft-task-and-the-runner-bar', async () => {
    navigate('/tasks');
    await until(() => card(T.seededKey), '시드 태스크 카드');
    assert(column('Draft')?.contains(card(T.seededKey)), 'Draft 열에 있다');
    assert(card(T.seededKey).textContent.includes('저장소 읽기 조사'), '카드 제목');
    assert(card(T.seededKey).querySelector('.task-progress')?.textContent.trim() === '0/1', '완료 기준 0/1');
    assert(document.querySelector('.task-board'), '보드');
    // 러너 줄: Ready 태스크를 개발 실행할 수 있다고 알린다
    assert(byText('.task-page', '러너') || document.querySelector('.runner-bar'), '러너 줄');
  });

  await test('quick-title-opens-the-form-and-creates-through-the-engine', async () => {
    const quick = document.querySelector('input[aria-label="요약만 쓰고 Enter"]');
    fillField(quick, '화면에서 만든 조사');
    quick.form.requestSubmit();
    await until(() => document.querySelector('.ui-modal input[aria-label="요약"]'), '태스크 폼');
    assert(document.querySelector('input[aria-label="요약"]').value === '화면에서 만든 조사', '빠른 입력의 요약이 폼에 채워진다');
    const repo = document.querySelector('select[aria-label="저장소"]') || document.querySelector('input[aria-label="저장소"]');
    assert(repo, '저장소 칸');
    fillField(repo, 'me/web');
    fillField(document.querySelector('textarea[aria-label="설명"]'), '실제 창에서 쓴 설명');
    if (!document.querySelector('[aria-label="완료 기준 1"]')) document.querySelector('.task-add').click();
    await until(() => document.querySelector('[aria-label="완료 기준 1"]'), '완료 기준 칸');
    fillField(document.querySelector('[aria-label="완료 기준 1"]'), '결과를 댓글로 남긴다');
    await wait(80);
    const submit = byText('.ui-modal footer button[type="submit"]', '만들기');
    assert(submit && !submit.disabled, '만들기 단추');
    submit.click();
    await until(() => /^\/task\/[^/]+$/.test(location.pathname), '만든 뒤 상세로 이동');
    created = decodeURIComponent(location.pathname.split('/')[2]);
    assert(created !== T.seededKey, '새 키');
    assert((await callsOf('create-task')).some(c => c.args.argv.includes('title=화면에서 만든 조사')), 'create-task 호출');
    await until(() => document.querySelector('.task-title')?.textContent === '화면에서 만든 조사', '상세 제목');
  });

  await test('task-detail-readiness-status-menu-and-comment', async () => {
    await until(() => document.querySelector('.task-ready li'), 'Ready 조건');
    assert(all('.task-ready li').length === 3 && all('.task-ready li').every(li => li.classList.contains('is-ok')), `조건 셋이 모두 찼다: ${all('.task-ready li').map(li => li.textContent).join('|')}`);
    document.querySelector('.task-status-button').click();
    await until(() => document.querySelector('.task-menu'), '상태 메뉴');
    const ready = all('.task-menu [role="menuitem"]').find(i => i.textContent.includes('Ready For Develop'));
    assert(ready && !ready.disabled, 'Ready For Develop 항목이 열려 있다');
    ready.click();
    await until(() => document.querySelector('.task-status-button')?.textContent.includes('Ready For Develop'), '상태가 바뀜');
    assert((await callsOf('transition-task')).some(c => c.args.argv.includes('to=ready-for-develop')), 'transition-task 호출');
    fillField(document.querySelector('input[aria-label="댓글"]'), '실제 창의 댓글');
    await wait(60);
    const send = document.querySelector('.task-comment-form button[type="submit"]');
    assert(send && !send.disabled, '댓글 단추');
    send.click();
    await until(() => byText('.task-comment-body', '실제 창의 댓글'), '댓글이 활동에 나옴');
    assert((await callsOf('add-task-comment')).length === 1, 'add-task-comment 호출');
  });

  await test('board-list-view-and-filter', async () => {
    navigate('/tasks');
    await until(() => card(created), '새 태스크 카드');
    assert(column('Ready For Develop')?.contains(card(created)), '새 태스크가 Ready For Develop 열에 있다');
    assert(column('Draft')?.contains(card(T.seededKey)), '시드 태스크는 Draft에 남는다');
    byText('.task-tabs [role="tab"]', '목록').click();
    await until(() => document.querySelector('.task-table'), '목록 보기');
    assert(document.querySelector(`.task-table tr[data-task-key="${T.seededKey}"]`) && document.querySelector(`.task-table tr[data-task-key="${created}"]`), '두 태스크가 목록에 있다');
    fillField(document.querySelector('input[aria-label="태스크 검색"]'), created);
    await until(() => !document.querySelector(`.task-table tr[data-task-key="${T.seededKey}"]`), '검색으로 하나만 남음');
    assert(document.querySelector(`.task-table tr[data-task-key="${created}"]`), '검색한 태스크가 남는다');
    fillField(document.querySelector('input[aria-label="태스크 검색"]'), '');
    byText('.task-tabs [role="tab"]', '보드').click();
    await until(() => document.querySelector('.task-board'), '보드로 돌아옴');
  });

  await test('runner-run-fails-visibly-and-leaves-a-record', async () => {
    await until(() => card(created)?.querySelector('.task-run'), '지금 실행 단추');
    const run = card(created).querySelector('.task-run');
    assert(!run.disabled, 'Ready 태스크를 개발 실행할 수 있다');
    run.click();
    // 저장소를 받지 못해 실패로 끝난다: 태스크는 Failed로 가고 실행 기록과 러너의 댓글이 남는다
    navigate(`/task/${encodeURIComponent(created)}`);
    await until(() => document.querySelector('.runner-phase-failed'), '실패한 실행 기록', 90000);
    const runPanel = document.querySelector('.runner-panel');
    assert(runPanel.querySelector('.runner-fail')?.textContent.includes('저장소를 준비하지 못했다'), `실패 까닭이 보인다: ${runPanel.textContent.slice(0, 200)}`);
    await until(() => document.querySelector('.task-status-button')?.textContent.includes('Failed'), '태스크 상태가 Failed');
    await until(() => byText('.task-comment-body', '실행이 실패했어요'), '러너의 실패 댓글');
    // 러너의 걸음은 호스트가 직접 엔진을 부르므로 화면의 호출 기록이 아니라 엔진에 남은 활동으로 본다
    const task = await engine(['get-task', created]);
    assert(task.status === 'failed', `엔진의 상태도 failed: ${task.status}`);
    assert(task.activity.some(e => e.kind === 'status' && e.to === 'failed' && e.by === 'runner'), '러너가 failed로 옮긴 활동이 남았다');
  });

  await test('notification-center-shows-the-failure-and-marks-it-read', async () => {
    await until(() => bell() && /알림 \d+개/.test(bell().getAttribute('aria-label')), '안 읽은 알림 배지', 30000);
    bell().click();
    await until(() => document.querySelector('.ntf-panel'), '알림 센터');
    const item = all('.ntf-panel li, .ntf-panel [role="listitem"], .ntf-panel .ntf-item').find(e => e.textContent.includes('실행이 실패했어요'));
    assert(item, `실패 알림이 있다: ${document.querySelector('.ntf-panel').textContent.slice(0, 200)}`);
    assert(item.textContent.includes('화면에서 만든 조사'), '알림에 태스크 제목');
    assert(item.querySelector('[aria-label="안 읽음"]'), '안 읽음 표시');
    const readAll = byText('.ntf-panel header button', '모두 읽음');
    assert(readAll && !readAll.disabled, '모두 읽음 단추가 열려 있다');
    readAll.click();
    await until(() => bell() && bell().getAttribute('aria-label') === '알림', '배지가 사라짐');
    assert(!all('.ntf-panel [aria-label="안 읽음"]').length, '안 읽음 표시가 없어졌다');
    // 알림의 열기는 그 태스크 화면으로 간다
    navigate('/tasks');
    await until(() => location.pathname === '/tasks', '태스크 목록');
    if (!document.querySelector('.ntf-panel')) bell().click();
    await until(() => document.querySelector('.ntf-panel'), '알림 센터를 다시 엶');
    byText('.ntf-panel [role="tab"]', '전체').click();
    await until(() => all('.ntf-panel li').some(li => li.textContent.includes('실행이 실패했어요')), '읽은 알림은 전체 보기에 남는다');
    const again = all('.ntf-panel li').find(li => li.textContent.includes('실행이 실패했어요'));
    byText('button', '태스크 보기', again).click();
    await until(() => location.pathname === `/task/${encodeURIComponent(created)}`, '알림에서 태스크로 이동');
  });
} catch { /* 첫 실패 뒤의 점검은 앞 상태에 기대므로 멈춘다 */ }
await post('/result', results);
