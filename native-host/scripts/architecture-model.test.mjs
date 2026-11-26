import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  createElementOps, createRelationOps, deletionImpact, editElementOps, editRelationOps, elementFormOf, emptyElementForm,
  kindsForLevel, newElementId, relationFormOf, viewIdFor,
} from '../src/ui/architecture/model.ts';

const element = (id, kind, level, parentId = null, extra = {}) => ({
  id, kind, level, name: id.toUpperCase(), description: '', technology: '', parentId, codeRef: null, ...extra,
});
const model = {
  project: 'demo', precondition: 'f1:x', views: {},
  elements: [
    element('me', 'person', 1), element('app', 'system', 1), element('gh', 'external', 1),
    element('engine', 'container', 2, 'app'), element('store', 'component', 3, 'engine'), element('cli', 'container', 2, 'app'),
  ],
  relationships: [
    { id: 'r1', source: 'me', target: 'app', label: '지시', technology: '' },
    { id: 'r2', source: 'engine', target: 'gh', label: 'push', technology: 'HTTPS' },
    { id: 'r3', source: 'cli', target: 'store', label: '', technology: '' },
  ],
};

test('뷰 이름은 단계와 부모로 정해지고, 부모가 필요한데 없으면 만들 수 없다', () => {
  assert.equal(viewIdFor(1, null), 'context');
  assert.equal(viewIdFor(2, 'app'), 'container:app');
  assert.equal(viewIdFor(3, 'engine'), 'component:engine');
  assert.equal(viewIdFor(2, null), null);
});

test('단계마다 새로 만들 수 있는 종류가 다르다', () => {
  assert.deepEqual(kindsForLevel(1), ['system', 'person', 'external']);
  assert.deepEqual(kindsForLevel(2), ['container']);
  assert.deepEqual(kindsForLevel(3), ['component']);
});

test('새 id는 이름에서 영문 slug를 만들고 겹치면 번호를 붙인다', () => {
  assert.equal(newElementId('Web App', 'container', []), 'web-app');
  assert.equal(newElementId('Web App', 'container', ['web-app']), 'web-app-2');
  assert.equal(newElementId('Web App', 'container', ['web-app', 'web-app-2']), 'web-app-3');
  assert.equal(newElementId('  --  ', 'system', []), 'system-1');
  assert.equal(newElementId('로컬 볼트', 'external', ['external-1']), 'external-2');
  assert.match(newElementId('x'.repeat(80), 'system', []), /^x{40}$/);
});

test('새 요소 변경은 부모와 선택 칸만 싣는다', () => {
  const [op] = createElementOps(model, { ...emptyElementForm('container'), name: ' API ', technology: 'Rust ' }, 'app');
  assert.deepEqual(op, { op: 'put_element', id: 'api', kind: 'container', name: 'API', description: '', technology: 'Rust', parent: 'app' });
  const [root] = createElementOps(model, { ...emptyElementForm('system'), name: '새 시스템', codeRef: ' src/a ' }, null);
  assert.equal(root.parent, undefined);
  assert.equal(root.codeRef, 'src/a');
  assert.equal(root.id, 'system-1');
});

test('고치는 변경은 바뀐 칸만 보내고, 바뀐 것이 없으면 보내지 않는다', () => {
  const before = element('engine', 'container', 2, 'app', { description: '엔진', technology: 'Rust' });
  const same = elementFormOf(before);
  assert.deepEqual(editElementOps(before, same), []);
  assert.deepEqual(editElementOps(before, { ...same, name: '새 이름 ' }), [{ op: 'put_element', id: 'engine', name: '새 이름' }]);
  assert.deepEqual(editElementOps(before, { ...same, description: '' }), [{ op: 'put_element', id: 'engine', description: '' }]);
  const relation = model.relationships[1];
  const form = relationFormOf(relation);
  assert.deepEqual(editRelationOps(relation, form), []);
  assert.deepEqual(editRelationOps(relation, { ...form, target: 'app' }), [{ op: 'put_relationship', id: 'r2', target: 'app' }]);
  assert.deepEqual(createRelationOps({ source: 'me', target: 'gh', label: ' 쓴다 ', technology: '' }),
    [{ op: 'put_relationship', source: 'me', target: 'gh', label: '쓴다', technology: '' }]);
});

test('요소를 지우면 자식과 닿은 관계가 함께 사라지는 수를 알려 준다', () => {
  assert.deepEqual(deletionImpact(model, 'store'), { elements: 0, relationships: 1 });
  assert.deepEqual(deletionImpact(model, 'engine'), { elements: 1, relationships: 2 });
  assert.deepEqual(deletionImpact(model, 'app'), { elements: 3, relationships: 3 });
  assert.deepEqual(deletionImpact(model, 'me'), { elements: 0, relationships: 1 });
});

test('엔진 호출은 아키텍처를 diagram으로 고르고, 쓰기는 읽은 모델의 아키텍처로 간다', async () => {
  const { getC4Model, applyC4Changes, listC4Diagrams, createC4Diagram, renameC4Diagram, deleteC4Diagram, readModel } = await import('../src/ui/architecture/client.ts');
  const sent = [];
  const answer = diagram => ({ ...model, diagram });
  const call = async request => {
    sent.push(request);
    if (request.command === 'list_c4_diagrams') return { diagrams: [{ id: 'main', name: '기본' }, { id: 'arch-2', name: '배포', elements: 1, relationships: 0 }] };
    if (request.command === 'create_c4_diagram' || request.command === 'rename_c4_diagram') return { diagram: { id: 'arch-2', name: request.args.name } };
    if (request.command === 'delete_c4_diagram') return { deleted: request.args.diagram };
    return answer({ id: request.args.diagram, name: '배포' });
  };
  const loaded = await getC4Model(call, 'demo', 'arch-2');
  assert.deepEqual(sent.at(-1), { command: 'get_c4_model', args: { project: 'demo', diagram: 'arch-2' } });
  await getC4Model(call, 'demo');
  assert.equal(sent.at(-1).args.diagram, 'main', '안 고르면 기본 아키텍처');

  await applyC4Changes(call, loaded, [{ op: 'delete_element', id: 'gh' }]);
  assert.deepEqual(sent.at(-1).args, { project: 'demo', diagram: 'arch-2', f1: 'f1:x', operations: [{ op: 'delete_element', id: 'gh' }] });

  assert.deepEqual((await listC4Diagrams(call, 'demo')).map(item => item.id), ['main', 'arch-2']);
  assert.equal((await createC4Diagram(call, 'demo', '배포')).id, 'arch-2');
  assert.equal((await renameC4Diagram(call, 'demo', 'arch-2', '운영')).name, '운영');
  assert.deepEqual(sent.at(-1).args, { project: 'demo', diagram: 'arch-2', name: '운영' });
  await deleteC4Diagram(call, 'demo', 'arch-2');
  assert.deepEqual(sent.at(-1), { command: 'delete_c4_diagram', args: { project: 'demo', diagram: 'arch-2' } });

  assert.throws(() => readModel({ ...model }), /모양/, 'diagram이 없는 응답은 받지 않는다');
});

test('아키텍처 관리 명령은 프로젝트 slug를 자리 인자로 보낸다', async () => {
  const { toArgv } = await import('../src/data/commands.ts');
  assert.deepEqual(toArgv({ command: 'rename_c4_diagram', args: { project: 'demo', diagram: 'arch-2', name: '운영' } }), ['rename-c4-diagram', 'demo', 'diagram=arch-2', 'name=운영']);
  assert.deepEqual(toArgv({ command: 'list_c4_diagrams', args: { project: 'demo' } }), ['list-c4-diagrams', 'demo']);
});

test('타임라인의 마지막 점은 늘 Now이고, 이력에 없는 변경이 있으면 Now가 따로 붙는다', async () => {
  const { travelPoints, describePoint } = await import('../src/ui/architecture/model.ts');
  const snap = (seq, extra = {}) => ({ seq, at: Date.UTC(2026, 10, 26, 0, 12), by: 'app', kind: 'change', summary: '요소 추가: API', elements: 1, relationships: 0, ...extra });
  const same = travelPoints({ snapshots: [snap(1), snap(2)], latestIsCurrent: true });
  assert.equal(same.length, 2, '마지막 점이 지금과 같으면 그 점이 Now다');
  const behind = travelPoints({ snapshots: [snap(1), snap(2)], latestIsCurrent: false });
  assert.equal(behind.length, 3);
  assert.equal(behind[2].snapshot, null, '이력에 없는 지금 상태는 빈 점이다');
  assert.equal(travelPoints({ snapshots: [], latestIsCurrent: true }).length, 1, '이력이 없어도 Now 하나는 있다');

  const fixed = () => '11월 26일 09:12';
  assert.equal(describePoint(behind[0], false, fixed), '11월 26일 09:12 · 앱 · 요소 추가: API');
  assert.equal(describePoint({ snapshot: snap(3, { by: 'ai', summary: '' }) }, false, fixed), '11월 26일 09:12 · AI · 변경');
  assert.equal(describePoint({ snapshot: snap(4, { by: '', kind: 'baseline', summary: '시작 상태' }) }, false, fixed), '11월 26일 09:12 · 시작 상태');
  assert.equal(describePoint(behind[2], true), '현재 상태');
  assert.equal(describePoint(same[1], true), '현재 상태 · 요소 추가: API');
});

test('앞 시점과 견줘 새로 생긴 요소와 내용이 바뀐 요소를 가린다', async () => {
  const { diffElements } = await import('../src/ui/architecture/model.ts');
  const before = { ...model, elements: model.elements.slice(0, 3) };
  const after = {
    ...model,
    elements: [
      ...model.elements.slice(0, 2),
      { ...model.elements[2], technology: 'Rust' },
      element('new', 'system', 1),
    ],
  };
  assert.deepEqual(diffElements(before, after), { gh: 'changed', new: 'added' });
  assert.deepEqual(diffElements(undefined, after), {}, '앞 시점이 없으면 표시하지 않는다');
  assert.deepEqual(diffElements(after, after), {});
});

test('이력 호출은 아키텍처와 점 번호를 싣고, 과거 응답에는 확인 값이 없어도 받는다', async () => {
  const { getC4Model, listC4History, readModel } = await import('../src/ui/architecture/client.ts');
  const sent = [];
  const call = async request => {
    sent.push(request);
    if (request.command === 'list_c4_history') return { snapshots: [{ seq: 1 }], latestIsCurrent: true };
    const { precondition, ...past } = { ...model, diagram: { id: 'main', name: '기본' } };
    return request.args.at ? past : { ...past, precondition };
  };
  const history = await listC4History(call, 'demo', 'arch-2');
  assert.deepEqual(sent.at(-1), { command: 'list_c4_history', args: { project: 'demo', diagram: 'arch-2' } });
  assert.equal(history.latestIsCurrent, true);
  const older = await getC4Model(call, 'demo', 'main', 3);
  assert.deepEqual(sent.at(-1).args, { project: 'demo', diagram: 'main', at: 3 });
  assert.equal(older.precondition, undefined);
  const { precondition, ...noToken } = { ...model, diagram: { id: 'main', name: '기본' } };
  assert.throws(() => readModel(noToken), /모양/, '지금 모델은 확인 값이 없으면 받지 않는다(쓰기에 쓰니까)');
});
