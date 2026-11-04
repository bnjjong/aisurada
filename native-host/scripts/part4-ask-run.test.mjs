import test from 'node:test';
import assert from 'node:assert/strict';
import { appendArgv, authorBundleFor, pictureName, refusalBeforeAsk, startAsk, migrateLegacyThread } from '../src/plugs/claude/askRun.ts';
import { askNow, setAskThread, unsavedAnswers } from '../src/plugs/claude/state.ts';
import {
  BUNDLE_BUDGET,
  authorBundleText,
  authorParts,
  fillFromEnd,
  railBundleText,
  turnsToLines,
} from '../src/plugs/claude/transcriptLines.ts';

const turn = (index, who, text, extra = {}) => ({ index, who, at: null, chars: text.length, truncated: false, text, ...extra });
const tool = (index, name) => ({ index, who: 'tool', at: null, chars: 0, truncated: false, text: '', tool: { name } });

// --------------------------------------------------------------------------
// 3.7 and 8.7: lines and windows
// --------------------------------------------------------------------------

test('3.7 lines: speech, one folded tool line, no arguments, no empty speech', () => {
  const lines = turnsToLines([
    turn(0, 'user', '  캐시 정리 규칙을 노트로 남겨 줘  '),
    tool(1, 'Read'), tool(2, 'Read'), tool(3, 'Bash'),
    turn(4, 'assistant', '노트를 만들었다.'),
    turn(5, 'user', '   '),
    turn(6, 'assistant', 'x'.repeat(2000), { truncated: true }),
  ]);
  assert.deepEqual(lines.map((l) => l.text), [
    '사용자: 캐시 정리 규칙을 노트로 남겨 줘',
    '(도구: Read ×2, Bash)',
    '클로드: 노트를 만들었다.',
    `클로드: ${'x'.repeat(2000)}…(잘림)`,
  ]);
  assert.deepEqual([lines[1].first, lines[1].last], [1, 3]);
});

test('3.7 fill: from the end, a line costs its length plus one, and the first line that does not fit stops it', () => {
  const lines = [
    { text: 'a'.repeat(10), first: 0, last: 0 },
    { text: 'b'.repeat(100), first: 1, last: 1 },
    { text: 'c'.repeat(5), first: 2, last: 2 },
  ];
  const result = fillFromEnd(lines, 50);
  assert.deepEqual(result.picked.map((l) => l.first), [2], 'the long line stops the fill; the shorter older line is not picked around it');
  assert.equal(result.left, 44);
  assert.equal(result.cut, true);
});

test('3.7 windows: one mention, one merged window, or two windows', () => {
  const small = (from, to) => Array.from({ length: to - from + 1 }, (_, i) => turn(from + i, i % 2 ? 'assistant' : 'user', `줄 ${from + i}`));
  const one = authorParts({ note: 'n', mentions: [4], latest: small(0, 4) });
  assert.deepEqual(one.map((p) => p.label), ['노트를 만들기까지']);
  const merged = authorParts({ note: 'n', mentions: [2, 9], creation: small(0, 2), latest: small(0, 9) });
  assert.deepEqual(merged.map((p) => p.label), ['노트를 만들고 고치기까지'], 'filling back from the last mention reaches the first one');
  // long lines: the last window cannot reach the first mention
  const long = Array.from({ length: 20 }, (_, i) => turn(100 + i, i % 2 ? 'assistant' : 'user', 'z'.repeat(2000)));
  const two = authorParts({ note: 'n', mentions: [100, 119], creation: [turn(99, 'user', '만들기'), turn(100, 'assistant', '노트')], latest: long });
  assert.deepEqual(two.map((p) => p.label), ['노트를 만들기까지', '마지막으로 고치기까지']);
  assert.deepEqual(two[0].lines, ['사용자: 만들기', '클로드: 노트']);
  const used = two[1].lines.reduce((sum, l) => sum + l.length + 1, 0);
  assert.ok(used <= BUNDLE_BUDGET - (16_000 - (16_000 - ('사용자: 만들기'.length + 1) - ('클로드: 노트'.length + 1))));
  assert.deepEqual(authorParts({ note: 'n', mentions: [], latest: [] }), []);
});

test('3.7 bundle text: header, parts and the 물음 header', () => {
  const { text, lines } = authorBundleText('11111111-2222-4333-8444-555555555555', { note: 'n', mentions: [1], latest: [turn(0, 'user', '캐시 정리 규칙을 노트로 남겨 줘'), tool(1, 'Read')] });
  assert.equal(lines, 2);
  assert.equal(text, [
    '## 이 노트를 쓴 세션의 대화',
    '',
    '이 노트를 만든 Claude Code 세션 `11111111-2222-4333-8444-555555555555` 의 대화에서 노트를 쓰던 무렵만 잘라 실었다. 도구 호출은 이름만 남기고 도구 결과는 뺐다.',
    '아래 대화만으로 답이 모자라면 짐작하지 말고 모자라다고 말하라.',
    '',
    '<session-transcript part="노트를 만들기까지">',
    '사용자: 캐시 정리 규칙을 노트로 남겨 줘',
    '(도구: Read)',
    '</session-transcript>',
    '',
    '## 물음',
    '',
    '',
  ].join('\n'));
  assert.equal(authorBundleText('s', null).text, '');
});

test('8.7 rail bundle: header lines, blank lines around the transcript, and the cut note', () => {
  const text = railBundleText('f1a50000-0000-4000-8000-000000000001', [turn(0, 'user', '노트 옆에 레일을 붙여줘.'), tool(1, 'Read'), tool(2, 'Bash'), tool(3, 'Agent'), tool(4, 'Agent'), turn(5, 'assistant', '레일을 붙였어요.')]);
  assert.equal(text, [
    '## 이 세션의 최근 대화',
    '',
    '사용자가 노트 옆에서 연 Claude Code 세션 `f1a50000-0000-4000-8000-000000000001` 의 대화다. 최신 쪽부터 24,000자까지 실었다. 도구 호출은 이름만 남겼다.',
    '이것은 읽은 순간의 스냅샷이라 가장 최근 활동이 빠졌을 수 있다.',
    '아래 대화만으로 답이 모자라면 짐작하지 말고 모자라다고 말하라.',
    '',
    '<session-transcript>',
    '사용자: 노트 옆에 레일을 붙여줘.',
    '(도구: Read, Bash, Agent ×2)',
    '클로드: 레일을 붙였어요.',
    '</session-transcript>',
    '',
    '## 물음',
    '',
    '',
  ].join('\n'));
  assert.equal(railBundleText('s', []), '');
});

// --------------------------------------------------------------------------
// 3.4 to 3.10: one question
// --------------------------------------------------------------------------

function fakeHost(options = {}) {
  const calls = { engine: [], ask: [], images: [], announced: [] };
  const answers = options.answers ?? [];
  const host = {
    native: true,
    calls,
    async engine(argv) {
      calls.engine.push(argv);
      if (argv[0] === 'read-session') return { status: 0, stdout: JSON.stringify({ author: options.author ?? null }), stderr: '' };
      if (argv[0] === 'append-ask-turn') {
        if (options.saveFails) return { status: 3, stdout: '', stderr: 'AIS-500 디스크가 가득 찼다' };
        const get = (name) => argv.find((a) => a.startsWith(`${name}=`))?.slice(name.length + 1);
        const prior = options.saved ?? (options.saved = { noteId: argv[1], sessionId: get('session-id'), turns: [] });
        prior.turns.push({ id: get('turn-id'), markId: get('mark-id'), askedAt: get('asked-at'), ...(get('quote') ? { quote: get('quote') } : {}), question: get('question'), answer: get('answer'), ...(get('error') ? { error: get('error') } : {}), model: get('model'), effort: get('effort'), elapsedMs: Number(get('elapsed-ms')), ...(get('session-lines') ? { sessionLines: Number(get('session-lines')) } : {}), ...(get('images') ? { images: get('images').split(',') } : {}) });
        return { status: 0, stdout: JSON.stringify({ ...prior, path: `notes/a/${argv[1]}/ask.json` }), stderr: '' };
      }
      return { status: 3, stdout: '', stderr: 'AIS-400' };
    },
    async askClaude(args) {
      calls.ask.push(args);
      const next = answers.shift();
      return typeof next === 'function' ? next(args) : next ?? { ok: true, answer: '답이다.', error: null, elapsed_ms: 1200 };
    },
    async askRuntime() { return options.runtime ?? 'cli'; },
    async writeImage(dir, name, bytes) {
      calls.images.push([dir, name, bytes.length]);
      if (options.imageFails) throw new Error('디스크가 가득 찼다');
    },
    announceSelfWrite(vault, rel) { calls.announced.push([vault, rel]); },
  };
  return host;
}

const baseRequest = (extra = {}) => ({
  target: { id: 'ndq-00000001' }, rootId: 'ndq-00000001', vault: '/v', noteDir: '/v/notes/a/ndq-00000001',
  quote: '노트를 다시 열 때 비운다', question: '언제 비우나?', markId: 'akq1-aaaaaaaa', turnId: 'akq1-aaaaaaaa',
  heading: '캐시 정리', repo: 'example/cache-service', authorSession: 'f1a50000-0000-4000-8000-000000000001',
  model: 'claude-sonnet-5-5', effort: 'low', timeoutSeconds: 7200, ...extra,
});

test('3.8 first question: nine keys, the text, the persona, the author bundle, and the saved turn', async () => {
  setAskThread('ndq-00000001', null);
  const host = fakeHost({ author: { note: 'ndq-00000001', mentions: [1], latest: [turn(0, 'user', '캐시 규칙을 남겨 줘'), turn(1, 'assistant', '남겼다')] } });
  await startAsk(host, baseRequest());
  assert.equal(host.calls.ask.length, 1);
  const args = host.calls.ask[0];
  assert.deepEqual(Object.keys(args).sort(), ['effort', 'keepAlive', 'model', 'persona', 'prompt', 'resume', 'sessionId', 'timeoutSeconds', 'vault']);
  assert.equal(args.vault, '/v', 'the vault, not the note folder');
  assert.equal(args.resume, false);
  assert.equal(args.keepAlive, false);
  assert.equal(args.timeoutSeconds, 7200, 'the limit of the press, not 3600');
  assert.match(args.sessionId, /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
  assert.ok(args.prompt.startsWith('## 이 노트를 쓴 세션의 대화\n'));
  assert.ok(args.prompt.includes('## 물음\n\n이 노트가 속한 아젠다의 소스 저장소: example/cache-service\n이 대목을 고른 노트의 id: ndq-00000001.'));
  assert.ok(args.prompt.endsWith('[대목] 「노트를 다시 열 때 비운다」\n\n이 대목에 대한 질문이다. 짧게 답해라.\n\n언제 비우나?'));
  assert.equal(args.persona.split('\n').length, 3);
  assert.deepEqual(host.calls.engine[0], ['read-session', 'f1a50000-0000-4000-8000-000000000001', 'limit=200', 'before=0', 'author=ndq-00000001']);
  const appended = host.calls.engine.at(-1);
  assert.ok(appended.includes('session-lines=2') && appended.includes('quote=노트를 다시 열 때 비운다') && appended.includes('elapsed-ms=1200'));
  assert.deepEqual(host.calls.announced, [['/v', 'notes/a/ndq-00000001/ask.json']], 'the answer path is announced as our own write');
  const state = askNow();
  assert.equal(state.threads['ndq-00000001'].turns.length, 1);
  assert.equal(state.pending['ndq-00000001'], undefined, 'the in-flight record is removed');
  assert.equal(state.unseen['ndq-00000001'], 'ndq-00000001');
});

test('3.5 line: the second question waits, sees the first turn, resumes and carries no bundle', async () => {
  setAskThread('ndq-00000002', null);
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  const host = fakeHost({ answers: [async () => { await gate; return { ok: true, answer: '첫 답', error: null, elapsed_ms: 10 }; }, { ok: true, answer: '둘째 답', error: null, elapsed_ms: 20 }] });
  const first = startAsk(host, baseRequest({ target: { id: 'ndq-00000002' }, rootId: 'ndq-00000002', markId: 'akq2-aaaaaaaa', turnId: 'akq2-aaaaaaaa', authorSession: undefined }));
  const second = startAsk(host, baseRequest({ target: { id: 'ndq-00000002' }, rootId: 'ndq-00000002', quote: '다른 대목', question: '이건?', markId: 'akq2-bbbbbbbb', turnId: 'akq2-bbbbbbbb', authorSession: undefined }));
  const waiting = askNow().pending['ndq-00000002'];
  assert.equal(waiting['akq2-aaaaaaaa'].queued, undefined);
  assert.equal(waiting['akq2-bbbbbbbb'].queued, true, 'the second record waits behind the first');
  await new Promise((resolve) => setTimeout(resolve, 20));
  assert.equal(host.calls.ask.length, 1, 'the second question has not left yet');
  assert.equal(askNow().pending['ndq-00000002']['akq2-bbbbbbbb'].queued, true);
  release();
  await Promise.all([first, second]);
  assert.equal(host.calls.ask.length, 2);
  assert.equal(host.calls.ask[1].resume, true, 'it sees the first turn');
  assert.equal(host.calls.ask[1].sessionId, host.calls.ask[0].sessionId);
  assert.ok(!host.calls.ask[1].prompt.includes('## 이 노트를 쓴 세션의 대화'), 'an answered thread carries no bundle again');
  assert.equal(askNow().threads['ndq-00000002'].turns.length, 2);
  assert.deepEqual(askNow().pending['ndq-00000002'], undefined);
});

test('4.5 follow-up: the text carries the folded quote of the underline it continues', async () => {
  setAskThread('ndq-00000006', null);
  const host = fakeHost();
  await startAsk(host, baseRequest({ target: { id: 'ndq-00000006' }, rootId: 'ndq-00000006', authorSession: undefined }));
  await startAsk(host, baseRequest({ target: { id: 'ndq-00000006' }, rootId: 'ndq-00000006', quote: '', question: '더 자세히', markId: 'akq1-aaaaaaaa', turnId: 'akq6-dddddddd', authorSession: undefined }));
  assert.equal(host.calls.ask[1].prompt, '[대목] 「노트를 다시 열 때 비운다」\n\n더 자세히');
});

test('3.9: a picture that cannot be written means ask_claude is not called and the turn records why', async () => {
  setAskThread('ndq-00000003', null);
  const host = fakeHost({ imageFails: true });
  const file = { name: 'a.png', type: 'image/png', arrayBuffer: async () => new ArrayBuffer(4) };
  await startAsk(host, baseRequest({ target: { id: 'ndq-00000003' }, rootId: 'ndq-00000003', images: [file] }));
  assert.equal(host.calls.ask.length, 0);
  const saved = askNow().threads['ndq-00000003'].turns[0];
  assert.equal(saved.error, '붙인 그림을 못 저장해서 묻지 않았어요: 디스크가 가득 찼다');
  assert.equal(saved.elapsedMs, 0);
  assert.equal(saved.images, undefined);
  assert.equal(refusalBeforeAsk({ picture: { written: [] }, hasPictures: true, runtime: 'http' }), '지금 묻기 길(HTTP)은 그림을 못 실어서 묻지 않았어요.');
  assert.equal(refusalBeforeAsk({ picture: { written: [] }, hasPictures: false, htmlRel: 'attach/r.html', runtime: 'cli' }), '이 노트의 폴더를 몰라서 HTML 문서를 못 실었어요.');
  assert.match(pictureName(file, 1762136140512, '5e6f7a8b-0000-4000-8000-000000000000'), /^ask-[0-9a-z]+-5e6f7a8b\.png$/);
});

test('3.9 with pictures: the picture lines go in the text and the turn keeps them (the bridge writes headers and body)', async () => {
  setAskThread('ndq-00000004', null);
  const host = fakeHost();
  const file = { name: 'a.png', type: 'image/png', arrayBuffer: async () => new ArrayBuffer(4) };
  await startAsk(host, baseRequest({ target: { id: 'ndq-00000004' }, rootId: 'ndq-00000004', authorSession: undefined, images: [file] }));
  assert.equal(host.calls.images.length, 1);
  assert.equal(host.calls.images[0][0], '/v/notes/a/ndq-00000001');
  assert.match(host.calls.ask[0].prompt, /\[첨부 그림\] 이 물음에 그림 1장을 붙였다\.[^\n]*\n- \/v\/notes\/a\/ndq-00000001\/attach\/ask-[0-9a-z]+-[0-9a-f]{8}\.png$/);
  assert.equal(askNow().threads['ndq-00000004'].turns[0].images.length, 1);
});

test('3.10: a save failure keeps the turn in memory as unsaved, and a later save keeps it behind the engine thread', async () => {
  setAskThread('ndq-00000005', null);
  const before = unsavedAnswers();
  const host = fakeHost({ saveFails: true });
  await startAsk(host, baseRequest({ target: { id: 'ndq-00000005' }, rootId: 'ndq-00000005', authorSession: undefined }));
  const turns = askNow().threads['ndq-00000005'].turns;
  assert.equal(turns.length, 1);
  assert.equal(turns[0].unsaved, true);
  assert.equal(unsavedAnswers(), before + 1);
  const ok = fakeHost();
  await startAsk(ok, baseRequest({ target: { id: 'ndq-00000005' }, rootId: 'ndq-00000005', quote: '', question: '또', markId: 'akq1-aaaaaaaa', turnId: 'akq5-cccccccc', authorSession: undefined }));
  const merged = askNow().threads['ndq-00000005'].turns;
  assert.deepEqual(merged.map((t) => [t.id, Boolean(t.unsaved)]), [['akq5-cccccccc', false], ['akq1-aaaaaaaa', true]], 'the engine thread first, the unsaved turn behind it');
});

test('3.10 argv: elapsed-ms stays a 32-bit integer, and an empty quote is not sent', () => {
  const base = { id: 'a', markId: 'a', askedAt: 't', question: 'q', answer: 'x', model: 'm', effort: 'e', elapsedMs: 3_000_000_000 };
  const argv = appendArgv({ id: 'n' }, 's', base);
  assert.ok(argv.includes('elapsed-ms=2147483647'));
  assert.ok(!argv.some((a) => a.startsWith('quote=')));
});

test('3.7: when the first note has nothing to carry, the next id is asked for', async () => {
  const asked = [];
  const host = {
    async engine(argv) {
      asked.push(argv.at(-1));
      return { status: 0, stdout: JSON.stringify({ author: asked.length === 1 ? { note: 'snq', mentions: [3], latest: [turn(0, 'user', '   ')] } : { note: 'ndq', mentions: [1], latest: [turn(0, 'user', '뿌리 노트의 대화')] } }), stderr: '' };
    },
  };
  const found = await authorBundleFor(host, 'f1a50000-0000-4000-8000-000000000001', ['snq', 'ndq']);
  assert.deepEqual(asked, ['author=snq,ndq', 'author=ndq']);
  assert.match(found.text, /사용자: 뿌리 노트의 대화/);
  const failing = { engine: async () => ({ status: 3, stdout: '', stderr: 'AIS-400' }) };
  assert.deepEqual(await authorBundleFor(failing, 's', ['x']), { text: '', lines: 0 }, 'an old engine means no bundle, never a blocked question');
});

test('2.10: an old thread moves turn by turn and leaves the key; a failure keeps the original', async () => {
  const map = new Map([['legacy', JSON.stringify({ ndq: { sessionId: 'S', turns: [{ id: 'a', question: 'q', answer: 'x', model: 'opus', effort: 'ultra', elapsedMs: 5 }] }, other: { turns: [] } })]]);
  const storage = { getItem: (k) => map.get(k) ?? null, setItem: (k, v) => map.set(k, v), removeItem: (k) => map.delete(k) };
  const host = fakeHost();
  const thread = await migrateLegacyThread(host, storage, 'legacy', '/v', { id: 'ndq' });
  assert.equal(thread.turns.length, 1);
  const argv = host.calls.engine[0];
  assert.ok(argv.includes('session-id=S') && argv.includes('model=claude-opus-5-5') && argv.includes('effort=low'));
  assert.deepEqual(Object.keys(JSON.parse(map.get('legacy'))), ['other']);
  const failing = fakeHost({ saveFails: true });
  map.set('legacy', JSON.stringify({ ndq: { sessionId: 'S', turns: [{ id: 'a', question: 'q', answer: 'x' }] } }));
  assert.equal(await migrateLegacyThread(failing, storage, 'legacy', '/v', { id: 'ndq' }), null);
  assert.ok(map.has('legacy'), 'the original stays');
});

test('second review: creation falls back to latest, and nameless tools contribute no line', () => {
  const latest = [turn(0, 'user', '노트를 만들었다'), ...Array.from({ length: 20 }, (_, i) => turn(i + 1, 'assistant', 'x'.repeat(2000)))];
  const parts = authorParts({ note: 'n', mentions: [0, 20], latest });
  assert.deepEqual(parts.map((part) => part.label), ['노트를 만들기까지', '마지막으로 고치기까지']);
  assert.deepEqual(parts[0].lines, ['사용자: 노트를 만들었다']);
  assert.deepEqual(turnsToLines([tool(0, null), tool(1, 'Read'), tool(2, null)]).map((line) => line.text), ['(도구: Read)']);
  assert.deepEqual(turnsToLines([tool(0, null)]), []);
});

test('second review: stored table quote keeps its final newline', async () => {
  const host = fakeHost();
  await startAsk(host, baseRequest({ target: { id: 'ndquote' }, quote: 'a\tb\n', authorSession: undefined }));
  assert.ok(host.calls.engine.find((argv) => argv[0] === 'append-ask-turn').includes('quote=a\tb\n'));
});

test('second review: legacy migration preserves session id, overlong text and optional fields', async () => {
  const old = { sessionId: '', turns: [{ id: 'a', question: 'q'.repeat(20001), answer: 'a', quote: 'x'.repeat(2001), sessionLines: 21, images: ['attach/a.png'] }] };
  const map = new Map([['legacy', JSON.stringify({ ndlegacy: old })]]);
  const storage = { getItem: (k) => map.get(k) ?? null, setItem: (k, v) => map.set(k, v), removeItem: (k) => map.delete(k) };
  const host = fakeHost({ saveFails: true });
  await migrateLegacyThread(host, storage, 'legacy', '/v', { id: 'ndlegacy' });
  const argv = host.calls.engine[0];
  assert.ok(argv.includes('session-id=') && argv.includes(`question=${old.turns[0].question}`));
  assert.ok(argv.includes(`quote=${old.turns[0].quote}`) && argv.includes('session-lines=21') && argv.includes('images=attach/a.png'));
  assert.deepEqual(JSON.parse(map.get('legacy')).ndlegacy, old);
});
