import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToString } from 'react-dom/server';
import { SessionRoute, TranscriptBody } from '../src/plugs/claude/session.tsx';
import {
  agentBarText,
  formatBytes,
  formatDuration,
  inlineCode,
  kindLabel,
  normalizeReadSession,
  pinBody,
  pinDefaultTitle,
  pinFence,
  pinScanText,
  statusCell,
  todoBarText,
} from '../src/plugs/claude/sessionLogic.ts';

// What the engine really answers (7.7): source.bytes, source.lines, source.skipped, source.messages and who-shaped turns.
const engineAnswer = {
  sessionId: 'f1a50000-0000-4000-8000-000000000001',
  file: '/Users/me/.claude/projects/-Users-me/f1a50000-0000-4000-8000-000000000001.jsonl',
  source: { bytes: 46410, lines: 1200, skipped: 2, messages: 380 },
  hasMore: true,
  urls: [{ url: 'https://example.com/a', count: 3 }],
  runningAgents: [{ what: '레일폭조사', at: '2026-09-20T01:00:02Z' }, {}],
  todos: [{ number: 1, title: '레일붙이기', status: 'completed' }, { number: 2, title: '검사', status: 'in_progress' }],
  runState: { model: 'claude-sonnet-5-5', effort: 'low', contextTokens: 40120, quotaBlocked: true, quotaResetsAt: 1789000100 },
  turns: [
    { index: 0, who: 'user', at: '2026-09-20T01:00:00Z', chars: 12, truncated: false, text: '캐시 정리해 줘' },
    { index: 1, who: 'tool', at: '2026-09-20T01:00:01Z', chars: 0, truncated: false, tool: { name: 'Bash', arg: 'ls -la', seconds: 1.5 }, text: '' },
    { index: 2, who: 'assistant', at: '2026-09-20T01:00:02Z', chars: 8, truncated: false, text: '정리했어요.' },
  ],
};

test('9.4 H3: the screen reads the engine answer as it is', () => {
  const view = normalizeReadSession(engineAnswer);
  assert.deepEqual(view.source, { bytes: 46410, lines: 1200, skipped: 2, messages: 380 });
  assert.equal(view.turns.length, 3);
  assert.equal(view.turns[1].tool.name, 'Bash');
  assert.deepEqual(view.runningAgents[1], {});
  assert.equal(view.todos[0].title, '레일붙이기', 'todos carry title, not content');
  assert.equal(view.runState.contextTokens, 40120);
  assert.equal(normalizeReadSession({ file: null, turns: [] }).file, null);
  assert.equal(normalizeReadSession(null).turns.length, 0);
});

test('9.4: formatDuration and formatBytes', () => {
  assert.equal(formatDuration(0.8), '0.8초');
  assert.equal(formatDuration(59.9), '59.9초');
  assert.equal(formatDuration(60), '1분');
  assert.equal(formatDuration(3600), '1.0시간');
  assert.equal(formatBytes(500), '500B');
  assert.equal(formatBytes(2048), '2KB');
  assert.equal(formatBytes(1048576 * 1.5), '1.5MB');
});

test('9.3 M3: an ended row says how long ago it ended; a list row says its run state', () => {
  const now = Date.now();
  assert.deepEqual(statusCell({ status: 'unknown', isEndedRow: true, endedAt: now - 3 * 3600_000 }, now), { text: '3시간 전에 끝남', ended: true });
  assert.equal(statusCell({ status: 'running' }, now).text, '도는 중');
  assert.equal(statusCell({ status: 'blocked' }, now).text, '멈춤');
  assert.equal(statusCell({ status: 'idle' }, now).text, '쉬는 중');
  assert.equal(statusCell({ status: 'unknown' }, now).text, '모름');
  assert.equal(kindLabel('background'), '백그라운드');
});

test('9.5 D26: the agent bar says "도는 중" only for a session whose pid is alive', () => {
  assert.equal(agentBarText(2, true), '서브에이전트 2개가 도는 중');
  assert.equal(agentBarText(2, false), '결과가 안 온 서브에이전트 2개, 지금 도는지는 몰라요');
  assert.equal(todoBarText([{ status: 'completed' }, { status: 'in_progress' }]), '할 일 2개 가운데 1개가 끝났어요');
});

test('9.8 D29: inline code uses a fence one longer than the longest run and pads where the manual says', () => {
  assert.equal(inlineCode('git status'), '`git status`');
  assert.equal(inlineCode('echo `date`'), '`` echo `date` ``');
  assert.equal(inlineCode('echo ``x``'), '``` echo ``x`` ```');
  assert.equal(inlineCode('a\r\nb\nc\rd'), '`a b c d`');
  assert.equal(inlineCode(''), '`  `', 'an empty text is padded');
  assert.equal(inlineCode(' x '), '`  x  `', 'a text that starts and ends with a space is padded');
  assert.equal(inlineCode(' x'), '` x`');
});

test('9.8 H5 M12: the pinned note body', () => {
  const turns = [
    { index: 0, who: 'user', text: '프로젝트 상태 확인해줘' },
    { index: 1, who: 'assistant', text: '```\n코드\n```\n끝' },
    { index: 2, who: 'tool', tool: { name: 'Bash', arg: 'echo `date`' }, text: '' },
    { index: 3, who: 'tool', text: '' },
  ];
  assert.equal(pinFence(turns), '````', 'one longer than the longest run of any spoken turn, never under 3');
  assert.equal(pinFence([{ who: 'user', text: 'a' }]), '```');
  const body = pinBody({ title: '세션 대화 12:30', sessionId: 'f1a5', turns, source: { bytes: 2048, lines: 12748, messages: 1312 }, shapeFindings: 0 });
  const lines = body.split('\n');
  assert.equal(lines[0], '# 세션 대화 12:30');
  assert.equal(lines[1], '');
  assert.equal(lines[2], 'Claude Code 세션 대화를 굳힌 노트입니다. 도구 결과 본문은 안 담아서 자격증명이 안 실립니다.');
  assert.deepEqual(lines.slice(4, 10), ['| 무엇 | 값 |', '| --- | --- |', '| 세션 id | `f1a5` |', '| 담은 회차 | 4 |', '| 원본 | 2KB, 12,748줄에서 1,312건 |', '| 모양으로 걸린 것 | 0 건 |']);
  assert.equal(lines[11], '> [!success] 모양 검사에 걸린 것이 0 건입니다', 'the callout is there at zero, with a space before 건');
  assert.equal(lines.indexOf('## 대화') > 8, true);
  assert.ok(body.includes('**나**\n\n````\n프로젝트 상태 확인해줘\n````\n'));
  assert.ok(body.includes('- `Bash` — `` echo `date` ``'));
  assert.equal(body.includes('- ``\n'), false, 'a tool turn without tool adds no line');
  const danger = pinBody({ title: 't', sessionId: 's', turns: [], source: { bytes: 0, lines: 0, messages: 0 }, shapeFindings: 2, shapeLines: ['토큰 2 건'] });
  assert.ok(danger.includes('> [!danger] 표에 없는데 비밀처럼 생긴 것 2 건\n> 토큰 2 건\n> 이 검사는 도구 결과 본문은 보지 못해요.'));
  assert.equal(pinScanText(turns), '프로젝트 상태 확인해줘\n```\n코드\n```\n끝\nBash\necho `date`\n\n');
  assert.match(pinDefaultTitle(new Date(2026, 10, 3, 9, 5)), /^세션 대화 09:05$/);
});

test('9.4: the four states of the conversation column', () => {
  const html = (read) => renderToString(React.createElement(TranscriptBody, { read })).replace(/<!-- -->/g, '');
  assert.match(html({ view: null, loading: true, error: null }), /대화 기록을 읽는 중이에요/);
  assert.match(html({ view: null, loading: false, error: 'boom' }), /동기화 엔진이 안 떠 있을 수 있어요/);
  assert.equal(html({ view: null, loading: false, error: 'boom' }).includes('boom'), true, 'the failure text is in the tooltip only');
  assert.match(html({ view: normalizeReadSession({ file: null }), loading: false, error: null }), /이 기계에 그 세션의 기록이 없어요/);
  assert.match(html({ view: normalizeReadSession({ file: '/x', turns: [] }), loading: false, error: null }), /기록은 있는데 주고받은 말이 없어요/);
  const full = html({ view: normalizeReadSession(engineAnswer), loading: false, error: null });
  assert.match(full, /45KB 중에서 골랐어요 · 1,200줄에서 380건/);
  assert.match(full, /<strong> · 못 읽은 줄 2<\/strong>/);
  assert.match(full, /앞의 대화가 더 있어요. 지금은 최근 3건만 보여요./);
  assert.match(full, /class="toolrow"/);
  assert.match(full, /1\.5초/);
});

test('9.1: the session route draws its header, the three right rail tabs and the composer', () => {
  const html = renderToString(React.createElement(SessionRoute, { sessionId: 'f1a50000-0000-4000-8000-000000000001', state: { workspace: { path: '/v', sample: false, notes: [{ id: 'n1', title: '노트', sessionId: 'f1a50000-0000-4000-8000-000000000001', created: '2026-11-03T10:00:00+09:00' }], agendas: [] }, prefs: {} } }));
  assert.match(html, /class="sess-screen/);
  assert.match(html, /관련 노트/);
  assert.match(html, /이 세션이 만들었어요\./);
  assert.match(html, /aria-label="세션으로 보내기"/);
  assert.match(html, /이어 가기 명령 복사/);
  assert.match(html, /대화가 없으면 굳힐 것이 없어요\./);
});
