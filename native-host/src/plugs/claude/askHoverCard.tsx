// Hover card appearing above/below ask mark in note editor.
// Adheres to Chapter 4 (4.5, 4.7) of withlog Part 4 Claude integration specifications.
import { useEffect, useRef, useState } from 'react';
import type { RefObject } from 'react';
import { AskTurnView, FollowUpRow, PendingLine } from './askParts.tsx';
import { turnsForUnderline } from './askText.ts';
import type { AskThread } from './askText.ts';
import { useAskState } from './state.ts';

export interface AskHoverCardProps {
  /** The store key of the note: the note id, or the sub note id. */
  docId: string;
  /** An underline id, or the id of a follow-up turn (it moves to its underline). */
  turnId: string;
  onFollowUp?: (question: string, underlineId: string, images?: File[]) => void;
  onFocusInput?: () => void;
  onClose?: () => void;
  pinned?: boolean;
  boundaryRef?: RefObject<HTMLElement | null>;
}

/** 4.5: the delay before a card that is not pinned closes once the pointer has left. */
export const CARD_CLOSE_DELAY_MS = 180;

export function AskHoverCard({ docId, turnId, onFollowUp, onClose, pinned = false }: AskHoverCardProps) {
  const thread = useAskState((state) => state.threads[docId] ?? null) as AskThread | null;
  const pendingByMark = useAskState((state) => state.pending[docId]);
  const [isPinned, setIsPinned] = useState(pinned);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const card = useRef<HTMLDivElement>(null);

  // 4.5: the card picks the turns of one underline; a follow-up's id moves to the underline it belongs to.
  const turns = turnsForUnderline(thread?.turns ?? [], turnId);
  const underline = turns.length ? (turns[0].markId || turns[0].id) : turnId;
  const record = pendingByMark?.[underline] ?? pendingByMark?.[turnId];

  useEffect(() => setIsPinned(pinned), [pinned]);
  useEffect(() => () => {
    if (timer.current) clearTimeout(timer.current);
  }, []);

  // 4.5: a press outside a pinned card closes it.
  useEffect(() => {
    if (!isPinned) return;
    const outside = (event: PointerEvent) => {
      if (card.current && !card.current.contains(event.target as Node)) onClose?.();
    };
    document.addEventListener('pointerdown', outside, true);
    return () => document.removeEventListener('pointerdown', outside, true);
  }, [isPinned, onClose]);

  const cancelClose = () => {
    if (timer.current) {
      clearTimeout(timer.current);
      timer.current = null;
    }
  };
  const scheduleClose = () => {
    if (isPinned) return;
    cancelClose();
    timer.current = setTimeout(() => onClose?.(), CARD_CLOSE_DELAY_MS);
  };

  return (
    <div
      ref={card}
      className={`ask-hovercard${isPinned ? ' is-pinned' : ''}`}
      data-ask-card={underline}
      onMouseEnter={cancelClose}
      onMouseLeave={scheduleClose}
      // Any press inside the card pins it; a focus that lands in its box does the same.
      onPointerDown={() => setIsPinned(true)}
      onFocus={() => setIsPinned(true)}
      onKeyDown={(event) => {
        // 4.5: Esc inside the card closes it and stops the native propagation, so a preview modal around it stays.
        if (event.key === 'Escape') {
          event.preventDefault();
          event.stopPropagation();
          event.nativeEvent.stopImmediatePropagation();
          onClose?.();
        }
      }}
    >
      {turns.length === 0 && !record ? (
        <div className="ask-hovercard-empty">이 기기에 그 문답이 없어요. 밑줄만 남아 있어요.</div>
      ) : (
        <div className="ask-hovercard-body">
          {turns.map((turn) => (
            <AskTurnView key={turn.id} turn={turn} fold={false} />
          ))}
          {record && <PendingLine record={record} />}
        </div>
      )}
      {turns.length > 0 && !isPinned && <div className="ask-hovercard-hint">눌러서 고정하면 이어 물을 수 있어요</div>}
      {turns.length > 0 && isPinned && !record && onFollowUp && (
        <FollowUpRow onSend={(question, images) => onFollowUp(question, underline, images)} />
      )}
    </div>
  );
}
