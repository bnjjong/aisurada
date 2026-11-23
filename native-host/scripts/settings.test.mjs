// 설정 화면의 컨트롤러 동작, 묶음 정의, 검색, 글꼴 감지 시험
import test from 'node:test';
import assert from 'node:assert/strict';
import { WorkspaceController } from '../src/workspace/controller.ts';
import { KEYS } from '../src/workspace/storage.ts';

function harness(t, { context = { kind: 'main' }, values = {} } = {}) {
  const map = new Map(Object.entries(values)), writes = [], warnings = [];
  const services = {
    host: { invoke: async () => undefined, documentDir: async () => '/documents/', chooseDirectory: async () => null, confirmLeave: async () => false },
    storage: { getItem: k => map.get(k) ?? null, setItem: (k, v) => { writes.push(k); map.set(k, v); } },
    files: { exists: async () => false },
    watch: async () => () => {},
    navigate: () => {},
    pending: { flushEditors: async () => {}, waitForWrites: async () => true, unsavedNotes: () => 0 },
    now: () => new Date('2026-11-23T00:00:00Z'), uuid: () => 'u', sleep: async () => {}, warn: (...args) => warnings.push(args),
  };
  const controller = new WorkspaceController(services, context, '/');
  t.after(() => controller.dispose());
  return { controller, map, writes, warnings };
}

test('설정 여는 곳과 묶음 상태: 초기값은 look, 지정한 묶음으로 열리고 닫아도 유지된다', t => {
  const h = harness(t);
  assert.equal(h.controller.getSnapshot().settingsOpen, false);
  assert.equal(h.controller.getSnapshot().settingsSection, 'look');

  // notifications 묶음으로 열기
  h.controller.settings(true, 'notifications');
  assert.equal(h.controller.getSnapshot().settingsOpen, true);
  assert.equal(h.controller.getSnapshot().settingsSection, 'notifications');

  // 닫을 때는 묶음을 그대로 둔다
  h.controller.settings(false);
  assert.equal(h.controller.getSnapshot().settingsOpen, false);
  assert.equal(h.controller.getSnapshot().settingsSection, 'notifications');

  // 섹션 지정 없이 다시 열면 마지막 묶음이 열린다
  h.controller.settings(true);
  assert.equal(h.controller.getSnapshot().settingsOpen, true);
  assert.equal(h.controller.getSnapshot().settingsSection, 'notifications');

  // 다른 묶음으로 바꾸기
  h.controller.settings(true, 'connections');
  assert.equal(h.controller.getSnapshot().settingsSection, 'connections');
});

test('노트 창(보조 창)에서는 설정이 열리지 않는다', t => {
  const h = harness(t, { context: { kind: 'note', docId: 'doc1' } });
  assert.equal(h.controller.getSnapshot().settingsOpen, false);
  h.controller.settings(true, 'notifications');
  assert.equal(h.controller.getSnapshot().settingsOpen, false);
});
