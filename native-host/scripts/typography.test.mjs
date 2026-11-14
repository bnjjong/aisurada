// 설정의 「글꼴과 크기」: 값 읽기와 보정, 글꼴 목록 만들기, CSS 변수 걸기, 배율 단축키, 컨트롤러의 배율 바꾸기, 설정 화면.
import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { KEYS, readPreferences, writePreferences } from '../src/workspace/storage.ts';
import {
  DEFAULT_TYPOGRAPHY, MAX_FONT_LENGTH, UI_SCALES, applyTypographyVars, cssFontFamily, fontProblem, normalizeFont, normalizeScale, previewFamily,
  readTypography, stepScale,
} from '../src/workspace/typography.ts';
import { zoomShortcutFor } from '../src/interaction/shortcuts.ts';
import { WorkspaceController } from '../src/workspace/controller.ts';
import { TypographySettings } from '../src/ui/TypographySettings.tsx';

const createStorage = (initial = {}) => {
  const map = new Map(Object.entries(initial)), writes = [];
  return { getItem: key => map.has(key) ? map.get(key) : null, setItem: (key, value) => { writes.push(key); map.set(key, value); }, writes };
};
const saved = state => createStorage({ [KEYS.prefs]: JSON.stringify({ state, version: 1 }) });

test('배율은 목록에 있는 값만 받고, 모르는 값은 기본 100으로 읽는다', () => {
  assert.deepEqual([...UI_SCALES], [90, 100, 110, 125, 150, 175]);
  for (const scale of UI_SCALES) assert.equal(normalizeScale(scale), scale);
  for (const bad of [undefined, null, '110', NaN, Infinity, 0, 99, 111, 200, {}]) assert.equal(normalizeScale(bad), 100, String(bad));
});

test('단축키가 오르내리는 차례: 한 단계씩, 맨 끝에서 멈추고, 0은 100으로', () => {
  assert.equal(stepScale(100, 1), 110);
  assert.equal(stepScale(110, 1), 125);
  assert.equal(stepScale(175, 1), 175);
  assert.equal(stepScale(100, -1), 90);
  assert.equal(stepScale(90, -1), 90);
  assert.equal(stepScale(150, 0), 100);
  // 목록 밖의 값에서는 기본에서 시작한다
  assert.equal(stepScale(133, 1), 110);
});

test('글꼴 글: 앞뒤 공백을 걷고, CSS를 깨는 글자와 너무 긴 글은 빈 글(기본)로 읽는다', () => {
  assert.equal(normalizeFont('  Pretendard  '), 'Pretendard');
  assert.equal(normalizeFont('Noto Sans KR, sans-serif'), 'Noto Sans KR, sans-serif');
  assert.equal(normalizeFont('a\nb'), 'a b');
  for (const bad of [undefined, null, 3, {}, 'x; color: red', 'a{b}', '<b>', 'a\\b', 'a/* */', 'x'.repeat(MAX_FONT_LENGTH + 1)]) assert.equal(normalizeFont(bad), '', String(bad));
  assert.equal(fontProblem('Pretendard'), null);
  assert.equal(fontProblem(''), null);
  assert.match(fontProblem('x; y'), /쓸 수 없는 글자/);
  assert.match(fontProblem('x'.repeat(MAX_FONT_LENGTH + 1)), /200자/);
});

test('글꼴 목록 만들기: 이름 하나는 따옴표로 감싸고, 직접 쓴 목록은 그대로 두고, 끝에 기본 글꼴을 붙인다', () => {
  assert.equal(cssFontFamily('Pretendard', 'var(--font-ui-base)'), '"Pretendard", var(--font-ui-base)');
  assert.equal(cssFontFamily('Source Han 2', 'var(--font-ui-base)'), '"Source Han 2", var(--font-ui-base)');
  assert.equal(cssFontFamily('Pretendard, sans-serif', 'var(--font-ui-base)'), 'Pretendard, sans-serif, var(--font-ui-base)');
  assert.equal(cssFontFamily("'Noto Sans KR'", 'var(--font-ui-base)'), "'Noto Sans KR', var(--font-ui-base)");
  // 비우거나 쓸 수 없으면 기본 글꼴 그대로
  assert.equal(cssFontFamily('', 'var(--font-ui-base)'), 'var(--font-ui-base)');
  assert.equal(cssFontFamily('a; b', 'var(--font-ui-base)'), 'var(--font-ui-base)');
  assert.equal(previewFamily.ui('MesloLGS NF'), '"MesloLGS NF", var(--font-ui-base)');
  assert.equal(previewFamily.terminal('MesloLGS NF'), '"MesloLGS NF", var(--font-mono)');
  assert.equal(previewFamily.terminal(''), 'var(--font-mono)');
});

test('저장소에서 읽기: 없으면 기본값, 이상한 값은 항목별로 기본값, 기본 설정 21개는 늘지 않는다', () => {
  assert.deepEqual(readTypography(createStorage()), DEFAULT_TYPOGRAPHY);
  assert.deepEqual(readTypography(saved({ uiScale: 125, uiFont: 'Pretendard', terminalFont: 'MesloLGS NF' })), { scale: 125, uiFont: 'Pretendard', terminalFont: 'MesloLGS NF' });
  assert.deepEqual(readTypography(saved({ uiScale: '125', uiFont: 3, terminalFont: 'a;b' })), DEFAULT_TYPOGRAPHY);
  assert.deepEqual(readTypography(createStorage({ [KEYS.prefs]: '{broken' })), DEFAULT_TYPOGRAPHY);
  const prefs = readPreferences(createStorage());
  for (const key of ['uiScale', 'uiFont', 'terminalFont']) assert.equal(key in prefs, false, key);
  assert.equal(Object.keys(prefs).length, 21);
});

test('저장하기: prefs에 함께 저장되고 다른 설정을 바꿔도 남는다', () => {
  const storage = createStorage();
  writePreferences(storage, { uiScale: 150, uiFont: 'Pretendard', terminalFont: 'Hack' });
  assert.deepEqual(readTypography(storage), { scale: 150, uiFont: 'Pretendard', terminalFont: 'Hack' });
  writePreferences(storage, { theme: 'dark' });
  assert.deepEqual(readTypography(storage), { scale: 150, uiFont: 'Pretendard', terminalFont: 'Hack' });
  writePreferences(storage, { uiFont: '' });
  assert.equal(readTypography(storage).uiFont, '');
});

test('CSS 변수 걸기: 고른 글꼴은 변수로, 비우면 변수를 지운다', () => {
  const style = new Map();
  const root = { style: { setProperty: (name, value) => style.set(name, value), removeProperty: name => { style.delete(name); } } };
  applyTypographyVars(root, { scale: 100, uiFont: 'Pretendard', terminalFont: 'MesloLGS NF' });
  assert.equal(style.get('--font-ui'), '"Pretendard", var(--font-ui-base)');
  assert.equal(style.get('--font-terminal'), '"MesloLGS NF", var(--font-mono)');
  applyTypographyVars(root, { scale: 100, uiFont: '', terminalFont: 'MesloLGS NF' });
  assert.equal(style.has('--font-ui'), false);
  assert.equal(style.has('--font-terminal'), true);
  applyTypographyVars(root, DEFAULT_TYPOGRAPHY);
  assert.equal(style.size, 0);
});

test('styles.css: 기본 글꼴 변수와 터미널 글꼴 변수가 있고, 터미널은 --font-terminal을 읽는다', () => {
  const css = readFileSync(new URL('../src/styles.css', import.meta.url), 'utf8');
  assert.match(css, /--font-ui-base: -apple-system, BlinkMacSystemFont, 'Apple SD Gothic Neo', system-ui, sans-serif;/);
  assert.match(css, /--font-ui: var\(--font-ui-base\);\s+font-family: var\(--font-ui\);/);
  assert.match(css, /--font-terminal: var\(--font-mono\);/);
  const terminal = readFileSync(new URL('../src/ui/terminal/TerminalHost.tsx', import.meta.url), 'utf8');
  assert.match(terminal, /getPropertyValue\('--font-terminal'\)/);
  assert.match(terminal, /addEventListener\(TYPOGRAPHY_EVENT/);
});

const key = (init) => ({ key: '', code: '', metaKey: false, ctrlKey: false, shiftKey: false, altKey: false, defaultPrevented: false, preventDefault() { this.defaultPrevented = true; }, ...init });

test('배율 단축키: Cmd나 Ctrl과 + - 0, 자판 배열이 달라도 물리 키로 알아본다', () => {
  assert.equal(zoomShortcutFor(key({ metaKey: true, key: '=', code: 'Equal' })), 1);
  assert.equal(zoomShortcutFor(key({ metaKey: true, shiftKey: true, key: '+', code: 'Equal' })), 1);
  assert.equal(zoomShortcutFor(key({ ctrlKey: true, key: '+', code: 'NumpadAdd' })), 1);
  assert.equal(zoomShortcutFor(key({ metaKey: true, key: '-', code: 'Minus' })), -1);
  assert.equal(zoomShortcutFor(key({ ctrlKey: true, key: '-', code: 'NumpadSubtract' })), -1);
  assert.equal(zoomShortcutFor(key({ metaKey: true, key: '0', code: 'Digit0' })), 0);
  assert.equal(zoomShortcutFor(key({ metaKey: true, key: 'ㅡ', code: 'Minus' })), -1);
  // 수정 키가 없거나 Alt가 함께면, 다른 단축키가 이미 처리했으면 쓰지 않는다
  assert.equal(zoomShortcutFor(key({ key: '=', code: 'Equal' })), null);
  assert.equal(zoomShortcutFor(key({ metaKey: true, altKey: true, key: '-', code: 'Minus' })), null);
  assert.equal(zoomShortcutFor(key({ metaKey: true, key: '-', code: 'Minus', defaultPrevented: true })), null);
  assert.equal(zoomShortcutFor(key({ metaKey: true, shiftKey: true, key: ')', code: 'Digit0' })), null);
  assert.equal(zoomShortcutFor(key({ metaKey: true, key: 'k', code: 'KeyK' })), null);
  assert.equal(zoomShortcutFor(key({ metaKey: true, key: '2', code: 'Digit2' })), null);
});

function harness(t, { context = { kind: 'main' }, values = {}, setZoom } = {}) {
  const map = new Map(Object.entries(values)), writes = [], warnings = [], zooms = [];
  const services = {
    host: { invoke: async () => undefined, documentDir: async () => '/documents/', chooseDirectory: async () => null, confirmLeave: async () => false },
    storage: { getItem: k => map.get(k) ?? null, setItem: (k, v) => { writes.push(k); map.set(k, v); } },
    files: { exists: async () => false },
    watch: async () => () => {},
    navigate: () => {},
    pending: { flushEditors: async () => {}, waitForWrites: async () => true, unsavedNotes: () => 0 },
    now: () => new Date('2026-11-14T00:00:00Z'), uuid: () => 'u', sleep: async () => {}, warn: (...args) => warnings.push(args),
    ...(setZoom === false ? {} : { setZoom: setZoom ?? (async factor => { zooms.push(factor); }) }),
  };
  const controller = new WorkspaceController(services, context, '/');
  t.after(() => controller.dispose());
  return { controller, map, writes, warnings, zooms };
}

test('메인 창: 배율을 단계로 바꾸면 저장하고, 맨 끝에서 멈추며, 0은 100으로 돌린다', t => {
  const h = harness(t);
  assert.equal(h.controller.getSnapshot().typography.scale, 100);
  h.controller.zoomBy(1);
  assert.equal(h.controller.getSnapshot().typography.scale, 110);
  assert.equal(readTypography({ getItem: k => h.map.get(k) ?? null, setItem() {} }).scale, 110);
  h.controller.zoomBy(0);
  assert.equal(h.controller.getSnapshot().typography.scale, 100);
  h.controller.zoomBy(-1); h.controller.zoomBy(-1);
  assert.equal(h.controller.getSnapshot().typography.scale, 90);
  const before = h.writes.length;
  h.controller.zoomBy(-1);
  assert.equal(h.writes.length, before, '맨 끝에서는 다시 쓰지 않는다');
});

test('메인 창: 단축키는 시작 화면에서도 듣고, 기본 동작을 막는다', t => {
  const h = harness(t);
  const event = key({ metaKey: true, key: '=', code: 'Equal' });
  assert.equal(h.controller.handleKey(event), true);
  assert.equal(event.defaultPrevented, true);
  assert.equal(h.controller.getSnapshot().typography.scale, 110);
  assert.equal(h.controller.handleKey(key({ metaKey: true, key: '0', code: 'Digit0' })), true);
  assert.equal(h.controller.getSnapshot().typography.scale, 100);
});

test('메인 창: 설정에서 글꼴을 고르면 상태가 바뀌고 저장소에 남는다', t => {
  const h = harness(t);
  h.controller.preference({ uiFont: 'Pretendard', terminalFont: 'MesloLGS NF', uiScale: 125 });
  assert.deepEqual(h.controller.getSnapshot().typography, { scale: 125, uiFont: 'Pretendard', terminalFont: 'MesloLGS NF' });
  assert.deepEqual(JSON.parse(h.map.get(KEYS.prefs)).state.uiFont, 'Pretendard');
  h.controller.preference({ uiFont: '' });
  assert.equal(h.controller.getSnapshot().typography.uiFont, '');
});

test('노트 창: 저장소를 따르되, 단축키로 바꾼 배율은 그 창에만 남고 저장소에 쓰지 않는다', t => {
  const h = harness(t, { context: { kind: 'note', docId: 'n1' }, values: { [KEYS.prefs]: JSON.stringify({ state: { uiScale: 125, uiFont: 'Pretendard' }, version: 1 }) } });
  assert.deepEqual(h.controller.getSnapshot().typography, { scale: 125, uiFont: 'Pretendard', terminalFont: '' });
  h.controller.zoomBy(1);
  assert.equal(h.controller.getSnapshot().typography.scale, 150);
  assert.equal(h.writes.length, 0, '노트 창은 설정을 저장소에 쓰지 않는다');
  // 메인 창이 설정을 바꿔 저장소가 달라져도(창을 다시 눈여겨볼 때 읽는다) 글꼴은 따르고 이 창의 배율은 지킨다
  h.map.set(KEYS.prefs, JSON.stringify({ state: { uiScale: 90, uiFont: 'Hack' }, version: 1 }));
  h.controller.refreshTheme();
  assert.deepEqual(h.controller.getSnapshot().typography, { scale: 150, uiFont: 'Hack', terminalFont: '' });
});

test('applyScale: 웹뷰 확대를 배율의 배수(1.25)로 부르고, 실패하면 알리기만 한다', async t => {
  const h = harness(t);
  h.controller.applyScale(125);
  assert.deepEqual(h.zooms, [1.25]);
  const failing = harness(t, { setZoom: async () => { throw new Error('denied'); } });
  failing.controller.applyScale(90);
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(failing.warnings.length, 1);
  assert.match(failing.warnings[0][0], /화면 배율/);
  // 웹뷰 확대가 없는 환경(시험, 브라우저)에서는 아무 일도 하지 않는다
  const none = harness(t, { setZoom: false });
  none.controller.applyScale(90);
  assert.equal(none.warnings.length, 0);
});

const render = (typography) => renderToStaticMarkup(React.createElement(TypographySettings, { controller: { preference() {} }, typography })).replace(/<!-- -->/g, '');

test('설정 화면: 글꼴과 크기 구역에 배율 단추 여섯 개, 글꼴 입력 둘, 미리보기 둘이 있다', () => {
  const html = render({ scale: 125, uiFont: 'Pretendard', terminalFont: '' });
  assert.match(html, /<legend>글꼴과 크기<\/legend>/);
  const buttons = [...html.matchAll(/<button type="button" aria-pressed="(true|false)">(\d+)%<\/button>/g)].map(m => [m[2], m[1]]);
  assert.deepEqual(buttons, [['90', 'false'], ['100', 'false'], ['110', 'false'], ['125', 'true'], ['150', 'false'], ['175', 'false']]);
  assert.match(html, /지금은 125%예요/);
  assert.match(html, /<input id="typo-ui-font"[^>]*value="Pretendard"/);
  assert.match(html, /<input id="typo-terminal-font"[^>]*value=""/);
  assert.equal([...html.matchAll(/<p class="typo-preview[^"]*"/g)].length, 2);
  // 미리보기는 고른 글꼴을 쓰고, 비운 터미널은 기본(--font-mono)을 쓴다
  assert.match(html, /style="font-family:&quot;Pretendard&quot;, var\(--font-ui-base\)"/);
  assert.match(html, /style="font-family:var\(--font-mono\)"/);
  // 기본 글꼴일 때는 「기본값」 단추가 꺼져 있다
  assert.match(html, /<button type="button" disabled="">기본값<\/button>/);
});
