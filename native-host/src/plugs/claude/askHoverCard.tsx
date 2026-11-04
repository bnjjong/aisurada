// Hover card appearing above/below ask mark in note editor.
// Adheres to Chapter 4 (4.5, 4.7) of withlog Part 4 Claude integration specifications.
import React, { useEffect, useRef, useState } from 'react';
import { ModelProse } from './modelProse.tsx';
import { ASK_MODELS } from './askPopover.tsx';

export interface AskHoverCardProps {
  docId: string;
  turnId: string;
  turns?: Array<{
    id: string;
    markId: string;
    quote?: string;
    question: string;
    answer: string;
    error?: string;
    model: string;
    effort: string;
    elapsedMs: number;
    sessionLines?: number;
    images?: string[];
  }>;
  onFollowUp?: (question: string, underlineId: string, images?: File[]) => void;
  onFocusInput?: () => void;
  onClose?: () => void;
  pinned?: boolean;
  boundaryRef?: React.RefObject<HTMLElement | null>;
}

export function AskHoverCard({
  turnId,
  turns,
  onFollowUp,
  onClose,
  pinned = false,
}: AskHoverCardProps) {
  const [isPinned, setIsPinned] = useState(pinned);
  const [followUpText, setFollowUpText] = useState('');
  const closeTimerRef = useRef<NodeJS.Timeout | null>(null);

  // 현재 밑줄(turnId 또는 markId)에 연결된 회차들 필터링
  const matchingTurns = (turns || []).filter(
    (t) => t.id === turnId || t.markId === turnId
  );

  const handleMouseEnter = () => {
    if (closeTimerRef.current) {
      clearTimeout(closeTimerRef.current);
      closeTimerRef.current = null;
    }
  };

  const handleMouseLeave = () => {
    if (isPinned) return;
    // 4.5: 180ms 지연 닫기
    closeTimerRef.current = setTimeout(() => {
      onClose?.();
    }, 180);
  };

  useEffect(() => {
    return () => {
      if (closeTimerRef.current) clearTimeout(closeTimerRef.current);
    };
  }, []);

  const handleFollowUpSubmit = (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    const trimmed = followUpText.trim();
    if (!trimmed) return;
    onFollowUp?.(trimmed, turnId);
    setFollowUpText('');
  };

  return (
    <div
      className={`ask-hovercard ${isPinned ? 'is-pinned' : ''}`}
      onMouseEnter={handleMouseEnter}
      onMouseLeave={handleMouseLeave}
    >
      <div className="ask-hovercard-header">
        <span className="ask-hovercard-title">문답</span>
        <button
          type="button"
          className="ask-hovercard-pin-btn"
          title={isPinned ? '핀 풀기' : '카드 고정'}
          onClick={() => setIsPinned(!isPinned)}
        >
          {isPinned ? '📌' : '📍'}
        </button>
        {onClose && (
          <button
            type="button"
            className="ask-hovercard-close-btn"
            onClick={onClose}
            aria-label="닫기"
          >
            ×
          </button>
        )}
      </div>

      <div className="ask-hovercard-body">
        {matchingTurns.length === 0 ? (
          <div className="ask-hovercard-empty">
            <p>이 대목의 문답을 불러오는 중이거나 찾을 수 없어요.</p>
          </div>
        ) : (
          matchingTurns.map((turn) => {
            const modelLabel =
              ASK_MODELS.find((m) => m.id === turn.model)?.label || turn.model;
            const elapsedSec = (turn.elapsedMs / 1000).toFixed(1);

            return (
              <div
                key={turn.id}
                className={`ask-turn ${turn.error ? 'is-error' : ''}`}
              >
                {turn.quote && (
                  <div className="ask-quote" title={turn.quote}>
                    “{turn.quote}”
                  </div>
                )}
                <div className="ask-question">{turn.question}</div>
                {turn.error ? (
                  <div className="ask-error">{turn.error}</div>
                ) : (
                  <div className="ask-answer">
                    <ModelProse content={turn.answer} foldable={false} />
                  </div>
                )}
                <div className="ask-meta">
                  {modelLabel} · {turn.effort} · {elapsedSec}초
                </div>
              </div>
            );
          })
        )}
      </div>

      {onFollowUp && (
        <form className="ask-hovercard-followup" onSubmit={handleFollowUpSubmit}>
          <input
            type="text"
            value={followUpText}
            onChange={(e) => setFollowUpText(e.target.value)}
            placeholder="이 대목에 이어 묻기"
          />
          <button type="submit" disabled={!followUpText.trim()}>
            묻기
          </button>
        </form>
      )}

      <div className="ask-hovercard-hint">
        {isPinned ? '카드가 고정됨' : '바깥으로 마우스를 빼면 닫혀요'}
      </div>
    </div>
  );
}
