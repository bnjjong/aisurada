// Implementation of @claude-plug/state for Part 4 Claude-enabled builds.
// Adheres to Chapters 3 (3.3, 3.10, 3.11), 4 (4.10) and 17 of aisurada Part 4 Claude integration specifications.
import { useSyncExternalStore } from 'react';
import { DEFAULT_EFFORT, DEFAULT_MODEL, normalizeEffort, normalizeModel } from './askText.ts';
import type { AskThread } from './askText.ts';

export const CLAUDE_ENABLED = true;

/** 3.11: the main window's key. A note window keeps its own key (`aisurada.ask.note-<note id>.v1`). */
export const ASK_MAIN_KEY = 'aisurada.ask.v1';
/** 2.10: the old threads of the main window's stored value are set aside here as they are. */
export const ASK_LEGACY_KEY = 'aisurada.ask.legacy.v1';

/** 3.11: the storage key of a window. A note window path is `/note-window/<note id>`. */
export function askStorageKey(pathname: string | undefined): string {
  const match = /^\/note-window\/([^/]+)$/.exec(pathname ?? '');
  if (match) {
    try {
      return `aisurada.ask.note-${decodeURIComponent(match[1])}.v1`;
    } catch {
      /* a malformed address belongs to the main key */
    }
  }
  return ASK_MAIN_KEY;
}

export interface StorageLike {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem?(key: string): void;
}

/** 3.11: one question in flight. The map key is the underline id. */
export interface PendingAsk {
  /** Start time (epoch ms). While waiting it is the press time; when the question leaves it is that time. */
  at: number;
  /** The conversation id of the question. */
  sessionId: string;
  /** The wait limit in seconds this question was sent with. Absent in old records; read as 3,600. */
  timeoutSeconds?: number;
  /** The way it went. Absent when unknown or old; read as `cli`. */
  runtime?: string;
  /** True only while it waits behind an earlier question of the same note. */
  queued?: boolean;
}

/** 3.11: a question that was in flight when the app closed. */
export interface AbandonedAsk {
  markId: string;
  at: number;
  sessionId: string;
  runtime?: string;
  queued?: boolean;
}

export type AskRuntime = 'cli' | 'agent-sdk' | 'http';

export interface AskStoreState {
  pending: Record<string, Record<string, PendingAsk>>;
  abandoned: Record<string, AbandonedAsk[]>;
  /** 4.10: key -> the root note id whose answer has not been seen. Memory only. */
  unseen: Record<string, string>;
  /** The threads of the notes this window opened. The vault's `ask.json` is the source; this is its cache. */
  threads: Record<string, AskThread>;
  /** Why a note's `get-ask` failed (2.5). */
  loadErrors: Record<string, string>;
  model: string;
  effort: string;
  /** The way questions go, asked once per window (3.8). Memory only. */
  runtime: AskRuntime | null;
}

function parseStored(storage: StorageLike | null, key: string): any {
  if (!storage) return null;
  try {
    const raw = storage.getItem(key);
    return raw ? JSON.parse(raw) : null;
  } catch {
    return null;
  }
}

function isObject(value: unknown): value is Record<string, any> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * 3.11 「되살리기」. The saved state is turned into a fresh one:
 * 1. model and effort become values the pickers can show (3.3);
 * 2. old `threads` are set aside under the legacy key unless that key already exists (2.10), and the memory
 *    starts with no threads;
 * 3. the unseen marks are empty;
 * 4. every saved pending record becomes an abandoned one, and a record whose `at` is not a number or whose
 *    `sessionId` is not a string is dropped; a mark already in the abandoned list is not added twice.
 */
export function restoreAskState(stored: any, options: { storage: StorageLike | null; main: boolean; mainStored?: any }): AskStoreState {
  const saved = isObject(stored) && isObject(stored.state) ? stored.state : {};
  const borrowed = !('model' in saved) && !options.main && isObject(options.mainStored?.state) ? options.mainStored.state : null;
  const state: AskStoreState = {
    pending: {},
    abandoned: {},
    unseen: {},
    threads: {},
    loadErrors: {},
    model: normalizeModel(borrowed ? borrowed.model : saved.model),
    effort: normalizeEffort(borrowed ? borrowed.effort : saved.effort),
    runtime: null,
  };

  if (options.main && 'threads' in saved && options.storage && options.storage.getItem(ASK_LEGACY_KEY) === null) {
    try {
      options.storage.setItem(ASK_LEGACY_KEY, JSON.stringify(saved.threads));
    } catch {
      /* a full storage must not stop the app */
    }
  }

  if (isObject(saved.abandoned)) {
    for (const [key, list] of Object.entries(saved.abandoned)) {
      if (!Array.isArray(list)) continue;
      const kept: AbandonedAsk[] = [];
      for (const item of list) {
        if (!isObject(item) || typeof item.markId !== 'string' || typeof item.at !== 'number' || typeof item.sessionId !== 'string') continue;
        if (kept.some((entry) => entry.markId === item.markId)) continue;
        kept.push({
          markId: item.markId,
          at: item.at,
          sessionId: item.sessionId,
          ...(typeof item.runtime === 'string' ? { runtime: item.runtime } : {}),
          ...(item.queued === true ? { queued: true } : {}),
        });
      }
      if (kept.length) state.abandoned[key] = kept;
    }
  }
  if (isObject(saved.pending)) {
    for (const [key, byMark] of Object.entries(saved.pending)) {
      if (!isObject(byMark)) continue;
      for (const [markId, record] of Object.entries(byMark)) {
        if (!isObject(record) || typeof record.at !== 'number' || typeof record.sessionId !== 'string') continue;
        const list = state.abandoned[key] ?? (state.abandoned[key] = []);
        if (list.some((entry) => entry.markId === markId)) continue;
        list.push({
          markId,
          at: record.at,
          sessionId: record.sessionId,
          ...(typeof record.runtime === 'string' ? { runtime: record.runtime } : {}),
          ...(record.queued === true ? { queued: true } : {}),
        });
      }
    }
  }
  return state;
}

/** The four values that are saved: pending, abandoned, model, effort (3.11). */
export function serializeAskState(state: AskStoreState): string {
  return JSON.stringify({
    state: { pending: state.pending, abandoned: state.abandoned, model: state.model, effort: state.effort },
    version: 0,
  });
}

export interface AskStore {
  get(): AskStoreState;
  subscribe(listener: () => void): () => void;
  update(change: (state: AskStoreState) => AskStoreState): void;
}

/** One store for one window. The window's storage and key are given so tests can run it. */
export function createAskStore(storage: StorageLike | null, key: string): AskStore {
  const main = key === ASK_MAIN_KEY;
  let state = restoreAskState(parseStored(storage, key), {
    storage,
    main,
    mainStored: main ? null : parseStored(storage, ASK_MAIN_KEY),
  });
  const listeners = new Set<() => void>();
  const persist = () => {
    if (!storage) return;
    try {
      storage.setItem(key, serializeAskState(state));
    } catch {
      /* a full storage must not stop the app */
    }
  };
  persist();
  return {
    get: () => state,
    subscribe(listener) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    update(change) {
      const next = change(state);
      if (next === state) return;
      const saved = state.pending !== next.pending || state.abandoned !== next.abandoned || state.model !== next.model || state.effort !== next.effort;
      state = next;
      if (saved) persist();
      for (const listener of [...listeners]) listener();
    },
  };
}

function browserStorage(): StorageLike | null {
  try {
    return typeof localStorage !== 'undefined' ? localStorage : null;
  } catch {
    return null;
  }
}

const store: AskStore = createAskStore(browserStorage(), askStorageKey(typeof location !== 'undefined' ? location.pathname : undefined));

// --------------------------------------------------------------------------
// Writers
// --------------------------------------------------------------------------

/** 3.4, 3.11: records or removes the in-flight record of one underline of one note. */
export function setPendingAsk(key: string, markId: string, record: PendingAsk | null): void {
  store.update((state) => {
    const byMark = { ...(state.pending[key] ?? {}) };
    if (record === null) {
      if (!(markId in byMark)) return state;
      delete byMark[markId];
    } else {
      byMark[markId] = record;
    }
    const pending = { ...state.pending };
    if (Object.keys(byMark).length === 0) delete pending[key];
    else pending[key] = byMark;
    return { ...state, pending };
  });
}

/** 4.10: an answer, or a failure, arrived for this key; the tab of the root note shows a dot. */
export function markNoteUnseen(key: string, rootNoteId: string): void {
  store.update((state) => (state.unseen[key] === rootNoteId ? state : { ...state, unseen: { ...state.unseen, [key]: rootNoteId } }));
}

export function clearNoteUnseen(key: string): void {
  store.update((state) => {
    if (!(key in state.unseen)) return state;
    const unseen = { ...state.unseen };
    delete unseen[key];
    return { ...state, unseen };
  });
}

/** 4.9: the close button of an abandoned line removes that line. */
export function dismissAbandoned(key: string, markId: string): void {
  store.update((state) => {
    const list = state.abandoned[key];
    if (!list) return state;
    const rest = list.filter((entry) => entry.markId !== markId);
    if (rest.length === list.length) return state;
    const abandoned = { ...state.abandoned };
    if (rest.length === 0) delete abandoned[key];
    else abandoned[key] = rest;
    return { ...state, abandoned };
  });
}

/** Puts a note's thread into memory, or removes it. */
export function setAskThread(key: string, thread: AskThread | null): void {
  store.update((state) => {
    const threads = { ...state.threads };
    if (thread === null) {
      if (!(key in threads)) return state;
      delete threads[key];
    } else {
      threads[key] = thread;
    }
    return { ...state, threads };
  });
}

/** 4.1: why the last read of a note's thread failed; `null` clears it. */
export function setAskLoadError(key: string, reason: string | null): void {
  store.update((state) => {
    if ((state.loadErrors[key] ?? null) === reason) return state;
    const loadErrors = { ...state.loadErrors };
    if (reason === null) delete loadErrors[key];
    else loadErrors[key] = reason;
    return { ...state, loadErrors };
  });
}

/** 3.3: the picked model and effort are one set for the window, shared by the compose box and every follow-up. */
export function setAskModel(model: string): void {
  const value = normalizeModel(model);
  store.update((state) => (state.model === value ? state : { ...state, model: value }));
}

export function setAskEffort(effort: string): void {
  const value = normalizeEffort(effort);
  store.update((state) => (state.effort === value ? state : { ...state, effort: value }));
}

/** 3.8: the way questions go is asked once and remembered for the window. */
export function setAskRuntime(runtime: AskRuntime): void {
  store.update((state) => (state.runtime === runtime ? state : { ...state, runtime }));
}

// --------------------------------------------------------------------------
// Readers
// --------------------------------------------------------------------------

export function askNow(): AskStoreState {
  return store.get();
}

export function useAskState<T = AskStoreState>(selector: (state: AskStoreState) => T = (s) => s as unknown as T): T {
  const snapshot = useSyncExternalStore(store.subscribe, store.get, store.get);
  return selector(snapshot);
}

/** How the last question ended. The logo shows it as a gesture. */
export type AskOutcome = 'ok' | 'error';

const OUTCOME_SHOWN_MS = 3000;
let askOutcome: AskOutcome | null = null;
let askOutcomeTimer: ReturnType<typeof setTimeout> | undefined;
const askOutcomeListeners = new Set<() => void>();

function emitAskOutcome(): void {
  askOutcomeListeners.forEach((listener) => listener());
}

function subscribeAskOutcome(listener: () => void): () => void {
  askOutcomeListeners.add(listener);
  return () => {
    askOutcomeListeners.delete(listener);
  };
}

/** Records how a question ended. The mark is kept for a few seconds and is never saved. */
export function noteAskOutcome(kind: AskOutcome): void {
  clearTimeout(askOutcomeTimer);
  askOutcome = kind;
  emitAskOutcome();
  askOutcomeTimer = setTimeout(() => {
    askOutcome = null;
    emitAskOutcome();
  }, OUTCOME_SHOWN_MS);
}

export function useAskOutcome(): AskOutcome | null {
  return useSyncExternalStore(subscribeAskOutcome, () => askOutcome, () => null);
}

/** The number of questions in flight, for the vault leaving check (3부 설명서 17.5). */
export function waitingAnswers(noteKeys?: Set<string>): number {
  let count = 0;
  for (const [key, byMark] of Object.entries(store.get().pending)) {
    if (!noteKeys || noteKeys.has(key)) count += Object.keys(byMark).length;
  }
  return count;
}

/** The number of answers that exist only in memory: threads' turns marked `unsaved`. */
export function unsavedAnswers(): number {
  let count = 0;
  for (const thread of Object.values(store.get().threads)) count += thread.turns.filter((turn) => turn.unsaved).length;
  return count;
}

export { DEFAULT_EFFORT, DEFAULT_MODEL };

// --------------------------------------------------------------------------
// Session / Note Rail State
// --------------------------------------------------------------------------

export interface NoteRailState {
  rails: Record<string, string>; // noteKey -> sessionId
  openRail: (noteKey: string, sessionId: string) => void;
  closeRail: (noteKey?: string) => void;
}

let railState: Record<string, string> = {};
const railListeners = new Set<() => void>();

function notifyRail() {
  for (const listener of railListeners) {
    listener();
  }
}

export function openNoteRail(noteKey: string, sessionId: string) {
  railState = { ...railState, [noteKey]: sessionId };
  notifyRail();
}

export function closeNoteRail(noteKey?: string) {
  if (!noteKey) {
    railState = {};
  } else {
    const copy = { ...railState };
    delete copy[noteKey];
    railState = copy;
  }
  notifyRail();
}

let railSnapshot: NoteRailState = { rails: railState, openRail: openNoteRail, closeRail: closeNoteRail };
function getRailSnapshot(): NoteRailState {
  if (railSnapshot.rails !== railState) railSnapshot = { ...railSnapshot, rails: railState };
  return railSnapshot;
}
function subscribeRail(listener: () => void): () => void {
  railListeners.add(listener);
  return () => { railListeners.delete(listener); };
}

export function useNoteRail<T = NoteRailState>(selector: (state: NoteRailState) => T = (s) => s as unknown as T): T {
  const snapshot = useSyncExternalStore(subscribeRail, getRailSnapshot, getRailSnapshot);
  return selector(snapshot);
}

export function noteRailNow(): NoteRailState {
  return getRailSnapshot();
}
