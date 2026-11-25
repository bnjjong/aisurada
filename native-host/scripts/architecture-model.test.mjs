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
