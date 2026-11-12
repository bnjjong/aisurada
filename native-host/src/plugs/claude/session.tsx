// Implementation of @claude-plug/session for Part 4 Claude-enabled builds.
// Adheres to Chapters 8, 9, 10 and 17 of aisurada Part 4 Claude integration specifications.
// The decisions live in sessionLogic.ts and transcriptLines.ts; this file draws them and calls the host.
import React, { Suspense, lazy, useCallback, useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { openUrl } from '@tauri-apps/plugin-opener';
import { Icon } from '../../ui/Icon.tsx';
import { Modal } from '../../ui/Modal.tsx';
import { agendaOrder, HUE_COLORS } from '../../ui/model.ts';
import { AskContext } from './askContext.ts';
import { askClaudeBridge, tauriAskHost } from './askHost.ts';
import { AskImageStrip, useAskImages } from './askPickers.tsx';
import type { PickedImage } from './askPickers.tsx';
import { writePictures } from './askRun.ts';
import { ASK_EFFORTS, ASK_MODELS, MAX_IMAGES, defaultEffortForModel, humanizeAskError, isGeminiModel, modelLabel, normalizeEffort, normalizeModel, randomUuid } from './askText.ts';
import { ModelProse } from './modelProse.tsx';
import { PendingLine } from './askParts.tsx';
import { getSessionStoreSnapshot, refreshSessions, useSessionList, useSessionListSync, useSessionStore } from './sessionStore.ts';
import { askNow, setAskEffort, setAskModel, useAskState } from './state.ts';
import {
  COMPOSER_BLOCK_TEXT,
  agentBarText,
  composerBlock,
  resumeCommand,
  formatBytes,
  formatDuration,
  formatTurnTime,
  kindLabel,
  normalizeReadSession,
  pinBody,
  pinDefaultTitle,
  pinScanText,
  railChatSessionId,
  railHeading,
  railPersona,
  railPrompt,
  railSeed,
  railTurnFrom,
  railTurnId,
  sendToSession,
  sinceText,
  statusCell,
  todoBarText,
} from './sessionLogic.ts';
import type { ComposerBlock, RailChat, RailTurn, ReadSessionView } from './sessionLogic.ts';
import type { ClaudeSession } from './sessionTypes.ts';
import { railBundleText } from './transcriptLines.ts';
import type { EngineTurn } from './transcriptLines.ts';

export { formatBytes, formatDuration, formatTurnTime };

// The terminal cell brings its own style sheet and the xterm library; it is loaded when a CLI pane first shows it.
const TerminalHost = lazy(() => import('../../ui/terminal/TerminalHost.tsx').then((module) => ({ default: module.TerminalHost })));

// --------------------------------------------------------------------------
// The engine and the host
// --------------------------------------------------------------------------

async function callEngine<T = any>(argv: string[]): Promise<T> {
  const reply = await tauriAskHost.engine(argv);
  if (reply.status === 0 && reply.stdout) return JSON.parse(reply.stdout) as T;
  throw new Error(reply.stderr?.trim() || '사이드카 명령 실패');
}

/** `session-titles <id>` and its `cwd`; `null` when it cannot be read or is absent (7.10). */
async function readFirstCwd(sessionId: string): Promise<string | null> {
  try {
    const reply = await callEngine<{ sessions?: Array<{ cwd?: string | null }> }>(['session-titles', sessionId]);
    const cwd = reply.sessions?.[0]?.cwd;
    return typeof cwd === 'string' && cwd ? cwd : null;
  } catch {
    return null;
  }
}

function vaultPathOf(state: any): string {
  const workspace = state?.workspace;
  return workspace && !workspace.sample ? String(workspace.path ?? '') : '';
}

// --------------------------------------------------------------------------
// 9.2: reading a session, with a generation number and no flash of the old conversation
// --------------------------------------------------------------------------

interface ReadState {
  sessionId: string;
  view: ReadSessionView | null;
  error: string | null;
}

/**
 * Reads `read-session <id> limit=<n> urls=true`. A read that is still running when the session changes, the screen goes
 * away or a newer read starts is not shown (the generation number). While a re-read runs the old conversation stays; if the
 * re-read fails the old one is kept, and with none to keep the failure is shown. A held value that belongs to another session
 * reads as "loading".
 */
export function useReadSession(sessionId: string, limit: number, signal: number): { view: ReadSessionView | null; loading: boolean; error: string | null } {
  const [held, setHeld] = useState<ReadState>({ sessionId: '', view: null, error: null });
  const generation = useRef(0);
  useEffect(() => {
    if (!sessionId || !isTauri()) {
      setHeld({ sessionId, view: null, error: sessionId ? '이 화면에서는 대화를 읽을 수 없어요(앱에서만 돼요).' : null });
      return;
    }
    const mine = ++generation.current;
    callEngine(['read-session', sessionId, `limit=${limit}`, 'urls=true'])
      .then((raw) => {
        if (generation.current === mine) setHeld({ sessionId, view: normalizeReadSession(raw), error: null });
      })
      .catch((error) => {
        if (generation.current !== mine) return;
        setHeld((prev) => (prev.sessionId === sessionId && prev.view ? prev : { sessionId, view: null, error: error?.message || String(error) }));
      });
    return () => {
      generation.current += 1;
    };
  }, [sessionId, limit, signal]);
  const mine = held.sessionId === sessionId;
  return { view: mine ? held.view : null, loading: !mine || (!held.view && !held.error), error: mine ? held.error : null };
}

// --------------------------------------------------------------------------
// 9.4: a transcript turn
// --------------------------------------------------------------------------

export function TranscriptTurn({ turn, model }: { turn: EngineTurn; model?: string }): React.JSX.Element | null {
  if (turn.who === 'tool') {
    if (!turn.tool) return null;
    const failed = turn.tool.failed === true;
    const seconds = turn.tool.seconds;
    return (
      <div className={`toolrow${failed ? ' is-failed' : ''}`}>
        <span className="tr-ic" data-state={failed ? 'failed' : seconds != null ? 'done' : 'unknown'}>{failed ? '✕' : seconds != null ? '✓' : '○'}</span>
        <span className="tr-name">{turn.tool.name}</span>
        {turn.tool.arg && <span className="tr-arg" title={turn.tool.arg}>{turn.tool.arg}</span>}
        {seconds != null && <span className="tr-ms">{formatDuration(seconds)}</span>}
      </div>
    );
  }
  const user = turn.who === 'user';
  const authorName = user ? '나' : isGeminiModel(model) ? 'Gemini' : 'Claude';
  return (
    <div className={`sess-turn ${user ? 'sess-turn-user' : 'sess-turn-assistant'}`}>
      <div className="sess-turn-head">
        <span className="sess-turn-author">{authorName}</span>
        <span className="sess-turn-at">{formatTurnTime(turn.at)}</span>
        {turn.truncated && <span className="sess-turn-cut">{turn.chars ?? 2000}자 중 앞부분</span>}
      </div>
      <div className="sess-turn-text">{user ? turn.text : <ModelProse content={turn.text ?? ''} />}</div>
    </div>
  );
}

/** The four states of the conversation column (9.4). */
export function TranscriptBody({ read }: { read: { view: ReadSessionView | null; loading: boolean; error: string | null } }): React.JSX.Element {
  const { view, loading, error } = read;
  if (error && !view) return <div className="sess-page-empty" title={error}>대화 기록을 못 읽었어요. 동기화 엔진이 안 떠 있을 수 있어요.</div>;
  if (loading || !view) return <div className="sess-page-empty">대화 기록을 읽는 중이에요.</div>;
  if (view.file === null) return <div className="sess-page-empty">이 기계에 그 세션의 기록이 없어요. 다른 기기에서 돌던 세션일 수 있어요.</div>;
  if (view.turns.length === 0) return <div className="sess-page-empty">기록은 있는데 주고받은 말이 없어요.</div>;
  return (
    <>
      <div className="sess-page-source">
        {formatBytes(view.source.bytes)} 중에서 골랐어요 · {view.source.lines.toLocaleString('en-US')}줄에서 {view.source.messages.toLocaleString('en-US')}건
        {view.source.skipped > 0 && <strong> · 못 읽은 줄 {view.source.skipped}</strong>}
      </div>
      {view.hasMore && <div className="sess-page-hasmore">앞의 대화가 더 있어요. 지금은 최근 {view.turns.length.toLocaleString('en-US')}건만 보여요.</div>}
      <div className="sess-page-turns">
        {view.turns.map((turn) => (
          <TranscriptTurn key={turn.index} turn={turn} model={view.runState?.model} />
        ))}
      </div>
    </>
  );
}

// --------------------------------------------------------------------------
// 9.3, 8.4: the session's title and status, one part for the header and the rail
// --------------------------------------------------------------------------

export function sessionTitle(row: ClaudeSession | undefined): string {
  if (!row) return '지금 안 돌고 있는 세션';
  return row.title || row.name || '(이름 없는 세션)';
}

function StatusCell({ row }: { row: ClaudeSession }): React.JSX.Element {
  const cell = statusCell(row);
  return <span className={`sess-page-status${cell.ended ? ' sess-ended' : ''}`}>{cell.text}</span>;
}

/** The last-turn line (9.3): drawn from `runState` whether or not the session is in the list. */
function RunStateLine({ runState }: { runState: ReadSessionView['runState'] }): React.JSX.Element | null {
  if (!runState) return null;
  const cleanModel = runState.model ? runState.model.replace(/\s*\[.*\]$/, '') : '';
  const displayModel = cleanModel ? modelLabel(cleanModel) : '';
  return (
    <div className="sess-page-runstate">
      <span>마지막 회차</span>
      {displayModel && <span>{displayModel}</span>}
      {runState.effort && <span>{runState.effort}</span>}
      {runState.contextTokens !== undefined && <span>컨텍스트 {runState.contextTokens.toLocaleString()}토큰</span>}
      {runState.quotaBlocked && (
        <span className="sess-page-runstate-quota" title={runState.quotaResetsAt ? `재설정 예정: ${formatTurnTime(new Date(runState.quotaResetsAt * 1000).toISOString())}` : undefined}>
          사용량 한도에 걸린 적 있음
        </span>
      )}
    </div>
  );
}

// --------------------------------------------------------------------------
// 10.2: the composer state of each session lives in a store, keyed by session id
// --------------------------------------------------------------------------

export interface ComposerState {
  text: string;
  /** Sending: set at the press, cleared with the result. */
  sending: boolean;
  /** The id of the send in flight. The stop button names it so a stop that arrives late cannot cut the next send. */
  askId?: string;
  error: string | null;
  notice: string | null;
  images: PickedImage[];
  imageNotice: string | null;
}

const EMPTY_COMPOSER: ComposerState = { text: '', sending: false, error: null, notice: null, images: [], imageNotice: null };
const composers = new Map<string, ComposerState>();
const composerListeners = new Set<() => void>();
/** The answer signal: how many answers each session has received (10.2, 9.2). */
const replySignals = new Map<string, number>();
const composerInputs = new Map<string, HTMLTextAreaElement>();

function emitComposers() {
  for (const listener of [...composerListeners]) listener();
}

export function getSessionComposerState(sessionId: string): ComposerState {
  return composers.get(sessionId) ?? EMPTY_COMPOSER;
}

/** 10.2: a session's box is removed when every field is empty again, or sent boxes would pile up. */
export function patchComposer(sessionId: string, patch: Partial<ComposerState>): void {
  const next = { ...getSessionComposerState(sessionId), ...patch };
  const empty = next.text === '' && !next.sending && next.error === null && next.notice === null && next.images.length === 0 && next.imageNotice === null;
  if (empty) composers.delete(sessionId);
  else composers.set(sessionId, next);
  emitComposers();
}

function useComposer(sessionId: string): ComposerState {
  return useSyncExternalStore(
    (listener) => {
      composerListeners.add(listener);
      return () => composerListeners.delete(listener);
    },
    () => getSessionComposerState(sessionId),
    () => getSessionComposerState(sessionId)
  );
}

function useReplySignal(sessionId: string): number {
  return useSyncExternalStore(
    (listener) => {
      composerListeners.add(listener);
      return () => composerListeners.delete(listener);
    },
    () => replySignals.get(sessionId) ?? 0,
    () => replySignals.get(sessionId) ?? 0
  );
}

/** 10.7: dropped paths: each one quoted when it holds a space or a quote, joined by one space, put at the selection or the end. */
export function formatDroppedPaths(paths: string[]): string {
  return paths.map((path) => (/[\s"]/.test(path) ? `"${path.replace(/"/g, '\\"')}"` : path)).join(' ');
}

export function insertDropped(text: string, paths: string[], selection: { start: number; end: number } | null): { text: string; caret: number } {
  const insert = formatDroppedPaths(paths);
  const start = selection ? selection.start : text.length;
  const end = selection ? selection.end : text.length;
  const before = text.slice(0, start);
  const after = text.slice(end);
  const lead = before && !/\s$/.test(before) ? ' ' : '';
  const trail = after === '' ? ' ' : /^\s/.test(after) ? '' : ' ';
  const piece = `${lead}${insert}${trail}`;
  return { text: before + piece + after, caret: before.length + piece.length };
}

// --------------------------------------------------------------------------
// 10: the composer
// --------------------------------------------------------------------------

export function SessionComposer({
  sessionId,
  sessionItem,
  sessionLoaded,
  transcriptFile,
  agent,
  vault,
  timeoutSeconds,
}: {
  sessionId: string;
  sessionItem: ClaudeSession | undefined;
  sessionLoaded: boolean;
  transcriptFile: string | null | undefined;
  /** The CLI that opened the session (`claude` when the engine named none). */
  agent?: string;
  vault: string;
  timeoutSeconds: () => number;
  /** Kept for the callers that still pass them. */
  sessionData?: unknown;
  state?: unknown;
  onSent?: () => void;
}): React.JSX.Element {
  const composer = useComposer(sessionId);
  const pictures = useAskImages({ hasNoteFolder: true, http: false });
  const input = useRef<HTMLTextAreaElement>(null);
  const block: ComposerBlock | null = composerBlock(sessionItem, { done: sessionLoaded, file: transcriptFile, agent });

  // The picture strip lives in the store so it follows the session, not the box (10.2).
  useEffect(() => {
    if (input.current) composerInputs.set(sessionId, input.current);
    return () => {
      composerInputs.delete(sessionId);
    };
  }, [sessionId, block]);

  if (block) {
    return (
      <div className="sess-composer-blocked" data-block={block}>
        {COMPOSER_BLOCK_TEXT[block]}
      </div>
    );
  }

  const send = () => {
    // 10.3 step 1: read the box from the store again, so two presses in one tick are stopped.
    const current = getSessionComposerState(sessionId);
    const text = current.text.trim();
    if (!text || !vault || current.sending) return;
    const model = askNow().model;
    if (model.toLowerCase().includes('gemini')) {
      patchComposer(sessionId, { error: '세션 이어 쓰기는 Claude만 돼요' });
      return;
    }
    const held = current.images; // 10.3 step 2
    const askId = randomUuid(); // this one send's own id, for the stop button
    patchComposer(sessionId, { text: '', images: [], sending: true, askId, error: null, notice: null });
    void (async () => {
      const bytes = await Promise.all(held.map(async (image) => ({ name: pictureFileName(image.file), bytes: new Uint8Array(await image.file.arrayBuffer()) })));
      const result = await sendToSession(
        {
          readList: async () => {
            await refreshSessions(vault);
            const snapshot = getSessionStoreSnapshot();
            return snapshot.status === 'ok' ? snapshot.sessions : null;
          },
          firstCwd: readFirstCwd,
          writeImage: (id, name, data) => invoke<string>('write_session_image', data, { headers: { 'x-ais-session-id': encodeURIComponent(id), 'x-ais-name': encodeURIComponent(name) } }),
          askClaude: (args) => askClaudeBridge(args),
        },
        { sessionId, text, images: bytes, vault, model: askNow().model, effort: askNow().effort, timeoutSeconds: timeoutSeconds(), askId }
      );
      if (result.ok) {
        if (result.cancelled) {
          replySignals.set(sessionId, (replySignals.get(sessionId) ?? 0) + 1);
          patchComposer(sessionId, { text, images: held, sending: false, error: null, notice: result.notice });
        } else {
          // 10.5: the box, its pictures and the picture notice are emptied; the answer signal rises so the screen reads again.
          replySignals.set(sessionId, (replySignals.get(sessionId) ?? 0) + 1);
          patchComposer(sessionId, { text: '', sending: false, error: null, notice: result.notice, images: [], imageNotice: null });
        }
      } else {
        const err = humanizeAskError(result.error);
        patchComposer(sessionId, { text, images: held, sending: false, error: err.message });
      }
    })();
  };

  const startDictation = async () => {
    // 10.8: the press does not take focus from the box (mousedown is stopped below); it is a no-op while sending.
    if (getSessionComposerState(sessionId).sending) return;
    input.current?.focus();
    try {
      const turnedOn = await invoke<boolean>('start_dictation');
      if (!turnedOn) patchComposer(sessionId, { error: '받아쓰기를 못 켰어요. 입력칸에 커서를 두고 시스템 받아쓰기 단축키를 눌러 주세요.' });
    } catch (error) {
      patchComposer(sessionId, { error: `받아쓰기를 못 켰어요. ${typeof error === 'string' ? error : error instanceof Error ? error.message : String(error)}` });
    }
  };

  const held = composer.images;
  return (
    <div className="sess-composer">
      {composer.notice && <div className="sess-composer-notice">{composer.notice}</div>}
      {composer.error && <div className="sess-composer-error">{composer.error}</div>}
      <AskImageStrip images={held} refusal={composer.imageNotice ?? pictures.refusal} onRemove={(index) => patchComposer(sessionId, { images: held.filter((_, at) => at !== index) })} />
      <div className="sess-composer-box">
        <textarea
          ref={input}
          className="sess-composer-input"
          aria-label="세션으로 보내기"
          placeholder="Claude에게 이어 물어보기…"
          value={composer.text}
          disabled={composer.sending}
          rows={3}
          onChange={(event) => patchComposer(sessionId, { text: event.target.value, error: null })}
          onKeyDown={(event) => {
            if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
              event.preventDefault();
              send();
            }
          }}
          onPaste={(event) => {
            const files = [...(event.clipboardData?.files ?? [])].filter((file) => file.type.startsWith('image/'));
            if (files.length === 0) return;
            event.preventDefault();
            // The picture goes to the session of the moment of pasting, even if the screen moves on while it is read.
            void (async () => {
              let taken = getSessionComposerState(sessionId).images;
              let notice: string | null = null;
              for (const file of files) {
                if (taken.length >= MAX_IMAGES) {
                  notice = '한 물음에 그림은 8장까지예요.';
                  continue;
                }
                if (!(file.type in IMAGE_TYPES)) {
                  notice = `${file.name || '이 파일'} 은 못 붙여요.`;
                  continue;
                }
                if (file.size > 5_000_000) {
                  notice = `그림 한 장은 5.0MB까지예요(이 그림은 ${(file.size / 1_000_000).toFixed(1)}MB).`;
                  continue;
                }
                taken = [...taken, { file, preview: await readAsDataUrl(file) }];
              }
              patchComposer(sessionId, { images: taken, imageNotice: notice });
            })();
          }}
        />
        <div className="sess-composer-actions">
          <ModelEffortPicks />
          <ComposerKeys />
          <button
            type="button"
            className="sess-composer-mic"
            title="받아쓰기"
            aria-label="받아쓰기"
            disabled={composer.sending}
            onMouseDown={(event) => event.preventDefault()}
            onClick={() => void startDictation()}
          >
            <Icon name="mic" size={16} />
          </button>
          {composer.sending ? (
            <button
              type="button"
              className="sess-composer-stop"
              aria-label="멈추기"
              onClick={() => void invoke('cancel_ask', { sessionId, ...(composer.askId ? { askId: composer.askId } : {}) })}
            >
              멈추기
            </button>
          ) : (
            <button type="button" className="sess-composer-send" aria-label="보내기" disabled={!composer.text.trim()} onClick={send}>
              보내기
              <Icon name="arrowUp" />
            </button>
          )}
        </div>
      </div>
    </div>
  );
}

/**
 * 입력창 도구줄에 놓는 단축키 안내 칩입니다. 긴 안내를 안내 글자(placeholder)에 넣으면 글자가 흐려 보여
 * 비활성처럼 읽혀서, 그림 붙이기(⌘V)와 보내기(⌘/Ctrl+Enter)를 칩으로 옮겼습니다.
 */
function ComposerKeys({ enter = '보내기' }: { enter?: string }): React.JSX.Element {
  return (
    <span className="sess-composer-keys" title={`⌘V로 그림 붙이기, ⌘/Ctrl+Enter로 ${enter}`}>
      <span className="sess-composer-hint">
        <kbd>⌘V</kbd>
        <Icon name="image" />
      </span>
      <span className="sess-composer-hint">
        <kbd>⌘↵</kbd>
        {enter}
      </span>
    </span>
  );
}

const IMAGE_TYPES: Record<string, true> = { 'image/png': true, 'image/jpeg': true, 'image/gif': true, 'image/webp': true, 'image/avif': true, 'image/bmp': true, 'image/svg+xml': true };

function readAsDataUrl(file: File): Promise<string> {
  return new Promise((resolve) => {
    const reader = new FileReader();
    reader.onload = () => resolve(typeof reader.result === 'string' ? reader.result : '');
    reader.onerror = () => resolve('');
    reader.readAsDataURL(file);
  });
}

const EXTENSIONS: Record<string, string> = { 'image/png': 'png', 'image/jpeg': 'jpg', 'image/gif': 'gif', 'image/webp': 'webp', 'image/avif': 'avif', 'image/bmp': 'bmp', 'image/svg+xml': 'svg' };

/** 10.6: `ask-<time in base 36>-<8 hex of a UUID>.<extension from the MIME>`. */
export function pictureFileName(file: { type: string; name: string }, now: number = Date.now()): string {
  return `ask-${now.toString(36)}-${randomUuid().replace(/-/g, '').slice(0, 8)}.${EXTENSIONS[file.type] ?? 'png'}`;
}

/** The two pickers above the composer: the window's one model and effort (3.3, 10.4). */
function ModelEffortPicks(): React.JSX.Element {
  const model = useAskState((state) => state.model);
  const effort = useAskState((state) => state.effort);
  const isGemini = isGeminiModel(model);

  const handleModelChange = (event: React.ChangeEvent<HTMLSelectElement>) => {
    const nextModel = event.target.value;
    setAskModel(nextModel);
    if (isGeminiModel(nextModel)) {
      setAskEffort(defaultEffortForModel(nextModel));
    }
  };

  const claudeModels = ASK_MODELS.filter((item) => item.provider === 'claude');
  const geminiModels = ASK_MODELS.filter((item) => item.provider === 'gemini');

  return (
    <span className="rchat-options">
      <select className="ask-option" aria-label="모델" value={model} onChange={handleModelChange}>
        <optgroup label="Claude">
          {claudeModels.map((item) => (
            <option key={item.id} value={item.id}>{item.label}</option>
          ))}
        </optgroup>
        <optgroup label="Gemini">
          {geminiModels.map((item) => (
            <option key={item.id} value={item.id}>{item.label}</option>
          ))}
        </optgroup>
      </select>
      <select
        className="ask-option"
        aria-label="effort"
        value={isGemini ? defaultEffortForModel(model) : effort}
        disabled={isGemini}
        title={isGemini ? `Gemini 모델은 ${defaultEffortForModel(model)} effort로 고정됩니다` : undefined}
        onChange={(event) => setAskEffort(event.target.value)}
      >
        {ASK_EFFORTS.map((item) => (
          <option key={item.id} value={item.id}>{item.label}</option>
        ))}
      </select>
    </span>
  );
}

// --------------------------------------------------------------------------
// 8: the rail beside a note
// --------------------------------------------------------------------------

export interface NoteSessionRailProps {
  sessionId: string;
  onClose?: () => void;
  onLeave?: () => void;
  chat?: {
    target: { noteId: string; subId?: string };
    noteMarkdown?: string;
    noteFormat?: string;
    noteDir?: string;
    vault?: string;
    /** The wait limit setting at the moment of the press (3.12). */
    timeoutSeconds?: () => number;
    /** Opens `/session/<id>`; a sub page closes first (8.4). */
    onOpenSession?: (sessionId: string) => void;
    /** The saved width (8.3) and the way to save a new one. */
    width?: number;
    onWidthChange?: (width: number) => void;
  };
}

interface Running {
  chatId: string;
  sessionId?: string;
  /** This one question's own id. See `PendingAsk.askId`: the stop button names it. */
  askId?: string;
  startedAt: number;
  images: number;
  previews: string[];
  question: string;
  timeoutSeconds: number;
}

/** 8.14: the file's list with the memory's laid over it. */
export function mergeChats(file: RailChat[], memory: RailChat[], hadBeforeRead: Set<string>): RailChat[] {
  const byId = new Map(file.map((chat) => [chat.chatId, chat]));
  const merged = file.map((chat) => {
    const mem = memory.find((item) => item.chatId === chat.chatId);
    return mem && mem.turns.length > chat.turns.length ? mem : chat;
  });
  for (const mem of memory) {
    if (byId.has(mem.chatId)) continue;
    if (!hadBeforeRead.has(mem.chatId) || mem.turns.some((turn) => turn.unsaved)) merged.push(mem); // created while the read ran
  }
  return merged;
}

export interface RailMemory {
  chats: RailChat[];
  unreadable: string[];
  chatsError: string | null;
  lastSent: string | null;
  newChat: boolean;
  running: Running | null;
  notice: string | null;
  text: string;
  images: PickedImage[];
  refusal: string | null;
  picked: { model?: string; effort?: string };
}
const railMemory = new Map<string, RailMemory>();
const railMemoryListeners = new Set<() => void>();
export function railMemoryKey(vault: string, noteId: string, subId: string | undefined, sessionId: string): string {
  return JSON.stringify([vault, noteId, subId ?? '', sessionId]);
}
export function getRailMemory(key: string): RailMemory {
  let value = railMemory.get(key);
  if (!value) {
    value = { chats: [], unreadable: [], chatsError: null, lastSent: null, newChat: false, running: null, notice: null, text: '', images: [], refusal: null, picked: {} };
    railMemory.set(key, value);
  }
  return value;
}
export function patchRailMemory(key: string, patch: Partial<RailMemory>): void {
  railMemory.set(key, { ...getRailMemory(key), ...patch });
  railMemoryListeners.forEach((listener) => listener());
}
function subscribeRailMemory(listener: () => void): () => void {
  railMemoryListeners.add(listener);
  return () => { railMemoryListeners.delete(listener); };
}
function useRailMemory(key: string): RailMemory {
  return useSyncExternalStore(subscribeRailMemory, () => getRailMemory(key), () => getRailMemory(key));
}
function railSetter<K extends keyof RailMemory>(key: string, field: K): React.Dispatch<React.SetStateAction<RailMemory[K]>> {
  return (change) => {
    const current = getRailMemory(key)[field];
    const value = typeof change === 'function' ? (change as (current: RailMemory[K]) => RailMemory[K])(current) : change;
    patchRailMemory(key, { [field]: value });
  };
}

export function NoteSessionRail({ sessionId, onClose, chat }: NoteSessionRailProps): React.JSX.Element {
  const [width, setWidth] = useState(() => Math.max(392, Math.min(960, chat?.width ?? 392)));
  const widthRef = useRef(width);
  const dragging = useRef(false);

  const rows = useSessionList();
  const vault = chat?.vault ?? '';
  const sessionItem = useMemo(() => rows.find((row) => row.sessionId === sessionId), [rows, sessionId]);
  // 8.4: the list is read once when the rail opens if it has never been read (a note window has no left rail).
  useSessionListSync(vault || undefined);

  const signal = useReplySignal(sessionId);
  const read = useReadSession(sessionId, 100, signal);
  const noteId = chat?.target.noteId ?? '';
  const subId = chat?.target.subId;

  const memoryKey = railMemoryKey(vault, noteId, subId, sessionId);
  const memory = useRailMemory(memoryKey);
  const { chats, unreadable, chatsError, lastSent, newChat, running, notice, text, picked } = memory;
  const setChats = railSetter(memoryKey, 'chats');
  const setUnreadable = railSetter(memoryKey, 'unreadable');
  const setChatsError = railSetter(memoryKey, 'chatsError');
  const setLastSent = railSetter(memoryKey, 'lastSent');
  const setNewChat = railSetter(memoryKey, 'newChat');
  const setRunning = railSetter(memoryKey, 'running');
  const setNotice = railSetter(memoryKey, 'notice');
  const setText = railSetter(memoryKey, 'text');
  const setPicked = railSetter(memoryKey, 'picked');
  const pictures = useAskImages({ hasNoteFolder: Boolean(chat?.noteDir) && !/\.(?:md|markdown|html|json)$/i.test(chat?.noteDir ?? ''), http: false }, {
    images: memory.images, setImages: railSetter(memoryKey, 'images'), refusal: memory.refusal, setRefusal: railSetter(memoryKey, 'refusal'),
  });
  const body = useRef<HTMLDivElement>(null);

  const reloadChats = useCallback(async () => {
    if (!noteId) return;
    const before = new Set(getRailMemory(memoryKey).chats.map((item) => item.chatId));
    try {
      const reply = await callEngine<{ chats: RailChat[]; unreadable: string[] }>(['list-chats', noteId, ...(subId ? [subId] : [])]);
      setChats(mergeChats(reply.chats ?? [], getRailMemory(memoryKey).chats, before));
      setUnreadable(reply.unreadable ?? []);
      setChatsError(null);
    } catch (error: any) {
      // 8.12: a failed read keeps the chats that are already drawn and writes the reason.
      setChatsError(`대화 기록을 못 읽었어요: ${error?.message ?? error}`);
    }
  }, [memoryKey, noteId, subId]);
  useEffect(() => {
    void reloadChats();
  }, [reloadChats]);

  // 8.5: the chat shown and asked on: the one running, then the last one sent, then none after "새 대화 열기", then the
  // last chat of the file whose source is this session (the list is in order of start).
  const active = useMemo(() => {
    const id = running?.chatId ?? lastSent;
    if (id) return chats.find((item) => item.chatId === id) ?? null;
    if (newChat) return null;
    const mine = chats.filter((item) => item.sourceSessionId === sessionId);
    return mine.length ? mine[mine.length - 1] : null;
  }, [chats, running, lastSent, newChat, sessionId]);
  const turns = active?.turns ?? [];
  const answeredBefore = turns.some((turn) => !turn.error && turn.answer.trim() !== '');
  const first = !answeredBefore;

  // 8.9: picked, then the last turn of this chat, then the session's last run state, then the window's drag-ask value.
  const choice = useMemo(() => {
    const last = turns[turns.length - 1];
    const state = read.view?.runState;
    const sessionModel = state?.model ? state.model.replace(/\s*\[.*\]$/, '') : undefined;
    const model = picked.model ?? (last ? normalizeModel(last.model) : undefined) ?? (sessionModel && ASK_MODELS.some((item) => item.id === sessionModel) ? sessionModel : undefined);
    const effort = picked.effort ?? (last ? normalizeEffort(last.effort) : undefined) ?? (state?.effort && ASK_EFFORTS.some((item) => item.id === state.effort) ? state.effort : undefined);
    const sources = {
      model: picked.model ? 'picked' : last ? 'chat' : model ? 'session' : 'default',
      effort: picked.effort ? 'picked' : last ? 'chat' : effort ? 'session' : 'default',
    };
    const hint = sources.model === sources.effort ? (sources.model === 'chat' ? '이 대화의 값' : sources.model === 'session' ? '세션 값' : '') : '';
    return { model: model ?? askNow().model, effort: effort ?? askNow().effort, hint };
  }, [picked, turns, read.view?.runState]);

  // 8.4: once the conversation is read the column goes to the bottom, once per session; not while the user is scrolled up.
  const scrolled = useRef<string>('');
  useEffect(() => {
    if (read.view && scrolled.current !== sessionId && body.current) {
      body.current.scrollTop = body.current.scrollHeight;
      scrolled.current = sessionId;
    }
  }, [read.view, sessionId]);

  const startResize = (event: React.MouseEvent) => {
    dragging.current = true;
    const startX = event.clientX;
    const startW = (event.currentTarget as HTMLElement).closest('.nsrail')!.getBoundingClientRect().width;
    widthRef.current = startW;
    const move = (e: MouseEvent) => {
      if (!dragging.current) return;
      const next = Math.max(392, Math.min(960, startW + (startX - e.clientX)));
      widthRef.current = next;
      setWidth(next);
    };
    const up = () => {
      dragging.current = false;
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
      chat?.onWidthChange?.(widthRef.current);
    };
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
  };

  const send = async () => {
    const question = text.trim();
    // 8.6 step 1
    if (!question || !vault || getRailMemory(memoryKey).running || (first && read.loading)) return;
    const previews = pictures.images.map((image) => image.preview);
    const files = pictures.images.map((image) => image.file);
    setText('');
    pictures.clear();
    const chatId = active?.chatId ?? randomUuid(); // 3
    const prior = active?.turns ?? []; // 4
    const timeoutSeconds = chat?.timeoutSeconds?.() ?? 3600; // 6
    const askedAt = new Date().toISOString(); // the time of 8.6 step 9
    const turnId = railTurnId(Date.now(), randomUuid());
    const askSessionId = await railChatSessionId(chatId);
    const askId = randomUuid(); // this one question's own id, for the stop button
    setRunning({ chatId, sessionId: askSessionId, askId, startedAt: Date.now(), images: files.length, previews, question, timeoutSeconds }); // 7
    setLastSent(chatId);
    setNewChat(false);
    setNotice(null);
    const model = choice.model;
    const effort = choice.effort;
    try {
      const written = await writePictures(tauriAskHost, chat?.noteDir, files); // 8
      const bundle = read.view ? railBundleText(sessionId, read.view.turns) : '';
      const prompt = railPrompt({ bundle, question, first, imagePaths: written.written.map((relative) => `${chat?.noteDir}/${relative}`) });
      let runtime: string = askNow().runtime ?? 'cli';
      if (!askNow().runtime) {
        try {
          runtime = (await tauriAskHost.askRuntime()) || 'cli';
        } catch {
          runtime = 'cli';
        }
      }
      let result: { ok: boolean; answer?: string; error?: string | null; elapsed_ms?: number; missing_cwd?: string | null };
      if (written.failure) result = { ok: false, error: `붙인 그림을 못 저장해서 묻지 않았어요: ${written.failure}`, elapsed_ms: 0 };
      else if (files.length > 0 && runtime === 'http') result = { ok: false, error: '지금 묻기 길(HTTP)은 그림을 못 실어서 묻지 않았어요.', elapsed_ms: 0 };
      else {
        const cwd = await readFirstCwd(sessionId);
        const seed = first ? undefined : railSeed(prior, bundle);
        result = await askClaudeBridge({
          vault,
          sessionId: await railChatSessionId(chatId),
          askId,
          model,
          effort,
          prompt,
          resume: prior.length > 0,
          timeoutSeconds,
          keepAlive: false,
          persona: railPersona({ heading: railHeading(chat?.noteMarkdown, chat?.noteFormat), noteId, subId, sessionId, hasBundle: bundle !== '', agent: read.view?.agent }),
          ...(cwd ? { cwd } : {}),
          ...(seed ? { seed } : {}),
        });
        if (result.ok && result.missing_cwd) setNotice(`이 세션이 처음 돌던 폴더(${result.missing_cwd})가 없어서 vault 에서 답했어요.`);
      }
      const turn = railTurnFrom({ id: turnId, askedAt, question, model, effort, images: written.written, answer: result });
      await appendTurns(vault, { noteId, subId, chatId, sessionId }, active, [turn], setChats);
    } catch (error) {
      const turn = railTurnFrom({ id: turnId, askedAt, question, model, effort, images: [], answer: { ok: false, error: error instanceof Error ? error.message : String(error), elapsed_ms: 0 } });
      await appendTurns(vault, { noteId, subId, chatId, sessionId }, active, [turn], setChats);
    } finally {
      setRunning(null); // 14
      requestAnimationFrame(() => {
        if (body.current) body.current.scrollTop = body.current.scrollHeight;
      });
    }
  };

  const querying = running !== null;
  const baseDir = chat?.noteDir;
  return (
    <AskContext.Provider value={{ baseDir, noteId }}>
      <aside className="nsrail" data-at-min={width <= 392 ? true : undefined} style={{ width: `${width}px` }}>
        <div className="nsrail-resizer" role="separator" aria-orientation="vertical" onMouseDown={startResize} onDoubleClick={() => { widthRef.current = 392; setWidth(392); chat?.onWidthChange?.(392); }} />
        <header className="nsrail-header">
          <div className="nsrail-title-row">
            <span className="nsrail-title">{sessionTitle(sessionItem)}</span>
            {sessionItem?.status === 'running' && <span className="sess-run" />}
            <button type="button" className="nsrail-close" aria-label="세션 레일 닫기" onClick={onClose}>✕</button>
          </div>
          {sessionItem && (
            <div className="nsrail-meta-row">
              <span className="nsrail-kind">{kindLabel(sessionItem.kind)}</span>
              <StatusCell row={sessionItem} />
              <span className="nsrail-started">{sinceText(sessionItem.startedAt)}에 시작</span>
              {sessionItem.cwd && <span className="nsrail-cwd">{sessionItem.cwd}</span>}
            </div>
          )}
          <RunStateLine runState={read.view?.runState ?? null} />
          <div className="nsrail-actions">
            <button type="button" className="nsrail-open-session" onClick={() => chat?.onOpenSession?.(sessionId)}>세션 화면에서 보기</button>
          </div>
        </header>

        <div className="nsrail-body" ref={body}>
          <section className="nsrail-session-section">
            <TranscriptBody read={read} />
          </section>

          <section className="rchat">
            <div className="rchat-header">
              <h4>새 대화</h4>
              {(turns.length > 0 || querying) && (
                <button type="button" className="rchat-new" disabled={querying} onClick={() => { setNewChat(true); setLastSent(null); setNotice(null); }}>새 대화 열기</button>
              )}
            </div>
            {chatsError && <div className="rchat-warn">{chatsError}</div>}
            {unreadable.length > 0 && <div className="rchat-warn">못 읽은 대화 파일이 있어요: {unreadable.join(', ')}</div>}
            {turns.length === 0 && !querying && (
              <div className="rchat-empty">이 세션에는 보내지 않아요. 새 대화를 열고, 이 세션의 최근 대화와 노트를 함께 실어 물어요.</div>
            )}
            <div className="rchat-turns">
              {turns.map((turn) => (
                <RailTurnPair key={turn.id} turn={turn} baseDir={baseDir} />
              ))}
              {running && (
                <div className="rchat-turn-pair rchat-pending">
                  <div className="sess-turn sess-turn-user">
                    <div className="sess-turn-head"><span className="sess-turn-author">나</span></div>
                    <div className="sess-turn-body">{running.question}</div>
                    {running.images > 0 && <div className="sess-turn-images">{running.previews.map((src, index) => <img key={index} src={src} alt="붙인 그림" style={{ maxWidth: 120, maxHeight: 90 }} />)}</div>}
                  </div>
                  <div className="sess-turn sess-turn-assistant">
                    <div className="sess-turn-body"><PendingLine record={{ at: running.startedAt, sessionId: running.sessionId ?? '', ...(running.askId ? { askId: running.askId } : {}), timeoutSeconds: running.timeoutSeconds }} /></div>
                  </div>
                </div>
              )}
            </div>
          </section>
        </div>

        <footer className="sess-composer nsrail-composer">
          <div className="rchat-options">
            <select
              className="ask-option"
              aria-label="모델"
              value={choice.model}
              onChange={(event) => {
                const nextModel = event.target.value;
                setPicked((prev) => ({
                  ...prev,
                  model: nextModel,
                  ...(isGeminiModel(nextModel) ? { effort: defaultEffortForModel(nextModel) } : {}),
                }));
              }}
            >
              <optgroup label="Claude">
                {ASK_MODELS.filter((item) => item.provider === 'claude').map((item) => (
                  <option key={item.id} value={item.id}>{item.label}</option>
                ))}
              </optgroup>
              <optgroup label="Gemini">
                {ASK_MODELS.filter((item) => item.provider === 'gemini').map((item) => (
                  <option key={item.id} value={item.id}>{item.label}</option>
                ))}
              </optgroup>
            </select>
            {!isGeminiModel(choice.model) && (
              <select
                className="ask-option"
                aria-label="추론 강도"
                title="추론 강도"
                value={choice.effort}
                onChange={(event) => setPicked((prev) => ({ ...prev, effort: event.target.value }))}
              >
                {ASK_EFFORTS.map((item) => <option key={item.id} value={item.id}>{item.label}</option>)}
              </select>
            )}
            {choice.hint && <span className="rchat-options-hint">{choice.hint}</span>}
          </div>
          {notice && <div className="sess-composer-notice">{notice}</div>}
          <AskImageStrip images={pictures.images} refusal={pictures.refusal} onRemove={pictures.remove} />
          <div className="sess-composer-box">
            <textarea
              className="sess-composer-input"
              aria-label="새 대화로 묻기"
              placeholder="이 세션과 노트에 대해 물어요…"
              value={text}
              rows={3}
              disabled={querying}
              onChange={(event) => setText(event.target.value)}
              onPaste={pictures.paste}
              onKeyDown={(event) => {
                if ((event.metaKey || event.ctrlKey) && event.key === 'Enter') {
                  event.preventDefault();
                  void send();
                }
              }}
            />
            <div className="sess-composer-footer">
              <ComposerKeys enter="묻기" />
              {querying ? (
                <button
                  type="button"
                  className="sess-composer-stop"
                  aria-label="멈추기"
                  onClick={() => {
                    if (running?.sessionId) void invoke('cancel_ask', { sessionId: running.sessionId, ...(running.askId ? { askId: running.askId } : {}) });
                  }}
                >
                  멈추기
                </button>
              ) : (
                <button type="button" className="sess-composer-send" disabled={!text.trim() || (first && read.loading)} onClick={() => void send()}>
                  {first && read.loading ? '세션 대화를 읽는 중…' : '묻기'}
                  <Icon name="arrowUp" />
                </button>
              )}
            </div>
          </div>
        </footer>
      </aside>
    </AskContext.Provider>
  );
}

function RailTurnPair({ turn, baseDir }: { turn: RailTurn; baseDir?: string }): React.JSX.Element {
  return (
    <div className="rchat-turn-pair">
      <div className="sess-turn sess-turn-user">
        <div className="sess-turn-head">
          <span className="sess-turn-author">나</span>
          <span className="sess-turn-time">{formatTurnTime(turn.askedAt)}</span>
        </div>
        <div className="sess-turn-body">{turn.question}</div>
        {turn.images && turn.images.length > 0 && <div className="sess-turn-images">{turn.images.map((relative) => <img key={relative} src={baseDir ? convertFileSrc(`${baseDir}/${relative}`) : undefined} alt="붙인 그림" style={{ maxWidth: 120, maxHeight: 90 }} />)}</div>}
      </div>
      <div className={`sess-turn sess-turn-assistant${turn.error ? ' rchat-failed' : ''}`}>
        <div className="sess-turn-head">
          <span className="sess-turn-author">{isGeminiModel(turn.model) ? 'Gemini' : 'Claude'}</span>
          <span className="sess-turn-model">{modelLabel(turn.model)}</span>
          {!isGeminiModel(turn.model) && <span className="sess-turn-effort">{turn.effort}</span>}
          <span className="sess-turn-time">{(turn.elapsedMs / 1000).toFixed(1)}초</span>
        </div>
        <div className="sess-turn-body">{turn.error ? <div className="rchat-error">못 받았어요: {turn.error}</div> : <ModelProse content={turn.answer} />}</div>
        {turn.unsaved && <div className="rchat-unsaved">이 답은 vault 에 못 적었어요. 앱을 끄면 사라져요.</div>}
      </div>
    </div>
  );
}

/**
 * 8.10: turns that could not be written before go first, in order, then the new one. A success registers the written path as our
 * own write and sets the chat from the response; a failure that would repeat marks that turn "못 적음" and goes on; a failure that
 * may pass on a retry (the engine unreachable, the host cut it off, the bridge did not run) marks it and every turn behind it and stops.
 */
export async function appendTurns(
  vault: string,
  at: { noteId: string; subId?: string; chatId: string; sessionId: string },
  current: RailChat | null,
  fresh: RailTurn[],
  setChats: React.Dispatch<React.SetStateAction<RailChat[]>>
): Promise<void> {
  const queue: RailTurn[] = [...(current?.turns.filter((turn) => turn.unsaved) ?? []), ...fresh];
  const base: RailChat = current ?? { noteId: at.subId ?? at.noteId, chatId: at.chatId, sourceSessionId: at.sessionId, createdAt: fresh[0]?.askedAt ?? new Date().toISOString(), turns: [] };
  let shown: RailChat = { ...base, turns: base.turns.filter((turn) => !turn.unsaved) };
  const unsaved: RailTurn[] = [];
  for (let index = 0; index < queue.length; index++) {
    const turn = queue[index];
    const { unsaved: _unsaved, ...plain } = turn;
    const argv = ['append-chat-turn', at.noteId, ...(at.subId ? [at.subId] : []), `chat-id=${at.chatId}`, `source-session=${at.sessionId}`, `turn-id=${plain.id}`, `asked-at=${plain.askedAt}`, `question=${plain.question}`, `answer=${plain.answer}`, ...(plain.error ? [`error=${plain.error}`] : []), `model=${plain.model}`, `effort=${plain.effort}`, `elapsed-ms=${Math.min(2_147_483_647, plain.elapsedMs)}`, ...(plain.images?.length ? [`images=${plain.images.join(',')}`] : [])];
    let retryable = false;
    try {
      const reply = await tauriAskHost.engine(argv);
      if (reply.status === 0) {
        const body = JSON.parse(reply.stdout);
        if (typeof body.path === 'string') tauriAskHost.announceSelfWrite(vault, body.path);
        shown = { ...(body.chat as RailChat), turns: [...(body.chat as RailChat).turns] };
        continue;
      }
      retryable = reply.status === 4 || reply.status < 0;
    } catch {
      retryable = true;
    }
    if (retryable) {
      unsaved.push(...queue.slice(index).map((item) => ({ ...item, unsaved: true })));
      break;
    }
    unsaved.push({ ...turn, unsaved: true });
  }
  const finished: RailChat = { ...shown, turns: [...shown.turns, ...unsaved] };
  // The written chat moves to the end of the memory list; reading again puts it back in order of start (8.12).
  setChats((prev) => [...prev.filter((item) => item.chatId !== at.chatId), finished]);
}

// --------------------------------------------------------------------------
// 9: the session screen
// --------------------------------------------------------------------------

export interface SessionRouteProps {
  sessionId?: string;
  controller?: any;
  state?: any;
}

export function SessionRoute({ sessionId: rawSessionId, controller, state }: SessionRouteProps): React.JSX.Element {
  const sessionId = useMemo(() => {
    if (rawSessionId) return rawSessionId;
    const match = /\/session\/([^/?#]+)/.exec(typeof window !== 'undefined' ? window.location.pathname : '');
    return match ? decodeURIComponent(match[1]) : '';
  }, [rawSessionId]);
  const vault = vaultPathOf(state);

  const rows = useSessionList();
  const store = useSessionStore();
  const sessionItem = useMemo(() => rows.find((row) => row.sessionId === sessionId), [rows, sessionId]);
  // 9.2: the list is read once when it has never been read.
  useSessionListSync(vault || undefined);

  // 9.2: the conversation is read again only when the answer signal of this session changes.
  const [manualSignal, setManualSignal] = useState(0);
  const autoSignal = useReplySignal(sessionId);
  const signal = autoSignal + manualSignal;
  const read = useReadSession(sessionId, 100, signal);
  const view = read.view;

  // 9.7: the chosen tab is not saved and goes back to "관련 노트" when the session changes, in this very render.
  const [tabFor, setTabFor] = useState<{ id: string; tab: 'notes' | 'links' | 'cli' }>({ id: sessionId, tab: 'notes' });
  const tab = tabFor.id === sessionId ? tabFor.tab : 'notes';
  if (tabFor.id !== sessionId) setTabFor({ id: sessionId, tab: 'notes' });
  const chooseTab = (next: 'notes' | 'links' | 'cli') => setTabFor({ id: sessionId, tab: next });

  const previewOpen = state?.prefs?.sessionPreviewOpen !== false;
  const [narrow, setNarrow] = useState(() => typeof window !== 'undefined' && window.innerWidth <= 1100);
  const [railOverlayOpen, setRailOverlayOpen] = useState(false);
  useEffect(() => {
    const check = () => setNarrow(window.innerWidth <= 1100);
    window.addEventListener('resize', check);
    return () => window.removeEventListener('resize', check);
  }, []);
  const railVisible = narrow ? railOverlayOpen : previewOpen;
  const setPreviewOpen = (open: boolean) => controller?.preference?.({ sessionPreviewOpen: open });
  const toggleRail = () => {
    if (narrow) {
      setRailOverlayOpen(!railOverlayOpen);
    } else {
      setPreviewOpen(!previewOpen);
    }
  };

  const talkRef = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (talkRef.current) {
      talkRef.current.scrollTop = talkRef.current.scrollHeight;
    }
  }, [view?.turns.length, signal]);

  const alive = sessionItem?.processAlive === true;
  const [copied, setCopied] = useState(false);
  const [pinOpen, setPinOpen] = useState(false);
  const [agentsOpen, setAgentsOpen] = useState(false);
  const [todosOpen, setTodosOpen] = useState(false);

  const related = useMemo(() => {
    const notes: any[] = state?.workspace?.notes ?? [];
    return notes
      .filter((note) => note.sessionId === sessionId || note.editedSessionId === sessionId)
      .map((note) => ({ note, made: note.sessionId === sessionId }))
      .sort((a, b) => String(b.note.created ?? '').localeCompare(String(a.note.created ?? '')) || String(a.note.id).localeCompare(String(b.note.id)));
  }, [state?.workspace?.notes, sessionId]);

  // 10.7: a file dropped on the middle column puts its path in the box. The Tauri event, not the browser's drop.
  const middle = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!isTauri() || !sessionId) return;
    let off: (() => void) | undefined;
    let live = true;
    void getCurrentWebview()
      .onDragDropEvent((event) => {
        const payload: any = event.payload;
        const mode = payload.type;
        const area = middle.current?.querySelector('.sess-composer-box');
        if (!area) return;
        const rect = middle.current!.getBoundingClientRect();
        const over = payload.position && payload.position.x >= rect.left && payload.position.x <= rect.right && payload.position.y >= rect.top && payload.position.y <= rect.bottom;
        area.setAttribute('data-drop', mode === 'drop' || mode === 'leave' ? 'none' : over ? 'over' : 'window');
        if (mode !== 'drop' || !over || !Array.isArray(payload.paths) || payload.paths.length === 0) return;
        const current = getSessionComposerState(sessionId);
        if (current.sending) return; // 10.7: not while sending
        const box = composerInputs.get(sessionId);
        const focused = box && document.activeElement === box ? { start: box.selectionStart, end: box.selectionEnd } : null;
        const next = insertDropped(current.text, payload.paths, focused);
        patchComposer(sessionId, { text: next.text, error: null });
        requestAnimationFrame(() => {
          const element = composerInputs.get(sessionId);
          if (element) {
            element.focus();
            element.setSelectionRange(next.caret, next.caret);
          }
        });
      })
      .then((fn) => {
        if (live) off = fn;
        else fn();
      });
    return () => {
      live = false;
      off?.();
    };
  }, [sessionId]);

  const openTerminal = () => {
    if (narrow) setRailOverlayOpen(true);
    else setPreviewOpen(true);
    chooseTab('cli');
  };

  const startedText = sessionItem ? sinceText(sessionItem.startedAt) : '';
  const meta = sessionItem ? (
    <>
      <span className="sess-page-kind">{kindLabel(sessionItem.kind)}</span>
      <StatusCell row={sessionItem} />
      <span className="sess-page-started">{startedText === '방금' ? '방금 시작' : `${startedText}에 시작`}</span>
      {sessionItem.pid !== undefined && <span className="sess-page-pid">pid {sessionItem.pid}</span>}
      {sessionItem.branch && <span className="sess-page-branch">{sessionItem.branch}</span>}
    </>
  ) : (
    <span>{store.status === 'idle' || store.status === 'loading' ? '세션 목록을 읽는 중이에요.' : '끝났거나 다른 기기의 세션이에요. 대화는 디스크에 남아 있어서 아래 명령으로 이어 갈 수 있어요.'}</span>
  );

  return (
    <div className={`sess-screen${railVisible ? '' : ' no-preview'}`}>
      <div className="sess-main-area" ref={middle}>
        <header className="sess-page-header">
          <div className="sess-page-head-row">
            <h1 className="sess-page-title">
              {sessionItem?.status === 'running' && <span className="sess-run" />}
              {sessionTitle(sessionItem)}
            </h1>
            <div className="sess-page-actions">
              <button type="button" className="sess-page-refresh" title="대화 새로 읽기" onClick={() => setManualSignal((s) => s + 1)}>새로 읽기</button>
              <button type="button" className="open-cli" onClick={openTerminal}>터미널 열기</button>
              <button
                type="button"
                className="sess-page-resume"
                title={resumeCommand(view?.agent, sessionId)}
                onClick={() => {
                  void navigator.clipboard?.writeText(resumeCommand(view?.agent, sessionId));
                  setCopied(true);
                  setTimeout(() => setCopied(false), 1600);
                }}
              >
                {copied ? '복사됨' : '이어 가기 명령 복사'}
              </button>
              <button
                type="button"
                className="sess-page-pin"
                disabled={read.loading || !view || view.turns.length < 1}
                title={!view || view.turns.length < 1 ? '대화가 없으면 굳힐 것이 없어요.' : undefined}
                onClick={() => setPinOpen(true)}
              >
                노트로 굳히기
              </button>
              <button type="button" className="prev-toggle" onClick={toggleRail}>{railVisible ? '레일 접기' : '레일 펴기'}</button>
            </div>
          </div>
          <div className="sess-page-meta">{meta}</div>
          {sessionItem?.cwd && <div className="sess-page-cwd">{sessionItem.cwd}</div>}
          <RunStateLine runState={view?.runState ?? null} />
        </header>

        <section className="sess-page-talk" ref={talkRef}>
          <div className="sess-page-talk-head">
            <h2>대화</h2>
            {view && <span className="sess-count">{view.source.messages}</span>}
          </div>
          <TranscriptBody read={read} />
        </section>

        <footer className="sess-dock">
          {view && view.runningAgents.length > 0 && (
            <div className="subagent-bar sess-dock-row">
              <button type="button" className="sess-dock-head" aria-expanded={agentsOpen} onClick={() => setAgentsOpen(!agentsOpen)}>
                {alive && <span className="sess-run" />}
                <span>{agentBarText(view.runningAgents.length, alive)}</span>
              </button>
              <button type="button" className="open-cli" onClick={openTerminal}>터미널 열기</button>
              {agentsOpen && (
                <div className="sess-dock-list">
                  {view.runningAgents.map((agent, index) => (
                    <div key={index} className="sess-dock-item">
                      <span className="sess-dock-what">{agent.what ?? '(무엇을 하는지 안 적힌 호출)'}</span>
                      {agent.at && !Number.isNaN(new Date(agent.at).getTime()) && <span className="sess-dock-at">{sinceText(new Date(agent.at).getTime())}</span>}
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
          {view && view.todos.length > 0 && (
            <div className="todo-bar sess-dock-row">
              <button type="button" className="sess-dock-head" aria-expanded={todosOpen} onClick={() => setTodosOpen(!todosOpen)}>{todoBarText(view.todos)}</button>
              {todosOpen && (
                <div className="sess-dock-list">
                  {view.todos.map((todo) => (
                    <div key={todo.number} className="sess-dock-item">
                      <span className="todo-status" data-status={todo.status}>{todo.status === 'completed' ? '✓' : todo.status === 'in_progress' ? '●' : '○'}</span>
                      <span className="sess-dock-what">{todo.title}</span>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
          <SessionComposer
            key={sessionId}
            sessionId={sessionId}
            sessionItem={sessionItem}
            sessionLoaded={!read.loading && !read.error}
            transcriptFile={view?.file}
            agent={view?.agent}
            vault={vault}
            timeoutSeconds={() => Number(state?.prefs?.askTimeoutSeconds) || 3600}
          />
        </footer>
      </div>

      <aside className={`prevcol${narrow && railOverlayOpen ? ' is-overlay' : ''}`} hidden={!railVisible} style={{ display: railVisible ? undefined : 'none' }}>
          {narrow && (
            <div className="prev-overlay-head" style={{ display: 'flex', justifyContent: 'flex-end', padding: '6px 12px', borderBottom: '1px solid var(--line)' }}>
              <button type="button" aria-label="레일 닫기" onClick={() => setRailOverlayOpen(false)} style={{ background: 'transparent', border: 0, cursor: 'pointer', fontSize: 14, color: 'var(--ink)' }}>✕</button>
            </div>
          )}
          <div className="prev-tabs">
            <button type="button" className={`prev-tab${tab === 'notes' ? ' on' : ''}`} onClick={() => chooseTab('notes')}>관련 노트 <span className="prev-tab-n">{related.length}</span></button>
            <button type="button" className={`prev-tab${tab === 'links' ? ' on' : ''}`} onClick={() => chooseTab('links')}>링크 <span className="prev-tab-n">{view?.urls.length ?? 0}</span></button>
            <button type="button" className={`prev-tab${tab === 'cli' ? ' on' : ''}`} onClick={() => chooseTab('cli')}>CLI</button>
          </div>

          <div className="prev-pane" hidden={tab !== 'notes'}>
            {related.length === 0 ? (
              <div className="prev-empty">이 세션이 만들거나 고친 노트가 없어요. 노트에 세션 도장이 찍히기 시작한 것이 2026-09-12 이라, 그보다 이른 세션은 만들었어도 안 적혀 있어요.</div>
            ) : (
              <div className="relnotes-list">
                {related.map(({ note, made }) => (
                  <div key={note.id} className="relnote" onClick={() => controller?.navigate?.(`/doc/${note.id}`)}>
                    <div className="relnote-title"><span className="relnote-agenda-dot" aria-hidden="true" style={{ display: 'inline-block', width: 7, height: 7, marginRight: 6, borderRadius: '50%', background: HUE_COLORS[state?.workspace?.agendas.find((agenda: any) => agenda.id === note.agendaId)?.hue as keyof typeof HUE_COLORS] ?? 'var(--muted)' }} />{note.title || '(제목 없는 노트)'}</div>
                    <div className="relnote-why">{made ? '이 세션이 만들었어요.' : '이 세션이 고쳤어요.'}</div>
                    <div className="relnote-date">{String(note.created ?? '').slice(0, 10)}</div>
                  </div>
                ))}
              </div>
            )}
          </div>

          <div className="prev-pane" hidden={tab !== 'links'}>
            {read.loading ? (
              <div className="prev-empty">대화 기록을 읽는 중이에요.</div>
            ) : read.error && !view ? (
              <div className="prev-empty">대화 기록을 못 읽어서 링크도 못 모았어요.</div>
            ) : !view || view.urls.length === 0 ? (
              <div className="prev-empty">그 세션에서 오간 링크가 없어요.</div>
            ) : (
              <div className="sess-links-list">
                {view.urls.map((item) => {
                  const url = splitUrl(item.url);
                  return (
                    <div key={item.url} className="sess-page-link" title={item.url} onClick={() => void openUrl(item.url).catch(() => {})}>
                      <strong>{url.host}</strong>
                      <span>{url.rest}</span>
                      {item.count > 1 && <span className="sess-page-link-count">({item.count})</span>}
                    </div>
                  );
                })}
              </div>
            )}
          </div>

          <SessionCliPane key={sessionId} sessionId={sessionId} visible={railVisible && tab === 'cli'} controller={controller} state={state} />
        </aside>

      {pinOpen && view && (
        <PinSessionModal
          sessionId={sessionId}
          controller={controller}
          state={state}
          onClose={() => setPinOpen(false)}
        />
      )}
    </div>
  );
}

function splitUrl(url: string): { host: string; rest: string } {
  try {
    const parsed = new URL(url);
    return { host: parsed.host, rest: parsed.pathname + parsed.search + parsed.hash };
  } catch {
    return { host: url, rest: '' };
  }
}

// --------------------------------------------------------------------------
// 10.9, 10.11: the CLI pane is the part 3 terminal cell with the key term:<session id>
// --------------------------------------------------------------------------

export function SessionCliPane({
  sessionId,
  visible,
  controller,
  state,
}: {
  sessionId: string;
  visible: boolean;
  controller?: any;
  state?: any;
  /** Kept for the callers that still pass them. */
  sessionCwd?: string;
  sessionItem?: unknown;
}): React.JSX.Element {
  const key = `term:${sessionId}`;
  // The shell starts the first time the cell is shown, and not for a session that was only opened (9.7, 10.9).
  const [opened, setOpened] = useState(false);
  useEffect(() => {
    if (visible) setOpened(true);
  }, [visible]);
  const tabOpen = Boolean(state?.tabs?.openIds?.includes(key));
  const workspace = state?.workspace;

  if (tabOpen) {
    return (
      <div className="prev-pane is-cli term-host" data-tab-key={key} hidden={!visible}>
        <div className="term-empty">이 세션의 터미널이 탭으로 열려 있어요. 그 탭에서 이어 쓰세요.</div>
        <button type="button" onClick={() => controller?.navigate?.(`/terminal/${encodeURIComponent(sessionId)}`)}>그 탭으로 가기</button>
      </div>
    );
  }
  if (!opened || !workspace) return <div className="prev-pane is-cli term-host" data-tab-key={key} hidden={!visible} />;
  return (
    <div className="prev-pane is-cli term-host" data-tab-key={key} hidden={!visible}>
      <Suspense fallback={null}>
        <TerminalHost
          tabKey={key}
          isVisible={visible}
          agendas={agendaOrder(workspace.agendas, workspace.notes)}
          onCreateDoc={async (agendaSlug, title, body) => createPinnedDoc(controller, state, agendaSlug, title, body, ['터미널기록'])}
          onNavigate={(path) => controller?.navigate?.(path)}
        />
      </Suspense>
    </div>
  );
}

/** 9.8: `create-doc` through the screen's data layer: `type=free`, `tier=record`; the memory is not touched on a failure. */
async function createPinnedDoc(controller: any, state: any, agendaSlug: string, title: string, body: string, tags: string[]): Promise<{ ok: boolean; id?: string; error?: string }> {
  try {
    const workspace = state?.workspace;
    const agenda = workspace?.agendas.find((item: any) => item.slug === agendaSlug);
    const result = await controller.data.createDoc({
      agendaId: agenda?.id || agendaSlug,
      type: 'free',
      title,
      body,
      date: workspace.today,
      time: new Date().toTimeString().slice(0, 5),
      tags,
      tier: 'record',
    });
    if (result.ok && result.value?.id) return { ok: true, id: String(result.value.id) };
    return { ok: false, error: '노트 생성에 실패했어요.' };
  } catch (error: any) {
    return { ok: false, error: error?.message || String(error) };
  }
}

// --------------------------------------------------------------------------
// 9.8: pin the transcript into a note
// --------------------------------------------------------------------------

interface ShapeCount {
  kind: string;
  label: string;
  count: number;
}

export function PinSessionModal({
  sessionId,
  controller,
  state,
  onClose,
}: {
  sessionId: string;
  controller: any;
  state: any;
  onClose: () => void;
}): React.JSX.Element {
  const agendas = useMemo(() => agendaOrder(state?.workspace?.agendas ?? [], state?.workspace?.notes ?? []), [state?.workspace?.agendas, state?.workspace?.notes]);
  const [count, setCount] = useState<100 | 200 | 500>(100);
  const [retry, setRetry] = useState(0);
  const [title, setTitle] = useState(() => pinDefaultTitle());
  const [agenda, setAgenda] = useState(() => agendas[0]?.slug ?? ''); // decided once, when the window opens
  const [view, setView] = useState<ReadSessionView | null>(null);
  const [shapes, setShapes] = useState<ShapeCount[] | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [saving, setSaving] = useState(false);
  const generation = useRef(0);

  // 9.8 steps 2 and 3: read without links, then scan the lines for shapes. Overlapping reads are told apart by a number.
  useEffect(() => {
    const mine = ++generation.current;
    setView(null);
    setShapes(null);
    setError(null);
    (async () => {
      try {
        const raw = await callEngine(['read-session', sessionId, `limit=${count}`]);
        const next = normalizeReadSession(raw);
        const found = await invoke<ShapeCount[]>('scan_text_shapes', { text: pinScanText(next.turns) });
        if (generation.current !== mine) return;
        setView(next);
        setShapes(found);
      } catch (cause: any) {
        if (generation.current === mine) setError(typeof cause === 'string' ? cause : cause?.message || '대화를 읽지 못했어요.');
      }
    })();
    return () => {
      generation.current += 1;
    };
  }, [sessionId, count, retry]);

  const findings = shapes ? shapes.reduce((sum, shape) => sum + shape.count, 0) : 0;
  const body = useMemo(
    () => (view && shapes ? pinBody({ title: title.trim() || '세션 대화', sessionId, turns: view.turns, source: view.source, shapeFindings: findings, shapeLines: shapes.map((shape) => `${shape.label} ${shape.count} 건`) }) : ''),
    [view, shapes, title, sessionId, findings]
  );
  const bytes = new TextEncoder().encode(body).length;
  const canSave = Boolean(view && shapes && body && bytes <= 300_000 && agenda && title.trim() && !saving);

  const save = async () => {
    if (!canSave) return;
    setSaving(true);
    setError(null);
    const result = await createPinnedDoc(controller, state, agenda, title.trim(), body, ['세션전사']);
    setSaving(false);
    if (result.ok && result.id) {
      onClose();
      controller?.navigate?.(`/doc/${result.id}`);
    } else {
      setError(result.error || '노트를 만들지 못했어요.'); // the window stays open (C32)
    }
  };

  return (
    <Modal title="노트로 굳히기" close={onClose} wide className="session-pin-modal">
      <div className="ndf modal-content" style={{ width: '100%', maxHeight: '75vh', overflowY: 'auto' }}>
        <div className="ndf-body">
          <div className="pin-ranges">
            {([100, 200, 500] as const).map((n) => (
              <button key={n} type="button" className={`btn-range${count === n ? ' active' : ''}`} onClick={() => { setCount(n); setRetry((value) => value + 1); }}>최근 {n}건</button>
            ))}
          </div>
          {view && shapes && <div className="pin-hint">실제로 담을 회차는 {view.turns.length}건이에요.</div>}
          {shapes && (
            <div className="pin-shapes">
              {findings > 0
                ? `표에 없는데 비밀처럼 생긴 것이 ${findings} 건이에요. 안 가려졌어요. ${shapes.map((shape) => `${shape.label} ${shape.count} 건`).join(', ')}`
                : '걸린 것이 0 건이에요. 도구 결과 본문은 애초에 안 담아서, 이 굳히기는 값 표를 안 써요.'}
            </div>
          )}
          <div className="form-group">
            <label>노트 제목</label>
            <input type="text" value={title} onChange={(event) => setTitle(event.target.value)} />
          </div>
          <div className="form-group">
            <label>아젠다</label>
            <select value={agenda} onChange={(event) => setAgenda(event.target.value)}>
              {agendas.map((item) => (
                <option key={item.id} value={item.slug}>{item.title || item.slug}</option>
              ))}
            </select>
          </div>
          {view && shapes && <div className="pin-size">
            본문 {bytes} 바이트 / 상한 300,000 바이트{bytes > 300_000 ? ' — 회차 수를 줄여 주세요.' : ''}
          </div>}
          {error && <div className="pin-warn">{error}</div>}
        </div>
        <footer className="ndf-foot modal-footer" style={{ display: 'flex', justifyContent: 'flex-end', gap: 8, marginTop: 16 }}>
          <button type="button" className="btn-secondary" onClick={onClose}>취소</button>
          <button type="button" className="btn-primary" disabled={!canSave} onClick={() => void save()}>{saving ? '저장 중…' : '굳히기'}</button>
        </footer>
      </div>
    </Modal>
  );
}

export function SessionView({ sessionId }: { sessionId?: string }): React.JSX.Element {
  return <SessionRoute sessionId={sessionId} />;
}

export function SessionRail(_props?: any): null {
  return null;
}

export function useSessionBadge(_docId?: string): null {
  return null;
}

