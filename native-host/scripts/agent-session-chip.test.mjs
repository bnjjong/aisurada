import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { NoteSessionChips } from '../src/ui/NoteParts.tsx';

const draw = (note, props = {}) => renderToStaticMarkup(React.createElement(NoteSessionChips, { note, ...props }));
const base = { id: 'n1', title: 't', body: '', agenda: 'a', agendaId: 'a', format: 'markdown', created: '', date: '', time: '' };

test('a claude note keeps its chip as it was: no agent name', () => {
  const html = draw({ ...base, sessionId: 'claude-1' });
  assert.match(html, /data-session-agent="claude"/);
  assert.doesNotMatch(html, /doc-meta-id-agent/);
  assert.match(html, /claude-1/);
});

test('a codex note names the CLI on its chip', () => {
  const html = draw({ ...base, sessionId: 'codex-1', sessionAgent: 'codex' });
  assert.match(html, /data-session-agent="codex"/);
  assert.match(html, /<span class="doc-meta-id-agent">Codex<\/span>/);
});

test('the created and the edited session each carry their own CLI', () => {
  const html = draw({ ...base, sessionId: 'codex-1', sessionAgent: 'codex', editedSessionId: 'gem-9', editedSessionAgent: 'gemini' });
  assert.match(html, /data-session-role="created" data-session-agent="codex"/);
  assert.match(html, /data-session-role="edited" data-session-agent="gemini"/);
  assert.match(html, /Gemini CLI/);
});

test('an edit session with no agent line is claude even when the note was made by codex', () => {
  const html = draw({ ...base, sessionId: 'codex-1', sessionAgent: 'codex', editedSessionId: 'claude-3' });
  assert.match(html, /data-session-role="edited" data-session-agent="claude"/);
});

test('the terminal button is drawn only when the screen can open one', () => {
  const note = { ...base, sessionId: 'codex-1', sessionAgent: 'codex' };
  assert.doesNotMatch(draw(note), /doc-meta-id-term/);
  const html = draw(note, { onOpenTerminal: () => {}, onOpenRail: () => {} });
  assert.match(html, /class="doc-meta-id-term"/);
  assert.match(html, /class="doc-meta-id-open"/);
});

test('a note with no session draws no chip', () => {
  assert.equal(draw(base), '');
});
