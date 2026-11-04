// Implementation of @claude-plug/state for Part 4 Claude-enabled builds.
// Adheres to Chapter 3, 4, 17 of withlog Part 4 Claude integration specifications.
import { useSyncExternalStore } from 'react';

export const CLAUDE_ENABLED = true;

const STORAGE_KEY = 'withlog.ask.v1';

export interface PendingAsk {
  queued?: boolean;
  askedAt: number;
  markId: string;
  quote?: string;
}

export interface AbandonedAsk {
  turnId: string;
  question: string;
  askedAt: string;
  markId: string;
  quote?: string;
  sessionId: string;
}

export interface AskStoreState {
  pending: Record<string, Record<string, PendingAsk>>;
  unseen: Record<string, string>;
  abandoned: Record<string, AbandonedAsk[]>;
  unsaved: number;
}

function loadInitialState(): AskStoreState {
  let pending: Record<string, Record<string, PendingAsk>> = {};
  let unseen: Record<string, string> = {};
  let abandoned: Record<string, AbandonedAsk[]> = {};

  try {
    const raw = typeof localStorage !== 'undefined' ? localStorage.getItem(STORAGE_KEY) : null;
    if (raw) {
      const parsed = JSON.parse(raw);
      if (parsed && typeof parsed === 'object') {
        unseen = parsed.unseen || {};
        abandoned = parsed.abandoned || {};

        // 3.11: 앱이 재시작될 때 이전 실행에서 완료되지 못한 pending 은 버려진 물음(abandoned)으로 전환
        if (parsed.pending && typeof parsed.pending === 'object') {
          for (const [noteKey, turns] of Object.entries(parsed.pending as Record<string, Record<string, any>>)) {
            if (!abandoned[noteKey]) abandoned[noteKey] = [];
            for (const [turnId, info] of Object.entries(turns || {})) {
              abandoned[noteKey].push({
                turnId,
                question: info.question || '(물음 내용 없음)',
                askedAt: new Date(info.askedAt || Date.now()).toISOString(),
                markId: info.markId || turnId,
                quote: info.quote,
                sessionId: info.sessionId || '',
              });
            }
          }
        }
      }
    }
  } catch {
    // Ignore localStorage parse errors
  }

  return {
    pending,
    unseen,
    abandoned,
    unsaved: 0,
  };
}

let askState: AskStoreState = loadInitialState();
const askListeners = new Set<() => void>();

function persistAskState() {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.setItem(
      STORAGE_KEY,
      JSON.stringify({
        pending: askState.pending,
        unseen: askState.unseen,
        abandoned: askState.abandoned,
      })
    );
  } catch {
    // Ignore storage quota errors
  }
}

function notifyAsk() {
  persistAskState();
  for (const listener of askListeners) {
    listener();
  }
}

export function setPendingAsk(
  noteKey: string,
  turnId: string,
  info: PendingAsk | null
) {
  const currentNotes = { ...askState.pending };
  const currentTurns = { ...(currentNotes[noteKey] || {}) };

  if (info === null) {
    delete currentTurns[turnId];
    if (Object.keys(currentTurns).length === 0) {
      delete currentNotes[noteKey];
    } else {
      currentNotes[noteKey] = currentTurns;
    }
  } else {
    currentTurns[turnId] = info;
    currentNotes[noteKey] = currentTurns;
  }

  askState = {
    ...askState,
    pending: currentNotes,
  };
  notifyAsk();
}

export function markNoteUnseen(noteKey: string, turnId: string) {
  askState = {
    ...askState,
    unseen: { ...askState.unseen, [noteKey]: turnId },
  };
  notifyAsk();
}

export function clearNoteUnseen(noteKey: string) {
  if (!askState.unseen[noteKey]) return;
  const copy = { ...askState.unseen };
  delete copy[noteKey];
  askState = { ...askState, unseen: copy };
  notifyAsk();
}

export function dismissAbandoned(noteKey: string, turnId: string) {
  const list = askState.abandoned[noteKey];
  if (!list) return;
  const filtered = list.filter((a) => a.turnId !== turnId);
  const copy = { ...askState.abandoned };
  if (filtered.length === 0) {
    delete copy[noteKey];
  } else {
    copy[noteKey] = filtered;
  }
  askState = { ...askState, abandoned: copy };
  notifyAsk();
}

export function incUnsavedAnswers() {
  askState = { ...askState, unsaved: askState.unsaved + 1 };
  notifyAsk();
}

export function decUnsavedAnswers() {
  askState = { ...askState, unsaved: Math.max(0, askState.unsaved - 1) };
  notifyAsk();
}

export function useAskState<T = AskStoreState>(
  selector: (state: AskStoreState) => T = (s) => s as unknown as T
): T {
  const snapshot = useSyncExternalStore(
    (onStoreChange) => {
      askListeners.add(onStoreChange);
      return () => askListeners.delete(onStoreChange);
    },
    () => selector(askState)
  );
  return snapshot;
}

export function waitingAnswers(noteKeys?: Set<string>): number {
  let count = 0;
  for (const [key, turns] of Object.entries(askState.pending)) {
    if (!noteKeys || noteKeys.has(key)) {
      count += Object.keys(turns).length;
    }
  }
  return count;
}

export function unsavedAnswers(): number {
  return askState.unsaved;
}

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

export function useNoteRail<T = NoteRailState>(
  selector: (state: NoteRailState) => T = (s) => s as unknown as T
): T {
  const snapshot = useSyncExternalStore(
    (onStoreChange) => {
      railListeners.add(onStoreChange);
      return () => railListeners.delete(onStoreChange);
    },
    () =>
      selector({
        rails: railState,
        openRail: openNoteRail,
        closeRail: closeNoteRail,
      })
  );
  return snapshot;
}

export function noteRailNow(): NoteRailState {
  return {
    rails: railState,
    openRail: openNoteRail,
    closeRail: closeNoteRail,
  };
}
