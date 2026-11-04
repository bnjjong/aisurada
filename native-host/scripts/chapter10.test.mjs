import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToString } from 'react-dom/server';
import {
  SessionComposer,
  SessionCliPane,
  getSessionComposerState,
} from '../src/plugs/claude/session.tsx';

test('Chapter 10: SessionComposer blocks rendering based on 10.1 decision rules', () => {
  const sessionId = '3f2a9c4e-1b2d-4c3e-8f40-00000000a001';

  // 1. live-terminal: 살아있는 대화형 터미널 세션인 경우 차단
  const htmlTerminal = renderToString(
    React.createElement(SessionComposer, {
      sessionId,
      sessionData: { turns: [] },
      sessionItem: { sessionId, processAlive: true, kind: 'terminal' },
      sessionLoaded: true,
      transcriptFile: '/tmp/session.jsonl',
      state: {},
      onSent: () => {},
    })
  );
  assert(
    htmlTerminal.includes('class="sess-composer-blocked"'),
    'Blocked container rendered for live terminal'
  );
  assert(
    htmlTerminal.includes('data-block="live-terminal"'),
    'data-block attribute set to live-terminal'
  );
  assert(
    htmlTerminal.includes('이 세션은 다른 터미널에서 열려 있어요.'),
    'Contains live-terminal warning text'
  );

  // 2. live-background: 살아있는 백그라운드 세션인 경우 차단
  const htmlBackground = renderToString(
    React.createElement(SessionComposer, {
      sessionId,
      sessionData: { turns: [] },
      sessionItem: { sessionId, processAlive: true, kind: 'background' },
      sessionLoaded: true,
      transcriptFile: '/tmp/session.jsonl',
      state: {},
      onSent: () => {},
    })
  );
  assert(
    htmlBackground.includes('data-block="live-background"'),
    'data-block attribute set to live-background'
  );
  assert(
    htmlBackground.includes('이 세션은 백그라운드에서 돌고 있어요.'),
    'Contains live-background warning text'
  );

  // 3. no-record: 대화를 읽었으나 기록 파일이 없는 경우 차단
  const htmlNoRecord = renderToString(
    React.createElement(SessionComposer, {
      sessionId,
      sessionData: { turns: [], file: null },
      sessionItem: { sessionId, processAlive: false },
      sessionLoaded: true,
      transcriptFile: null,
      state: {},
      onSent: () => {},
    })
  );
  assert(
    htmlNoRecord.includes('data-block="no-record"'),
    'data-block attribute set to no-record'
  );
  assert(
    htmlNoRecord.includes('이 기계에 이 세션의 대화 기록이 없어서 이어 쓸 수 없어요.'),
    'Contains no-record text'
  );

  // 4. 정상 세션: 입력창 활성화
  const htmlNormal = renderToString(
    React.createElement(SessionComposer, {
      sessionId,
      sessionData: { turns: [], file: '/tmp/session.jsonl' },
      sessionItem: { sessionId, processAlive: false },
      sessionLoaded: true,
      transcriptFile: '/tmp/session.jsonl',
      state: {},
      onSent: () => {},
    })
  );
  assert(htmlNormal.includes('class="sess-composer"'), 'Composer form rendered');
  assert(
    htmlNormal.includes('aria-label="세션으로 보내기"'),
    'Composer textarea has aria-label="세션으로 보내기"'
  );
  assert(htmlNormal.includes('class="sess-composer-send"'), 'Send button rendered');
  assert(htmlNormal.includes('class="sess-composer-mic"'), 'Mic button rendered');
});

test('Chapter 10: SessionComposer state persistence per session (10.2)', () => {
  const sidA = 'aaaaaaaa-1111-4000-8000-000000000001';
  const sidB = 'bbbbbbbb-2222-4000-8000-000000000002';

  const stateA = getSessionComposerState(sidA);
  stateA.text = '작업하던 글 A';
  stateA.notice = '알림 A';

  const stateB = getSessionComposerState(sidB);
  assert.equal(stateB.text, '', 'State B is initially empty');

  // Re-read A
  const stateA2 = getSessionComposerState(sidA);
  assert.equal(stateA2.text, '작업하던 글 A', 'State A preserved across lookups');
  assert.equal(stateA2.notice, '알림 A', 'Notice preserved');
});

test('Chapter 10: SessionCliPane conforms to 10.9 & 10.13 terminal pane contracts', () => {
  const sessionId = '3f2a9c4e-1b2d-4c3e-8f40-00000000a001';

  // 1. 다른 터미널에서 돌고 있는 대화형 세션인 경우 차단
  const htmlBlocked = renderToString(
    React.createElement(SessionCliPane, {
      sessionId,
      sessionCwd: '/tmp/cwd',
      sessionItem: { sessionId, processAlive: true, kind: 'terminal' },
    })
  );
  assert(
    htmlBlocked.includes('data-tab-key="term:3f2a9c4e-1b2d-4c3e-8f40-00000000a001"'),
    'data-tab-key attribute matches term:<sessionId>'
  );
  assert(
    htmlBlocked.includes('class="term-empty"'),
    'term-empty container rendered'
  );
  assert(
    htmlBlocked.includes('이 세션은 지금 다른 터미널에서 돌고 있어요.'),
    'Contains 10.9-3 blocked message'
  );

  // 2. 정상 세션의 CLI 패널
  const htmlNormal = renderToString(
    React.createElement(SessionCliPane, {
      sessionId,
      sessionCwd: '/tmp/cwd',
      sessionItem: { sessionId, processAlive: false },
    })
  );
  assert(
    htmlNormal.includes('data-tab-key="term:3f2a9c4e-1b2d-4c3e-8f40-00000000a001"'),
    'data-tab-key present'
  );
  assert(
    htmlNormal.includes('class="prev-pane is-cli term-host"'),
    'Rendered with .prev-pane.is-cli.term-host'
  );
  assert(htmlNormal.includes('class="term-screen"'), 'term-screen rendered');
});
