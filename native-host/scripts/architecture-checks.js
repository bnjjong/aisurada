/* 실제 WKWebView와 앱 안의 Rust 엔진, 격리된 vault로 C4 아키텍처 화면을 점검한다(TSK-24).
   요소와 관계를 화면에서 만들고, 끌어서 옮기고, 고치고, 지우고, 다른 곳이 먼저 고쳤을 때 거절되는 것까지 본다.
   엔진이 쓴 값은 화면이 아니라 엔진 호출로 다시 읽어 확인한다. */
const cfg = window.__AIS_FIXTURE__;
const A = cfg.architecture;
const invoke = (...args) => window.__TAURI_INTERNALS__.invoke(...args);
const assert = (condition, message) => { if (!condition) throw Error(message || 'assertion failed'); };
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(predicate, what = '화면 상태', timeout = 30000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) { if (await predicate()) return; await wait(40); }
  throw Error(`timed out: ${what} (${location.pathname}) ${document.body.textContent.slice(-300)}`);
}
const post = (path, value) => fetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(value) });
const navigate = path => { history.pushState({}, '', path); dispatchEvent(new PopStateEvent('popstate')); };
const all = (selector, root = document) => [...root.querySelectorAll(selector)];
const byText = (selector, text, root = document) => all(selector, root).find(e => e.textContent.trim().includes(text));
const fillField = (element, value) => {
  assert(element, '채울 칸이 없다');
  const proto = element instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : element instanceof HTMLSelectElement ? HTMLSelectElement.prototype : HTMLInputElement.prototype;
  Object.getOwnPropertyDescriptor(proto, 'value').set.call(element, value);
  element.dispatchEvent(new Event(element instanceof HTMLSelectElement ? 'change' : 'input', { bubbles: true }));
};
const model = () => invoke('engine_call', { command: 'get_c4_model', args: { project: A.projectSlug } });
const cards = () => all('.c4-card');
const cardOf = name => cards().find(c => c.querySelector('strong')?.textContent === name);
const toolbar = text => byText('.c4-toolbar button', text);
const form = () => document.querySelector('.c4-form');
const submit = () => form().querySelector('button[type="submit"]');
const pointer = (target, type, x, y) => target.dispatchEvent(new PointerEvent(type, { bubbles: true, pointerId: 7, button: 0, clientX: x, clientY: y }));

async function addElement(name, kind) {
  toolbar('요소 추가').click();
  await until(form, '요소 추가 창');
  if (kind) fillField(form().querySelector('select'), kind);
  fillField(form().querySelector('input[required]'), name);
  submit().click();
  await until(() => !form(), '창이 닫힘');
  await until(() => cardOf(name), `${name} 카드`);
}

const results = [];
async function test(id, work) {
  try { await work(); results.push({ id, ok: true }); }
  catch (error) { results.push({ id, ok: false, error: String(error) }); }
  await post('/progress', results.at(-1));
  if (!results.at(-1).ok) throw Error(id);
}

try {
  await until(() => document.querySelector('[data-rail-section="nav"]'), '레일');

  await test('a-new-project-starts-empty-and-reading-writes-no-file', async () => {
    navigate('/architecture/' + encodeURIComponent(A.projectSlug));
    await until(() => document.querySelector('.c4-page'), 'C4 화면');
    await until(() => document.querySelector('.c4-diagram-empty'), '빈 안내');
    assert(document.querySelector('.c4-diagram-empty').textContent.includes('요소를 추가') || document.querySelector('.c4-diagram-empty').textContent.includes('요소 추가'), '빈 안내가 요소 추가를 알린다');
    const read = await model();
    assert(read.elements.length === 0 && read.precondition.startsWith('f1:'), '엔진도 빈 모델이다');
    assert(toolbar('관계 추가').disabled, '요소가 둘 미만이면 관계 추가는 닫혀 있다');
  });

  await test('elements-created-on-screen-are-written-through-the-engine', async () => {
    await addElement('aisurada', 'system');
    await addElement('개발자', 'person');
    await addElement('GitHub', 'external');
    const read = await model();
    assert(read.elements.map(e => e.name).sort().join() === 'GitHub,aisurada,개발자', `엔진의 요소: ${read.elements.map(e => e.name)}`);
    const system = read.elements.find(e => e.name === 'aisurada');
    assert(system.id === 'aisurada' && system.kind === 'system' && system.level === 1, '영문 이름은 slug id가 된다');
    assert(read.elements.find(e => e.name === '개발자').id === 'person-1', '한글 이름은 종류 이름 id가 된다');
  });

  await test('a-relationship-is-made-by-clicking-two-cards', async () => {
    toolbar('관계 추가').click();
    await until(() => document.querySelector('.c4-connect-hint'), '연결 안내');
    cardOf('개발자').click();
    await until(() => document.querySelector('.c4-connect-hint')?.textContent.includes('이을 요소'), '대상 안내');
    cardOf('aisurada').click();
    await until(form, '관계 추가 창');
    assert(all('.c4-form select').map(s => s.selectedOptions[0].textContent).join('|').includes('개발자') , '출발이 채워져 있다');
    fillField(form().querySelector('input[maxlength="200"]'), '작업 지시');
    submit().click();
    await until(() => !form(), '창이 닫힘');
    await until(() => all('.c4-edge').some(e => e.textContent.includes('작업 지시')), '관계 라벨');
    const read = await model();
    assert(read.relationships.length === 1 && read.relationships[0].source === 'person-1' && read.relationships[0].target === 'aisurada', '엔진의 관계');
    assert(!document.querySelector('.c4-connect-hint'), '연결 모드가 끝났다');
  });

  await test('dragging-a-card-saves-every-visible-position-for-the-view', async () => {
    const target = cardOf('GitHub');
    const before = target.getBoundingClientRect();
    pointer(target, 'pointerdown', before.x + 20, before.y + 20);
    pointer(target, 'pointermove', before.x + 60, before.y + 50);
    pointer(target, 'pointermove', before.x + 120, before.y + 90);
    pointer(target, 'pointerup', before.x + 120, before.y + 90);
    await until(async () => Object.keys((await model()).views.context || {}).length === 3, '세 요소의 위치가 저장됨');
    const read = await model();
    const moved = read.views.context[read.elements.find(e => e.name === 'GitHub').id];
    assert(moved && Number.isInteger(moved.x) && Number.isInteger(moved.y), '정수 좌표');
    await until(() => Math.abs(cardOf('GitHub').getBoundingClientRect().x - before.x - 100) < 6 && Math.abs(cardOf('GitHub').getBoundingClientRect().y - before.y - 70) < 6, `카드가 끈 만큼(100,70) 움직임: ${JSON.stringify(cardOf('GitHub').getBoundingClientRect())} / ${JSON.stringify(before)}`);
    assert(!all('.c4-error').length, '끌어도 오류가 없다');
    // 끌고 난 뒤의 클릭은 선택으로 새지 않는다
    assert(cardOf('GitHub').getAttribute('aria-pressed') === 'false', '끌기는 선택하지 않는다');
  });

  await test('positions-survive-leaving-and-coming-back', async () => {
    const saved = (await model()).views.context;
    const id = (await model()).elements.find(e => e.name === 'GitHub').id;
    navigate('/tasks');
    await until(() => !document.querySelector('.c4-page'), '화면을 떠남');
    navigate('/architecture/' + encodeURIComponent(A.projectSlug));
    await until(() => cardOf('GitHub'), '카드가 다시 보임');
    const left = parseFloat(cardOf('GitHub').style.left), top = parseFloat(cardOf('GitHub').style.top);
    assert(left === saved[id].x && top === saved[id].y, `저장한 자리(${saved[id].x},${saved[id].y})에 놓인다: ${left},${top}`);
  });

  await test('mouse-wheel-zooms-around-the-pointer', async () => {
    const diagram = document.querySelector('.c4-diagram');
    const percent = () => parseInt(document.querySelector('.c4-zoom output').textContent, 10);
    const centre = () => { const r = cardOf('aisurada').getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; };
    const wheel = (deltaY, at) => {
      const event = new WheelEvent('wheel', { bubbles: true, cancelable: true, deltaY, clientX: at.x, clientY: at.y });
      cardOf('aisurada').dispatchEvent(event);
      return event;
    };
    const start = percent();
    const anchor = centre();
    const zoomIn = wheel(-240, anchor);
    assert(zoomIn.defaultPrevented, '페이지가 같이 스크롤되면 안 된다');
    await until(() => percent() > start, '휠로 확대됨');
    const moved = centre();
    assert(Math.abs(moved.x - anchor.x) <= 2 && Math.abs(moved.y - anchor.y) <= 2, `포인터 아래 카드가 제자리에 있다: ${anchor.x},${anchor.y} -> ${moved.x},${moved.y}`);
    const zoomed = percent();
    wheel(240, centre());
    await until(() => percent() < zoomed, '휠로 축소됨');
    document.querySelector('.c4-zoom button[aria-label="화면에 맞추기"]').click();
    await until(() => percent() === start, '화면에 맞추면 처음 배율로 돌아옴');
    assert(diagram, 'diagram');
  });

  await test('editing-a-selected-element-changes-only-what-was-edited', async () => {
    cardOf('aisurada').click();
    await until(() => document.querySelector('.c4-details-head strong')?.textContent === 'aisurada', '상세');
    byText('.c4-details-actions button', '편집').click();
    await until(form, '편집 창');
    assert(form().querySelector('input[required]').value === 'aisurada', '현재 값이 채워진다');
    fillField(form().querySelector('textarea'), 'vault 기반 지식 관리');
    submit().click();
    await until(() => !form(), '창이 닫힘');
    await until(async () => (await model()).elements.find(e => e.id === 'aisurada').description === 'vault 기반 지식 관리', '설명이 엔진에 저장됨');
    const read = await model();
    assert(read.elements.find(e => e.id === 'aisurada').name === 'aisurada', '이름은 그대로');
    await until(() => document.querySelector('.c4-details-body')?.textContent.includes('vault 기반 지식 관리'), '상세에 설명이 보임');
  });

  await test('a-container-is-made-inside-a-system-and-deleting-the-system-takes-it-along', async () => {
    cardOf('aisurada').dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
    await until(() => toolbar('컨테이너').getAttribute('aria-pressed') === 'true', '컨테이너 단계로 들어감');
    await addElement('엔진');
    const inside = await model();
    const engine = inside.elements.find(e => e.name === '엔진');
    assert(engine.kind === 'container' && engine.parentId === 'aisurada', '컨테이너의 부모는 시스템');
    toolbar('시스템 컨텍스트').click();
    await until(() => cardOf('aisurada'), '컨텍스트 단계로 돌아옴');
    cardOf('aisurada').click();
    await until(() => document.querySelector('.c4-details-head strong')?.textContent === 'aisurada', '상세');
    byText('.c4-details-actions button', '삭제').click();
    await until(() => document.querySelector('.ui-modal'), '확인 창');
    const text = document.querySelector('.ui-modal').textContent;
    assert(text.includes('안쪽 요소 1개') && text.includes('관계 1개'), `함께 사라질 것을 알린다: ${text}`);
    byText('.ui-modal button', '삭제').click();
    await until(() => !cardOf('aisurada'), '카드가 사라짐');
    const read = await model();
    assert(read.elements.map(e => e.id).sort().join() === 'github,person-1', `남은 요소: ${read.elements.map(e => e.id)}`);
    assert(read.relationships.length === 0, '닿은 관계도 사라졌다');
    assert(!Object.keys(read.views).some(v => v.startsWith('container:')), '없어진 뷰의 좌표도 지워졌다');
  });

  await test('a-change-made-elsewhere-is-refused-and-the-screen-catches-up', async () => {
    const fresh = await model();
    await invoke('engine_call', { command: 'apply_c4_changes', args: { project: A.projectSlug, f1: fresh.precondition, operations: [{ op: 'put_element', id: 'elsewhere', kind: 'system', name: '다른 곳에서 만든 것' }] } });
    toolbar('요소 추가').click();
    await until(form, '요소 추가 창');
    fillField(form().querySelector('input[required]'), '늦은 요소');
    submit().click();
    await until(() => form()?.querySelector('[role="alert"]')?.textContent.includes('먼저 고쳤어요'), '충돌 안내');
    assert(!(await model()).elements.some(e => e.name === '늦은 요소'), '거절된 변경은 쓰이지 않았다');
    await until(() => cardOf('다른 곳에서 만든 것'), '화면이 최신 모델로 따라감');
    submit().click();
    await until(() => !form(), '다시 보내면 들어간다');
    await until(async () => (await model()).elements.some(e => e.name === '늦은 요소'), '두 번째 시도는 쓰임');
  });

  await test('an-invalid-edit-shows-the-engine-reason-and-writes-nothing', async () => {
    const before = (await model()).precondition;
    cardOf('GitHub').click();
    await until(() => document.querySelector('.c4-details-head strong')?.textContent === 'GitHub', '상세');
    byText('.c4-details-actions button', '편집').click();
    await until(form, '편집 창');
    fillField(form().querySelector('input[required]'), 'x'.repeat(130));
    // maxLength가 입력을 막지 않는 값 설정이라 엔진이 길이를 거절한다
    submit().click();
    await until(() => form()?.querySelector('[role="alert"]'), '엔진의 거절 이유');
    assert(form().querySelector('[role="alert"]').textContent.includes('120'), `길이 이유: ${form().querySelector('[role="alert"]').textContent}`);
    assert((await model()).precondition === before, '아무것도 쓰이지 않았다');
    byText('.c4-form button', '취소').click();
    await until(() => !form(), '창이 닫힘');
  });
  // 선 끌기: 카드 가장자리의 연결 지점에서 다른 카드로 끌어 관계를 만들고, 고른 관계의 끝을 다른 카드로 옮긴다
  const middle = element => { const r = element.getBoundingClientRect(); return { x: r.left + r.width / 2, y: r.top + r.height / 2 }; };
  const idOf = async name => (await model()).elements.find(e => e.name === name).id;
  const portOf = (id, side) => document.querySelector(`.c4-ports[data-for="${id}"] .c4-port[data-port="${side}"]`);
  const down = (target, at) => target.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, cancelable: true, pointerId: 11, button: 0, clientX: at.x, clientY: at.y }));
  const moveTo = at => window.dispatchEvent(new PointerEvent('pointermove', { bubbles: true, pointerId: 11, clientX: at.x, clientY: at.y }));
  const upAt = at => window.dispatchEvent(new PointerEvent('pointerup', { bubbles: true, pointerId: 11, button: 0, clientX: at.x, clientY: at.y }));
  const preview = () => document.querySelector('.c4-link path');
  // 누르고 나서 화면이 끌기 상태로 바뀌고 리스너가 붙을 때까지 기다린다(사람은 누른 뒤에야 움직인다)
  const begin = async (target, at) => { down(target, at); await until(preview, '끌기가 시작됨'); await wait(60); };

  await test('dragging-from-a-card-port-makes-a-relationship-through-the-form', async () => {
    const source = await idOf('GitHub');
    const port = portOf(source, 'right');
    assert(port, '카드에 연결 지점이 있다');
    const target = cardOf('다른 곳에서 만든 것');
    // 빈 곳에 놓으면 아무 일도 없다
    const empty = { x: middle(port).x + 40, y: middle(port).y + 150 };
    await begin(port, middle(port));
    moveTo(empty);
    assert(!document.querySelector('.c4-card[data-link-target="true"]'), '빈 곳 위에서는 대상 카드가 없다');
    upAt(empty);
    await until(() => !preview(), '놓으면 점선이 사라짐');
    assert(!form(), '빈 곳에 놓으면 창이 열리지 않는다');
    // Esc로 취소
    await begin(port, middle(port));
    moveTo(middle(target));
    await until(() => document.querySelector('.c4-card[data-link-target="true"]'), '대상 카드가 강조됨');
    assert(document.querySelector('.c4-port[data-near="true"]'), '가장 가까운 연결 지점이 강조됨');
    window.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    await until(() => !preview(), 'Esc로 끌기가 취소됨');
    upAt(middle(target));
    assert(!form(), 'Esc 뒤에는 놓아도 창이 열리지 않는다');
    // 카드 위에 놓으면 출발과 도착이 채워진 관계 창이 열리고, 취소하면 관계가 늘지 않는다
    const before = (await model()).relationships.length;
    await begin(port, middle(port));
    moveTo(middle(target));
    upAt(middle(target));
    await until(form, '관계 추가 창');
    const picked = all('.c4-form select').map(select => select.selectedOptions[0].textContent);
    assert(picked[0].startsWith('GitHub') && picked[1].startsWith('다른 곳에서 만든 것'), `출발과 도착이 채워져 있다: ${picked}`);
    byText('.c4-form button', '취소').click();
    await until(() => !form(), '창이 닫힘');
    assert((await model()).relationships.length === before, '취소하면 관계가 늘지 않는다');
  });

  await test('dragging-a-line-end-moves-the-relationship-to-another-card', async () => {
    // 끌어서 관계를 하나 만든다: GitHub -> 다른 곳에서 만든 것
    const github = await idOf('GitHub');
    const elsewhere = await idOf('다른 곳에서 만든 것');
    const late = await idOf('늦은 요소');
    const port = portOf(github, 'right');
    await begin(port, middle(port));
    moveTo(middle(cardOf('다른 곳에서 만든 것')));
    upAt(middle(cardOf('다른 곳에서 만든 것')));
    await until(form, '관계 추가 창');
    fillField(form().querySelector('input[maxlength="200"]'), '끌어서 만든 관계');
    submit().click();
    await until(() => !form(), '창이 닫힘');
    await until(async () => (await model()).relationships.some(r => r.label === '끌어서 만든 관계'), '엔진에 관계가 생김');
    const line = (await model()).relationships.find(r => r.label === '끌어서 만든 관계');
    assert(line.source === github && line.target === elsewhere, `끌어서 만든 방향: ${JSON.stringify(line)}`);
    const count = (await model()).relationships.length;
    // 관계를 고르면 양 끝에 손잡이가 보이고, 도착 끝을 다른 카드로 옮긴다
    // 라벨이 아이콘만 보일 수 있으니 접근성 이름으로 찾고, 라벨이 없으면 카드 상세의 관계 목록으로 고른다
    const labelButton = () => all('.c4-edge').find(e => (e.getAttribute('aria-label') || '').includes('끌어서 만든 관계'));
    if (labelButton()) labelButton().click();
    else {
      cardOf('GitHub').click();
      await until(() => byText('.c4-relations button', '끌어서 만든 관계'), '상세의 관계 목록');
      byText('.c4-relations button', '끌어서 만든 관계').click();
    }
    await until(() => document.querySelector('.c4-edge-end[data-end="target"]'), '끝 손잡이가 보임');
    const handle = document.querySelector('.c4-edge-end[data-end="target"]');
    await begin(handle, middle(handle));
    moveTo(middle(cardOf('늦은 요소')));
    await until(() => document.querySelector('.c4-card[data-link-target="true"]'), '옮길 카드가 강조됨');
    upAt(middle(cardOf('늦은 요소')));
    await until(async () => (await model()).relationships.find(r => r.id === line.id)?.target === late, '엔진에 도착 끝이 바뀜');
    const moved = (await model()).relationships.find(r => r.id === line.id);
    assert(moved.source === github && moved.label === '끌어서 만든 관계', `출발과 이름은 그대로다: ${JSON.stringify(moved)}`);
    assert((await model()).relationships.length === count, '관계 수는 그대로다');
    // 시작 끝을 다른 카드로 옮긴다
    await until(() => document.querySelector('.c4-edge-end[data-end="source"]'), '관계가 선택된 채로 남음');
    const start = document.querySelector('.c4-edge-end[data-end="source"]');
    await begin(start, middle(start));
    moveTo(middle(cardOf('개발자')));
    upAt(middle(cardOf('개발자')));
    await until(async () => (await model()).relationships.find(r => r.id === line.id)?.source === 'person-1', '엔진에 시작 끝이 바뀜');
    const again = (await model()).relationships.find(r => r.id === line.id);
    assert(again.target === late && again.label === '끌어서 만든 관계', `도착과 이름은 그대로다: ${JSON.stringify(again)}`);
  });
} catch { /* 첫 실패 뒤의 점검은 앞 상태에 기대므로 멈춘다 */ }
await post('/result', results);
