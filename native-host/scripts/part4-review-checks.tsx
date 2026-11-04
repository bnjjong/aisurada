// Browser regression checks for the second Part 4 review. Run with Vite at /scripts/part4-review.html.
// Unlike renderToString tests these mount effects, unmount screens, and resolve requests while away.
import React from 'react';
import { createRoot, type Root } from 'react-dom/client';
import { flushSync } from 'react-dom';
import { NoteSessionRail, SessionRoute, PinSessionModal, getRailMemory, patchRailMemory, railMemoryKey } from '../src/plugs/claude/session.tsx';
import { useNoteRail, noteRailNow, openNoteRail, closeNoteRail, setAskThread, setPendingAsk } from '../src/plugs/claude/state.ts';
import { AskContext } from '../src/plugs/claude/askContext.ts';
import { EditorAskPlugins } from '../src/plugs/claude/ask.tsx';
import { AskHoverCard } from '../src/plugs/claude/askHoverCard.tsx';
import { tauriAskHost } from '../src/plugs/claude/askHost.ts';
import { refreshSessions } from '../src/plugs/claude/projects.tsx';

const assert = (value: unknown, why: string) => { if (!value) throw Error(why); };
const wait = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));
async function until(check: () => unknown, why: string) {
  const end = Date.now() + 5000;
  while (Date.now() < end) { if (check()) return; await wait(20); }
  throw Error(`Timed out: ${why}`);
}
const sessionId = '11111111-2222-4333-8444-555555555555';
const readAnswer = { sessionId, file: '/fixture/session.jsonl', source: { bytes: 2048, lines: 12748, messages: 1312, skipped: 0 }, turns: [{ index: 0, who: 'user', text: '이전 물음' }], hasMore: true };
const calls: Array<{ command: string; args: any }> = [];
let answer: ((value: any) => void) | null = null;
let failPin = false;
let listGate: Promise<any> | null = null;
const internals = {
  metadata: { currentWindow: { label: 'main' }, currentWebview: { label: 'main' } },
  transformCallback: () => 1,
  unregisterCallback: () => {},
  convertFileSrc: (path: string) => `data:image/svg+xml,${encodeURIComponent('<svg xmlns="http://www.w3.org/2000/svg" width="20" height="20"><rect width="20" height="20" fill="teal"/></svg>')}#${encodeURIComponent(path)}`,
  invoke: async (command: string, args: any = {}) => {
    calls.push({ command, args });
    if (command === 'ask_claude') return new Promise((resolve) => { answer = resolve; });
    if (command === 'list_claude_agents') return listGate ?? { ok: true, stdout: '[]' };
    if (command === 'alive_pids' || command === 'held_agent_sessions' || command === 'scan_text_shapes') return [];
    if (command === 'sidecar_call') {
      const argv = args.argv;
      if (argv[0] === 'read-session') {
        if (failPin && !argv.includes('urls=true')) throw Error('fixture pin read failure');
        return { status: 0, stderr: '', stdout: JSON.stringify(readAnswer) };
      }
      if (argv[0] === 'list-chats') return { status: 0, stdout: '{"chats":[],"unreadable":[]}' };
      if (argv[0] === 'append-chat-turn') return { status: 4, stderr: 'fixture offline', stdout: '' };
      if (argv[0] === 'session-titles') return { status: 0, stdout: '{"sessions":[]}' };
      if (argv[0] === 'recent-sessions') return { status: 0, stdout: '{"sessions":[],"dropped":0}' };
    }
    if (command === 'ask_runtime') return 'cli';
    return null;
  },
};
(globalThis as any).isTauri = true;
(window as any).__TAURI_INTERNALS__ = internals;
const host = document.getElementById('probe')!;
let root: Root | null = null;
const mount = (node: React.ReactNode) => {
  if (root) flushSync(() => root!.unmount());
  root = createRoot(host, { onUncaughtError: (error) => { (window as any).__PROBE_ERROR__ = String(error); } });
  flushSync(() => root!.render(node));
};
const unmount = () => { if (root) flushSync(() => root!.unmount()); root = null; };
const text = (selector: string, value: string) => {
  const field = host.querySelector<HTMLTextAreaElement>(selector)!;
  Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')!.set!.call(field, value);
  field.dispatchEvent(new Event('input', { bubbles: true }));
};
const results: Array<{ id: string; ok: boolean; error?: string }> = [];
async function test(id: string, run: () => Promise<void>) {
  try { await run(); results.push({ id, ok: true }); }
  catch (error) { results.push({ id, ok: false, error: String(error) }); }
  unmount();
}

await test('note-rail-client-snapshot', async () => {
  let renders = 0;
  function Probe() { const state = useNoteRail(); renders++; return <p>{state.rails.note ?? 'closed'}</p>; }
  mount(<Probe />);
  await wait(80);
  assert(renders < 5 && !(window as any).__PROBE_ERROR__, 'unstable external-store snapshot');
  assert(noteRailNow() === noteRailNow(), 'snapshot identity changed without a write');
  openNoteRail('note', sessionId);
  await until(() => host.textContent?.includes(sessionId), 'rail notification');
  closeNoteRail('note');
  await until(() => host.textContent === 'closed', 'rail close notification');
});

await test('rail-draft-pictures-request-unsaved-survive-remount', async () => {
  const key = railMemoryKey('/v', 'ndfixture', undefined, sessionId);
  const props = { sessionId, chat: { target: { noteId: 'ndfixture' }, vault: '/v', noteDir: '/v/note' } };
  mount(<NoteSessionRail {...props} />);
  await until(() => host.textContent?.includes('이전 물음'), 'session read');
  text('textarea', '임시 물음');
  await until(() => getRailMemory(key).text === '임시 물음', 'draft store');
  patchRailMemory(key, { images: [{ file: new File(['x'], 'picture.png', { type: 'image/png' }), preview: 'data:image/png;base64,eA==' }] });
  unmount(); mount(<NoteSessionRail {...props} />);
  assert(host.querySelector('textarea')?.value === '임시 물음', 'draft lost on remount');
  assert(host.querySelector('.ask-image-thumb img'), 'draft picture lost on remount');
  patchRailMemory(key, { images: [] });
  await wait(20);
  host.querySelector<HTMLButtonElement>('.sess-composer-send')!.click();
  await until(() => answer, 'outgoing request');
  const askCount = calls.filter((call) => call.command === 'ask_claude').length;
  unmount(); mount(<NoteSessionRail {...props} />);
  assert(host.querySelector('textarea')?.disabled, 'pending request did not disable composer');
  assert(host.textContent?.includes('임시 물음'), 'pending question lost');
  host.querySelector<HTMLButtonElement>('.sess-composer-send')!.click();
  assert(calls.filter((call) => call.command === 'ask_claude').length === askCount, 'duplicate resume request');
  unmount();
  answer!({ ok: true, answer: '탭 밖에서 온 답', elapsed_ms: 20 }); answer = null;
  await until(() => !getRailMemory(key).running, 'offscreen completion');
  mount(<NoteSessionRail {...props} />);
  await until(() => host.textContent?.includes('탭 밖에서 온 답'), 'unsaved answer restored');
  assert(host.textContent?.includes('못 적었어요'), 'failed append was lost');
  await wait(80);
  assert(host.textContent?.includes('탭 밖에서 온 답'), 'reload erased unsaved turn');
});

await test('hover-card-pinned-draft-resize', async () => {
  setAskThread('ndcard', { noteId: 'ndcard', sessionId, turns: [{ id: 'akcard-00000001', askedAt: '', question: 'q', answer: 'a', model: 'claude-sonnet-5-5', effort: 'low', elapsedMs: 0 }] });
  let pinned = false;
  let savedSize: number[] = [];
  mount(<AskContext.Provider value={{ baseDir: '/v/note', saveCardSize: (width, height) => { savedSize = [width, height]; } }}><AskHoverCard docId="ndcard" turnId="akcard-00000001" onFollowUp={() => {}} onPin={() => { pinned = true; }} /></AskContext.Provider>);
  host.querySelector('.ask-hovercard')!.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true }));
  await until(() => host.querySelector('textarea'), 'follow up after pin');
  text('textarea', '계속 쓸 글');
  host.querySelector('.ask-hovercard')!.dispatchEvent(new MouseEvent('mouseleave', { bubbles: false }));
  await wait(240);
  assert(pinned && host.querySelector('textarea')?.value === '계속 쓸 글', 'pinned draft vanished');
  const grip = host.querySelector('[aria-label="문답 카드 크기 조절"]');
  assert(grip, 'resize handle missing');
  grip!.dispatchEvent(new PointerEvent('pointerdown', { bubbles: true, clientX: 0, clientY: 0 }));
  document.dispatchEvent(new PointerEvent('pointermove', { clientX: 2000, clientY: 2000 }));
  document.dispatchEvent(new PointerEvent('pointerup'));
  assert(savedSize[0] === 720 && savedSize[1] === 1600, `resize bounds were not saved: ${savedSize}`);
});

await test('busy-mark-decoration-settles', async () => {
  setPendingAsk('ndbusy', 'akbusy-00000001', { at: Date.now(), sessionId });
  mount(<div className="lex-editor"><span className="lex-ask" data-ask-id="akbusy-00000001">대목</span><EditorAskPlugins docId="ndbusy" /></div>);
  await until(() => host.querySelector('.lex-ask-busy'), 'busy ring');
  let changes = 0;
  const observer = new MutationObserver((records) => { changes += records.length; });
  observer.observe(host, { subtree: true, attributes: true });
  await wait(150); observer.disconnect();
  assert(changes < 3, `decoration kept repainting (${changes})`);
  setPendingAsk('ndbusy', 'akbusy-00000001', null);
});

await test('list-read-awaits-inflight-request', async () => {
  await refreshSessions('/v');
  let release!: (value: any) => void;
  listGate = new Promise((resolve) => { release = resolve; });
  const first = refreshSessions('/v');
  const second = refreshSessions('/v');
  let finished = false;
  void second.then(() => { finished = true; });
  await wait(30);
  assert(!finished, 'second refresh returned before in-flight read');
  release({ ok: true, stdout: '[]' });
  await Promise.all([first, second]); listGate = null;
});

await test('pin-reread-isolated-retry-clears-counts', async () => {
  failPin = true;
  mount(<PinSessionModal sessionId={sessionId} controller={{}} state={{ workspace: { agendas: [], notes: [] } }} onClose={() => {}} />);
  await until(() => host.querySelector('.pin-warn'), 'pin failure');
  assert(!host.querySelector('.pin-hint,.pin-size'), 'failure left fake zero counts');
  failPin = false;
  host.querySelector<HTMLButtonElement>('.btn-range')!.click();
  await until(() => host.querySelector('.pin-hint'), 'same range retried');
  assert(host.querySelector('.pin-hint')?.textContent?.includes('1건'), 'retry did not read');
});

await test('session-cli-pane-survives-collapsing', async () => {
  let prefs = { sessionPreviewOpen: true };
  const state = () => ({ workspace: { path: '/v', notes: [], agendas: [] }, prefs });
  const controller = { preference: (next: any) => { prefs = { ...prefs, ...next }; } };
  mount(<SessionRoute sessionId={sessionId} controller={controller} state={state()} />);
  const cli = [...host.querySelectorAll<HTMLButtonElement>('.prev-tab')].find((button) => button.textContent === 'CLI')!;
  cli.click();
  await until(() => host.querySelector('.is-cli:not([hidden])'), 'CLI becomes visible');
  const pane = host.querySelector('.is-cli');
  assert(pane, 'CLI cell not mounted');
  await until(() => calls.some((call) => call.command === 'pty_alive'), 'terminal mount effect');
  const detaches = calls.filter((call) => call.command === 'pty_detach').length;
  prefs.sessionPreviewOpen = false;
  flushSync(() => root!.render(<SessionRoute sessionId={sessionId} controller={controller} state={state()} />));
  await wait(80);
  assert(host.querySelector('.is-cli') === pane, 'collapse replaced CLI cell');
  assert(calls.filter((call) => call.command === 'pty_detach').length === detaches, 'collapse detached the shell');
  assert(host.querySelector<HTMLElement>('.prevcol')?.style.display === 'none', 'rail not hidden');
});

const summary = document.getElementById('results')!;
summary.textContent = JSON.stringify(results, null, 2);
summary.dataset.done = 'true';
summary.dataset.ok = String(results.every((result) => result.ok));
(window as any).__PART4_RESULTS__ = results;
// Keep the real production bridge untouched; only this standalone fixture runs with fake commands.
void tauriAskHost;
