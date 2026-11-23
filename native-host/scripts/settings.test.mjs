// 설정 화면의 컨트롤러 동작, 묶음 정의, 검색, 글꼴 감지 시험
import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { WorkspaceController } from '../src/workspace/controller.ts';
import { KEYS } from '../src/workspace/storage.ts';
import { SETTINGS_SECTIONS, SETTINGS_INDEX, searchSettings } from '../src/ui/settingsSections.ts';
import { SettingsLayoutView, nextSection } from '../src/ui/Settings.tsx';
import { isFontAvailable, clearFontCache, filterAvailableFonts } from '../src/ui/fontAvailability.ts';
import { TypographySettings } from '../src/ui/TypographySettings.tsx';
import { toSettings } from '../src/ui/notificationsStore.ts';
import { LookSettings } from '../src/ui/LookSettings.tsx';
import { NoteSettings } from '../src/ui/NoteSettings.tsx';
import { NotificationSettingsView } from '../src/ui/NotificationSettings.tsx';
import { SettingsGroup, SettingsPane, SettingsRow } from '../src/ui/SettingsParts.tsx';
import { ConnectView } from '../src/plugs/claude/connect.tsx';
import { GithubConnect } from '../src/ui/GithubConnect.tsx';
import { ArchylConnect } from '../src/ui/archyl/ArchylConnect.tsx';

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

test('글꼴 감지 유틸: fallback과의 폭 차이로 설치 여부를 판정하고 캐시한다', () => {
  clearFontCache();
  // measure mock: 'InstalledFont'는 폭이 500, 나머지는 monospace 300, serif 350
  const mockMeasure = (font) => {
    if (font.includes('InstalledFont')) return 500;
    if (font.includes('monospace')) return 300;
    if (font.includes('serif')) return 350;
    return 300;
  };

  assert.equal(isFontAvailable('InstalledFont', mockMeasure), true);
  assert.equal(isFontAvailable('MissingFont', mockMeasure), false);

  // 빈 이름은 true
  assert.equal(isFontAvailable(''), true);

  // 필터 함수 확인
  const filtered = filterAvailableFonts(['InstalledFont', 'MissingFont'], mockMeasure);
  assert.deepEqual(filtered, ['InstalledFont']);
});

test('글꼴 칩 렌더링: 기본 칩과 설치된 추천 글꼴 칩이 보이고 현재 값에 따라 눌림 상태가 맞는다', () => {
  const dummyController = { preference() {} };

  // 1. 글꼴이 설정되지 않은 상태 (기본값)
  const htmlDefault = renderToStaticMarkup(
    React.createElement(TypographySettings, {
      controller: dummyController,
      typography: { uiFont: '', terminalFont: '', scale: 100 },
      availableFonts: ['Pretendard', 'D2Coding'],
    })
  );

  // 앱 글꼴 및 터미널 글꼴 영역에 각각 기본 칩이 aria-pressed="true" 상태여야 한다
  const defaultChips = [...htmlDefault.matchAll(/<button[^>]*class="set-chip"[^>]*aria-pressed="true"[^>]*>기본<\/button>/g)];
  assert.equal(defaultChips.length, 2);

  // 2. uiFont가 Pretendard로 설정된 상태
  const htmlCustom = renderToStaticMarkup(
    React.createElement(TypographySettings, {
      controller: dummyController,
      typography: { uiFont: 'Pretendard', terminalFont: 'D2Coding', scale: 100 },
      availableFonts: ['Pretendard', 'D2Coding'],
    })
  );

  // Pretendard 칩과 D2Coding 칩이 aria-pressed="true" 상태여야 한다
  assert.match(htmlCustom, /<button[^>]*class="set-chip"[^>]*aria-pressed="true"[^>]*>Pretendard<\/button>/);
  assert.match(htmlCustom, /<button[^>]*class="set-chip"[^>]*aria-pressed="true"[^>]*>D2Coding<\/button>/);
});

test('설정 검색: 검색어로 라벨과 키워드를 찾고, AI 제외 옵션 및 최대 8개 제한을 지킨다', () => {
  // 1. 빈 검색어는 빈 배열
  assert.deepEqual(searchSettings('', { claude: true }), []);
  assert.deepEqual(searchSettings('   ', { claude: true }), []);

  // 2. '글꼴' 검색 -> 앱 글꼴, 터미널 글꼴 등
  const fontResults = searchSettings('글꼴', { claude: true });
  assert.ok(fontResults.length >= 2);
  assert.ok(fontResults.some(r => r.key === 'ui-font'));
  assert.ok(fontResults.some(r => r.key === 'terminal-font'));

  // 3. 'claude' 검색 (대소문자 무시)
  const claudeResults = searchSettings('CLAUDE', { claude: true });
  assert.ok(claudeResults.some(r => r.key === 'claude'));

  // 4. claude: false 일 때 AI 항목 제외
  const noClaudeResults = searchSettings('CLAUDE', { claude: false });
  assert.equal(noClaudeResults.length, 0);

  // 5. '설정' 또는 넓은 검색어 시 최대 8개 제한
  const broadResults = searchSettings('알림', { claude: true });
  assert.ok(broadResults.length <= 8);
});

test('색인 유효성: SETTINGS_INDEX의 모든 항목은 유효한 6개 묶음 중 하나에 속한다', () => {
  const validSections = new Set(SETTINGS_SECTIONS.map(s => s.id));
  for (const item of SETTINGS_INDEX) {
    assert.ok(validSections.has(item.section), `잘못된 섹션: ${item.section}`);
    assert.ok(item.key.length > 0, 'key는 비어있지 않아야 함');
    assert.ok(item.label.length > 0, 'label은 비어있지 않아야 함');
    assert.ok(Array.isArray(item.keywords) && item.keywords.length > 0, 'keywords는 배열이어야 함');
  }
});

test('탭 키보드 탐색(nextSection): 화살표 및 Home/End 키로 올바른 다음 묶음을 순환 계산한다', () => {
  const visible = ['look', 'fonts', 'notes', 'notifications', 'ai', 'connections'];

  // 아래/오른쪽: 다음 묶음 (마지막에서 첫 번째로 순환)
  assert.equal(nextSection('look', 'ArrowDown', visible), 'fonts');
  assert.equal(nextSection('connections', 'ArrowDown', visible), 'look');
  assert.equal(nextSection('fonts', 'ArrowRight', visible), 'notes');

  // 위/왼쪽: 이전 묶음 (첫 번째에서 마지막으로 순환)
  assert.equal(nextSection('look', 'ArrowUp', visible), 'connections');
  assert.equal(nextSection('notes', 'ArrowUp', visible), 'fonts');
  assert.equal(nextSection('look', 'ArrowLeft', visible), 'connections');

  // Home / End: 처음과 끝
  assert.equal(nextSection('notes', 'Home', visible), 'look');
  assert.equal(nextSection('notes', 'End', visible), 'connections');

  // 지원하지 않는 키는 null
  assert.equal(nextSection('look', 'Enter', visible), null);
  assert.equal(nextSection('look', 'Tab', visible), null);
});

test('색인 전수 검증: SETTINGS_INDEX의 모든 key가 실제 설정 화면에 data-setting으로 정확히 하나씩 존재한다', (t) => {
  const h = harness(t);
  const dummyState = h.controller.getSnapshot();

  const panes = {
    look: React.createElement(LookSettings, { controller: h.controller, state: dummyState }),
    fonts: React.createElement(TypographySettings, { controller: h.controller, typography: dummyState.typography }),
    notes: React.createElement(NoteSettings, { controller: h.controller, state: dummyState }),
    notifications: React.createElement(NotificationSettingsView, {
      settings: toSettings({}),
      native: true,
      banner: '',
      error: '',
      onChange() {},
      onTest() {},
    }),
    ai: React.createElement(SettingsPane, { title: 'AI', desc: 'Claude가 노트를 다루는 범위와 기다리는 시간을 정해요.' },
      React.createElement('div', { className: 'set-group' },
        React.createElement('h4', null, 'Claude 연결'),
        React.createElement(ConnectView, {
          status: { claudeFound: true, registered: true, write: false, stale: false },
          busy: false,
          error: '',
          onConnect() {},
          onDisconnect() {},
        })
      ),
      React.createElement(SettingsGroup, { title: '묻기' },
        React.createElement(SettingsRow, {
          as: 'label',
          settingKey: 'ask-timeout',
          className: 'set-switch set-ask-timeout',
          title: 'AI 응답 대기 시간',
          desc: '노트에서 AI에게 질문하고 응답을 기다리는 최대 시간이에요.',
          control: React.createElement('select', null)
        })
      )
    ),
    connections: React.createElement('div', null,
      React.createElement(GithubConnect, { controller: h.controller }),
      React.createElement(ArchylConnect, { controller: h.controller })
    ),
  };

  const html = renderToStaticMarkup(
    React.createElement(SettingsLayoutView, {
      section: 'look',
      onSelectSection() {},
      claudeEnabled: true,
      panes,
    })
  );

  const renderedKeys = [...html.matchAll(/data-setting="([^"]+)"/g)].map((m) => m[1]);
  const indexKeys = SETTINGS_INDEX.map((item) => item.key);

  // 1. SETTINGS_INDEX의 모든 키가 화면에 있어야 한다
  for (const key of indexKeys) {
    assert.ok(
      renderedKeys.includes(key),
      `색인 key '${key}'가 실제 설정 화면 DOM에 존재하지 않습니다. (data-setting 누락)`
    );
  }

  // 2. 각 키는 중복 없이 정확히 1개여야 한다
  const keyCounts = new Map();
  for (const k of renderedKeys) {
    keyCounts.set(k, (keyCounts.get(k) || 0) + 1);
  }
  for (const key of indexKeys) {
    assert.equal(
      keyCounts.get(key),
      1,
      `key '${key}'가 화면에 ${keyCounts.get(key)}개 존재합니다 (정확히 1개여야 함).`
    );
  }
});
