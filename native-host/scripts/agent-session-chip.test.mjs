import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { NoteSessionChips } from '../src/ui/NoteParts.tsx';
import { AgentMark, noteAgents } from '../src/ui/AgentMark.tsx';

const draw = (note, props = {}) => renderToStaticMarkup(React.createElement(NoteSessionChips, { note, ...props }));
const base = { id: 'n1', title: 't', body: '', agenda: 'a', agendaId: 'a', format: 'markdown', created: '', date: '', time: '' };

test('a claude note names Claude Code on its chip even though its file has no sessionAgent line', () => {
  const html = draw({ ...base, sessionId: 'claude-1' });
  assert.match(html, /data-session-agent="claude"/);
  assert.match(html, /<span class="doc-meta-id-agent">Claude Code<\/span>/);
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
  assert.match(html, /Claude Code/);
  assert.match(html, /Codex/);
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

test('the list marks a note with a session id and no agent line as Claude', () => {
  assert.deepEqual(noteAgents({ ...base, sessionId: 'claude-1' }), [{ agent: 'claude', role: 'created' }]);
});

test('the list marks a gemini note and names the role in the tooltip', () => {
  const [info] = noteAgents({ ...base, sessionId: 'gem-1', sessionAgent: 'gemini' });
  assert.deepEqual(info, { agent: 'gemini', role: 'created' });
  const html = renderToStaticMarkup(React.createElement(AgentMark, { info }));
  assert.match(html, /data-agent="gemini"/);
  assert.match(html, /title="Gemini CLI가 작성"/);
});

test('the list marks a note only edited by an agent as edited', () => {
  assert.deepEqual(noteAgents({ ...base, editedSessionId: 'claude-3' }), [{ agent: 'claude', role: 'edited' }]);
});

test('the list shows one mark when the same CLI created and edited the note', () => {
  const note = { ...base, sessionId: 'claude-1', editedSessionId: 'claude-2' };
  assert.deepEqual(noteAgents(note), [{ agent: 'claude', role: 'both' }]);
});

test('the list shows both CLIs when they differ, creator first', () => {
  const note = { ...base, sessionId: 'gem-1', sessionAgent: 'gemini', editedSessionId: 'claude-3' };
  assert.deepEqual(noteAgents(note), [
    { agent: 'gemini', role: 'created' },
    { agent: 'claude', role: 'edited' },
  ]);
});

test('the list ignores a repeated edit session id and keeps the creator only', () => {
  const note = { ...base, sessionId: 'gem-1', sessionAgent: 'gemini', editedSessionId: 'gem-1', editedSessionAgent: 'gemini' };
  assert.deepEqual(noteAgents(note), [{ agent: 'gemini', role: 'created' }]);
});

test('the list draws no mark for a note written by a person', () => {
  assert.deepEqual(noteAgents(base), []);
  assert.deepEqual(noteAgents({ ...base, sessionId: '  ' }), []);
});

test('a codex note gets the neutral terminal mark instead of a brand shape', () => {
  const [info] = noteAgents({ ...base, sessionId: 'codex-1', sessionAgent: 'codex' });
  const html = renderToStaticMarkup(React.createElement(AgentMark, { info }));
  assert.match(html, /data-agent="other"/);
  assert.match(html, /title="Codex가 작성"/);
});
