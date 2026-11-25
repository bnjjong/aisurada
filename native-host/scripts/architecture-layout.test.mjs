import assert from 'node:assert/strict';
import { test } from 'node:test';
import { CARD_HEIGHT, CARD_WIDTH, measureEdge, placeLabels, positionNodes, routeEdges } from '../src/ui/architecture/layout.ts';

const points = [
  ['person', 0, 64], ['app', 478, 0], ['vault', 1140, -145],
  ['llm', 1130, 87], ['cli', 1122, 338], ['archyl', 312, 491], ['github', 639, 491],
];
const nodes = points.map(([id, x, y]) => ({ id, x, y }));
const edges = routeEdges(nodes, [
  ['person', 'app', '작업 지시, 노트 작성 및 의사결정 승인'],
  ['app', 'vault', '노트, 설정 및 워크트리 파일 읽기/쓰기'],
  ['app', 'llm', '프롬프트 전송 및 에이전트 추론 요청'],
  ['app', 'cli', '터미널 세션 및 빌드/테스트 도구 구동'],
  ['app', 'archyl', 'C4 아키텍처 모델 조회 및 쓰기'],
  ['app', 'github', '원격 브랜치 push, PR 생성 및 변경 검토'],
].map(([source, target, label], index) => ({ id: String(index), source, target, label, technology: '' })));

test('관계선은 같은 카드 가장자리에서 간격을 두고 시작한다', () => {
  const right = edges.slice(1, 4);
  assert.ok(right.every(edge => edge.start.x === 478 + CARD_WIDTH));
  assert.deepEqual(right.map(edge => edge.start.y), [CARD_HEIGHT / 2 - 20, CARD_HEIGHT / 2, CARD_HEIGHT / 2 + 20]);
  assert.ok(edges[4].start.x < edges[5].start.x);
  assert.equal(edges[4].start.y, CARD_HEIGHT);
});

test('Archyl에 저장된 C4 요소 좌표를 유지한다', () => {
  const model = points.map(([id, x, y]) => ({ id, position: { x, y } }));
  assert.deepEqual(positionNodes(model).map(({ id, x, y }) => [id, x, y]), points);
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
