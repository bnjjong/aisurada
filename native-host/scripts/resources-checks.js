/* 실제 WKWebView와 앱 안의 Rust 엔진, 격리된 vault로 스킬·에이전트, 승인 대기함, 프로젝트 참조와 저장소 링크, 레일 탭을 점검한다. 이슈 #73. */
const cfg = window.__AIS_FIXTURE__;
const R = cfg.resources;
const invoke = (...args) => window.__TAURI_INTERNALS__.invoke(...args);
const assert = (condition, message) => { if (!condition) throw Error(message || 'assertion failed'); };
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(predicate, what = '화면 상태', timeout = 25000) {
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
const resource = name => all('.res-item:not(.res-trashed)').find(item => item.querySelector('.res-name')?.textContent === name);
const trashed = name => all('.res-trashed').find(item => item.querySelector('.res-name')?.textContent === name);
const closeDialog = async () => { const close = document.querySelector('.ui-modal button[aria-label$="닫기"]'); assert(close, '모달 닫기 단추'); close.click(); await wait(100); };

const results = [];
async function test(id, work) {
  try { await work(); results.push({ id, ok: true }); }
  catch (error) { results.push({ id, ok: false, error: String(error) }); }
  await post('/progress', results.at(-1));
  if (!results.at(-1).ok) throw Error(id);
}

try {
  await until(() => document.querySelector('[data-rail-section="nav"]'), '레일');

  await test('resources-page-lists-the-seeded-definitions-and-the-approval-banner', async () => {
    navigate('/resources');
    await until(() => resource('번역 검수') && resource('리뷰어') && resource('직접 고칠 스킬'), '글로벌 자원 목록');
    assert(resource('번역 검수').textContent.includes('번역을 검수한다'), '스킬 설명');
    assert(resource('리뷰어').textContent.includes('권한'), '에이전트 줄에 권한');
    assert(!resource('AI 제안 스킬'), '승인 전의 AI 제안은 목록에 없어야 한다');
    // 대기 둘과 승인 안 된 직접 수정 하나가 배너와 레일 배지로 알려진다
    await until(() => document.querySelector('.apv-banner')?.textContent.includes('3건'), '승인 배너');
    assert(document.querySelector('[aria-label^="승인이 필요한 변경"]'), '레일 메뉴의 승인 배지');
    assert(byText('.res-chip, .apv-chip', '직접 고쳐짐', resource('직접 고칠 스킬')), '직접 고쳐짐 꼬리표');
    // 승인 전에는 이전 판이 쓰인다
    assert((await engine(['get-resource', R.skillId])).body.includes('원래 지침'), '승인 전에는 이전 판');
  });

  await test('skill-create-form-writes-through-the-engine', async () => {
    byText('.res-section-head button', '새 스킬').click();
    await until(() => document.querySelector('.res-form'), '스킬 폼');
    fillField(document.querySelector('input[aria-label="스킬 이름"]'), '화면 검증 스킬');
    fillField(document.querySelector('textarea[aria-label="스킬 설명"]'), '실제 창에서 만든 스킬');
    fillField(document.querySelector('textarea[aria-label="스킬 지침"]'), '# 화면에서 쓴 지침');
    await wait(60);
    const submit = document.querySelector('.res-form button[type="submit"]');
    assert(submit && !submit.disabled && submit.textContent.trim() === '만들기', '만들기 단추');
    submit.click();
    await until(() => resource('화면 검증 스킬'), '새 스킬이 목록에 나옴');
    assert((await callsOf('save-resource')).some(c => c.args.argv.includes('name=화면 검증 스킬')), 'save-resource 호출');
    assert((await engine(['list-resources', 'scope=global'])).resources.some(r => r.name === '화면 검증 스킬'), '엔진에 저장됨');
  });

  await test('skill-edit-form-updates-the-description', async () => {
    byText('.res-actions button', '편집', resource('화면 검증 스킬')).click();
    await until(() => document.querySelector('.res-form textarea[aria-label="스킬 설명"]'), '편집 폼');
    assert(document.querySelector('input[aria-label="스킬 이름"]').value === '화면 검증 스킬', '이름이 채워져 있다');
    fillField(document.querySelector('textarea[aria-label="스킬 설명"]'), '고친 설명');
    await wait(60);
    const save = document.querySelector('.res-form button[type="submit"]');
    assert(save.textContent.trim() === '저장', '저장 단추');
    save.click();
    await until(() => resource('화면 검증 스킬')?.textContent.includes('고친 설명'), '고친 설명이 목록에 나옴');
  });

  await test('approval-inbox-compares-and-approves-the-ai-edit', async () => {
    byText('.res-toolbar button', '승인 대기함').click();
    await until(() => document.querySelector('.apv-modal .apv-card'), '승인 대기함');
    const cards = () => all('.apv-modal [data-change-id]:not(.apv-decided)');
    assert(cards().length === 2, `대기 변경 둘이어야 한다: ${cards().length}`);
    const card = () => document.querySelector(`.apv-modal [data-change-id="${R.editChangeId}"]`);
    assert(card().textContent.includes('번역 검수'), '요약에 자원 이름');
    byText('button', '이전 판과 비교', card()).click();
    await until(() => card().querySelector('.apv-compare'), '비교 칸');
    const text = card().querySelector('.apv-compare').textContent;
    assert(text.includes('AI가 고친 지침') && text.includes('원래 지침'), `비교에 이전 판과 새 판이 보인다: ${text.slice(0, 200)}`);
    byText('button', '승인', card()).click();
    await until(() => !card(), '승인한 카드가 대기에서 사라짐');
    assert((await engine(['get-resource', R.skillId])).body.includes('AI가 고친 지침'), '승인하면 적용된다');
    assert((await callsOf('approve-resource-change')).length === 1, 'approve-resource-change 호출 한 번');
  });

  await test('approval-inbox-rejects-with-a-reason', async () => {
    const card = () => document.querySelector(`.apv-modal [data-change-id="${R.proposalChangeId}"]`);
    byText('button', '거절', card()).click();
    await until(() => card().querySelector('.apv-reject textarea'), '거절 이유 칸');
    fillField(card().querySelector('.apv-reject textarea'), '이름이 너무 넓다');
    await wait(60);
    byText('button', '거절하기', card()).click();
    await until(() => document.querySelector('.apv-modal .apv-empty')?.textContent.includes('승인을 기다리는 변경이 없어요'), '대기가 비었다');
    const change = await engine(['get-resource-change', R.proposalChangeId]);
    assert(change.status === 'rejected' && change.reason === '이름이 너무 넓다', `거절 상태와 이유: ${JSON.stringify(change).slice(0, 200)}`);
    assert(!(await engine(['list-resources', 'scope=global'])).resources.some(r => r.name === 'AI 제안 스킬'), '거절한 제안은 안 생긴다');
    byText('.apv-tabs [role="tab"]', '지난 결정').click();
    await until(() => document.querySelector('.apv-decided[data-status="rejected"]'), '지난 결정에 거절이 보인다');
    assert(document.querySelector('.apv-decided[data-status="rejected"]').textContent.includes('이름이 너무 넓다'), '거절 이유가 보인다');
  });

  await test('approval-inbox-resolves-a-direct-file-edit', async () => {
    byText('.apv-tabs [role="tab"]', '직접 수정').click();
    const card = () => document.querySelector('.apv-modal [data-unapproved-key]');
    await until(card, '승인 안 된 직접 수정 카드');
    assert(card().textContent.includes('직접 고쳐짐'), '직접 고쳐짐 상태');
    // 승인 전에 화면과 엔진은 승인해 둔 판(원래 설명)을 쓴다
    assert((await engine(['get-resource', R.dirtyId])).description === '원래 설명', '승인해 둔 판이 쓰인다');
    byText('button', '이 내용을 승인', card()).click();
    await until(() => !card(), '직접 수정 카드가 사라짐');
    assert((await engine(['get-resource', R.dirtyId])).description === '직접 고친 설명', '받아들이면 그 내용이 쓰인다');
    assert((await callsOf('resolve-resource-edit')).length === 1, 'resolve-resource-edit 호출');
  });

  await test('approval-policy-tab-changes-the-global-policy', async () => {
    byText('.apv-tabs [role="tab"]', '정책').click();
    await until(() => document.querySelector('.apv-policy select'), '정책 칸');
    const select = all('.apv-policy select').find(s => s.getAttribute('aria-label') === '글로벌 자원의 변경');
    assert(select, '글로벌 자원 정책 칸');
    const before = (await engine(['get-resource-policy'])).policy.global;
    fillField(select, before === 'auto' ? 'approve' : 'auto');
    await until(async () => (await engine(['get-resource-policy'])).policy.global !== before, '정책이 바뀜');
    fillField(select, before);
    await until(async () => (await engine(['get-resource-policy'])).policy.global === before, '정책을 되돌림');
    await closeDialog();
    await until(() => !document.querySelector('.apv-modal'), '대기함이 닫힘');
  });

  await test('delete-confirm-trash-and-restore', async () => {
    byText('.res-actions button', '삭제', resource('화면 검증 스킬')).click();
    await until(() => byText('.ui-modal button', '휴지통으로'), '삭제 확인 창');
    byText('.ui-modal button', '휴지통으로').click();
    await until(() => !resource('화면 검증 스킬') && trashed('화면 검증 스킬') && byText('.res-section-head h2', '지운 것'), '휴지통으로 감');
    byText('.res-trashed button', '되살리기').click();
    await until(() => resource('화면 검증 스킬'), '되살아남');
    assert((await callsOf('delete-resource')).length === 1 && (await callsOf('restore-resource')).length === 1, '삭제와 되살리기 호출');
  });

  await test('project-page-references-and-unreferences-a-global-skill', async () => {
    navigate(`/project/${encodeURIComponent(R.projectSlug)}`);
    await until(() => byText('.res-section h2', '글로벌 자원 참조하기'), '프로젝트 페이지');
    assert(!all('.res-item').some(i => i.dataset.resourceId === R.skillId && i.textContent.includes('글로벌에서 참조')), '처음에는 참조하지 않는다');
    const row = () => all('.res-item').find(i => i.dataset.resourceId === R.skillId);
    byText('button', '참조하기', row()).click();
    await until(() => row()?.textContent.includes('글로벌에서 참조'), '참조한 스킬이 쓸 수 있는 목록에 나옴');
    assert((await engine(['usable-resources', R.projectSlug])).skills.some(s => s.id === R.skillId), '엔진의 쓸 수 있는 자원');
    byText('button', '참조 끄기', row()).click();
    await until(() => !row() || !row().textContent.includes('글로벌에서 참조'), '참조를 끔');
    assert(!(await engine(['usable-resources', R.projectSlug])).skills.some(s => s.id === R.skillId), '참조를 끄면 쓸 수 없다');
  });

  await test('project-repo-links-edit-through-the-form', async () => {
    assert(document.querySelector('section[aria-label="저장소"]')?.textContent.includes('web'), '프로젝트 페이지에 저장소 web');
    byText('.res-toolbar button', '프로젝트 편집').click();
    await until(() => document.querySelector('.repo-editor'), '저장소 편집칸');
    assert(document.querySelector('input[aria-label="1번째 저장소 이름"]').value === 'web', '첫 저장소 이름');
    assert(document.querySelector('input[aria-label="1번째 저장소 GitHub"]').value === 'me/web', '첫 저장소 GitHub');
    document.querySelector('.repo-add').click();
    await until(() => document.querySelector('input[aria-label="2번째 저장소 이름"]'), '둘째 저장소 줄');
    fillField(document.querySelector('input[aria-label="2번째 저장소 이름"]'), 'docs');
    fillField(document.querySelector('input[aria-label="2번째 저장소 GitHub"]'), 'me/docs');
    await wait(80);
    const save = byText('.ui-modal footer button[type="submit"]', '저장');
    assert(save && !save.disabled, '프로젝트 저장 단추');
    save.click();
    await until(() => document.querySelector('section[aria-label="저장소"]')?.textContent.includes('docs'), '저장소 목록에 docs');
    const project = (await engine(['list-projects'])).find(p => p.slug === R.projectSlug);
    assert(project.repos.length === 2 && project.repos.some(r => r.github === 'me/docs'), `엔진에 저장소 둘: ${JSON.stringify(project.repos)}`);
  });

  await test('rail-tabs-switch-and-remember-the-last-screen', async () => {
    navigate('/notes');
    await until(() => document.querySelector('#rail-tab-notes[aria-selected="true"]'), '노트 탭');
    assert(document.querySelector('#rail-tab-agents').getAttribute('aria-selected') === 'false', '에이전트 탭은 꺼져 있다');
    document.querySelector('#rail-tab-agents').click();
    // 에이전트 탭을 마지막으로 본 화면은 프로젝트 페이지였다
    await until(() => location.pathname.startsWith('/project/'), '에이전트 탭이 마지막 화면으로 돌아감');
    assert(document.querySelector('#rail-tab-agents').getAttribute('aria-selected') === 'true', '에이전트 탭이 켜짐');
    assert(byText('[data-rail-section="nav"] a, [data-rail-section="nav"] button', '태스크') || document.querySelector('[data-rail-section="nav"]').textContent.includes('태스크'), '에이전트 메뉴에 태스크');
    document.querySelector('#rail-tab-notes').click();
    await until(() => location.pathname === '/notes', '노트 탭이 마지막 노트 화면으로 돌아감');
    assert(document.querySelector('#rail-tab-notes').getAttribute('aria-selected') === 'true', '노트 탭이 켜짐');
  });
} catch { /* 첫 실패 뒤의 점검은 앞 상태에 기대므로 멈춘다 */ }
await post('/result', results);
