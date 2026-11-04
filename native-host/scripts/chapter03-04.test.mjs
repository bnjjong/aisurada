import test from 'node:test';
import assert from 'node:assert/strict';
import {
  ASK_MAIN_KEY,
  ASK_LEGACY_KEY,
  CLAUDE_ENABLED,
  askStorageKey,
  createAskStore,
  dismissAbandoned,
  clearNoteUnseen,
  markNoteUnseen,
  noteRailNow,
  openNoteRail,
  closeNoteRail,
  restoreAskState,
  serializeAskState,
  setAskThread,
  setPendingAsk,
  unsavedAnswers,
  waitingAnswers,
} from '../src/plugs/claude/state.ts';
import { generateUnderlineId } from '../src/plugs/claude/ask.tsx';
import {
  ASK_EFFORTS,
  ASK_MODELS,
  askPersona,
  buildAskPrompt,
  cutText,
  foldQuote,
  groupTurns,
  newUnderlineId,
  noteHeading,
  noteIdLine,
  normalizeEffort,
  normalizeModel,
  turnsForUnderline,
} from '../src/plugs/claude/askText.ts';

// --------------------------------------------------------------------------
// Chapter 3: state, lists and the question text
// --------------------------------------------------------------------------

test('Chapter 3: pending records and the waiting count', () => {
  assert.equal(CLAUDE_ENABLED, true);
  const before = waitingAnswers();
  setPendingAsk('note-1', 'ak1-aaaaaaaa', { at: Date.now(), sessionId: 's1', timeoutSeconds: 3600 });
  assert.equal(waitingAnswers(), before + 1);
  setPendingAsk('note-1', 'ak1-aaaaaaaa', null);
  assert.equal(waitingAnswers(), before);
});

test('Chapter 3.3: model and effort lists and the way stored values come back (M5)', () => {
  assert.deepEqual(ASK_MODELS.map((m) => m.id), ['claude-opus-5-5', 'claude-opus-5', 'claude-sonnet-5-5', 'claude-sonnet-5', 'claude-fable-5-1', 'claude-fable-5']);
  assert.deepEqual(ASK_MODELS.map((m) => m.label), ['opus 5.5', 'opus 5', 'sonnet 5.5', 'sonnet 5', 'fable 5.1', 'fable 5']);
  assert.deepEqual(ASK_EFFORTS.map((e) => e.id), ['low', 'medium', 'high', 'xhigh', 'max']);
  assert.equal(normalizeModel('opus'), 'claude-opus-5-5');
  assert.equal(normalizeModel('sonnet'), 'claude-sonnet-5-5');
  assert.equal(normalizeModel('fable'), 'claude-fable-5-1');
  assert.equal(normalizeModel('claude-haiku-5-5'), 'claude-sonnet-5-5', 'haiku is not in the list');
  assert.equal(normalizeModel('broken\u0000text'), 'claude-sonnet-5-5');
  assert.equal(normalizeEffort('xhigh'), 'xhigh');
  assert.equal(normalizeEffort('nope'), 'low');
});

test('Chapter 2.1: the underline id tail is 8 hexadecimal digits (L1)', () => {
  assert.match(generateUnderlineId(), /^ak[0-9a-z]+-[0-9a-f]{8}$/);
  assert.equal(newUnderlineId(1762136140512, () => '9c1e5a7b-1111-4222-8333-444444444444'), `ak${(1762136140512).toString(36)}-9c1e5a7b`);
});

test('Chapter 3.10: cutting keeps the limit and never leaves half a character (M12)', () => {
  assert.equal(cutText('짧은 글', 10), '짧은 글');
  assert.equal(cutText('a'.repeat(20_000), 20_000).length, 20_000);
  assert.equal(cutText('a'.repeat(20_001), 20_000), `${'a'.repeat(19_999)}…`);
  // The cut falls between the two halves of an emoji: the high half goes too.
  const text = `${'a'.repeat(1_998)}😀${'b'.repeat(10)}`;
  assert.equal(cutText(text, 2_000), `${'a'.repeat(1_998)}…`);
});

test('Chapter 3.6: the question text of a new underline, a follow-up, an HTML box and pictures (M1, M2)', () => {
  const target = { id: 'ndm1abcd-00000001' };
  const first = buildAskPrompt({ target, quote: '  노트를 다시 열 때 비운다 ', question: ' 언제 비우나? ', repo: 'example/cache-service' });
  assert.equal(first, [
    '이 노트가 속한 아젠다의 소스 저장소: example/cache-service',
    '이 대목을 고른 노트의 id: ndm1abcd-00000001. 답하기 전에 `wl-cli get_doc \'{"id":"ndm1abcd-00000001"}\'` 로 이 노트를 먼저 읽고, 대목의 앞뒤 글을 보고 답해라.',
    '',
    '[대목] 「노트를 다시 열 때 비운다」',
    '',
    '이 대목에 대한 질문이다. 짧게 답해라.',
    '',
    '언제 비우나?',
  ].join('\n'));
  const followUp = buildAskPrompt({
    target, quote: '', question: '그림의 빨간 칸은 무엇인가?', openingQuote: '노트를 다시\t열 때   비운다',
    imagePaths: ['/Users/me/vault/notes/a/attach/ask-1-5e6f7a8b.png'],
  });
  assert.equal(followUp, [
    '[대목] 「노트를 다시|열 때 비운다」',
    '',
    '그림의 빨간 칸은 무엇인가?',
    '',
    '[첨부 그림] 이 물음에 그림 1장을 붙였다. 답하기 전에 Read 도구로 하나씩 열어 본다.',
    '- /Users/me/vault/notes/a/attach/ask-1-5e6f7a8b.png',
  ].join('\n'));
  const html = buildAskPrompt({ target, quote: 'HTML 문서: 보고서', question: '요약해 줘', htmlPath: '/v/notes/a/attach/report.html' });
  assert.match(html, /\[HTML 문서\] 이 물음은 고른 대목이 아니라 노트에 넣은 HTML 문서 전체에 대한 것이다\./);
  assert.ok(html.endsWith('- /v/notes/a/attach/report.html'));
  assert.equal(foldQuote(`${'가'.repeat(201)}`), `${'가'.repeat(200)}…`);
  // A sub note reads with get_sub_note and names both ids.
  assert.match(noteIdLine({ id: 'snm1abcd-00000001', parent: 'ndm1abcd-00000001' }), /^이 대목을 고른 노트: 노트 ndm1abcd-00000001 의 하위 노트 snm1abcd-00000001\. .*get_sub_note '\{"parent":"ndm1abcd-00000001","id":"snm1abcd-00000001"\}'/);
});

test('Chapter 3.6: the persona has three lines, the heading from the editor buffer, and the sub note place (M2)', () => {
  assert.equal(noteHeading('<!-- x -->\n# 캐시 정리 <!-- y -->\n본문', 'markdown'), '캐시 정리');
  assert.equal(noteHeading('본문만', 'markdown'), null);
  assert.equal(noteHeading('# 제목', 'json'), null, 'a json note never has a heading');
  const lines = askPersona({ id: 'ndm1abcd-00000001' }, '캐시 정리').split('\n');
  assert.equal(lines.length, 3);
  assert.equal(lines[0], '너는 사용자의 withlog 노트 안에서 질문을 받는다. 노트는 「캐시 정리」(ndm1abcd-00000001)이다.');
  assert.equal(lines[2], '질문마다 [대목] 줄이 붙는다. 그 대목을 기준으로 답하고, 같은 대화에서 다른 대목을 두고 오간 문답과 섞지 않는다.');
  assert.equal(askPersona({ id: 'ndm1abcd-00000001' }, null).split('\n')[0], '너는 사용자의 withlog 노트 안에서 질문을 받는다. 노트는 ndm1abcd-00000001이다.');
  assert.match(askPersona({ id: 'snm1abcd-00000001', parent: 'ndm1abcd-00000001' }, '하위').split('\n')[0], /\(노트 ndm1abcd-00000001의 하위 노트 snm1abcd-00000001\)이다\.$/);
});

test('Chapter 2.4: a turn belongs to its markId, else the nearest earlier turn with a quote (4장 카드)', () => {
  const turn = (id, extra = {}) => ({ id, askedAt: '', question: id, answer: 'a', model: 'm', effort: 'e', elapsedMs: 0, ...extra });
  const turns = [
    turn('akA', { quote: 'q' }),
    turn('akB', { quote: 'q', markId: 'akB' }),
    turn('akC', { markId: 'akA' }),
    turn('akD'),
  ];
  assert.deepEqual(groupTurns(turns).map((g) => [g.markId, g.turns.map((t) => t.id)]), [['akA', ['akA', 'akC']], ['akB', ['akB', 'akD']]]);
  // a card given a follow-up turn's id shows the whole underline
  assert.deepEqual(turnsForUnderline(turns, 'akC').map((t) => t.id), ['akA', 'akC']);
  assert.deepEqual(turnsForUnderline(turns, 'akB').map((t) => t.id), ['akB', 'akD']);
  assert.deepEqual(turnsForUnderline(turns, 'nope'), []);
});

// --------------------------------------------------------------------------
// Chapter 3.11: the saved shape and the restore (M6, M8)
// --------------------------------------------------------------------------

function memoryStorage(initial = {}) {
  const map = new Map(Object.entries(initial));
  return { map, getItem: (k) => (map.has(k) ? map.get(k) : null), setItem: (k, v) => void map.set(k, v), removeItem: (k) => void map.delete(k) };
}

test('Chapter 3.11: the storage key of a note window is its own', () => {
  assert.equal(askStorageKey('/calendar'), 'withlog.ask.v1');
  assert.equal(askStorageKey('/note-window/ndm1abcd-00000001'), 'withlog.ask.note-ndm1abcd-00000001.v1');
});

test('Chapter 3.11: restore turns pending into abandoned, drops bad records and clears what is not saved', () => {
  const storage = memoryStorage();
  const stored = {
    state: {
      model: 'opus',
      effort: 'ultra',
      threads: { n1: { sessionId: 's' } },
      pending: {
        n1: {
          akP1: { at: 1000, sessionId: 'S1', timeoutSeconds: 3600, runtime: 'cli' },
          akP2: { at: 'bad', sessionId: 'S2' },
          akP3: { at: 3000, sessionId: 5 },
          akQ: { at: 4000, sessionId: 'S4', queued: true },
        },
      },
      abandoned: { n1: [{ markId: 'akQ', at: 1, sessionId: 'X' }] },
    },
    version: 0,
  };
  const state = restoreAskState(stored, { storage, main: true });
  assert.equal(state.model, 'claude-opus-5-5');
  assert.equal(state.effort, 'low');
  assert.deepEqual(state.pending, {});
  assert.deepEqual(state.unseen, {});
  assert.deepEqual(state.threads, {});
  assert.deepEqual(state.abandoned.n1.map((a) => a.markId), ['akQ', 'akP1'], 'a mark already in the list is not added twice; bad records are dropped');
  assert.deepEqual(state.abandoned.n1[1], { markId: 'akP1', at: 1000, sessionId: 'S1', runtime: 'cli' }, 'timeoutSeconds is not carried');
  assert.equal(storage.getItem(ASK_LEGACY_KEY), JSON.stringify({ n1: { sessionId: 's' } }), 'old threads are set aside whole');
  // an existing legacy key is not overwritten
  const keep = memoryStorage({ [ASK_LEGACY_KEY]: '{"old":1}' });
  restoreAskState(stored, { storage: keep, main: true });
  assert.equal(keep.getItem(ASK_LEGACY_KEY), '{"old":1}');
});

test('Chapter 3.11: only pending, abandoned, model and effort are saved, in the stored shape of version 0', () => {
  const storage = memoryStorage();
  const store = createAskStore(storage, ASK_MAIN_KEY);
  const written = [];
  const original = storage.setItem;
  storage.setItem = (k, v) => { written.push(k); original(k, v); };
  // memory-only changes do not write at all
  store.update((s) => ({ ...s, unseen: { n1: 'n1' }, runtime: 'http', loadErrors: { n1: 'x' }, threads: { n1: { noteId: 'n1', sessionId: 's', turns: [] } } }));
  assert.deepEqual(written, []);
  store.update((s) => ({ ...s, model: 'claude-opus-5' }));
  assert.deepEqual(written, [ASK_MAIN_KEY]);
  const saved = JSON.parse(storage.getItem(ASK_MAIN_KEY));
  assert.deepEqual(Object.keys(saved), ['state', 'version']);
  assert.equal(saved.version, 0);
  assert.deepEqual(Object.keys(saved.state).sort(), ['abandoned', 'effort', 'model', 'pending']);
  assert.equal(saved.state.model, 'claude-opus-5');
});

test('Chapter 3.11: a note window borrows model and effort from the main key once', () => {
  const storage = memoryStorage({ [ASK_MAIN_KEY]: JSON.stringify({ state: { model: 'claude-opus-5', effort: 'high' }, version: 0 }) });
  const note = createAskStore(storage, 'withlog.ask.note-n1.v1');
  assert.equal(note.get().model, 'claude-opus-5');
  assert.equal(note.get().effort, 'high');
  note.update((s) => ({ ...s, model: 'claude-fable-5' }));
  assert.equal(JSON.parse(storage.getItem(ASK_MAIN_KEY)).state.model, 'claude-opus-5', 'a pick in the note window does not go back to main');
  assert.equal(createAskStore(storage, 'withlog.ask.note-n1.v1').get().model, 'claude-fable-5', 'the note window keeps its own value afterwards');
});

test('Chapter 3.10 and 4.10: unsaved answers are counted from the threads, and the unseen mark is a key to a root id', () => {
  const before = unsavedAnswers();
  setAskThread('note-u', { noteId: 'note-u', sessionId: 's', turns: [{ id: 'a', askedAt: '', question: 'q', answer: 'x', model: 'm', effort: 'e', elapsedMs: 0, unsaved: true }] });
  assert.equal(unsavedAnswers(), before + 1);
  setAskThread('note-u', null);
  assert.equal(unsavedAnswers(), before, 'the count falls when the thread is replaced, with no separate counter to forget');
  markNoteUnseen('sub-1', 'root-1');
  clearNoteUnseen('sub-1');
  dismissAbandoned('none', 'x');
});

test('Chapter 4: note session rail opening and closing tracking', () => {
  openNoteRail('note-a', 'sess-1234');
  assert.equal(noteRailNow().rails['note-a'], 'sess-1234');
  closeNoteRail('note-a');
  assert.equal(noteRailNow().rails['note-a'], undefined);
  assert.ok(serializeAskState);
});
