// 설정 화면의 컨트롤러 동작, 묶음 정의, 검색, 글꼴 감지 시험
import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { WorkspaceController } from '../src/workspace/controller.ts';
import { KEYS } from '../src/workspace/storage.ts';
import { SETTINGS_SECTIONS } from '../src/ui/settingsSections.ts';
import { SettingsLayoutView } from '../src/ui/Settings.tsx';

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

test('묶음 목록: 6개 묶음의 id, 이름, 아이콘, 설명이 정해진 순서대로 정의된다', () => {
  assert.deepEqual(
    SETTINGS_SECTIONS.map(s => s.id),
    ['look', 'fonts', 'notes', 'notifications', 'ai', 'connections']
  );
  assert.deepEqual(
    SETTINGS_SECTIONS.map(s => s.name),
    ['모양', '글꼴', '노트', '알림', 'AI', '연결']
  );
  assert.deepEqual(
    SETTINGS_SECTIONS.map(s => s.icon),
    ['palette', 'type', 'note', 'bell', 'bot', 'chain']
  );
});

test('그리기: SettingsLayoutView는 4부 유무에 따라 탭 수가 달라지고 고른 묶음 하나만 보인다', () => {
  const panes = {
    look: React.createElement('div', { id: 'p-look' }, 'Look'),
    fonts: React.createElement('div', { id: 'p-fonts' }, 'Fonts'),
    notes: React.createElement('div', { id: 'p-notes' }, 'Notes'),
    notifications: React.createElement('div', { id: 'p-ntf' }, 'Notifications'),
    ai: React.createElement('div', { id: 'p-ai' }, 'AI'),
    connections: React.createElement('div', { id: 'p-conn' }, 'Connections'),
  };

  // 4부를 켠 판 (6탭)
  const htmlFull = renderToStaticMarkup(React.createElement(SettingsLayoutView, {
    section: 'look',
    onSelectSection() {},
    claudeEnabled: true,
    panes,
  }));
  const fullTabs = [...htmlFull.matchAll(/role="tab"/g)];
  assert.equal(fullTabs.length, 6);
  assert.match(htmlFull, /id="set-tab-look"[^>]*aria-selected="true"/);
  assert.match(htmlFull, /id="set-tab-fonts"[^>]*aria-selected="false"/);
  // 숨지 않은 패널(hidden 없는 것)은 고른 묶음 하나뿐이어야 한다
  const visiblePanesFull = [...htmlFull.matchAll(/<section[^>]*class="set-pane"(?![^>]*hidden)[^>]*>/g)];
  assert.equal(visiblePanesFull.length, 1);
  assert.match(htmlFull, /id="set-pane-look"[^>]*class="set-pane"/);

  // 4부를 끈 판 (5탭, ai 제외)
  const htmlNoClaude = renderToStaticMarkup(React.createElement(SettingsLayoutView, {
    section: 'look',
    onSelectSection() {},
    claudeEnabled: false,
    panes,
  }));
  const noClaudeTabs = [...htmlNoClaude.matchAll(/role="tab"/g)];
  assert.equal(noClaudeTabs.length, 5);
  assert.ok(!htmlNoClaude.includes('id="set-tab-ai"'));
  assert.ok(!htmlNoClaude.includes('id="set-pane-ai"'));
});
