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

  await test('dragging-an-edge-label-saves-its-position-in-the-view', async () => {
    const label = document.querySelector('.c4-edge');
    assert(label, '관계 라벨이 화면에 있다');
    const before = label.getBoundingClientRect();
    pointer(label, 'pointerdown', before.x + 10, before.y + 10);
    pointer(label, 'pointermove', before.x + 60, before.y + 50);
    pointer(label, 'pointermove', before.x + 100, before.y + 70);
    pointer(label, 'pointerup', before.x + 100, before.y + 70);
    const m = await model();
    const relId = m.relationships[0]?.id;
    assert(relId, '관계가 있다');
    await until(async () => {
      const current = await model();
      return Boolean(current.views.context && current.views.context[relId]);
    }, '관계 라벨의 위치가 뷰에 저장됨');
    const read = await model();
    const pos = read.views.context[relId];
    assert(pos && Number.isInteger(pos.x) && Number.isInteger(pos.y), '라벨 정수 좌표');
    assert(!all('.c4-error').length, '라벨을 끌어도 오류가 없다');
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
  const PORT_IDS = ['top-1', 'top-2', 'top-3', 'bottom-1', 'bottom-2', 'bottom-3', 'left-1', 'left-2', 'left-3', 'right-1', 'right-2', 'right-3', 'left', 'right'];
  // 누르고 나서 화면이 끌기 상태로 바뀌고 리스너가 붙을 때까지 기다린다(사람은 누른 뒤에야 움직인다)
  const begin = async (target, at) => { down(target, at); await until(preview, '끌기가 시작됨'); await wait(60); };

  await test('dragging-from-a-card-port-makes-a-relationship-through-the-form', async () => {
    const source = await idOf('GitHub');
    const port = portOf(source, 'right-2');
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
    const port = portOf(github, 'right-2');
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
    assert(line.sourcePort === 'right-2' || line.sourcePort === 'right', `출발 연결 지점이 저장된다: ${line.sourcePort}`);
    assert(PORT_IDS.includes(line.targetPort), `도착 연결 지점은 놓은 자리에서 가장 가까운 지점이다: ${line.targetPort}`);
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
    assert(PORT_IDS.includes(again.sourcePort), `옮긴 끝의 연결 지점이 저장된다: ${again.sourcePort}`);
  });

  await test('dragging-a-line-end-onto-another-port-of-the-same-card-changes-where-it-attaches', async () => {
    const line = (await model()).relationships.find(r => r.label === '끌어서 만든 관계');
    const late = await idOf('늦은 요소');
    assert(line.target === late, '앞 점검에서 도착이 늦은 요소다');
    await until(() => document.querySelector('.c4-edge-end[data-end="target"]'), '관계가 선택된 채로 남음');
    // 누르기만 하고 놓으면 아무것도 바뀌지 않는다
    const stamp = (await model()).precondition;
    const handle = () => document.querySelector('.c4-edge-end[data-end="target"]');
    await begin(handle(), middle(handle()));
    upAt(middle(handle()));
    await until(() => !preview(), '눌렀다 놓으면 끌기가 끝남');
    await wait(150);
    assert((await model()).precondition === stamp, '움직이지 않은 눌림은 모델을 바꾸지 않는다');
    // 같은 카드의 다른 연결 지점으로 옮기면 지점만 바뀐다
    const wanted = line.targetPort === 'bottom-3' ? 'top-1' : 'bottom-3';
    const port = portOf(late, wanted);
    await begin(handle(), middle(handle()));
    moveTo(middle(port));
    await until(() => document.querySelector('.c4-port[data-near="true"]')?.dataset.port === wanted, '가장 가까운 연결 지점이 강조됨');
    upAt(middle(port));
    await until(async () => (await model()).relationships.find(r => r.id === line.id)?.targetPort === wanted, '엔진에 도착 연결 지점이 바뀜');
    const moved = (await model()).relationships.find(r => r.id === line.id);
    assert(moved.target === late && moved.source === line.source && moved.label === line.label, `카드와 이름은 그대로다: ${JSON.stringify(moved)}`);
    assert(moved.sourcePort === line.sourcePort, '반대쪽 지점도 그대로다');
    // 선의 끝이 그 지점으로 옮겨 그려진다
    await until(() => { const a = middle(handle()), b = middle(portOf(late, wanted)); return Math.abs(a.x - b.x) <= 4 && Math.abs(a.y - b.y) <= 4; }, '선의 끝이 새 연결 지점에 붙음');
  });

  const modelOf = diagram => invoke('engine_call', { command: 'get_c4_model', args: { project: A.projectSlug, diagram } });
  const chips = () => all('.c4-diagram-tabs button');
  const chip = name => chips().find(e => e.textContent.trim() === name);
  const bar = label => document.querySelector(`.c4-diagram-actions button[aria-label="${label}"]`);
  const barButton = text => byText('.c4-diagram-actions button', text);

  await test('a-second-architecture-is-made-switched-and-kept-apart', async () => {
    await until(() => chip('기본')?.getAttribute('aria-pressed') === 'true', '처음에는 기본 아키텍처가 골라져 있다');
    const mainBefore = await model();
    barButton('새 아키텍처').click();
    await until(form, '이름 창');
    fillField(form().querySelector('input[required]'), '배포 구조');
    submit().click();
    await until(() => !form(), '창이 닫힘');
    await until(() => chip('배포 구조')?.getAttribute('aria-pressed') === 'true', '새 아키텍처로 바뀜');
    await until(() => document.querySelector('.c4-diagram-empty') && cards().length === 0, '새 아키텍처는 비어 있다');
    const list = await invoke('engine_call', { command: 'list_c4_diagrams', args: { project: A.projectSlug } });
    assert(list.diagrams.map(d => d.id).join() === 'main,arch-2', `엔진의 목록: ${JSON.stringify(list)}`);
    await addElement('배포 서버', 'system');
    const second = await modelOf('arch-2');
    assert(second.elements.map(e => e.name).join() === '배포 서버', `둘째에 들어갔다: ${JSON.stringify(second.elements)}`);
    const mainNow = await model();
    assert(mainNow.precondition === mainBefore.precondition && !mainNow.elements.some(e => e.name === '배포 서버'), '기본 아키텍처는 그대로다');
    chip('기본').click();
    await until(() => chip('기본').getAttribute('aria-pressed') === 'true' && cardOf('GitHub') && !cardOf('배포 서버'), '기본으로 돌아오면 원래 카드만 보인다');
    chip('배포 구조').click();
    await until(() => cardOf('배포 서버') && !cardOf('GitHub'), '둘째로 가면 둘째 카드만 보인다');
  });

  await test('a-diagram-can-be-renamed-and-the-name-must-be-free', async () => {
    bar('아키텍처 이름 바꾸기').click();
    await until(form, '이름 창');
    assert(form().querySelector('input[required]').value === '배포 구조', '지금 이름이 채워져 있다');
    fillField(form().querySelector('input[required]'), '기본');
    submit().click();
    await until(() => form()?.querySelector('.c4-error'), '겹치는 이름은 거절된다');
    assert(chip('배포 구조'), '거절되면 이름이 그대로다');
    fillField(form().querySelector('input[required]'), '운영 구조');
    submit().click();
    await until(() => !form(), '창이 닫힘');
    await until(() => chip('운영 구조')?.getAttribute('aria-pressed') === 'true' && !chip('배포 구조'), '칩 이름이 바뀜');
    const read = await modelOf('arch-2');
    assert(read.diagram.name === '운영 구조' && read.elements.length === 1, '엔진에도 이름이 바뀌고 요소는 그대로다');
  });

  await test('deleting-a-diagram-asks-first-and-returns-to-main', async () => {
    bar('아키텍처 지우기').click();
    await until(() => document.querySelector('.ui-modal'), '확인 창');
    const text = document.querySelector('.ui-modal').textContent;
    assert(text.includes('운영 구조') && text.includes('요소 1개'), `무엇이 사라지는지 알린다: ${text}`);
    byText('.ui-modal button', '취소').click();
    await until(() => !document.querySelector('.ui-modal'), '취소하면 닫힘');
    assert(chip('운영 구조'), '취소하면 그대로 남는다');
    bar('아키텍처 지우기').click();
    await until(() => document.querySelector('.ui-modal'), '확인 창');
    byText('.ui-modal button', '삭제').click();
    await until(() => !chip('운영 구조') && chip('기본')?.getAttribute('aria-pressed') === 'true', '지우면 기본으로 돌아온다');
    const list = await invoke('engine_call', { command: 'list_c4_diagrams', args: { project: A.projectSlug } });
    assert(list.diagrams.map(d => d.id).join() === 'main', `엔진에서도 사라졌다: ${JSON.stringify(list)}`);
    await until(() => cardOf('GitHub'), '기본 아키텍처의 카드가 보인다');
  });

  const historyOf = () => invoke('engine_call', { command: 'list_c4_history', args: { project: A.projectSlug } });
  const travelBar = () => document.querySelector('.c4-travel');
  const dots = () => all('.c4-travel-dot');
  const travelState = () => document.querySelector('.c4-travel-state')?.textContent ?? '';
  const settled = () => !travelState().includes('불러오는 중');
  const levelOneNames = async seq => (await invoke('engine_call', { command: 'get_c4_model', args: { project: A.projectSlug, at: seq } })).elements.filter(e => e.level === 1).map(e => e.name).sort().join('|');
  const shownNames = () => cards().map(c => c.querySelector('strong')?.textContent).sort().join('|');

  await test('time-travel-shows-past-states-read-only-and-comes-back-to-now', async () => {
    const live = await model();
    const before = await historyOf();
    assert(before.snapshots.length >= 4 && before.latestIsCurrent === true, `앞 점검들이 이력을 쌓았다: ${before.snapshots.length}점, 현재와 같음 ${before.latestIsCurrent}`);
    assert(before.snapshots.every((p, i) => i === 0 || p.at >= before.snapshots[i - 1].at), '점은 시간 순서다');

    byText('.c4-toolbar button', '타임 트래블').click();
    await until(travelBar, '타임 트래블 막대');
    assert(dots().length === before.snapshots.length, `점 ${dots().length}개, 이력 ${before.snapshots.length}개`);
    assert(dots().at(-1).getAttribute('aria-current') === 'step' && byText('.c4-travel-now', 'Now'), '처음에는 Now가 골라져 있다');
    assert(travelState().startsWith('현재 상태'), `상태 글: ${travelState()}`);
    assert(byText('.c4-toolbar button', '요소 추가').disabled && byText('.c4-toolbar button', '관계 추가').disabled, '과거를 보는 동안 편집 단추는 닫혀 있다');
    assert(!document.querySelector('.c4-card[data-movable="true"]'), '카드를 끌 수 없다');

    // 첫 점: 그때의 카드만 보인다
    dots()[0].click();
    await until(() => dots()[0].getAttribute('aria-current') === 'step' && settled(), '첫 점 선택');
    const firstSeq = before.snapshots[0].seq;
    const levelOneNamesCache = await levelOneNames(firstSeq);
    await until(() => shownNames() === levelOneNamesCache, `첫 점의 카드가 그때 모양이다: 화면 ${shownNames()} / 엔진 ${levelOneNamesCache}`);
    assert(!document.querySelector('.c4-card[data-change]'), '첫 점에는 견줄 앞 점이 없어 표시가 없다');
    assert(!travelState().startsWith('현재 상태'), `과거 점의 설명: ${travelState()}`);

    // 다음 점: 새로 생긴 요소가 강조된다
    document.querySelector('button[aria-label="다음 시점"]').click();
    await until(() => dots()[1].getAttribute('aria-current') === 'step' && settled(), '둘째 점');
    const second = await levelOneNames(before.snapshots[1].seq);
    await until(() => shownNames() === second, '둘째 점의 카드');
    const added = [...new Set(second.split('|'))].filter(n => !levelOneNamesCache.split('|').includes(n));
    if (added.length) await until(() => all('.c4-card[data-change="added"]').some(c => added.includes(c.querySelector('strong')?.textContent)), `새로 생긴 요소(${added})가 강조됨`);

    // 과거에서는 상세의 편집·삭제 단추가 숨는다
    cards()[0].click();
    await until(() => document.querySelector('.c4-details-actions'), '상세가 열림');
    assert(getComputedStyle(document.querySelector('.c4-details-actions')).display === 'none', '상세의 편집 단추가 숨어 있다');
    cards()[0].click();

    // 타임랩스: 끝에서 두 점 앞에서 시작하면 Now에 닿으면 멈춘다
    dots().at(-3).click();
    await until(() => dots().at(-3).getAttribute('aria-current') === 'step' && settled(), '끝에서 셋째 점');
    byText('.c4-travel-lapse', '타임랩스').click();
    await until(() => dots().at(-1).getAttribute('aria-current') === 'step', '타임랩스가 Now에 닿음', 60000);
    await until(() => byText('.c4-travel-lapse', '타임랩스').getAttribute('aria-pressed') === 'false', '닿으면 멈춘다');
    await until(() => shownNames() === live.elements.filter(e => e.level === 1).map(e => e.name).sort().join('|'), 'Now에서는 지금 카드가 보인다');

    // 지켜본 것만으로는 아무것도 바뀌지 않았다
    assert((await model()).precondition === live.precondition, '읽기만으로 모델이 바뀌지 않았다');
    assert((await historyOf()).snapshots.length === before.snapshots.length, '읽기만으로 점이 늘지 않았다');

    document.querySelector('button[aria-label="타임 트래블 닫기"]').click();
    await until(() => !travelBar(), '막대가 닫힘');
    assert(!byText('.c4-toolbar button', '요소 추가').disabled, '닫으면 편집 단추가 열린다');
  });

  await test('a-change-after-time-travel-adds-exactly-one-point', async () => {
    const before = (await historyOf()).snapshots.length;
    await addElement('타임 트래블 뒤', 'system');
    const after = await historyOf();
    assert(after.snapshots.length === before + 1 && after.latestIsCurrent === true, `점 ${before} → ${after.snapshots.length}`);
    assert(after.snapshots.at(-1).summary.includes('타임 트래블 뒤') && after.snapshots.at(-1).by === 'app', `마지막 점: ${JSON.stringify(after.snapshots.at(-1))}`);
  });
  // 노트 붙이기: 요소에 노트를 붙이면 배지가 생기고, 누르면 노트 팝업이 열리며, 떼면 사라진다. 없어진 노트는 따로 보이고 정리할 수 있다.
  const elementOf = async name => (await model()).elements.find(e => e.name === name);
  const badgeOf = name => all('.c4-note-badge').find(b => b.getAttribute('aria-label')?.startsWith(name));
  const detailsNotes = () => all('.c4-details .c4-note-row');
  const putNotes = async (id, notes) => {
    const read = await model();
    await invoke('engine_call', { command: 'apply_c4_changes', args: { project: A.projectSlug, f1: read.precondition, operations: [{ op: 'put_element', id, notes }] } });
  };

  await test('notes-are-attached-from-the-details-and-shown-as-a-badge', async () => {
    assert(!badgeOf('GitHub'), '처음에는 배지가 없다');
    cardOf('GitHub').click();
    await until(() => document.querySelector('.c4-details'), '상세가 열림');
    assert(document.querySelector('.c4-details .c4-note-empty'), '노트가 없으면 안내가 보인다');
    byText('.c4-details .c4-note-add', '노트 연결').click();
    await until(() => document.querySelector('.c4-pick'), '노트 고르기 창');
    const row = await (async () => { await until(() => byText('.c4-pick-row', '실제 엔진 기록'), '목록에 점검용 노트'); return byText('.c4-pick-row', '실제 엔진 기록'); })();
    assert(document.querySelector('.c4-pick footer .button-primary').disabled, '고르기 전에는 저장이 닫혀 있다');
    row.click();
    await until(() => !document.querySelector('.c4-pick footer .button-primary').disabled, '고르면 저장이 열린다');
    document.querySelector('.c4-pick footer .button-primary').click();
    await until(() => !document.querySelector('.c4-pick'), '창이 닫힘');
    const item = await elementOf('GitHub');
    assert(item.notes.join() === cfg.id, `엔진에 붙었다: ${item.notes}`);
    await until(() => badgeOf('GitHub'), '카드에 배지가 생김');
    assert(badgeOf('GitHub').textContent.trim() === '1', '배지는 개수를 보여 준다');
    await until(() => detailsNotes().length === 1 && detailsNotes()[0].textContent.includes('실제 엔진 기록'), '상세에 노트 줄이 보인다');
  });

  await test('a-single-note-badge-opens-the-note-popup', async () => {
    badgeOf('GitHub').click();
    await until(() => document.querySelector(".note-preview[role='dialog']"), '노트 팝업이 열림');
    assert(document.querySelector(".note-preview[role='dialog']").textContent.includes('실제 엔진 기록'), '붙인 노트가 열린다');
    document.querySelector(".note-preview[role='dialog'] .modal-ctrl-close").click();
    await until(() => !document.querySelector(".note-preview[role='dialog']"), '팝업이 닫힘');
  });

  await test('missing-notes-are-shown-apart-and-can-be-cleaned-up', async () => {
    await putNotes('github', [cfg.id, 'ndgone0000-0123abcd']);
    document.querySelector('.c4-toolbar button[aria-label="다시 불러오기"]').click();
    await until(() => detailsNotes().length === 2, '두 줄이 보인다');
    const gone = detailsNotes().find(r => r.hasAttribute('data-missing'));
    assert(gone && gone.textContent.includes('찾을 수 없는 노트'), '없는 노트는 따로 보인다');
    gone.querySelector('.c4-note-unlink').click();
    await until(() => detailsNotes().length === 1, '연결 정리로 한 줄이 남는다');
    assert((await elementOf('GitHub')).notes.join() === cfg.id, '엔진에는 있는 노트만 남았다');
  });

  await test('notes-can-be-detached-and-time-travel-keeps-them-read-only', async () => {
    byText('.c4-toolbar button', '타임 트래블').click();
    await until(() => document.querySelector('.c4-travel'), '타임 트래블 막대');
    assert(!document.querySelector('.c4-details .c4-note-unlink') && !document.querySelector('.c4-details .c4-note-link-btn'), '과거를 보는 동안에는 노트를 붙이거나 뗄 수 없다');
    document.querySelector('button[aria-label="타임 트래블 닫기"]').click();
    await until(() => !document.querySelector('.c4-travel'), '막대가 닫힘');
    cardOf('GitHub').click();
    await until(() => document.querySelector('.c4-details .c4-note-unlink'), '상세의 연결 해제 단추');
    document.querySelector('.c4-details .c4-note-unlink').click();
    await until(async () => (await elementOf('GitHub')).notes.length === 0, '엔진에서 떨어졌다');
    await until(() => !badgeOf('GitHub'), '배지가 사라진다');
  });
  // 요소 아이콘: 편집 창에서 고르면(검색도 된다) 엔진에 저장되고 카드와 상세 머리에 그려진다. 모르는 이름은 엔진이 거절하고, 「자동」으로 되돌릴 수 있다.
  const iconOf = async name => (await model()).elements.find(e => e.name === name)?.icon ?? null;
  const tileOf = name => cardOf(name)?.querySelector('.c4-card-icon');
  const openEdit = async name => {
    cardOf(name).click();
    await until(() => document.querySelector('.c4-details-head strong')?.textContent === name, `${name} 상세`);
    byText('.c4-details-actions button', '편집').click();
    await until(form, '편집 창');
  };

  await test('an-element-icon-is-chosen-in-the-edit-form-and-drawn-on-card-and-details', async () => {
    await openEdit('GitHub');
    assert(tileOf('GitHub').dataset.icon === 'git-branch', '고르기 전에는 이름의 단서(GitHub)로 자동');
    assert(form().querySelector('.c4-icon-auto').getAttribute('aria-pressed') === 'true', '처음에는 자동이 눌려 있다');
    const rowOf = selector => form().querySelector(selector).getBoundingClientRect();
    assert(Math.abs(rowOf('.c4-icon-search').top - rowOf('.c4-icon-preview').top) < rowOf('.c4-icon-preview').height, '검색칸은 미리보기와 같은 줄에 있다');
    assert(rowOf('.c4-icon-search').height < 48, `검색칸이 한 줄 높이다: ${rowOf('.c4-icon-search').height}`);
    const search = form().querySelector('.c4-icon-search');
    assert(Math.abs(search.querySelector('svg').getBoundingClientRect().top - search.querySelector('input').getBoundingClientRect().top) < 24, '돋보기와 입력칸이 한 줄에 놓인다');
    fillField(form().querySelector('.c4-icon-search input'), 'tele');
    await until(() => all('.c4-icon-opt', form()).length === 1 && all('.c4-icon-opt', form())[0].dataset.icon === 'telegram', '검색이 telegram 하나로 좁힌다');
    fillField(form().querySelector('.c4-icon-search input'), '');
    await until(() => form().querySelector('.c4-icon-opt[data-icon="github"]'), '로고 목록');
    assert(all('.c4-icon-opt', form()).length >= 100, '선 아이콘과 서비스 로고가 모두 보인다');
    form().querySelector('.c4-icon-opt[data-icon="github"]').click();
    await until(() => form().querySelector('.c4-icon-preview').dataset.icon === 'github' && form().querySelector('.c4-icon-auto').getAttribute('aria-pressed') === 'false', '미리보기가 바뀜');
    submit().click();
    await until(() => !form(), '창이 닫힘');
    await until(async () => (await iconOf('GitHub')) === 'github', '엔진에 icon이 저장됨');
    await until(() => tileOf('GitHub')?.dataset.icon === 'github' && tileOf('GitHub').querySelector('svg[viewBox="0 0 24 24"]'), '카드에 GitHub 로고');
    assert(tileOf('GitHub').style.getPropertyValue('--kind').toLowerCase() === '#181717', `로고 타일은 브랜드 색: ${tileOf('GitHub').getAttribute('style')}`);
    assert(document.querySelector('.c4-details-icon')?.dataset.icon === 'github', '상세 머리도 같은 아이콘');
  });

  await test('an-unknown-icon-is-refused-and-auto-brings-back-the-default', async () => {
    const read = await model();
    const github = read.elements.find(e => e.name === 'GitHub');
    let refusal = '';
    try {
      await invoke('engine_call', { command: 'apply_c4_changes', args: { project: A.projectSlug, f1: read.precondition, operations: [{ op: 'put_element', id: github.id, icon: 'githab' }] } });
    } catch (failure) { refusal = JSON.stringify(failure); }
    assert(refusal.includes('icon을 모른다'), `엔진이 모르는 이름을 거절한다: ${refusal}`);
    assert((await iconOf('GitHub')) === 'github', '거절하면 아무것도 바뀌지 않는다');

    await openEdit('GitHub');
    assert(form().querySelector('.c4-icon-opt[data-icon="github"]').getAttribute('aria-pressed') === 'true', '고른 아이콘이 눌려 있다');
    form().querySelector('.c4-icon-auto').click();
    await until(() => form().querySelector('.c4-icon-auto').getAttribute('aria-pressed') === 'true', '자동으로 돌아감');
    submit().click();
    await until(() => !form(), '창이 닫힘');
    await until(async () => (await iconOf('GitHub')) === null, '엔진에서 icon이 비워짐');
    await until(() => tileOf('GitHub')?.dataset.icon === 'git-branch' && !tileOf('GitHub').querySelector('svg[viewBox="0 0 24 24"]'), '카드가 기본 아이콘으로 돌아옴');
  });
} catch { /* 첫 실패 뒤의 점검은 앞 상태에 기대므로 멈춘다 */ }
await post('/result', results);
