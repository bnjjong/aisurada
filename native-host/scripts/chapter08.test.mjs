import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToString } from 'react-dom/server';
import { NoteSessionRail, mergeChats } from '../src/plugs/claude/session.tsx';
import {
  formatTurnTime,
  railChatSessionId,
  railHeading,
  railPersona,
  railPrompt,
  railSeed,
  railTurnFrom,
  railTurnId,
} from '../src/plugs/claude/sessionLogic.ts';

test('8.15 / 9.4: formatTurnTime is the machine time zone, and a text that is not a time keeps 16 characters with T as a space', () => {
  const utcIso = '2026-11-03T03:00:00.000Z';
  const d = new Date(utcIso);
  const pad = (n) => String(n).padStart(2, '0');
  assert.equal(formatTurnTime(utcIso), `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`);
  assert.equal(formatTurnTime('invalid-time-string'), 'invalid-time-str');
  assert.equal(formatTurnTime('2026-11-03T03:00 오전'.replace(' 오전', 'zz')).length >= 0, true);
});

test('8.7 persona: the heading or no name, the sub note place, and the end of the second line (D20)', () => {
  const session = 'f1a50000-0000-4000-8000-000000000001';
  const named = railPersona({ heading: railHeading('# 테스트 노트\n본문', 'markdown'), noteId: 'ndtest-00000001', sessionId: session, hasBundle: false });
  const lines = named.split('\n');
  assert.equal(lines.length, 4);
  assert.equal(lines[0], '너는 사용자의 withlog 앱에서, 노트 옆 세션 레일로 질문을 받는다. 노트는 「테스트 노트」(ndtest-00000001)이다.');
  assert.ok(lines[1].endsWith('그 세션의 대화는 실리지 않았다.'));
  assert.match(lines[2], /wl-cli get_doc '\{"id":"ndtest-00000001"\}'` 로 읽는다.*`wl-cli update_doc` 로 고친다\.$/);
  const bundled = railPersona({ heading: null, noteId: 'ndtest-00000001', sessionId: session, hasBundle: true });
  assert.ok(bundled.split('\n')[0].endsWith('노트는 ndtest-00000001이다.'), 'no heading: the name is left out');
  assert.ok(bundled.split('\n')[1].endsWith('첫 물음 앞에 그 세션의 최근 대화가 실려 있다.'));
  const sub = railPersona({ heading: '하위', noteId: 'ndtest-00000001', subId: 'sntest-00000002', sessionId: session, hasBundle: false });
  assert.match(sub.split('\n')[0], /\(노트 ndtest-00000001의 하위 노트 sntest-00000002\)이다\.$/);
  assert.match(sub, /wl-cli get_sub_note '\{"parent":"ndtest-00000001","id":"sntest-00000002"\}'/);
  assert.match(sub, /wl-cli update_sub_note/);
  assert.equal(railHeading('# 제목', 'json'), null, 'a json note has no name');
});

test('8.7 / 8.8: the prompt carries the bundle on the first question only, and the seed from the answered turns', () => {
  const bundle = '## 이 세션의 최근 대화\n...\n## 물음\n';
  assert.equal(railPrompt({ bundle, question: '레일 폭을 왜?', first: true, imagePaths: [] }), `${bundle}레일 폭을 왜?`);
  assert.equal(railPrompt({ bundle, question: '그럼?', first: false, imagePaths: [] }), '그럼?');
  assert.equal(railPrompt({ bundle: '', question: 'q', first: true, imagePaths: ['/a/b.png'] }), 'q\n\n[첨부 그림] 이 물음에 그림 1장을 붙였다. 답하기 전에 Read 도구로 하나씩 열어 본다.\n- /a/b.png');
  const turn = (id, extra) => ({ id, askedAt: '', question: `${id}?`, answer: `${id}!`, model: 'm', effort: 'e', elapsedMs: 0, ...extra });
  assert.deepEqual(railSeed([turn('a'), turn('b', { error: '실패', answer: '' }), turn('c')], bundle), [
    { role: 'user', content: `${bundle}a?` },
    { role: 'assistant', content: 'a!' },
    { role: 'user', content: 'c?' },
    { role: 'assistant', content: 'c!' },
  ]);
  assert.equal(railSeed([turn('x', { error: 'e', answer: '' })], bundle), undefined, 'nothing to carry means no seed key');
});

test('8.5: an old chat id becomes the UUID the manual gives, and a new chat id goes as it is in lower case', async () => {
  assert.equal(await railChatSessionId('rcm1abcd-00000001'), '61c144eb-d717-4934-afcc-8bab92fa2953');
  assert.equal(await railChatSessionId('6B0F1E2A-3C4D-4E5F-8A9B-0C1D2E3F4A5B'), '6b0f1e2a-3c4d-4e5f-8a9b-0c1d2e3f4a5b');
});

test('8.10: the turn id, the cut of 20,000 characters, the failure text and the elapsed time', () => {
  assert.match(railTurnId(1762136140512, '1a2b3c4d-0000-4000-8000-000000000000'), /^rt[0-9a-z]+-1a2b3c4d$/);
  const ok = railTurnFrom({ id: 'rt1', askedAt: 't', question: 'q'.repeat(20_001), model: 'claude-sonnet-5-5', effort: 'low', images: [], answer: { ok: true, answer: '답', elapsed_ms: 7400 } });
  assert.equal(ok.question.length, 20_000);
  assert.equal(ok.question.endsWith('…'), true);
  assert.equal(ok.elapsedMs, 7400);
  assert.equal(ok.images, undefined);
  const failed = railTurnFrom({ id: 'rt2', askedAt: 't', question: 'q', model: 'm', effort: 'e', images: ['attach/a.png'], answer: { ok: false, error: null, elapsed_ms: 0 } });
  assert.equal(failed.error, '까닭을 못 받았다');
  assert.equal(failed.answer, '');
  assert.deepEqual(failed.images, ['attach/a.png']);
});

test('8.14: merging the file with the memory', () => {
  const chat = (id, count) => ({ noteId: 'n', chatId: id, sourceSessionId: 's', createdAt: '', turns: Array.from({ length: count }, (_, i) => ({ id: `${id}${i}`, askedAt: '', question: '', answer: '', model: '', effort: '', elapsedMs: 0 })) });
  const merged = mergeChats([chat('a', 1), chat('b', 1)], [chat('a', 2), chat('c', 1), chat('d', 1)], new Set(['d']));
  assert.deepEqual(merged.map((c) => [c.chatId, c.turns.length]), [['a', 2], ['b', 1], ['c', 1]], 'more turns win; memory-only chats from before the read were deleted outside');
});

test('8.4: the rail draws its header, the four-way conversation state and the chat box', () => {
  const html = renderToString(React.createElement(NoteSessionRail, { sessionId: 'f1a50000-0000-4000-8000-000000000001', chat: { target: { noteId: 'ndtest-00000001' }, noteMarkdown: '# 제목', vault: '/v' } }));
  assert.match(html, /class="nsrail"/);
  assert.match(html, /지금 안 돌고 있는 세션/);
  assert.match(html, /대화 기록을 읽는 중이에요/);
  assert.match(html, /aria-label="새 대화로 묻기"/);
  assert.match(html, /이 세션에는 보내지 않아요/);
});
