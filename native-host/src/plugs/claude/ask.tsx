// Implementation of @claude-plug/ask for Part 4 Claude-enabled builds.
// Adheres to Chapters 2, 3, 4 of aisurada Part 4 Claude integration specifications.
import { createElement, useEffect, useMemo, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import type { Note } from '../../workspace/types.ts';
import { AskComposePopover } from './askPopover.tsx';
import type { AskComposePopoverProps, AskComposeState } from './askPopover.tsx';
import { AskHoverCard } from './askHoverCard.tsx';
import type { AskHoverCardProps } from './askHoverCard.tsx';
import { AskPanel } from './askPanel.tsx';
import { AskContext } from './askContext.ts';
import type { AskContextValue } from './askContext.ts';
import { tauriAskHost } from './askHost.ts';
import { migrateLegacyThread, startAsk } from './askRun.ts';
import type { AskHost, AskRequest } from './askRun.ts';
import { HTML_ID_PREFIX, htmlAskId, htmlQuote, mergeUnsaved, newUnderlineId, noteHeading, normalizeEffort, normalizeModel, targetArgs, turnsForUnderline } from './askText.ts';
import type { AskTarget, AskThread, AskTurn } from './askText.ts';
import { registerDocOpener } from './docLinks.ts';
import { ASK_LEGACY_KEY, askNow, setAskLoadError, setAskThread, useAskState } from './state.ts';

export type { AskComposePopoverProps, AskComposeState, AskHoverCardProps, AskThread, AskTurn };
export { AskComposePopover, AskHoverCard, AskPanel };

export interface NoteBodyPort {
  kind?: 'main' | 'sub';
  id?: string;
  parent?: string;
  write(body: string): Promise<void> | void;
  saveDraft?(body: string): void;
  read?(): string;
  autosaveBody?(body?: string): Promise<void> | void;
  currentBody?(): string;
  commitTitle?(title?: string): Promise<boolean | void> | void;
  protect?(enabled?: boolean): void;
  flush?(mode?: string): Promise<any> | void;
  discard?(): Promise<void> | void;
  keep?(): Promise<any> | void;
  restore?(body: string): Promise<any> | void;
  conflicted?(): boolean;
  editing?(): boolean;
}

export interface AskEditorProps {
  onAskAi?: (quote: string, question: string, turnId: string, images?: File[]) => void;
  askDocId?: string;
  onAskFollowUp?: (question: string, underlineId: string, images?: File[]) => void;
}

export interface NoteAskInput {
  doc: Note;
  port: NoteBodyPort;
  /** The absolute path of the note's folder. */
  baseDir?: string;
  /** The absolute path of the vault. Without it nothing is asked (3.2). */
  vault?: string;
  currentDraft: () => string;
  /** The wait limit setting at the moment of the press (3.12). */
  timeoutSeconds?: () => number;
  /** The `repo` of the note's agenda (3.6). */
  repo?: string;
  /** The root note of a sub page chain: the first segment of `<root>/<sub>/<grandchild>` (3.13). */
  root?: string;
  /** The author session of the root note, used by a sub note without its own (3.7). */
  rootSessionId?: string;
  /** Opens the preview of a note for a `#doc:` link in an answer (4.11). */
  openDoc?: (id: string) => boolean | void;
  onFollowUp?: (question: string, underlineId: string, images?: File[]) => void;
}

export interface NoteAskOutput {
  editor: AskEditorProps;
  panel: ReactNode;
}

/** 2.1: kept under its old name for the callers that build a new underline id. */
export function generateUnderlineId(): string {
  return newUnderlineId();
}

function sessionOf(note: Note | undefined): string | undefined {
  const value = (note as Record<string, unknown> | undefined)?.sessionId;
  return typeof value === 'string' && value.trim() ? value.trim() : undefined;
}

/** A folder note's path is its folder; a single-file note's path ends with the file name (3.1). */
export function hasNoteFolder(baseDir: string | undefined): boolean {
  return Boolean(baseDir) && !/\.(?:md|markdown|html|json)$/i.test(baseDir as string);
}

/**
 * 4.1: reads the thread of a note and puts it in memory. A thread replaces the memory one (turns kept only in
 * memory stay behind it); `null` with an old thread moves it (2.10); a failure leaves the memory as it is and
 * records why.
 */
export async function loadThread(host: AskHost, vault: string, target: AskTarget): Promise<void> {
  const key = target.id;
  try {
    const reply = await host.engine(['get-ask', ...targetArgs(target)]);
    if (reply.status !== 0) {
      setAskLoadError(key, reply.stderr.trim() || '까닭을 못 받았어요.');
      return;
    }
    const text = reply.stdout.trim();
    const parsed = text && text !== 'null' ? JSON.parse(text) : null;
    if (parsed) {
      setAskThread(key, mergeUnsaved(readThread(parsed), askNow().threads[key] ?? null));
    } else if (typeof localStorage !== 'undefined') {
      const moved = await migrateLegacyThread(host, localStorage, ASK_LEGACY_KEY, vault, target);
      if (moved) setAskThread(key, mergeUnsaved(readThread(moved), askNow().threads[key] ?? null));
    }
    setAskLoadError(key, null);
  } catch (error) {
    setAskLoadError(key, error instanceof Error ? error.message : String(error));
  }
}

/** 2.3: the screen keeps the keys it knows and brings model and effort back to values the pickers can show. */
export function readThread(raw: any): AskThread {
  const turns: AskTurn[] = Array.isArray(raw?.turns) ? raw.turns : [];
  return {
    noteId: String(raw?.noteId ?? ''),
    sessionId: String(raw?.sessionId ?? ''),
    turns: turns.map((turn) => ({
      id: String(turn.id ?? ''),
      ...(typeof turn.markId === 'string' ? { markId: turn.markId } : {}),
      askedAt: String(turn.askedAt ?? ''),
      ...(typeof turn.quote === 'string' ? { quote: turn.quote } : {}),
      question: String(turn.question ?? ''),
      answer: String(turn.answer ?? ''),
      ...(typeof turn.error === 'string' ? { error: turn.error } : {}),
      model: normalizeModel(turn.model),
      effort: normalizeEffort(turn.effort),
      elapsedMs: Number(turn.elapsedMs) || 0,
      ...(typeof turn.sessionLines === 'number' ? { sessionLines: turn.sessionLines } : {}),
      ...(Array.isArray(turn.images) ? { images: turn.images.filter((image: unknown) => typeof image === 'string') } : {}),
    })),
  };
}

export function useNoteAsk(props?: NoteAskInput): NoteAskOutput {
  const doc = props?.doc;
  const parent = props?.root ?? props?.port.parent;
  const key = doc?.id ?? '';
  const native = tauriAskHost.native;
  const vault = props?.vault ?? '';
  const latest = useRef(props);
  latest.current = props;

  // 4.1: the thread is read when the panel appears, when the note changes and when the vault changes. Not outside the app.
  useEffect(() => {
    if (!doc?.id || !native || !vault) return;
    void loadThread(tauriAskHost, vault, { id: doc.id, ...(parent ? { parent } : {}) });
  }, [doc?.id, parent, vault, native]);

  // 4.11: a `#doc:` link in an answer opens the note's preview.
  useEffect(() => (props?.openDoc ? registerDocOpener(props.openDoc) : undefined), [props?.openDoc]);

  const target = useMemo<AskTarget | null>(() => (doc?.id ? { id: doc.id, ...(parent ? { parent } : {}) } : null), [doc?.id, parent]);
  // 3.1: a sub note on an old single-file layout has no target, and no ask part is drawn for it.
  const hasTarget = Boolean(target) && (!parent || hasNoteFolder(props?.baseDir));

  const ask = (quote: string, question: string, markIdIn: string, images: File[] | undefined, followUpOf?: string) => {
    const input = latest.current;
    if (!input || !target || !input.vault) return; // 3.2: no vault, nothing is asked
    const markId = followUpOf ?? markIdIn;
    const state = askNow();
    const request: AskRequest = {
      target,
      rootId: parent ?? target.id,
      vault: input.vault,
      noteDir: input.baseDir || undefined,
      quote,
      question,
      markId,
      turnId: followUpOf ? newUnderlineId() : markId,
      images,
      htmlRel: markId.startsWith(HTML_ID_PREFIX) ? markId.slice(HTML_ID_PREFIX.length) : undefined,
      heading: noteHeading(input.currentDraft(), input.doc.format),
      repo: input.repo,
      authorSession: sessionOf(input.doc) ?? (parent ? input.rootSessionId : undefined),
      model: state.model,
      effort: state.effort,
      timeoutSeconds: input.timeoutSeconds?.() ?? 3600,
    };
    void startAsk(tauriAskHost, request);
  };

  const followUp = (question: string, underlineId: string, images?: File[]) => ask('', question, underlineId, images, underlineId);

  const frame: AskContextValue = { baseDir: props?.baseDir, noteId: doc?.id };
  return {
    editor: hasTarget
      ? { askDocId: doc?.id, onAskAi: (quote, question, turnId, images) => ask(quote, question, turnId, images), onAskFollowUp: followUp }
      : {},
    panel: hasTarget ? createElement(AskContext.Provider, { value: frame }, createElement(AskPanel, { noteKey: key, onFollowUp: followUp })) : null,
  };
}

// --------------------------------------------------------------------------
// UI Helper Frame & Editor Plugin
// --------------------------------------------------------------------------

export function NoteAskFrame({ children, baseDir, noteId }: { baseDir?: string; noteId?: string; children?: ReactNode }): ReactNode {
  return createElement(AskContext.Provider, { value: { baseDir, noteId } }, children);
}

interface CardAt {
  id: string;
  left: number;
  top: number;
  pinned: boolean;
}

function unionRect(elements: Element[]): DOMRect | null {
  let rect: DOMRect | null = null;
  for (const element of elements) {
    const next = element.getBoundingClientRect();
    rect = rect
      ? new DOMRect(Math.min(rect.left, next.left), Math.min(rect.top, next.top), Math.max(rect.right, next.right) - Math.min(rect.left, next.left), Math.max(rect.bottom, next.bottom) - Math.min(rect.top, next.top))
      : next;
  }
  return rect;
}

/** 4.8: the text of an HTML box's button from the state of its document. */
export function htmlButtonText(state: { pending: { queued?: boolean } | undefined; turns: number }): string {
  if (state.pending) return state.pending.queued ? '기다리는 중…' : '요청 중…';
  return state.turns > 0 ? `AI 문답 ${state.turns}` : 'AI에게 묻기';
}

export function EditorAskPlugins(props?: {
  docId?: string;
  onAsk?: (quote: string, question: string, turnId: string, images?: File[]) => void;
  onFollowUp?: (question: string, underlineId: string, images?: File[]) => void;
  /** Opens the question box for an HTML box (4.8). */
  onOpenCompose?: (quote: string, rect: { left: number; top: number }, markId?: string) => void;
}): React.JSX.Element | null {
  const docId = props?.docId ?? '';
  const anchor = useRef<HTMLSpanElement>(null);
  const [card, setCard] = useState<CardAt | null>(null);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const askedHtml = useRef<Set<string>>(new Set());
  const thread = useAskState((state) => state.threads[docId] ?? null);
  const pending = useAskState((state) => state.pending[docId]);
  const live = useRef({ thread, pending, props });
  live.current = { thread, pending, props };

  const cancel = () => {
    if (timer.current) {
      clearTimeout(timer.current);
      timer.current = null;
    }
  };
  const closeSoon = () => {
    cancel();
    timer.current = setTimeout(() => setCard((current) => (current?.pinned ? current : null)), 180);
  };

  // 4.5: a mark outside a table cell opens the card on hover (not pinned) and on click (pinned).
  useEffect(() => {
    const root = anchor.current?.closest('.lex-editor') ?? null;
    if (!root || !docId) return;
    const markOf = (event: Event): HTMLElement | null => {
      const mark = (event.target as Element | null)?.closest<HTMLElement>('.lex-ask[data-ask-id]') ?? null;
      return mark && !mark.closest('td,th') ? mark : null;
    };
    const place = (id: string): { left: number; top: number } | null => {
      const marks = [...root.querySelectorAll<HTMLElement>('[data-ask-id]')].filter((el) => el.dataset.askId === id && !el.closest('td,th'));
      const rect = unionRect(marks);
      return rect ? { left: rect.left, top: rect.bottom + 6 } : null;
    };
    const over = (event: Event) => {
      const mark = markOf(event);
      if (!mark) return;
      cancel();
      const at = place(mark.dataset.askId!);
      if (at) setCard((current) => (current?.pinned && current.id === mark.dataset.askId ? current : { id: mark.dataset.askId!, ...at, pinned: false }));
    };
    const out = (event: Event) => {
      const mark = markOf(event);
      if (!mark) return;
      const next = (event as MouseEvent).relatedTarget as Element | null;
      // Moving from a mark to the card does not schedule a close; the card is not part of the root.
      if (next?.closest('.ask-card-anchor')) return;
      closeSoon();
    };
    const click = (event: MouseEvent) => {
      const mark = markOf(event);
      if (!mark || event.metaKey || event.ctrlKey || event.altKey) return;
      event.preventDefault();
      cancel();
      const at = place(mark.dataset.askId!);
      if (at) setCard({ id: mark.dataset.askId!, ...at, pinned: true });
    };
    root.addEventListener('mouseover', over);
    root.addEventListener('mouseout', out);
    root.addEventListener('click', click as EventListener);
    return () => {
      root.removeEventListener('mouseover', over);
      root.removeEventListener('mouseout', out);
      root.removeEventListener('click', click as EventListener);
      cancel();
    };
  }, [docId]);

  // 4.6, 4.8: the busy ring on the last piece of each question in flight, and the HTML box buttons. Painted when
  // the editor root appears, whenever the editor changes, and whenever the in-flight record changes.
  useEffect(() => {
    const root = anchor.current?.closest('.lex-editor') ?? null;
    if (!root || !docId) return;
    let frame = 0;
    const paint = () => {
      frame = 0;
      const { thread: current, pending: records, props: latest } = live.current;
      // Clear every ring first: Enter inside a mark rewrites the earlier piece's DOM and would leave a stale ring.
      root.querySelectorAll('.lex-ask-busy').forEach((el) => el.classList.remove('lex-ask-busy', 'is-queued'));
      for (const [markId, record] of Object.entries(records ?? {})) {
        const pieces = [...root.querySelectorAll<HTMLElement>('.lex-ask[data-ask-id]')].filter((el) => el.dataset.askId === markId && !el.closest('td,th'));
        const lastPiece = pieces[pieces.length - 1];
        if (lastPiece) {
          lastPiece.classList.add('lex-ask-busy');
          if (record.queued) lastPiece.classList.add('is-queued');
        }
      }
      root.querySelectorAll<HTMLButtonElement>('.wl-html-frame-ask').forEach((button) => {
        const id = htmlAskId(button.dataset.rel ?? '');
        const text = htmlButtonText({ pending: records?.[id], turns: turnsForUnderline(current?.turns ?? [], id).length });
        button.hidden = !latest?.onAsk; // only an editor that takes questions shows the button
        if (button.textContent !== text) button.textContent = text; // written only when it changes
        button.classList.toggle('is-busy', Boolean(records?.[id]));
        button.classList.toggle('is-queued', Boolean(records?.[id]?.queued));
      });
    };
    const schedule = () => {
      if (!frame) frame = requestAnimationFrame(paint);
    };
    paint();
    const observer = new MutationObserver(schedule);
    observer.observe(root, { childList: true, subtree: true, attributes: true, attributeFilter: ['data-ask-id', 'class'] });
    const clickButton = (event: Event) => {
      const button = (event.target as Element | null)?.closest<HTMLButtonElement>('.wl-html-frame-ask');
      if (!button || !root.contains(button)) return;
      event.preventDefault();
      event.stopPropagation();
      const rel = button.dataset.rel ?? '';
      const id = htmlAskId(rel);
      const header = button.closest('.wl-html-frame-header') ?? button;
      const rect = header.getBoundingClientRect();
      const { thread: current, pending: records, props: latest } = live.current;
      if (turnsForUnderline(current?.turns ?? [], id).length === 0 && !records?.[id]) {
        askedHtml.current.add(id);
        latest?.onOpenCompose?.(htmlQuote(button.dataset.label ?? '', rel), { left: rect.left, top: rect.bottom + 6 }, id);
      } else {
        setCard({ id, left: rect.left, top: rect.bottom + 6, pinned: true });
      }
    };
    root.addEventListener('click', clickButton, true);
    return () => {
      if (frame) cancelAnimationFrame(frame);
      observer.disconnect();
      root.removeEventListener('click', clickButton, true);
    };
  }, [docId, thread, pending]);

  // 4.8: once the first question of an HTML box leaves, the card opens at once so the box shows its "요청 중".
  useEffect(() => {
    for (const id of [...askedHtml.current]) {
      if (!pending?.[id]) continue;
      askedHtml.current.delete(id);
      const root = anchor.current?.closest('.lex-editor');
      const button = [...(root?.querySelectorAll<HTMLButtonElement>('.wl-html-frame-ask') ?? [])].find((el) => htmlAskId(el.dataset.rel ?? '') === id);
      const rect = (button?.closest('.wl-html-frame-header') ?? button)?.getBoundingClientRect();
      if (rect) setCard({ id, left: rect.left, top: rect.bottom + 6, pinned: true });
    }
  }, [pending]);

  return (
    <>
      <span ref={anchor} hidden />
      {card && docId && (
        <div className="ask-card-anchor" style={{ position: 'fixed', left: card.left, top: card.top, zIndex: 50 }} onMouseEnter={cancel} onMouseLeave={() => !card.pinned && closeSoon()}>
          <AskHoverCard key={card.id} docId={docId} turnId={card.id} pinned={card.pinned} onFollowUp={props?.onFollowUp} onClose={() => setCard(null)} />
        </div>
      )}
    </>
  );
}

// Backwards-compatible stub
export function NoteAskPanel(_props?: any): null {
  return null;
}
