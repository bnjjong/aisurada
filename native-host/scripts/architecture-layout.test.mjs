import assert from 'node:assert/strict';
import { test } from 'node:test';
import { CARD_HEIGHT, CARD_WIDTH, MAX_SCALE, MIN_ZOOM, linkCurve, measureEdge, nearestPort, nodeAt, placeLabels, portById, portNormal, portPoint, portsFor, positionNodes, routeEdges, toWorld, viewOrigin, viewScale, wheelFactor, zoomCameraAt } from '../src/ui/architecture/layout.ts';

const points = [
  ['person', 0, 64], ['app', 478, 0], ['vault', 1140, -145],
  ['llm', 1130, 87], ['cli', 1122, 338], ['catalog', 312, 491], ['github', 639, 491],
];
const nodes = points.map(([id, x, y]) => ({ id, x, y }));
const edges = routeEdges(nodes, [
  ['person', 'app', '작업 지시, 노트 작성 및 의사결정 승인'],
  ['app', 'vault', '노트, 설정 및 워크트리 파일 읽기/쓰기'],
  ['app', 'llm', '프롬프트 전송 및 에이전트 추론 요청'],
  ['app', 'cli', '터미널 세션 및 빌드/테스트 도구 구동'],
  ['app', 'catalog', 'C4 아키텍처 모델 조회 및 쓰기'],
  ['app', 'github', '원격 브랜치 push, PR 생성 및 변경 검토'],
].map(([source, target, label], index) => ({ id: String(index), source, target, label, technology: '' })));

test('관계선은 같은 카드 가장자리에서 간격을 두고 시작한다', () => {
  const right = edges.slice(1, 4);
  assert.ok(right.every(edge => edge.start.x === 478 + CARD_WIDTH));
  assert.deepEqual(right.map(edge => edge.start.y), [CARD_HEIGHT / 2 - 20, CARD_HEIGHT / 2, CARD_HEIGHT / 2 + 20]);
  assert.ok(edges[4].start.x < edges[5].start.x);
  assert.equal(edges[4].start.y, CARD_HEIGHT);
});

test('저장된 요소 좌표는 그대로 두고, 좌표가 없는 요소는 저장된 요소 아래 격자에 놓는다', () => {
  const elements = points.map(([id]) => ({ id }));
  const positions = Object.fromEntries(points.map(([id, x, y]) => [id, { x, y }]));
  assert.deepEqual(positionNodes(elements, positions).map(({ id, x, y }) => [id, x, y]), points);

  const placed = positionNodes([...elements, { id: 'new-1' }, { id: 'new-2' }], positions);
  const fresh = placed.slice(points.length);
  const bottom = Math.max(...points.map(([, , y]) => y)) + CARD_HEIGHT;
  assert.ok(fresh.every(node => node.y >= bottom), '새 요소는 기존 요소와 겹치지 않는 아래쪽에 놓인다');
  assert.notDeepEqual([fresh[0].x, fresh[0].y], [fresh[1].x, fresh[1].y]);
  assert.deepEqual(positionNodes([{ id: 'only' }]).map(({ x, y }) => [x, y]), [[0, 0]]);
});

test('관계 흐름은 관계선 위에서 출발점부터 화살표까지 이동한다', () => {
  const edge = edges[4];
  const locate = measureEdge(edge);
  assert.deepEqual(locate(0), edge.start);
  assert.deepEqual(locate(1), edge.end);
  const samples = [0, .25, .5, .75, 1].map(locate);
  const steps = samples.slice(1).map((point, index) => Math.hypot(
    point.x - samples[index].x, point.y - samples[index].y,
  ));
  assert.ok(Math.min(...steps) > Math.max(...steps) * .95);
  assert.ok(samples.every((point, index) => index === 0 || point.y > samples[index - 1].y));
});

test('맞춤 배율에서 긴 관계 라벨이 카드나 다른 라벨을 가리지 않는다', () => {
  const view = { width: 767, height: 502, scale: .53, x: 21, y: 108 };
  const labels = placeLabels(edges, nodes, view, '5', text => text.length * 10);
  assert.equal(labels.length, edges.length);
  assert.equal(labels[0].edge.id, '5');
  const rect = ({ x, y, width, height }) => ({ left: x, top: y, right: x + width, bottom: y + height });
  const hit = (a, b) => a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
  const cards = nodes.map(node => rect({
    x: view.x + node.x * view.scale, y: view.y + node.y * view.scale,
    width: CARD_WIDTH * view.scale, height: CARD_HEIGHT * view.scale,
  }));
  const boxes = labels.map(label => rect({
    x: label.center.x - label.width / 2, y: label.center.y - label.height / 2,
    width: label.width, height: label.height,
  }));
  boxes.forEach((box, index) => {
    assert.ok(cards.every(card => !hit(box, card)));
    assert.ok(boxes.slice(index + 1).every(other => !hit(box, other)));
  });
});

const frame = (fit = .5) => ({ fit, width: 1000, height: 600, box: { minX: 0, minY: 0, width: 800, height: 400 } });
const worldAt = (camera, f, point) => {
  const origin = viewOrigin(camera, f);
  const scale = viewScale(camera, f);
  return { x: (point.x - origin.x) / scale, y: (point.y - origin.y) / scale };
};

test('휠 확대는 포인터 아래의 그림 위치를 그대로 둔다', () => {
  const f = frame();
  const camera = { zoom: 1, x: 10, y: -20 };
  const anchor = { x: 640, y: 210 };
  for (const factor of [1.5, 1.1, .6]) {
    const next = zoomCameraAt(camera, factor, anchor, f);
    const before = worldAt(camera, f, anchor);
    const after = worldAt(next, f, anchor);
    assert.ok(Math.abs(before.x - after.x) < 1e-9 && Math.abs(before.y - after.y) < 1e-9, `factor ${factor}`);
    assert.ok(factor > 1 ? next.zoom > camera.zoom : next.zoom < camera.zoom);
  }
});

test('확대는 하한과 배율 상한에서 멈추고, 상한에서 돌아설 때 바로 줄어든다', () => {
  const f = frame(1.12);
  let camera = { zoom: 1, x: 0, y: 0 };
  for (let step = 0; step < 40; step++) camera = zoomCameraAt(camera, 1.5, { x: 500, y: 300 }, f);
  assert.ok(viewScale(camera, f) <= MAX_SCALE + 1e-9);
  assert.equal(viewScale(camera, f), MAX_SCALE);
  assert.equal(zoomCameraAt(camera, 1.5, { x: 500, y: 300 }, f), camera, '상한에서는 같은 카메라를 돌려준다');
  const back = zoomCameraAt(camera, .9, { x: 500, y: 300 }, f);
  assert.ok(viewScale(back, f) < MAX_SCALE, '돌아서면 바로 반응한다');
  let low = { zoom: 1, x: 0, y: 0 };
  for (let step = 0; step < 40; step++) low = zoomCameraAt(low, .5, { x: 500, y: 300 }, f);
  assert.equal(low.zoom, MIN_ZOOM);
});

test('휠 움직임은 위로 굴리면 확대, 아래로 굴리면 축소이고 한 번에 크게 뛰지 않는다', () => {
  assert.ok(wheelFactor(-100, 0, false) > 1);
  assert.ok(wheelFactor(100, 0, false) < 1);
  assert.ok(wheelFactor(-10, 0, true) > wheelFactor(-10, 0, false), '모아 쥐기는 더 민감하다');
  assert.ok(wheelFactor(-5000, 0, false) <= Math.exp(.0015 * 240) + 1e-9, '아주 큰 값도 막는다');
  assert.ok(wheelFactor(-3, 1, false) > wheelFactor(-3, 0, false), '줄 단위 휠은 줄 높이만큼 곱한다');
});

test('연결 지점은 상자 카드에 12개(네 변마다 3개씩), 사람 카드에는 네 변 가운데 4개다', () => {
  const box = portsFor('system');
  assert.deepEqual(box.map(port => port.id), ['top-1', 'top-2', 'top-3', 'bottom-1', 'bottom-2', 'bottom-3', 'left-1', 'left-2', 'left-3', 'right-1', 'right-2', 'right-3']);
  assert.deepEqual(portsFor('person').map(port => port.id), ['top-2', 'bottom-2', 'left-2', 'right-2']);
  const node = { x: 100, y: 50 };
  const at = id => portPoint(node, box.find(port => port.id === id));
  assert.deepEqual(at('top-2'), { x: 100 + CARD_WIDTH / 2, y: 50 });
  assert.deepEqual(at('top-1'), { x: 100 + CARD_WIDTH / 4, y: 50 });
  assert.deepEqual(at('bottom-3'), { x: 100 + CARD_WIDTH * .75, y: 50 + CARD_HEIGHT });
  assert.deepEqual(at('left-1'), { x: 100, y: 50 + CARD_HEIGHT * .25 });
  assert.deepEqual(at('left-2'), { x: 100, y: 50 + CARD_HEIGHT / 2 });
  assert.deepEqual(at('left-3'), { x: 100, y: 50 + CARD_HEIGHT * .75 });
  assert.deepEqual(at('right-1'), { x: 100 + CARD_WIDTH, y: 50 + CARD_HEIGHT * .25 });
  assert.deepEqual(at('right-2'), { x: 100 + CARD_WIDTH, y: 50 + CARD_HEIGHT / 2 });
  assert.deepEqual(at('right-3'), { x: 100 + CARD_WIDTH, y: 50 + CARD_HEIGHT * .75 });
  assert.equal(portById('left').id, 'left-2', '예전 left 이름은 left-2로 찾는다');
  assert.equal(portById('right').id, 'right-2', '예전 right 이름은 right-2로 찾는다');
});

test('가장 가까운 연결 지점은 포인터가 놓인 쪽의 지점이다', () => {
  const node = { x: 0, y: 0 };
  assert.equal(nearestPort(node, { x: CARD_WIDTH * .3, y: -20 }, 'system').id, 'top-1');
  assert.equal(nearestPort(node, { x: CARD_WIDTH * .7, y: CARD_HEIGHT + 30 }, 'system').id, 'bottom-3');
  assert.equal(nearestPort(node, { x: -40, y: CARD_HEIGHT * .2 }, 'system').id, 'left-1');
  assert.equal(nearestPort(node, { x: -40, y: CARD_HEIGHT * .5 }, 'system').id, 'left-2');
  assert.equal(nearestPort(node, { x: -40, y: CARD_HEIGHT * .8 }, 'system').id, 'left-3');
  assert.equal(nearestPort(node, { x: CARD_WIDTH + 40, y: CARD_HEIGHT * .2 }, 'system').id, 'right-1');
  assert.equal(nearestPort(node, { x: CARD_WIDTH + 40, y: CARD_HEIGHT * .8 }, 'system').id, 'right-3');
  assert.equal(nearestPort(node, { x: CARD_WIDTH * .3, y: -20 }, 'person').id, 'top-2', '사람은 가운데 지점만 있다');
});

test('포인터 아래의 카드를 월드 좌표로 찾고, 겹치면 위에 그린 카드가 이기며, 여백만큼 바깥도 친다', () => {
  const items = [{ id: 'a', x: 0, y: 0 }, { id: 'b', x: 100, y: 50 }];
  assert.equal(nodeAt(items, { x: 10, y: 10 }), 'a');
  assert.equal(nodeAt(items, { x: 150, y: 80 }), 'b', '겹치면 나중 카드');
  assert.equal(nodeAt(items, { x: CARD_WIDTH + 200, y: 0 }), null);
  assert.equal(nodeAt(items, { x: -8, y: 10 }), null);
  assert.equal(nodeAt(items, { x: -8, y: 10 }, 12), 'a', '여백 안은 카드로 친다');
  const world = toWorld({ x: 330, y: 210 }, { x: 30, y: 10 }, 2);
  assert.deepEqual(world, { x: 150, y: 100 });
});

test('미리보기 곡선은 가로로 뻗었다가 도착점으로 들어온다', () => {
  const { a, b } = linkCurve({ x: 0, y: 0 }, { x: 200, y: 80 });
  assert.deepEqual(a, { x: 100, y: 0 });
  assert.deepEqual(b, { x: 100, y: 80 });
});

test('저장된 연결 지점이 있으면 선은 그 지점에서 시작·도착하고 그 변이 향하는 쪽으로 뻗는다', () => {
  const pair = [{ id: 'a', x: 0, y: 0 }, { id: 'b', x: 600, y: 40 }];
  const [auto] = routeEdges(pair, [{ id: 'r', source: 'a', target: 'b', label: '', technology: '' }]);
  const [fixed] = routeEdges(pair, [{ id: 'r', source: 'a', target: 'b', label: '', technology: '', sourcePort: 'bottom-3', targetPort: 'top-1' }]);
  assert.deepEqual(fixed.start, portPoint(pair[0], portById('bottom-3')));
  assert.deepEqual(fixed.end, portPoint(pair[1], portById('top-1')));
  assert.ok(fixed.a.y > fixed.start.y && fixed.a.x === fixed.start.x, '아래 변에서는 아래로 뻗는다');
  assert.ok(fixed.b.y < fixed.end.y && fixed.b.x === fixed.end.x, '위 변으로는 위에서 들어온다');
  assert.notDeepEqual(auto.start, fixed.start, '지점이 없으면 자동 배치다');
  assert.deepEqual(portNormal(portById('left')), { x: -1, y: 0 });
});

test('지점이 한쪽만 정해지면 다른 쪽은 자동 배치를 따르고, 모르는 이름은 자동으로 본다', () => {
  const pair = [{ id: 'a', x: 0, y: 0 }, { id: 'b', x: 600, y: 40 }];
  const base = { id: 'r', source: 'a', target: 'b', label: '', technology: '' };
  const [auto] = routeEdges(pair, [base]);
  const [half] = routeEdges(pair, [{ ...base, sourcePort: 'top-2' }]);
  assert.deepEqual(half.start, portPoint(pair[0], portById('top-2')));
  assert.deepEqual(half.end, auto.end, '도착은 그대로 자동이다');
  const [unknown] = routeEdges(pair, [{ ...base, sourcePort: 'nowhere', targetPort: '' }]);
  assert.deepEqual(unknown.start, auto.start);
  assert.deepEqual(unknown.end, auto.end);
});

test('지점이 정해진 선은 자동 간격 나눔에 끼지 않고, 같은 변의 자동 선들만 나눠 선다', () => {
  const trio = [{ id: 'a', x: 0, y: 0 }, { id: 'b', x: 600, y: 0 }, { id: 'c', x: 600, y: 300 }];
  const lines = ['b', 'c'].map((target, index) => ({ id: 'r' + index, source: 'a', target, label: '', technology: '' }));
  const [one, two] = routeEdges(trio, [{ ...lines[0], sourcePort: 'right' }, lines[1]]);
  assert.deepEqual(one.start, portPoint(trio[0], portById('right')), '지점이 정해진 끝은 움직이지 않는다');
  assert.ok(two.start.x >= 0, '자동 선은 그대로 그려진다');
});

test('라벨은 배율(unit)에 비례해 카드와 같은 비율로 커진다', () => {
  const view = { width: 3200, height: 1800, scale: 1, x: 0, y: 0 };
  const at = unit => placeLabels(edges, nodes, view, null, text => text.length * 8 * unit, unit).find(label => label.edge.id === '0');
  const one = at(1);
  const two = at(2);
  assert.equal(one.height, 22);
  assert.equal(two.height, 42, '한 줄 높이는 20*unit+2다');
  const ratio = (two.width - 2) / (one.width - 2);
  assert.ok(ratio > 1.95 && ratio < 2.05, `너비도 같은 비율로 커진다: ${one.width} -> ${two.width}`);
});

test('한 줄로 들어가면 줄바꿈 없이 한 줄이고, 칸이 좁을 때만 두 줄이나 아이콘으로 줄인다', () => {
  const measure = text => text.length * 10;
  const title = '작업 지시, 노트 작성 및 의사결정 승인';
  const wide = placeLabels(edges, nodes, { width: 1600, height: 900, scale: 1, x: 0, y: 0 }, null, measure).find(label => label.edge.id === '0');
  assert.equal(wide.title, title);
  assert.equal(wide.height, 22, '넓은 칸에서는 한 줄이다');
  assert.equal(wide.width, title.length * 10 + 16, '글 폭에 맞춘 너비다(잘리지 않는다)');
  assert.equal(wide.compact, false);
  const narrow = placeLabels(edges, nodes, { width: 360, height: 900, scale: .4, x: 0, y: 0 }, null, measure).find(label => label.edge.id === '0');
  assert.ok(!narrow || narrow.compact || narrow.height === 38 || narrow.width < wide.width, '좁은 칸에서는 줄이거나 아이콘이다');
});

test('직접 지정된 라벨 좌표(positions)가 있으면 그 위치에 라벨을 놓고 곡선과 가장 가까운 지점을 앵커로 잡는다', () => {
  const measure = text => text.length * 10;
  const view = { width: 1600, height: 900, scale: 1, x: 0, y: 0 };
  const customPositions = { '0': { x: 500, y: 350 } };
  const labels = placeLabels(edges, nodes, view, null, measure, 1, customPositions);
  const label = labels.find(l => l.edge.id === '0');
  assert.ok(label, '라벨이 배치되어야 한다');
  assert.equal(label.center.x, 500);
  assert.equal(label.center.y, 350);
  assert.notEqual(label.center.x, label.anchor.x);
});

