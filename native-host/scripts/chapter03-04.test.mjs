import test from 'node:test';
import assert from 'node:assert/strict';
import {
  CLAUDE_ENABLED,
  setPendingAsk,
  waitingAnswers,
  markNoteUnseen,
  clearNoteUnseen,
  dismissAbandoned,
  incUnsavedAnswers,
  unsavedAnswers,
  openNoteRail,
  closeNoteRail,
  noteRailNow,
} from '../src/plugs/claude/state.ts';
import { generateUnderlineId } from '../src/plugs/claude/ask.tsx';
import { ASK_MODELS, ASK_EFFORTS } from '../src/plugs/claude/askPopover.tsx';

// --------------------------------------------------------------------------
// Chapter 3: State & Prompting specifications
// --------------------------------------------------------------------------

test('Chapter 3: State module reflects Claude enabled and maintains pending counters', () => {
  assert.equal(CLAUDE_ENABLED, true);

  const initialWaiting = waitingAnswers();
  setPendingAsk('note-1', 'turn-1', {
    queued: false,
    askedAt: Date.now(),
    markId: 'turn-1',
    quote: '대목',
  });
  assert.equal(waitingAnswers(), initialWaiting + 1);

  // Clear pending
  setPendingAsk('note-1', 'turn-1', null);
  assert.equal(waitingAnswers(), initialWaiting);
});

test('Chapter 3: Underline ID conforms to ak<time36>-<uuid8> specification', () => {
  const id = generateUnderlineId();
  assert.match(id, /^ak[0-9a-z]+-[0-9a-z]{8}$/);
});

test('Chapter 3: Model and effort options conform to 3.3 specification', () => {
  const modelIds = ASK_MODELS.map((m) => m.id);
  assert.ok(modelIds.includes('claude-sonnet-5-5'));
  assert.ok(modelIds.includes('claude-haiku-5-5'));
  assert.ok(modelIds.includes('claude-opus-5-5'));

  const effortIds = ASK_EFFORTS.map((e) => e.id);
  assert.deepEqual(effortIds, ['low', 'medium', 'high']);
});

test('Chapter 3 & 4: Unsaved answers counter tracking', () => {
  const cur = unsavedAnswers();
  incUnsavedAnswers();
  assert.equal(unsavedAnswers(), cur + 1);
});

test('Chapter 4: Note session rail opening and closing tracking', () => {
  openNoteRail('note-a', 'sess-1234');
  assert.equal(noteRailNow().rails['note-a'], 'sess-1234');

  closeNoteRail('note-a');
  assert.equal(noteRailNow().rails['note-a'], undefined);
});
