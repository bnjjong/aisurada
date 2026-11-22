import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import {
  DEFAULT_RUNNER_SETTINGS, RECENT_RUNS, STEPS, activeRunOf, answerTask, formatCost, formatDuration, formatTokens, getRunnerSnapshot, isMissingCommand, loadRecentRuns, loadRuns,
  refreshRunner, resetRunner, runSubline, runTask, runTip, runTone, runnerChanged, runnerSummary, saveRunnerSettings, sinceKo, stepIndex, stopTask, taskOfSession, toEvents, toRun, toRuns,
  toSettings, waitingRunOf,
} from '../src/ui/runnerStore.ts';
import { RailRuns } from '../src/ui/RailRuns.tsx';
import { RunnerBar } from '../src/ui/RunnerBar.tsx';
import { RunnerPanel, clock } from '../src/ui/RunnerPanel.tsx';
import { TasksPage } from '../src/ui/TasksPage.tsx';
import { TaskDetailPage } from '../src/ui/TaskDetailPage.tsx';
import { toDetail, toTask } from '../src/ui/tasksClient.ts';

const run = (extra = {}) => ({
  runId: 'r1', taskKey: 'TSK-1', repo: 'o/r', sessionId: 's', phase: 'running', step: '일하기', startedAt: 1_000_000, endedAt: null, turns: 1, questions: 0,
  question: null, answer: null, workdir: '/w', workedSecs: 192, costUsd: 0.0071, inputTokens: 130346, outputTokens: 1487, summary: null, error: null, ...extra,
});

const status = (extra = {}) => ({
  settings: { autoPickup: false, concurrency: 3, model: 'sonnet' }, claudeAvailable: true, running: 0, active: [], waiting: [], ...extra,
});

/** 호스트 명령을 기록하는 가짜. 답은 명령 이름으로 정한다. */
const fakeHost = (answers = {}) => {
  const calls = [];
  const call = async (command, args) => {
    calls.push([command, args]);
    const a = answers[command];
    if (a instanceof Error) throw a;
    return typeof a === 'function' ? a(args) : a;
  };
  return { call, calls };
};

test('toRun reads a host record, defaults the gaps, and refuses a row without ids', () => {
  const r = toRun(run({ phase: 'waiting-answer', question: '어느 브랜치?' }));
  assert.equal(r.phase, 'waiting-answer');
  assert.equal(r.question, '어느 브랜치?');
  assert.equal(toRun(run({ phase: 'sleeping' })).phase, 'running', 'unknown phase falls back to running');
  const sparse = toRun({ runId: 'x', taskKey: 'TSK-2' });
  assert.deepEqual([sparse.turns, sparse.costUsd, sparse.endedAt, sparse.summary, sparse.workdir], [0, 0, null, null, null]);
  for (const bad of [null, 'x', [], { taskKey: 'TSK-1' }, { runId: 'r' }, { runId: '', taskKey: 'TSK-1' }]) assert.equal(toRun(bad), null);
  assert.equal(toRuns([run(), 5, null, run({ runId: 'r2' })]).length, 2);
  assert.deepEqual(toRuns('x'), []);
  assert.deepEqual(toEvents([{ at: 1, step: '일하기', text: 'a' }, { text: 5 }, null]), [{ at: 1, step: '일하기', text: 'a' }]);
});

test('toSettings clamps the concurrency and keeps the defaults for bad fields', () => {
  assert.deepEqual(toSettings(null), DEFAULT_RUNNER_SETTINGS);
  assert.equal(toSettings({ concurrency: 99 }).concurrency, 6);
  assert.equal(toSettings({ concurrency: 0 }).concurrency, 1);
  assert.equal(toSettings({ concurrency: 'x' }).concurrency, 3);
  assert.equal(toSettings({ autoPickup: 'yes' }).autoPickup, false);
  assert.equal(toSettings({ autoPickup: true, model: 'opus' }).model, 'opus');
  assert.equal(DEFAULT_RUNNER_SETTINGS.autoPickup, false, 'the runner does not pick up tasks by itself until the person turns it on');
});

test('formatters speak in Korean units', () => {
  assert.equal(formatDuration(5), '5초');
  assert.equal(formatDuration(192), '3분 12초');
  assert.equal(formatDuration(3720), '1시간 2분');
  assert.equal(formatTokens(999), '999');
  assert.equal(formatTokens(130346), '130.3k');
  assert.equal(formatTokens(2_500_000), '2.5M');
  assert.equal(formatCost(0), '0달러');
  assert.equal(formatCost(0.004), '0.01달러 미만');
  assert.equal(formatCost(1.234), '1.23달러');
  assert.equal(stepIndex('일하기'), 4);
  assert.equal(stepIndex('없음'), -1);
  assert.deepEqual([...STEPS], ['받기', '집기', '판단', '받아오기', '일하기', '내보내기', '정리']);
  assert.match(clock(1_700_000_000_000), /^\d+월 \d+일 \d\d:\d\d$/);
  assert.equal(clock(0), '');
});

test('refreshRunner stores the host status; a missing command marks the runner as unsupported', async () => {
  resetRunner();
  const host = fakeHost({ runner_status: status({ running: 1, active: [run()], waiting: [run({ runId: 'r2', taskKey: 'TSK-2', phase: 'waiting-answer', question: '?' })] }) });
  await refreshRunner(host.call);
  let s = getRunnerSnapshot();
  assert.equal(s.loaded, true);
  assert.equal(s.running, 1);
  assert.equal(activeRunOf(s, 'TSK-1').step, '일하기');
  assert.equal(waitingRunOf(s, 'TSK-2').question, '?');
  assert.equal(activeRunOf(s, 'TSK-9'), undefined);
  assert.equal(runnerSummary(s), '일하는 중 1 · 답 기다림 1');
  assert.equal(runnerSummary({ ...s, active: [], waiting: [] }), '쉬는 중');
  assert.equal(runnerSummary({ ...s, active: [], waiting: [], settings: { ...s.settings, autoPickup: true } }), '집을 일을 기다려요');
  assert.equal(runnerSummary({ ...s, claudeAvailable: false }), 'claude를 못 찾았어요');

  await refreshRunner(fakeHost({ runner_status: new Error('Command runner_status not found') }).call);
  s = getRunnerSnapshot();
  assert.equal(s.supported, false);
  assert.equal(s.error, '');
  // 열린 vault가 없는 오류는 지원 여부를 바꾸지 않고 오류 글만 남긴다
  await refreshRunner(fakeHost({ runner_status: 'vault가 아직 열리지 않았어요' instanceof Error ? 0 : new Error('vault가 아직 열리지 않았어요') }).call);
  s = getRunnerSnapshot();
  assert.equal(s.supported, true);
  assert.match(s.error, /vault/);
  await refreshRunner(fakeHost({ runner_status: 5 }).call);
  assert.match(getRunnerSnapshot().error, /읽지 못/);
  assert.equal(isMissingCommand(new Error('Command x not found')), true);
  assert.equal(isMissingCommand(new Error('디스크가 꽉 찼어요')), false);
});

test('runnerChanged raises the version so the task pages reload, then re-reads the status', async () => {
  resetRunner();
  const before = getRunnerSnapshot().version;
  const host = fakeHost({ runner_status: status({ running: 2 }) });
  await runnerChanged(host.call);
  assert.equal(getRunnerSnapshot().version, before + 1);
  assert.equal(getRunnerSnapshot().running, 2);
  // 상태를 읽을 때 레일의 「최근 실행」도 함께 읽는다(5건)
  assert.deepEqual(host.calls.map((c) => c[0]), ['runner_status', 'runner_recent_runs']);
  assert.deepEqual(host.calls[1], ['runner_recent_runs', { limit: RECENT_RUNS }]);
});

test('actions send the right host commands and keep the error text when they fail', async () => {
  resetRunner();
  const host = fakeHost({ runner_run: undefined, runner_stop: undefined, runner_answer: undefined, runner_settings_set: undefined, runner_status: status() });
  assert.equal(await runTask(host.call, 'TSK-1'), true);
  assert.equal(await stopTask(host.call, 'TSK-1'), true);
  assert.equal(await answerTask(host.call, 'TSK-1', 'main 으로'), true);
  assert.equal(await saveRunnerSettings(host.call, { autoPickup: true, concurrency: 2, model: 'sonnet' }), true);
  const sent = host.calls.filter((c) => c[0] !== 'runner_status' && c[0] !== 'runner_recent_runs');
  assert.deepEqual(sent, [
    ['runner_run', { taskKey: 'TSK-1' }],
    ['runner_stop', { taskKey: 'TSK-1' }],
    ['runner_answer', { taskKey: 'TSK-1', text: 'main 으로' }],
    ['runner_settings_set', { settings: { autoPickup: true, concurrency: 2, model: 'sonnet' } }],
  ]);
  const bad = fakeHost({ runner_run: 'Ready For Develop 상태에서만 실행할 수 있어요' && new Error('Ready For Develop 상태에서만 실행할 수 있어요(지금은 draft)') });
  assert.equal(await runTask(bad.call, 'TSK-1'), false);
  assert.match(getRunnerSnapshot().error, /Ready For Develop/);
  assert.equal(bad.calls.length, 1, 'a failed action does not re-read the status');
});

test('loadRuns returns runs and events, and an empty answer when the host fails', async () => {
  const host = fakeHost({ runner_runs: { runs: [run(), 5], events: [{ at: 1, step: '받기', text: '읽었다' }] } });
  const out = await loadRuns(host.call, 'TSK-1');
  assert.equal(out.runs.length, 1);
  assert.equal(out.events[0].text, '읽었다');
  assert.deepEqual(host.calls[0], ['runner_runs', { taskKey: 'TSK-1' }]);
  assert.deepEqual(await loadRuns(fakeHost({ runner_runs: new Error('x') }).call, 'TSK-1'), { runs: [], events: [] });
  assert.deepEqual(await loadRuns(fakeHost({ runner_runs: 7 }).call, 'TSK-1'), { runs: [], events: [] });
});

const snapshot = (extra = {}) => ({ ...status(), loaded: true, supported: true, version: 0, error: '', ...extra });

test('the runner bar shows the state, the auto-pickup switch and the concurrency choices', () => {
  const idle = renderToStaticMarkup(React.createElement(RunnerBar, { call: async () => {}, preset: snapshot() }));
  assert.match(idle, /aria-label="러너"/);
  assert.match(idle, /runner-pill is-idle/);
  assert.match(idle, /러너 · 쉬는 중/);
  assert.match(idle, /Ready가 되면 스스로 집기/);
  assert.doesNotMatch(idle, /checked=""/, 'auto pickup starts off');
  for (const n of [1, 2, 3, 4, 5, 6]) assert.match(idle, new RegExp(`<option value="${n}"[^>]*>${n}개`));
  assert.match(idle, /<option value="3" selected="">3개/);

  const busy = renderToStaticMarkup(React.createElement(RunnerBar, { call: async () => {}, preset: snapshot({ active: [run()], waiting: [run({ runId: 'r2', taskKey: 'TSK-2', phase: 'waiting-answer' })], settings: { autoPickup: true, concurrency: 2, model: 'sonnet' } }) }));
  assert.match(busy, /runner-pill is-busy/);
  assert.match(busy, /일하는 중 1 · 답 기다림 1/);
  assert.match(busy, /checked=""/);
  assert.match(busy, /<option value="2" selected="">2개/);

  const noClaude = renderToStaticMarkup(React.createElement(RunnerBar, { call: async () => {}, preset: snapshot({ claudeAvailable: false, settings: { autoPickup: true, concurrency: 3, model: 'sonnet' } }) }));
  assert.match(noClaude, /runner-pill is-warn/);
  assert.match(noClaude, /자동 조사에 쓸 Claude/);

  const withError = renderToStaticMarkup(React.createElement(RunnerBar, { call: async () => {}, preset: snapshot({ error: '동시에 돌릴 수 있는 수에 닿았어요' }) }));
  assert.match(withError, /role="alert"[^>]*>동시에 돌릴 수 있는 수에 닿았어요/);

  assert.equal(renderToStaticMarkup(React.createElement(RunnerBar, { call: async () => {}, preset: snapshot({ supported: false }) })), '', 'builds without a runner draw nothing');
});

const detailRaw = (extra = {}) => ({
  key: 'TSK-1', id: 'tkx-1', title: 'README 정리', type: 'feature', priority: 'medium', status: 'ready-for-develop', repo: 'o/r', labels: [], criteriaDone: 0, criteriaTotal: 1,
  blockedBy: [], blocks: [], path: '.aisurada/tasks/TSK-1.md', body: '', criteria: [{ text: '기준', done: false }], refs: [], links: [], readiness: [], ready: true,
  transitions: [], activity: [], precondition: 'f1:x', createdAt: '2026-11-13T14:00:00+09:00', editedAt: '2026-11-13T14:00:00+09:00', ...extra,
});
const detail = (extra = {}) => toDetail(detailRaw(extra));
const fakeController = (call = async () => ({})) => ({ navigate() {}, callEngine: async () => ({}), callHost: call, isNative: () => true });
const panel = (task, preset) => renderToStaticMarkup(React.createElement(RunnerPanel, { task, controller: fakeController(), preset }));

test('the panel offers development from 「지금 실행」 without requiring Claude', () => {
  const off = panel(detail(), { snapshot: snapshot(), runs: [] });
  assert.match(off, /aria-label="실행"/);
  assert.match(off, /<button type="button" class="button-primary">지금 실행<\/button>/);
  assert.match(off, /오케스트레이터가 개발·검사·리뷰를 진행하고 PR을 열어요/);
  const on = panel(detail(), { snapshot: snapshot({ settings: { autoPickup: true, concurrency: 3, model: 'sonnet' } }), runs: [] });
  assert.match(on, /진행하고 PR을 열어요/);
  const noClaude = panel(detail(), { snapshot: snapshot({ claudeAvailable: false }), runs: [] });
  assert.doesNotMatch(noClaude, /disabled=""[^>]*>지금 실행/);
  // 다른 상태이고 실행도 없으면 영역 자체를 그리지 않는다
  assert.equal(panel(detail({ status: 'draft' }), { snapshot: snapshot(), runs: [] }), '');
  assert.equal(panel(detail(), { snapshot: snapshot({ supported: false }), runs: [] }), '');
});

test('the panel shows the seven steps with the current one, the elapsed time, and 「중지」 while running', () => {
  const html = panel(detail({ status: 'in-progress' }), { snapshot: snapshot({ active: [run({ step: '일하기', startedAt: 1_000_000, turns: 2 })] }), runs: [run({ step: '일하기' })], now: 1_000_000 + 192_000 });
  for (const s of STEPS) assert.match(html, new RegExp(`>${s}</li>`));
  assert.match(html, /class="is-done"[^>]*>받기</);
  assert.match(html, /class="is-current" aria-current="step">일하기</);
  assert.doesNotMatch(html, /class="is-done"[^>]*>정리</);
  assert.match(html, /3분 12초째/);
  assert.match(html, /모델 2번 호출/);
  assert.match(html, />중지<\/button>/);
  assert.doesNotMatch(html, /지금 실행/);
});

test('the panel shows the runner question with an answer box when the task waits for an answer', () => {
  const waiting = run({ runId: 'r2', phase: 'waiting-answer', step: '내보내기', question: '어느 브랜치를 기준으로 볼까요?' });
  const html = panel(detail({ status: 'waiting-answer' }), { snapshot: snapshot({ waiting: [waiting] }), runs: [waiting] });
  assert.match(html, /러너가 답을 기다려요/);
  assert.match(html, /<blockquote>어느 브랜치를 기준으로 볼까요\?<\/blockquote>/);
  assert.match(html, /aria-label="질문에 대한 답"/);
  assert.match(html, /disabled=""[^>]*>답하기/, 'an empty answer cannot be sent');
  assert.match(html, /진행하던 작업을 이어가요/);
  assert.match(html, />중지<\/button>/);
});

test('the panel lists past runs with result, time, tokens, cost, summary, errors and the event log', () => {
  const runs = [
    run({ runId: 'a', phase: 'done', workedSecs: 9, summary: 'README에 빌드 방법이 있다', questions: 1 }),
    run({ runId: 'b', phase: 'failed', error: 'claude가 종료 코드 1로 끝났다: boom', startedAt: 500_000 }),
    run({ runId: 'c', phase: 'cancelled', startedAt: 400_000 }),
  ];
  const events = [{ at: 1, step: '받기', text: 'TSK-1을 읽었다' }, { at: 2, step: '일하기', text: 'claude에게 조사를 맡겼다' }];
  const html = panel(detail({ status: 'in-review' }), { snapshot: snapshot(), runs, events, openEvents: true });
  assert.match(html, /<h3>지난 실행<\/h3>/);
  assert.match(html, /runner-phase runner-phase-done">보고함/);
  assert.match(html, /runner-phase runner-phase-failed">실패/);
  assert.match(html, /runner-phase runner-phase-cancelled">중지함/);
  assert.match(html, /9초 · 질문 1개 · 토큰 130\.3k\/1\.5k · 0\.01달러 미만/);
  assert.match(html, /README에 빌드 방법이 있다/);
  assert.match(html, /claude가 종료 코드 1로 끝났다: boom/);
  assert.match(html, /<details open="">/);
  assert.match(html, /진행 기록 2/);
  assert.match(html, /runner-event-step">받기<\/span> TSK-1을 읽었다/);
  // 진행 기록은 가장 최근 실행에만 붙는다
  assert.equal((html.match(/진행 기록/g) ?? []).length, 1);
});

test('the detail page and the board embed the runner pieces', () => {
  const state = { workspace: { path: '/vault', name: 'v', sample: false, today: '2026-11-13', notes: [], templates: [], trash: [], skipped: [], conflicts: [], noteReadFailed: false, agendas: [], projects: [] }, prefs: { collapsedProjects: [] }, hiddenAgendas: [], theme: 'dark', route: '/task/TSK-1' };
  const page = renderToStaticMarkup(React.createElement(TaskDetailPage, { taskKey: 'TSK-1', state, controller: fakeController(), preset: { task: detail(), runner: { snapshot: snapshot(), runs: [] } } }));
  assert.match(page, /aria-label="실행"/);
  assert.ok(page.indexOf('aria-label="실행"') < page.indexOf('aria-label="설명"'), 'the runner area comes first in the main column');

  const rows = [
    toTask(detailRaw({ id: 'tk1', title: '준비된 일', priority: 'high' })),
    toTask(detailRaw({ key: 'TSK-2', id: 'tk2', title: '막힌 일', blockedBy: ['TSK-9'] })),
    toTask(detailRaw({ key: 'TSK-3', id: 'tk3', title: '하는 중', status: 'in-progress' })),
  ];
  const runner = snapshot({ active: [run({ taskKey: 'TSK-3', step: '받아오기' })] });
  const board = renderToStaticMarkup(React.createElement(TasksPage, { state: { ...state, route: '/tasks' }, controller: fakeController(), preset: { tasks: rows, view: 'board', runner } }));
  assert.match(board, /aria-label="러너"/);
  assert.match(board, /data-task-key="TSK-1"[\s\S]*?▶ 지금 실행/);
  assert.equal((board.match(/▶ 지금 실행/g) ?? []).length, 1, 'a blocked task has no run button');
  assert.match(board, /러너 · 받아오기/);
  assert.match(board, /■ 중지/);
  assert.match(board, /자동으로 집은 태스크는 Claude가 읽기 전용으로 조사해요/);
});

test('refreshRunner also reads the recent runs for the rail and keeps the old list when that read fails', async () => {
  resetRunner();
  const a = run({ runId: 'a', taskKey: 'TSK-2', phase: 'done', endedAt: 2_000_000 });
  await refreshRunner(fakeHost({ runner_status: status(), runner_recent_runs: [a, 5, null] }).call);
  assert.deepEqual(getRunnerSnapshot().recent.map((r) => r.runId), ['a'], 'rows that are not records are dropped');
  // 못 읽으면(오류, 배열이 아닌 답) 이전 목록을 그대로 둔다. 러너 상태 읽기는 성공으로 친다
  await refreshRunner(fakeHost({ runner_status: status({ running: 1 }), runner_recent_runs: new Error('x') }).call);
  assert.equal(getRunnerSnapshot().running, 1);
  assert.deepEqual(getRunnerSnapshot().recent.map((r) => r.runId), ['a']);
  await refreshRunner(fakeHost({ runner_status: status(), runner_recent_runs: 7 }).call);
  assert.deepEqual(getRunnerSnapshot().recent.map((r) => r.runId), ['a']);
  assert.equal(getRunnerSnapshot().error, '');
});

test('loadRecentRuns asks for the limit and taskOfSession finds the task whose run opened a session', async () => {
  const host = fakeHost({ runner_recent_runs: [run({ runId: 'x', taskKey: 'TSK-3', sessionId: 'sess-3' }), run({ runId: 'y', taskKey: 'TSK-4', sessionId: '' })] });
  assert.equal((await loadRecentRuns(host.call, 7)).length, 2);
  assert.deepEqual(host.calls[0], ['runner_recent_runs', { limit: 7 }]);
  assert.equal(await taskOfSession(host.call, 'sess-3'), 'TSK-3');
  assert.equal(await taskOfSession(host.call, 'other'), null);
  assert.equal(await taskOfSession(host.call, ''), null, 'an empty session id never matches a run that has no session yet');
  assert.equal(await loadRecentRuns(fakeHost({ runner_recent_runs: new Error('x') }).call, 5), null);
  assert.equal(await loadRecentRuns(fakeHost({ runner_recent_runs: {} }).call, 5), null);
});

test('rail row helpers: time words, tone, subline and the hover tip', () => {
  const NOW = 100 * 86_400_000;
  assert.deepEqual(
    [20e3, 3 * 60e3, 3600e3, 26 * 3600e3, 2 * 86_400e3, 65 * 86_400e3].map((ago) => sinceKo(NOW - ago, NOW)),
    ['방금', '3분 전', '1시간 전', '어제', '2일 전', '2달 전'],
  );
  const r = (extra) => run({ phase: 'done', endedAt: NOW - 3600e3, costUsd: 0.03, ...extra });
  assert.deepEqual(
    ['running', 'waiting-answer', 'done', 'failed', 'cancelled', 'abandoned'].map((phase) => runTone(r({ phase }))),
    ['busy', 'wait', 'ok', 'err', 'off', 'off'],
  );
  // 아랫줄: 「상태 · 때 · 비용 · 대화 없음」. 끝나지 않은 실행에는 비용을 적지 않는다
  assert.equal(runSubline(r({ phase: 'running', endedAt: null, startedAt: NOW - 3 * 60e3 }), NOW), '일하는 중 · 3분째');
  assert.equal(runSubline(r({ phase: 'running', endedAt: null, startedAt: NOW - 20e3 }), NOW), '일하는 중 · 방금 시작');
  assert.equal(runSubline(r({ phase: 'waiting-answer', endedAt: null, startedAt: NOW - 14 * 60e3 }), NOW), '답을 기다림 · 14분 전');
  assert.equal(runSubline(r({}), NOW), '보고함 · 1시간 전 · 0.03달러');
  assert.equal(runSubline(r({ phase: 'failed', costUsd: 0, sessionId: '', endedAt: NOW - 26 * 3600e3 }), NOW), '실패 · 어제 · 대화 없음');
  // 마우스를 올리면: 걸음, 질문, 보고 앞부분, 실패 까닭
  assert.equal(runTip(r({ phase: 'running', step: '일하기' })), '일하는 중 · 일하기 걸음 (5/7)');
  assert.equal(runTip(r({ phase: 'running', step: '모르는걸음' })), '일하는 중 · 모르는걸음 걸음');
  assert.equal(runTip(r({ phase: 'waiting-answer', question: '어느 브랜치?' })), '러너의 질문: 어느 브랜치?');
  assert.equal(runTip(r({ summary: '세션 만료가 원인' })), '보고 앞부분: 세션 만료가 원인');
  assert.equal(runTip(r({})), '보고함');
  assert.equal(runTip(r({ phase: 'failed', error: '저장소를 받지 못했어요' })), '실패 까닭: 저장소를 받지 못했어요');
  assert.equal(runTip(r({ phase: 'failed' })), '실패 까닭: 알 수 없어요');
  assert.equal(runTip(r({ phase: 'cancelled' })), '사람이 중지했어요');
  assert.match(runTip(r({ phase: 'abandoned' })), /^태스크를 손으로/);
});

test('the panel offers 「대화 보기」 on a past run only when it has a session', () => {
  const html = panel(detail({ status: 'in-review' }), { snapshot: snapshot(), runs: [
    run({ runId: 'a', phase: 'done', sessionId: 'abc', endedAt: 2_000_000 }),
    run({ runId: 'b', phase: 'failed', sessionId: '', error: '저장소를 받지 못했어요', startedAt: 500_000, endedAt: 600_000 }),
  ] });
  assert.equal((html.match(/대화 보기 ›/g) ?? []).length, 1, 'a run without a session has no chat to open');
  assert.match(html, /class="runner-chat" title="이 실행의 대화를 읽기 전용으로 열어요">대화 보기 ›/);
});

test('the rail lists the recent runs with tone, subline, tip and the open task marked; the empty state explains itself', () => {
  const NOW = 100 * 86_400_000;
  const rail = (taskKey) => renderToStaticMarkup(React.createElement(RailRuns, { controller: fakeController(), taskKey, now: NOW }));
  resetRunner({ recent: [
    run({ runId: 'a', taskKey: 'TSK-14', phase: 'running', startedAt: NOW - 3 * 60e3 }),
    run({ runId: 'b', taskKey: 'TSK-12', phase: 'waiting-answer', startedAt: NOW - 14 * 60e3, question: 'main인가요?' }),
    run({ runId: 'c', taskKey: 'TSK-9', phase: 'failed', sessionId: '', endedAt: NOW - 26 * 3600e3, error: '저장소를 받지 못했어요', costUsd: 0 }),
  ] });
  const html = rail('TSK-12');
  assert.match(html, /aria-label="최근 실행"/);
  assert.match(html, /<small>3<\/small>/);
  assert.match(html, /class="rr t-busy"[\s\S]*?<b>TSK-14<\/b>[\s\S]*?일하는 중 · 3분째/);
  assert.match(html, /class="rr t-wait is-open"/, 'the row of the task that is open is marked');
  assert.match(html, /title="러너의 질문: main인가요\?"/);
  assert.match(html, /class="rr t-err no-chat"/, 'a run without a session is dimmed');
  assert.match(html, /실패 · 어제 · 대화 없음/);
  assert.doesNotMatch(rail(undefined), /is-open/);
  resetRunner({ recent: [] });
  assert.match(rail(undefined), /아직 러너가 돌린 실행이 없어요/);
  assert.match(rail(undefined), /<small>0<\/small>/);
  resetRunner();
});
