import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToString } from 'react-dom/server';
import { SessionComposer, formatDroppedPaths, insertDropped, patchComposer, getSessionComposerState } from '../src/plugs/claude/session.tsx';
import { AGENT_BUSY_NOTICE, LIVE_ELSEWHERE_NOTICE, launchNameOf, planSessionShellWith } from '../src/plugs/claude/terminal.ts';
import { COMPOSER_BLOCK_TEXT, agentLabel, composerBlock, normalizeReadSession, pictureLines, railPersona, resumeCommand, sendToSession } from '../src/plugs/claude/sessionLogic.ts';

const ID = '3f2a9c4e-1b2d-4c3e-8f40-00000000a001';

test('a session another CLI opened is read here, resumed there, and never sent to from this box', () => {
  assert.equal(normalizeReadSession({ sessionId: ID, agent: 'codex', turns: [] }).agent, 'codex');
  assert.equal(normalizeReadSession({ sessionId: ID, turns: [] }).agent, 'claude', 'the engine names no agent for Claude');
  assert.equal(composerBlock(undefined, { done: true, file: '/x', agent: 'codex' }), 'other-agent');
  assert.equal(composerBlock(undefined, { done: true, file: '/x', agent: 'claude' }), null, 'a claude session keeps its box');
  assert.equal(composerBlock({ processAlive: true, kind: 'interactive' }, { done: true, file: '/x', agent: 'gemini' }), 'other-agent', 'the agent decides before the live row');
  assert.match(COMPOSER_BLOCK_TEXT['other-agent'], /읽기만/);
  assert.equal(resumeCommand('codex', ID), `codex resume ${ID}`);
  assert.equal(resumeCommand('gemini', ID), `gemini --resume ${ID}`);
  assert.equal(resumeCommand('claude', ID), `claude --resume ${ID}`);
  assert.equal(resumeCommand('gemini', ID, 'antigravity'), `agy --conversation ${ID}`, 'an Antigravity conversation is not a Gemini CLI session');
  assert.equal(resumeCommand('gemini', ID, null), `gemini --resume ${ID}`);
  assert.equal(resumeCommand(undefined, ID), `claude --resume ${ID}`);
  assert.equal(agentLabel('codex'), 'Codex');
  assert.equal(agentLabel(undefined), 'Claude Code');
  const base = { heading: null, noteId: 'n1', sessionId: ID, hasBundle: true };
  assert.match(railPersona({ ...base, agent: 'gemini' }), /Gemini CLI 세션/);
  assert.equal(railPersona(base), railPersona({ ...base, agent: 'claude' }), 'the Claude text did not change');
  assert.match(railPersona(base), /Claude Code 세션/);
});

test('10.1: the composer decision table', () => {
  assert.equal(composerBlock({ processAlive: true, kind: 'interactive' }, { done: false, file: undefined }), 'live-terminal');
  assert.equal(composerBlock({ processAlive: true, kind: 'background' }, { done: false, file: undefined }), 'live-background');
  assert.equal(composerBlock({ processAlive: true, heldByAgent: true, kind: 'interactive' }, { done: true, file: '/x' }), null);
  assert.equal(composerBlock(undefined, { done: true, file: null }), 'no-record');
  assert.equal(composerBlock(undefined, { done: false, file: null }), null, 'no record is only decided after the conversation was read');
  assert.equal(composerBlock({ processAlive: false, kind: 'interactive' }, { done: true, file: '/x' }), null);
  assert.equal(composerBlock({ kind: 'interactive' }, { done: true, file: '/x' }), null, 'not knowing opens it');
  assert.equal(composerBlock({ processAlive: true, kind: 'interactive' }, { done: true, file: null }), 'live-terminal', 'alive is seen before no-record');
});

test('10.1: a closed composer draws its reason on the line with data-block', () => {
  const draw = (row, file) => renderToString(React.createElement(SessionComposer, { sessionId: ID, sessionItem: row, sessionLoaded: true, transcriptFile: file, vault: '/v', timeoutSeconds: () => 3600 }));
  for (const [row, file, block] of [[{ sessionId: ID, processAlive: true, kind: 'interactive' }, '/x', 'live-terminal'], [{ sessionId: ID, processAlive: true, kind: 'background' }, '/x', 'live-background'], [undefined, null, 'no-record']]) {
    const html = draw(row, file);
    assert.match(html, new RegExp(`class="sess-composer-blocked" data-block="${block}"`));
    assert.ok(html.includes(COMPOSER_BLOCK_TEXT[block]));
  }
  const open = draw({ sessionId: ID, processAlive: false, kind: 'interactive' }, '/x');
  assert.match(open, /class="sess-composer"/);
  assert.match(open, /aria-label="세션으로 보내기"/);
  assert.match(open, /aria-label="받아쓰기"/);
});

function fakeDeps(options = {}) {
  const calls = { ask: [], images: [] };
  return {
    calls,
    readList: async () => options.list ?? [],
    firstCwd: async () => options.cwd ?? null,
    writeImage: async (id, name, bytes) => { calls.images.push([id, name, bytes.length]); if (options.imageFails) throw new Error('디스크가 가득 찼다'); return `/data/session-images/${id}/${name}`; },
    askClaude: async (args) => { calls.ask.push(args); return options.answer ?? { ok: true, answer: 'x' }; },
  };
}
const base = { sessionId: ID, text: '  이어서 해 줘  ', images: [], vault: '/v', model: 'claude-sonnet-5-5', effort: 'low', timeoutSeconds: 7200 };

test('10.4 H1: the send calls ask_claude with the eight keys, the first folder only when it was read, and no persona', async () => {
  const deps = fakeDeps({ cwd: '/Users/me/app' });
  const result = await sendToSession(deps, base);
  assert.deepEqual(result, { ok: true, notice: null });
  const args = deps.calls.ask[0];
  assert.deepEqual(Object.keys(args).sort(), ['cwd', 'effort', 'keepAlive', 'model', 'prompt', 'resume', 'resumeOnly', 'sessionId', 'timeoutSeconds', 'vault']);
  assert.equal(args.vault, '/v');
  assert.equal(args.prompt, '이어서 해 줘');
  assert.equal(args.resume, true);
  assert.equal(args.resumeOnly, true);
  assert.equal(args.keepAlive, true);
  assert.equal(args.timeoutSeconds, 7200);
  assert.equal(args.cwd, '/Users/me/app');
  const noCwd = fakeDeps();
  await sendToSession(noCwd, base);
  assert.equal('cwd' in noCwd.calls.ask[0], false, 'no first folder: the key is not sent');
  assert.equal('askId' in noCwd.calls.ask[0], false, 'no send id given: the key is not sent');
});

test('10.4: a send id given by the screen goes to ask_claude, so a stop can name this one send', async () => {
  const deps = fakeDeps();
  await sendToSession(deps, { ...base, askId: 'ask-send-1' });
  assert.equal(deps.calls.ask[0].askId, 'ask-send-1');
  assert.equal(deps.calls.ask[0].sessionId, ID, 'the conversation id is still the session id');
});

test('10.3 / 10.5: the list is read again before sending; a live row refuses before any picture is written; failures keep the box', async () => {
  const live = fakeDeps({ list: [{ sessionId: ID, processAlive: true, kind: 'interactive' }] });
  const image = { name: 'a.png', bytes: new Uint8Array(3) };
  const refused = await sendToSession(live, { ...base, images: [image] });
  assert.equal(refused.ok, false);
  assert.match(refused.error, /이 세션이 지금 다른 터미널에서 돌고 있어서 안 보냈어요/);
  assert.equal(live.calls.images.length, 0);
  assert.equal(live.calls.ask.length, 0);
  const unreadable = fakeDeps({ list: null });
  assert.equal((await sendToSession(unreadable, base)).ok, true, 'a list that could not be read sends');
  const badImage = fakeDeps({ imageFails: true });
  assert.deepEqual(await sendToSession(badImage, { ...base, images: [image] }), { ok: false, error: '붙인 그림을 못 저장해서 안 보냈어요: 디스크가 가득 찼다' });
  const withPicture = fakeDeps();
  await sendToSession(withPicture, { ...base, images: [image] });
  assert.ok(withPicture.calls.ask[0].prompt.endsWith(pictureLines([`/data/session-images/${ID}/a.png`])));
  assert.deepEqual(await sendToSession(fakeDeps({ answer: { ok: false, error: null } }), base), { ok: false, error: '까닭을 못 받았다' });
  const missing = await sendToSession(fakeDeps({ answer: { ok: true, missing_cwd: '/gone' } }), base);
  assert.deepEqual(missing, { ok: true, notice: '이 세션이 처음 돌던 폴더(/gone)가 없어서 vault 에서 이어 갔어요.' });
});

test('10.2: the box of each session is in a store and is removed when it is empty again', () => {
  patchComposer('s-a', { text: '쓰던 글' });
  assert.equal(getSessionComposerState('s-a').text, '쓰던 글');
  assert.equal(getSessionComposerState('s-b').text, '', 'another session is untouched');
  patchComposer('s-a', { text: '' });
  assert.equal(getSessionComposerState('s-a'), getSessionComposerState('s-never'), 'an emptied box is the shared empty one');
});

test('10.7: dropped paths are quoted when they hold a space or a quote and go in at the selection or the end', () => {
  assert.equal(formatDroppedPaths(['/a/b.txt', '/Users/me/my file.png', '/q"x']), '/a/b.txt "/Users/me/my file.png" "/q\\"x"');
  assert.deepEqual(insertDropped('열어 줘', ['/a'], null), { text: '열어 줘 /a ', caret: 8 });
  assert.deepEqual(insertDropped('앞 뒤', ['/a'], { start: 1, end: 2 }), { text: '앞 /a 뒤', caret: 5 }, 'the selection is replaced');
  assert.deepEqual(insertDropped('', ['/a'], null), { text: '/a ', caret: 3 });
});

function planHost(options = {}) {
  const calls = { release: [], start: [] };
  return {
    calls,
    alive: async () => options.alive ?? false,
    vault: async () => ('vault' in options ? options.vault : '/v'),
    refresh: async () => {},
    rows: () => options.rows ?? [],
    firstCwd: async () => options.firstCwd ?? null,
    ...(options.agent !== undefined ? { agentOf: async () => options.agent } : {}),
    release: async (v, id) => { calls.release.push([v, id]); return options.release ?? 'not_live'; },
    start: async (key, cols, rows, action) => { calls.start.push(action); return { fellBack: false }; },
  };
}

test('10.9: the shell plan for term:<session id>', async () => {
  assert.deepEqual(await planSessionShellWith(planHost(), 'term:main', undefined), { kind: 'plain' });
  assert.deepEqual(await planSessionShellWith(planHost({ alive: true }), `term:${ID}`, ID), { kind: 'plain' }, 'a shell that is alive is reattached and nothing is planted');
  const live = await planSessionShellWith(planHost({ rows: [{ sessionId: ID, processAlive: true, cwd: '/x' }] }), `term:${ID}`, ID);
  assert.deepEqual(live, { kind: 'refuse', notice: LIVE_ELSEWHERE_NOTICE });
  const attach = planHost({ rows: [{ sessionId: ID, processAlive: true, id: '753ecd82', cwd: '/x' }] });
  const plan = await planSessionShellWith(attach, `term:${ID}`, ID);
  await plan.start(80, 24, () => {});
  assert.deepEqual(attach.calls.start[0], { mode: 'attach', sessionId: '753ecd82' }, 'the short id, not the session id');
  assert.equal(attach.calls.release.length, 0);
  const resume = planHost({ rows: [{ sessionId: ID, cwd: '/row' }], firstCwd: '/first' });
  await (await planSessionShellWith(resume, `term:${ID}`, ID)).start(80, 24, () => {});
  assert.deepEqual(resume.calls.start[0], { mode: 'resume', sessionId: ID, cwd: '/first' }, 'the first folder wins over the row folder');
  assert.deepEqual(resume.calls.release, [['/v', ID]]);
  const rowOnly = planHost({ rows: [{ sessionId: ID, cwd: '/row' }] });
  await (await planSessionShellWith(rowOnly, `term:${ID}`, ID)).start(80, 24, () => {});
  assert.equal(rowOnly.calls.start[0].cwd, '/row');
  const held = planHost({ rows: [{ sessionId: ID, cwd: '/row', heldByAgent: true }] });
  await (await planSessionShellWith(held, `term:${ID}`, ID)).start(80, 24, () => {});
  assert.equal('cwd' in held.calls.start[0], false, 'an agent-held row gives no folder');
  assert.deepEqual(await planSessionShellWith(planHost({ release: 'busy' }), `term:${ID}`, ID), { kind: 'refuse', notice: AGENT_BUSY_NOTICE });
  // Codex and Gemini sessions resume with their own CLI in their own first folder; Claude's live list and release do not apply.
  const codex = planHost({ agent: 'codex', firstCwd: '/work/proj', rows: [{ sessionId: ID, processAlive: true, cwd: '/x' }] });
  await (await planSessionShellWith(codex, `term:${ID}`, ID)).start(80, 24, () => {});
  assert.deepEqual(codex.calls.start[0], { mode: 'resume', sessionId: ID, agent: 'codex', cwd: '/work/proj' });
  assert.equal(codex.calls.release.length, 0, 'nothing of Claude is released for a codex session');
  const gemini = planHost({ agent: 'gemini' });
  await (await planSessionShellWith(gemini, `term:${ID}`, ID)).start(80, 24, () => {});
  assert.deepEqual(gemini.calls.start[0], { mode: 'resume', sessionId: ID, agent: 'gemini' }, 'no folder key when none was read');
  const antigravity = planHost({ agent: 'antigravity', firstCwd: '/Users/df/github/aisurada' });
  await (await planSessionShellWith(antigravity, `term:${ID}`, ID)).start(80, 24, () => {});
  assert.deepEqual(antigravity.calls.start[0], { mode: 'resume', sessionId: ID, agent: 'antigravity', cwd: '/Users/df/github/aisurada' }, 'an Antigravity conversation resumes in its own folder');
  assert.equal(launchNameOf({ agent: 'gemini', client: 'antigravity' }), 'antigravity', 'the client wins over the shared agent name');
  assert.equal(launchNameOf({ agent: 'gemini' }), 'gemini');
  assert.equal(launchNameOf({ agent: 'codex' }), 'codex');
  assert.equal(launchNameOf({}), null, 'no name is Claude');
  assert.equal(launchNameOf(undefined), null);
  const claudeNamed = planHost({ agent: 'claude', firstCwd: '/first' });
  await (await planSessionShellWith(claudeNamed, `term:${ID}`, ID)).start(80, 24, () => {});
  assert.deepEqual(claudeNamed.calls.start[0], { mode: 'resume', sessionId: ID, cwd: '/first' }, 'a claude session keeps the old action, without an agent key');
  const noVault = planHost({ vault: null, rows: [{ sessionId: ID, cwd: '/row' }] });
  await (await planSessionShellWith(noVault, `term:${ID}`, ID)).start(80, 24, () => {});
  assert.equal(noVault.calls.release.length, 0, 'without a vault nothing is released');
});
