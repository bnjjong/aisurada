import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToString } from 'react-dom/server';
import {
  formatTurnTime,
  buildTranscriptBundle,
  extractNoteHeading,
  buildRailPersona,
  NoteSessionRail,
} from '../src/plugs/claude/session.tsx';

test('Chapter 8 D22: formatTurnTime formats to local YYYY-MM-DD HH:mm without UTC raw slice', () => {
  const utcIso = '2026-11-03T03:00:00.000Z';
  const formatted = formatTurnTime(utcIso);
  
  // 기기 시간대로 변환되므로 로컬 Date 객체의 연월일시분과 일치해야 함
  const d = new Date(utcIso);
  const expectedY = d.getFullYear();
  const expectedM = String(d.getMonth() + 1).padStart(2, '0');
  const expectedD = String(d.getDate()).padStart(2, '0');
  const expectedH = String(d.getHours()).padStart(2, '0');
  const expectedMin = String(d.getMinutes()).padStart(2, '0');
  const expected = `${expectedY}-${expectedM}-${expectedD} ${expectedH}:${expectedMin}`;

  assert.equal(formatted, expected, 'Time formatted in local timezone');

  // fallback for invalid string
  const invalid = 'invalid-time-string';
  assert.equal(formatTurnTime(invalid), 'invalid-time-str', 'Fallback slices first 16 chars');
});

test('Chapter 8 D20: buildTranscriptBundle & buildRailPersona behavior when turns empty vs present', () => {
  const sessionId = 'f1a50000-0000-4000-8000-000000000001';

  // 1. 회차가 없을 때 (empty)
  const emptyRes = buildTranscriptBundle(sessionId, []);
  assert.equal(emptyRes.bundle, '', 'Empty turns must produce empty bundle');

  const personaEmpty = buildRailPersona({
    noteMarkdown: '# 테스트 노트\n본문 내용',
    noteId: 'ndtest-00000001',
    sessionId,
    hasTranscript: false,
  });
  assert(personaEmpty.includes('그 세션의 대화는 실리지 않았다.'), 'D20: Second line ends with notice that transcript is not included');
  assert(personaEmpty.includes('노트는 「테스트 노트」(ndtest-00000001)이다.'), 'Note title and ID included');
  assert(personaEmpty.includes("wl-cli get_doc '{\"id\":\"ndtest-00000001\"}'"), 'get_doc command included');
  assert(personaEmpty.includes('wl-cli update_doc'), 'update_doc command included');

  // 2. 회차가 있을 때 (present)
  const sampleTurns = [
    { turnId: '1', user: '안녕하세요', assistant: '반갑습니다', tools: [{ name: 'Read' }, { name: 'Read' }] },
    { turnId: '2', user: '작업 현황은?', assistant: '작업 중입니다' }
  ];
  const presentRes = buildTranscriptBundle(sessionId, sampleTurns);
  assert(presentRes.bundle.includes('<session-transcript>'), 'Contains <session-transcript> tag');
  assert(presentRes.bundle.includes('사용자: 안녕하세요'), 'Contains user turn');
  assert(presentRes.bundle.includes('클로드: 반갑습니다'), 'Contains assistant turn');
  assert(presentRes.bundle.includes('(도구: Read×2)'), 'Tools formatted with count');

  const personaPresent = buildRailPersona({
    noteMarkdown: '# 테스트 노트\n본문 내용',
    noteId: 'ndtest-00000001',
    sessionId,
    hasTranscript: true,
  });
  assert(personaPresent.includes('첫 물음 앞에 그 세션의 최근 대화가 실려 있다.'), 'D20: Second line ends with notice that transcript is included');
});

test('Chapter 8: extractNoteHeading extracts first # heading ignoring comments', () => {
  const mdWithComments = '<!-- comment -->\n# 실제 제목\n## 부제목';
  assert.equal(extractNoteHeading(mdWithComments), '실제 제목');

  const mdNoHeading = '그냥 본문 텍스트입니다.';
  assert.equal(extractNoteHeading(mdNoHeading), '');
});

test('Chapter 8: NoteSessionRail renders rail structure conforming to 8.3 and 8.15', () => {
  const sessionId = 'f1a50000-0000-4000-8000-000000000001';
  const html = renderToString(React.createElement(NoteSessionRail, {
    sessionId,
    chat: {
      target: { noteId: 'ndm1abcd-00000001' },
      noteMarkdown: '# 내 노트\n내용',
    }
  }));

  assert(html.includes('class="nsrail"'), 'Contains .nsrail element');
  assert(html.includes('data-at-min="true"'), 'Has data-at-min at minimum default width 392px');
  assert(html.includes('class="nsrail-resizer"'), 'Contains .nsrail-resizer handle');
  assert(html.includes('role="separator"'), 'Resizer has role="separator"');
  assert(html.includes('aria-label="세션 레일 닫기"'), 'Contains close button with aria-label');
  assert(html.includes('class="rchat"'), 'Contains .rchat section');
  assert(html.includes('aria-label="새 대화로 묻기"'), 'Composer textarea has correct aria-label');
  assert(html.includes('class="rchat-options"'), 'Contains model & effort options row');
});
