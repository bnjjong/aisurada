import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToString } from 'react-dom/server';
import {
  formatDuration,
  formatBytes,
  buildPinTranscriptMarkdown,
  SessionRoute,
} from '../src/plugs/claude/session.tsx';

test('Chapter 9: formatDuration & formatBytes helpers conform to specifications', () => {
  // Duration: 9.4
  assert.equal(formatDuration(0.8), '0.8초');
  assert.equal(formatDuration(59.9), '59.9초');
  assert.equal(formatDuration(60), '1분');
  assert.equal(formatDuration(120), '2분');
  assert.equal(formatDuration(3600), '1.0시간');
  assert.equal(formatDuration(7200), '2.0시간');

  // Bytes: 9.4
  assert.equal(formatBytes(500), '500B');
  assert.equal(formatBytes(2048), '2KB');
  assert.equal(formatBytes(1048576 * 1.5), '1.5MB');
});

test('Chapter 9 D29: buildPinTranscriptMarkdown constructs valid markdown structure', () => {
  const turns = [
    { who: 'user', text: '프로젝트 상태 확인해줘' },
    { who: 'assistant', text: '상태는 정상입니다.' },
    { who: 'tool', tools: [{ name: 'Bash', arg: 'git status' }] }
  ];

  // 1. 비밀 검사 0건인 경우
  const mdClean = buildPinTranscriptMarkdown({
    title: '세션대화 12:30',
    turns,
    findingsCount: 0,
  });
  assert(mdClean.startsWith('# 세션대화 12:30'), 'Title is first heading line');
  assert(mdClean.includes('[!success] 모양 검사에 걸린 것이 0건입니다.'), 'Clean callout included');
  assert(mdClean.includes('**나**\n\n```\n프로젝트 상태 확인해줘\n```'), 'User turn fenced');
  assert(mdClean.includes('**클로드**\n\n```\n상태는 정상입니다.\n```'), 'Assistant turn fenced');
  assert(mdClean.includes('- `Bash` — `git status`'), 'Tool inline code formatted');

  // 2. 비밀 검사 걸린 경우
  const mdWarn = buildPinTranscriptMarkdown({
    title: '세션대화 12:30',
    turns,
    findingsCount: 3,
  });
  assert(mdWarn.includes('[!danger] 표에 없는데 비밀처럼 생긴 것 3건'), 'Danger callout included');
});

test('Chapter 9: SessionRoute renders core layout conforming to 9.1 ~ 9.7', () => {
  const fakeState = {
    workspace: {
      path: '/vault',
      name: 'TestVault',
      notes: [
        { id: 'nd1', title: '연관 노트 1', sessionId: 'sess-001', created: '2026-11-03T01:00:00.000Z' }
      ],
      agendas: [
        { id: 'ag1', slug: 'demo', title: '데모 아젠다' }
      ],
    }
  };

  const html = renderToString(React.createElement(SessionRoute, {
    sessionId: 'sess-001',
    state: fakeState,
  }));

  assert(html.includes('class="sess-screen'), 'Container .sess-screen rendered');
  assert(html.includes('class="sess-page-header"'), 'Header .sess-page-header rendered');
  assert(html.includes('class="sess-page-title"'), 'Title .sess-page-title rendered');
  assert(html.includes('class="open-cli"'), 'Button .open-cli rendered');
  assert(html.includes('class="sess-page-resume"'), 'Button .sess-page-resume rendered');
  assert(html.includes('class="prev-toggle"'), 'Button .prev-toggle rendered');
  assert(html.includes('class="sess-page-talk"'), 'Talk area .sess-page-talk rendered');
  assert(html.includes('class="prevcol"'), 'Right rail .prevcol rendered');
  assert(html.includes('관련 노트'), 'Right rail has 관련 노트 tab');
  assert(html.includes('링크'), 'Right rail has 링크 tab');
  assert(html.includes('CLI'), 'Right rail has CLI tab');
  assert(html.includes('연관 노트 1'), 'Related note listed in notes tab');
  assert(html.includes('이 세션이 만들었어요.'), 'Related note why reason text rendered');
});
