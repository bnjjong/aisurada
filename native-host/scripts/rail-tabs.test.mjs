// 레일의 두 탭(노트 · 에이전트): 주소가 탭을 정하는 규칙, 탭이 마지막 화면을 기억하는 규칙, 탭별 메뉴와 배지.
import test, { beforeEach } from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { parseRoute } from '../src/navigation/routes.ts';
import { RAIL_TAB_HOME, activeRailTab, railTabOf, railTabTarget, rememberRailTab, resetRailTabs } from '../src/navigation/railTabs.ts';
import { resetRunner } from '../src/ui/runnerStore.ts';
import { resetApprovalsStore, setApprovalCounts } from '../src/ui/approvalsStore.ts';
import { Rail } from '../src/ui/Rail.tsx';

beforeEach(() => {
  resetRailTabs();
  resetRunner();
  resetApprovalsStore();
});

const fakeController = () => ({
  navigate() {}, form() {}, filterAgendas() {}, preference() {}, settings() {}, launcher() {},
  callEngine: async () => ({}), isNative: () => true,
});
const workspace = () => ({
  path: '/vault', name: 'v', sample: false, today: '2026-11-14', notes: [], templates: [], trash: [], skipped: [], conflicts: [], noteReadFailed: false,
  agendas: [{ id: 'ag1', slug: 'a1', title: '앱 아젠다', project: '앱' }],
  projects: [{ id: 'pj1', slug: '앱', title: '앱', hue: 'blue', summary: '앱 프로젝트', repos: [] }],
});
const state = (route) => ({ workspace: workspace(), prefs: { collapsedProjects: [] }, hiddenAgendas: [], theme: 'dark', route });
const rail = (route) => renderToStaticMarkup(React.createElement(Rail, { controller: fakeController(), state: state(route) })).replace(/<!-- -->/g, '');
const navLabels = (html) => [...html.matchAll(/<button class="nav-item[^"]*">([^<]+)/g)].map((m) => m[1].trim());
const run = (taskKey) => ({ runId: 'r-' + taskKey, taskKey, repo: '', sessionId: '', phase: 'waiting-answer', step: '일하기', startedAt: 0, endedAt: null, turns: 1, questions: 1, question: '?', answer: null, workdir: null, workedSecs: 0, costUsd: 0, inputTokens: 0, outputTokens: 0, summary: null, error: null });

test('the address decides the tab: note screens, agent screens, and screens that keep the tab', () => {
  const notes = ['/notes', '/notes?open=a,b', '/calendar?mode=week', '/agenda/ag1', '/favorites', '/connections', '/templates', '/trash', '/review'];
  const agents = ['/tasks', '/task/TSK-1', '/resources', '/project/' + encodeURIComponent('앱')];
  const keeps = ['/', '/guide', '/session/s1', '/terminal', '/terminal/t1', '/launcher', '/doc/n1', '/file?path=%2Fa.md', '/nowhere'];
  for (const a of notes) assert.equal(railTabOf(parseRoute(a)), 'notes', a);
  for (const a of agents) assert.equal(railTabOf(parseRoute(a)), 'agents', a);
  for (const a of keeps) assert.equal(railTabOf(parseRoute(a)), null, a);
});

test('a screen without a tab keeps the tab that was on, and the first press goes to the tab home', () => {
  assert.equal(activeRailTab('/guide'), 'notes', 'the app starts on the notes tab');
  rememberRailTab('/task/TSK-4');
  assert.equal(activeRailTab('/guide'), 'agents');
  assert.equal(activeRailTab('/session/s1'), 'agents');
  assert.equal(activeRailTab('/calendar'), 'notes', 'the address still wins');
  resetRailTabs();
  assert.equal(railTabTarget('agents', '/calendar'), RAIL_TAB_HOME.agents);
  assert.equal(railTabTarget('notes', '/guide'), RAIL_TAB_HOME.notes);
});

test('each tab remembers the last screen, and pressing the tab you are already on does nothing', () => {
  rememberRailTab('/agenda/ag1?open=n1');
  rememberRailTab('/task/TSK-4');
  assert.equal(railTabTarget('notes', '/task/TSK-4'), '/agenda/ag1?open=n1', 'back to the note screen that was open');
  assert.equal(railTabTarget('agents', '/task/TSK-4'), null, 'already on the agents tab');
  rememberRailTab('/guide');
  rememberRailTab('/doc/n1');
  assert.equal(railTabTarget('agents', '/agenda/ag1'), '/task/TSK-4', 'screens without a tab do not overwrite the memory');
  assert.equal(railTabTarget('agents', '/guide'), '/task/TSK-4', 'pressing the on tab from a neutral screen goes back to it');
});

test('the notes tab shows the six note menus and the project tree, and no agent menus', () => {
  const html = rail('/calendar');
  assert.match(html, /role="tablist"/);
  assert.match(html, /role="tab"[^>]*aria-selected="true"[^>]*>노트/);
  assert.match(html, /role="tab"[^>]*aria-selected="false"[^>]*>에이전트/);
  assert.deepEqual(navLabels(html), ['전체 노트', '캘린더', '즐겨찾기', '연결', '템플릿', '휴지통']);
  assert.match(html, /class="nav-item active">캘린더/);
  assert.match(html, /data-rail-section="agendas"/);
  assert.match(html, /앱 아젠다/);
  assert.match(html, /aria-label="앱 프로젝트 편집"/);
  assert.doesNotMatch(html, /프로젝트 페이지 열기/, 'the project page button moved to the agents tab');
  assert.doesNotMatch(html, /러너 ·/, 'the runner line is only on the agents tab');
});

test('the agents tab shows tasks, skills and agents, the runner line and one row per project', () => {
  const html = rail('/tasks');
  assert.match(html, /role="tab"[^>]*aria-selected="true"[^>]*>에이전트/);
  assert.match(html, /role="tab"[^>]*aria-selected="false"[^>]*>노트/);
  assert.deepEqual(navLabels(html), ['태스크', '스킬·에이전트']);
  assert.match(html, /class="nav-item active">태스크/);
  assert.match(html, />러너 · 쉬는 중</);
  assert.match(html, /data-rail-section="resources"/);
  assert.match(html, /aria-label="앱 프로젝트 페이지 열기"/);
  assert.doesNotMatch(html, /data-rail-section="agendas"/, 'the note tree stays on the notes tab');
  resetRunner({ supported: false });
  assert.doesNotMatch(rail('/tasks'), /러너 ·/, 'a build without the runner has no runner line');
  const empty = renderToStaticMarkup(React.createElement(Rail, { controller: fakeController(), state: { ...state('/resources'), workspace: { ...workspace(), projects: [] } } }));
  assert.match(empty, /프로젝트를 만들면/);
});

test('the guide stays in the footer for both tabs and keeps whichever tab was on', () => {
  for (const route of ['/calendar', '/tasks']) assert.match(rail(route), /<button class="iconbtn icon-btn " title="연동 가이드"/, route);
  rememberRailTab('/task/TSK-2');
  const onGuide = rail('/guide');
  assert.match(onGuide, /aria-selected="true"[^>]*>에이전트/, 'the guide opened from the agents tab leaves that tab on');
  assert.match(onGuide, /class="iconbtn icon-btn active" title="연동 가이드" aria-label="연동 가이드" aria-current="page"/);
  assert.deepEqual(navLabels(onGuide), ['태스크', '스킬·에이전트']);
});

test('the agents tab badge adds waiting questions and waiting approvals, and shows on the notes tab too', () => {
  assert.doesNotMatch(rail('/calendar'), /rail-tab-badge/, 'no badge while nothing waits');
  setApprovalCounts({ pending: 1, unapproved: 1 });
  resetRunner({ waiting: [run('TSK-1')] });
  const notes = rail('/calendar');
  assert.match(notes, /<span class="rail-tab-badge" role="img" aria-label="내 차례 3건">3<\/span>/);
  const agents = rail('/tasks');
  assert.match(agents, /<span class="rail-tab-badge" role="img" aria-label="내 차례 3건">3<\/span>/);
  assert.match(agents, /class="nav-item active">태스크<span class="nav-badge" role="img" aria-label="사람의 답을 기다리는 태스크 1개">1<\/span>/);
  assert.match(agents, /class="nav-item ">스킬·에이전트<span class="nav-badge" role="img" aria-label="승인이 필요한 변경 2개">2<\/span>/);
  assert.match(agents, /러너 · 답 기다림 1/);
});
