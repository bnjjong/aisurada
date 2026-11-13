import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import {
  DEFAULT_SETTINGS, EVENT_KINDS, MAX_ITEMS, MAX_TOASTS, applyChanged, bannerMessage, dismissToast, getNotificationsSnapshot, kindLabel, markRead,
  refreshNotifications, resetNotificationsStore, saveSettings, sendTest, toItem, toSettings, visibleItems, withSwitch,
} from '../src/ui/notificationsStore.ts';
import { NotificationPanel, ToastStack, actionLabel } from '../src/ui/NotificationCenter.tsx';
import { NotificationSettingsView } from '../src/ui/NotificationSettings.tsx';
import { Rail } from '../src/ui/Rail.tsx';

const NOW = Date.UTC(2026, 10, 13, 5, 0, 0);
const raw = (extra = {}) => ({ id: 'n1', kind: 'question', title: '질문이 도착했어요', body: '폴더 이름은?', taskKey: 'TSK-14', at: NOW, read: false, ...extra });
const item = (extra = {}) => toItem(raw(extra));
const snap = () => getNotificationsSnapshot();
const reset = () => resetNotificationsStore();

test('toItem reads a host row, fills defaults and refuses a row with the wrong shape', () => {
  const i = item();
  assert.deepEqual(i, { id: 'n1', kind: 'question', title: '질문이 도착했어요', body: '폴더 이름은?', taskKey: 'TSK-14', at: NOW, read: false });
  assert.equal(item({ taskKey: undefined }).taskKey, null);
  assert.equal(item({ taskKey: '' }).taskKey, null);
  assert.equal(item({ body: undefined }).body, '');
  assert.equal(item({ at: 'x' }).at, 0);
  assert.equal(item({ read: 'yes' }).read, false, 'only a real true is read');
  for (const bad of [null, 'x', [], raw({ id: '' }), raw({ id: undefined }), raw({ kind: 'sms' }), raw({ title: undefined })]) assert.equal(toItem(bad), null);
});

test('toSettings keeps what is valid and falls back to the defaults for the rest', () => {
  assert.deepEqual(toSettings(null), DEFAULT_SETTINGS);
  assert.deepEqual(toSettings({}), DEFAULT_SETTINGS);
  const s = toSettings({ inApp: false, macos: 'x', events: { pr: { inApp: false, macos: false }, done: { inApp: 1 }, question: 5 } });
  assert.equal(s.inApp, false);
  assert.equal(s.macos, true, 'a wrong type falls back');
  assert.deepEqual(s.events.pr, { inApp: false, macos: false });
  assert.deepEqual(s.events.done, { inApp: true, macos: false }, 'a bad field keeps its own default');
  assert.deepEqual(s.events.question, DEFAULT_SETTINGS.events.question);
  assert.equal(DEFAULT_SETTINGS.events.done.macos, false, 'Done is quiet by default');
});

test('withSwitch changes one switch and leaves the rest', () => {
  assert.equal(withSwitch(DEFAULT_SETTINGS, 'inApp', false).inApp, false);
  assert.equal(withSwitch(DEFAULT_SETTINGS, 'macos', false).macos, false);
  const e = withSwitch(DEFAULT_SETTINGS, 'macos', false, 'question');
  assert.deepEqual(e.events.question, { inApp: true, macos: false });
  assert.deepEqual(e.events.pr, DEFAULT_SETTINGS.events.pr);
  assert.equal(e.macos, true, 'the channel switch is untouched');
  assert.deepEqual(DEFAULT_SETTINGS.events.question, { inApp: true, macos: true }, 'the defaults are not mutated');
});

test('applyChanged puts the newest first, replaces the same id, caps the list and the toasts, and honors the toast flag', () => {
  reset();
  applyChanged({ unread: 1, pushed: raw({ id: 'a' }), toast: true });
  applyChanged({ unread: 2, pushed: raw({ id: 'b' }), toast: false });
  assert.deepEqual(snap().items.map((i) => i.id), ['b', 'a']);
  assert.equal(snap().unread, 2);
  assert.deepEqual(snap().toasts.map((t) => t.id), ['a'], 'only the one the settings allowed becomes a toast');
  applyChanged({ unread: 2, pushed: raw({ id: 'a', title: '바뀐 제목' }), toast: false });
  assert.equal(snap().items.length, 2, 'the same id is replaced, not doubled');
  assert.equal(snap().items[0].title, '바뀐 제목');
  applyChanged({ unread: 0, pushed: null, toast: false });
  assert.equal(snap().unread, 0, 'a read-only event only changes the count');
  assert.equal(snap().items.length, 2);
  reset();
  for (let i = 0; i < MAX_TOASTS + 2; i++) applyChanged({ unread: i, pushed: raw({ id: `t${i}` }), toast: true });
  assert.equal(snap().toasts.length, MAX_TOASTS);
  assert.equal(snap().toasts[0].id, `t${MAX_TOASTS + 1}`, 'the newest toast is first');
  reset();
  for (let i = 0; i < MAX_ITEMS + 3; i++) applyChanged({ unread: i, pushed: raw({ id: `i${i}` }), toast: false });
  assert.equal(snap().items.length, MAX_ITEMS);
  reset();
  applyChanged({ unread: 1, pushed: raw({ id: 'quiet' }), toast: true }, false);
  assert.equal(snap().toasts.length, 0, 'a note window follows the list but shows no toast');
  assert.equal(snap().items.length, 1);
  applyChanged({ unread: 1, pushed: { bad: true }, toast: true });
  assert.equal(snap().items.length, 1, 'a malformed pushed row is ignored');
  reset();
});

test('dismissToast removes one toast and is harmless for an unknown id', () => {
  reset();
  applyChanged({ unread: 1, pushed: raw({ id: 'a' }), toast: true });
  applyChanged({ unread: 2, pushed: raw({ id: 'b' }), toast: true });
  dismissToast('a');
  assert.deepEqual(snap().toasts.map((t) => t.id), ['b']);
  dismissToast('nope');
  assert.equal(snap().toasts.length, 1);
  reset();
});

test('refreshNotifications reads the list and the settings, and keeps an error text when the host fails', async () => {
  reset();
  const host = async (command) => {
    if (command === 'notifications_list') return { items: [raw({ id: 'a' }), { bad: 1 }, raw({ id: 'b', read: true })], unread: 1 };
    if (command === 'notifications_settings_get') return { macos: false };
    throw new Error('모르는 명령');
  };
  await refreshNotifications(host);
  assert.equal(snap().items.length, 2, 'a malformed row is dropped');
  assert.equal(snap().unread, 1);
  assert.equal(snap().settings.macos, false);
  assert.equal(snap().settings.inApp, true);
  assert.equal(snap().loaded, true);
  assert.equal(snap().error, '');
  await refreshNotifications(async () => { throw new Error('열린 vault가 없다'); });
  assert.equal(snap().error, '열린 vault가 없다');
  assert.equal(snap().items.length, 2, 'the last good list stays');
  reset();
});

test('markRead updates the screen first, asks the host, and puts things back if the host refuses', async () => {
  reset();
  applyChanged({ unread: 2, pushed: raw({ id: 'a' }), toast: false });
  applyChanged({ unread: 2, pushed: raw({ id: 'b' }), toast: false });
  const calls = [];
  await markRead(async (command, args) => { calls.push([command, args]); return 1; }, ['a']);
  assert.deepEqual(calls, [['notifications_mark_read', { ids: ['a'] }]]);
  assert.deepEqual(snap().items.map((i) => [i.id, i.read]), [['b', false], ['a', true]]);
  assert.equal(snap().unread, 1, 'the host count wins');
  await markRead(async (command, args) => { calls.push([command, args]); return 0; });
  assert.deepEqual(calls.at(-1), ['notifications_mark_read', { ids: null }], 'no ids means all');
  assert.equal(snap().items.every((i) => i.read), true);
  reset();
  applyChanged({ unread: 1, pushed: raw({ id: 'z' }), toast: false });
  await markRead(async () => { throw new Error('저장 실패'); }, ['z']);
  assert.equal(snap().items[0].read, false, 'put back');
  assert.equal(snap().unread, 1);
  assert.equal(snap().error, '저장 실패');
  reset();
});

test('saveSettings shows the change at once, saves what the host answers, and reverts on failure', async () => {
  reset();
  const next = withSwitch(DEFAULT_SETTINGS, 'inApp', false);
  const seen = [];
  await saveSettings(async (command, args) => { seen.push([command, args.settings.inApp]); return args.settings; }, next);
  assert.deepEqual(seen, [['notifications_settings_set', false]]);
  assert.equal(snap().settings.inApp, false);
  await saveSettings(async () => { throw new Error('못 썼다'); }, withSwitch(snap().settings, 'macos', false));
  assert.equal(snap().settings.macos, true, 'reverted');
  assert.equal(snap().settings.inApp, false, 'to the previous settings');
  assert.equal(snap().error, '못 썼다');
  reset();
});

test('sendTest goes through the host push and keeps the banner answer for the settings screen', async () => {
  reset();
  let sent;
  const out = await sendTest(async (command, args) => { sent = [command, args]; return { item: raw({ kind: 'test' }), toast: true, banner: 'sent', unread: 1 }; });
  assert.equal(sent[0], 'notifications_push');
  assert.equal(sent[1].item.kind, 'test');
  assert.equal(out.banner, 'sent');
  assert.equal(snap().lastBanner, 'sent');
  assert.equal(await sendTest(async () => { throw new Error('vault 없음'); }), null);
  assert.equal(snap().error, 'vault 없음');
  assert.equal(snap().lastBanner, '');
  reset();
});

test('texts: banner results, kind names, visible items', () => {
  assert.equal(bannerMessage(''), '');
  assert.match(bannerMessage('off'), /꺼져 있어서/);
  assert.match(bannerMessage('sent'), /집중 모드/);
  assert.match(bannerMessage('sent-fallback'), /Script Editor/);
  assert.match(bannerMessage('failed: 권한이 없다'), /보내지 못했어요\. 권한이 없다/);
  assert.equal(kindLabel('question'), '질문');
  assert.equal(kindLabel('pr'), '리뷰');
  const items = [item({ id: 'a', read: false }), item({ id: 'b', read: true })];
  assert.deepEqual(visibleItems(items, 'unread').map((i) => i.id), ['a']);
  assert.deepEqual(visibleItems(items, 'all').map((i) => i.id), ['a', 'b']);
  assert.equal(actionLabel({ kind: 'question', taskKey: 'TSK-1' }), '답하기');
  assert.equal(actionLabel({ kind: 'pr', taskKey: 'TSK-1' }), '태스크 보기');
  assert.equal(actionLabel({ kind: 'test', taskKey: null }), null);
  assert.deepEqual(EVENT_KINDS.map((e) => e.id), ['question', 'pr', 'failed', 'done']);
});

const noop = () => {};

test('the panel: unread first by tab, action buttons by kind, unread dot, empty states and the settings button', () => {
  const items = [
    item({ id: 'a', kind: 'question' }),
    item({ id: 'b', kind: 'pr', title: '리뷰할 일이 생겼어요', body: '분석 보고가 올라왔어요', taskKey: 'TSK-6', read: true }),
    item({ id: 'c', kind: 'test', title: '시험 알림이에요', body: '', taskKey: undefined }),
  ];
  const unreadTab = renderToStaticMarkup(React.createElement(NotificationPanel, { items, unread: 2, tab: 'unread', onTab: noop, onOpen: noop, onReadAll: noop, onSettings: noop }));
  assert.match(unreadTab, /role="dialog" aria-label="알림 센터"/);
  assert.match(unreadTab, /안 읽음 2/);
  assert.match(unreadTab, /data-notification-id="a"/);
  assert.doesNotMatch(unreadTab, /data-notification-id="b"/, 'a read one is not on the unread tab');
  assert.match(unreadTab, /답하기/);
  assert.match(unreadTab, /aria-label="안 읽음"/);
  assert.match(unreadTab, /읽음으로/, 'an unread one without a task can be marked read');
  assert.match(unreadTab, />TSK-14</);
  assert.match(unreadTab, /aria-label="알림 설정"/);
  const all = renderToStaticMarkup(React.createElement(NotificationPanel, { items, unread: 2, tab: 'all', onTab: noop, onOpen: noop, onReadAll: noop, onSettings: noop }));
  assert.match(all, /data-notification-id="b"/);
  assert.match(all, /태스크 보기/);
  const none = renderToStaticMarkup(React.createElement(NotificationPanel, { items: [], unread: 0, tab: 'unread', onTab: noop, onOpen: noop, onReadAll: noop, onSettings: noop }));
  assert.match(none, /안 읽은 알림이 없어요/);
  assert.match(none, /disabled=""[^>]*>모두 읽음/, 'nothing to mark read');
  const noneAll = renderToStaticMarkup(React.createElement(NotificationPanel, { items: [], unread: 0, tab: 'all', onTab: noop, onOpen: noop, onReadAll: noop, onSettings: noop }));
  assert.match(noneAll, /아직 알림이 없어요/);
});

test('the toast stack shows the title, the task key, and one action with a close button', () => {
  const html = renderToStaticMarkup(React.createElement(ToastStack, { toasts: [item()], onDismiss: noop, onOpen: noop }));
  assert.match(html, /role="status" aria-live="polite"/);
  assert.match(html, /질문이 도착했어요/);
  assert.match(html, /TSK-14/);
  assert.match(html, /답하기/);
  assert.match(html, /aria-label="닫기"/);
  assert.equal(renderToStaticMarkup(React.createElement(ToastStack, { toasts: [], onDismiss: noop, onOpen: noop })).includes('ntf-toast"'), false);
});

test('the settings section: three channels, the table of four news, the focus-mode note, and the test button', () => {
  const view = (extra = {}) => renderToStaticMarkup(React.createElement(NotificationSettingsView, { settings: DEFAULT_SETTINGS, native: true, banner: '', error: '', onChange: noop, onTest: noop, ...extra }));
  const html = view();
  assert.match(html, /aria-label="앱 안 알림"/);
  assert.match(html, /aria-label="macOS 알림센터"/);
  assert.match(html, /aria-label="Telegram"/);
  assert.match(html, /준비 중이에요/);
  assert.match(html, /집중 모드에서 aisurada를 허용하지 않으면/);
  for (const e of EVENT_KINDS) assert.match(html, new RegExp(`aria-label="${e.label} · 앱 안"`));
  assert.equal((html.match(/class="ntf-switch"/g) ?? []).length, 3 + 8, 'three channels and four news in two channels');
  assert.doesNotMatch(html, /set-switch/, 'not the class the settings acceptance test counts');
  assert.match(html, /시험 알림 보내기/);
  const doneMacos = /aria-label="Done으로 바뀌었을 때 · macOS"[^>]*>/.exec(html)[0];
  assert.doesNotMatch(doneMacos, /checked/, 'Done is not on macOS by default');
  const off = view({ settings: { ...DEFAULT_SETTINGS, inApp: false } });
  assert.doesNotMatch(/aria-label="앱 안 알림"[^>]*>/.exec(off)[0], /checked/);
  const browser = view({ native: false });
  assert.match(browser, /앱에서만 바꿀 수 있어요/);
  assert.match(/aria-label="앱 안 알림"[^>]*>/.exec(browser)[0], /disabled/);
  assert.match(view({ banner: 'sent' }), /macOS로 배너를 보냈어요/);
  assert.match(view({ error: '못 썼다' }), /role="alert"[^>]*>못 썼다/);
});

const fakeController = () => ({
  navigate() {}, form() {}, filterAgendas() {}, preference() {}, settings() {}, launcher() {},
  callEngine: async () => ({}), callHost: async () => ({}), isNative: () => true,
});
const state = (route) => ({
  workspace: { path: '/vault', name: 'v', sample: false, today: '2026-11-13', notes: [], templates: [], trash: [], skipped: [], conflicts: [], noteReadFailed: false, agendas: [], projects: [] },
  prefs: { collapsedProjects: [] }, hiddenAgendas: [], theme: 'dark', route,
});

test('the rail has the bell with the unread count in its name', () => {
  reset();
  assert.match(renderToStaticMarkup(React.createElement(Rail, { controller: fakeController(), state: state('/calendar') })), /class="rail-top"[\s\S]*aria-label="알림"/);
  applyChanged({ unread: 3, pushed: raw({ id: 'a' }), toast: false });
  const html = renderToStaticMarkup(React.createElement(Rail, { controller: fakeController(), state: state('/calendar') }));
  assert.match(html, /aria-label="알림 3개"/);
  assert.match(html, /rail-bell-dot">3</);
  reset();
});
