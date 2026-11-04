// AI Q&A panel rendered below note body.
// Adheres to Chapter 4 (4.1, 4.2, 4.3, 4.4, 4.9, 4.12) of withlog Part 4 Claude integration specifications.
import { useEffect, useState } from 'react';
import { ModelProse } from './modelProse.tsx';
import { ASK_MODELS } from './askPopover.tsx';
import { dismissAbandoned, useAskState } from './state.ts';

export interface AskTurn {
  id: string;
  markId: string;
  askedAt: string;
  quote?: string;
  question: string;
  answer: string;
  error?: string;
  model: string;
  effort: string;
  elapsedMs: number;
  sessionLines?: number;
  images?: string[];
  unsaved?: boolean;
}

export interface AskThread {
  v?: number;
  noteId: string;
  sessionId: string;
  turns: AskTurn[];
}

interface AskPanelProps {
  noteKey: string;
  thread: AskThread | null;
  loadError: string | null;
  onFollowUp?: (question: string, underlineId: string, images?: File[]) => void;
}

function formatElapsedTimer(startedAt: number, now: number): string {
  const diffSec = Math.max(0, Math.floor((now - startedAt) / 1000));
  const m = Math.floor(diffSec / 60);
  const s = diffSec % 60;
  return `${m}:${s < 10 ? '0' : ''}${s}`;
}

export function AskPanel({
  noteKey,
  thread,
  loadError,
  onFollowUp,
}: AskPanelProps) {
  const askState = useAskState();
  const [now, setNow] = useState(Date.now());
  const [activeFollowUpMark, setActiveFollowUpMark] = useState<string | null>(null);
  const [followUpText, setFollowUpText] = useState('');

  const pendingMap = askState.pending[noteKey] || {};
  const pendingEntries = Object.entries(pendingMap);
  const abandonedList = askState.abandoned[noteKey] || [];
  const turns = thread?.turns || [];

  // 요청 중 타이머 1초마다 갱신 (4.3)
  useEffect(() => {
    if (pendingEntries.length === 0) return;
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [pendingEntries.length]);

  // 4.3 그리는 때: 회차, 묻는 중, 버려진 물음, 못 읽은 까닭 중 하나라도 있을 때만 그림
  const hasContent =
    turns.length > 0 ||
    pendingEntries.length > 0 ||
    abandonedList.length > 0 ||
    Boolean(loadError);

  if (!hasContent) {
    return null;
  }

  // 회차들을 markId 순서대로 묶음 (2.4 규칙)
  const markGroups: Array<{ markId: string; quote?: string; turns: AskTurn[] }> = [];
  const markIndexMap = new Map<string, number>();

  for (const t of turns) {
    const mId = t.markId || t.id;
    let groupIdx = markIndexMap.get(mId);
    if (groupIdx === undefined) {
      groupIdx = markGroups.length;
      markIndexMap.set(mId, groupIdx);
      markGroups.push({
        markId: mId,
        quote: t.quote,
        turns: [],
      });
    }
    markGroups[groupIdx].turns.push(t);
  }

  const handleFollowUpSubmit = (markId: string) => {
    const trimmed = followUpText.trim();
    if (!trimmed) return;
    onFollowUp?.(trimmed, markId);
    setFollowUpText('');
    setActiveFollowUpMark(null);
  };

  return (
    <section className="ask-panel">
      <div className="ask-panel-header">
        <h3 className="ask-panel-title">AI 문답</h3>
        {thread?.sessionId && (
          <span
            className="ask-panel-resume"
            title="터미널에서 이 세션을 이어 갈 때 쓰는 명령어예요."
          >
            claude --resume {thread.sessionId}
          </span>
        )}
      </div>

      {loadError && (
        <div className="ask-load-error">
          이 노트의 문답 기록을 못 읽었어요: {loadError}
        </div>
      )}

      {/* 버려진 물음 줄 (4.9) */}
      {abandonedList.map((item) => (
        <div key={item.turnId} className="ask-abandoned">
          <span className="ask-abandoned-text">
            앱을 닫기 전에 보낸 물음: “{item.question}”
          </span>
          {item.sessionId && (
            <code className="ask-abandoned-cmd">
              claude --resume {item.sessionId}
            </code>
          )}
          <button
            type="button"
            className="ask-abandoned-x"
            onClick={() => dismissAbandoned(noteKey, item.turnId)}
            title="이 줄 지우기"
          >
            ×
          </button>
        </div>
      ))}

      {/* 밑줄 묶음 목록 (4.3) */}
      {markGroups.map((group) => {
        const isFollowUpOpen = activeFollowUpMark === group.markId;

        return (
          <div key={group.markId} className="ask-mark-group">
            {group.quote && (
              <div className="ask-quote" title={group.quote}>
                “{group.quote}”
              </div>
            )}

            {group.turns.map((turn) => {
              const modelLabel =
                ASK_MODELS.find((m) => m.id === turn.model)?.label || turn.model;
              const elapsedSec = (turn.elapsedMs / 1000).toFixed(1);

              return (
                <div
                  key={turn.id}
                  className={`ask-turn ${turn.error ? 'is-error' : ''}`}
                >
                  <div className="ask-question">{turn.question}</div>

                  {turn.images && turn.images.length > 0 && (
                    <div className="ask-images">
                      <span className="ask-images-count">
                        그림 {turn.images.length}장 첨부
                      </span>
                    </div>
                  )}

                  {turn.error ? (
                    <div className="ask-error">{turn.error}</div>
                  ) : (
                    <div className="ask-answer">
                      <ModelProse content={turn.answer} foldable={true} />
                    </div>
                  )}

                  <div className="ask-meta">
                    {modelLabel} · {turn.effort} · {elapsedSec}초
                    {turn.sessionLines && turn.sessionLines > 0 ? (
                      <span> · 작성 세션 대화 {turn.sessionLines}줄 함께 보냄</span>
                    ) : null}
                  </div>

                  {turn.unsaved && (
                    <div className="ask-unsaved">
                      이 답은 vault에 저장되지 않았어요. 앱을 닫으면 사라져요.
                    </div>
                  )}
                </div>
              );
            })}

            {/* 이어 묻는 줄 (4.4) */}
            {onFollowUp && (
              <div className="ask-followup-container">
                {isFollowUpOpen ? (
                  <form
                    className="ask-followup"
                    onSubmit={(e) => {
                      e.preventDefault();
                      handleFollowUpSubmit(group.markId);
                    }}
                  >
                    <input
                      type="text"
                      className="ask-followup-input"
                      value={followUpText}
                      onChange={(e) => setFollowUpText(e.target.value)}
                      placeholder="이 대목에 이어 묻기"
                      autoFocus
                    />
                    <button type="submit" disabled={!followUpText.trim()}>
                      이어 묻기
                    </button>
                    <button
                      type="button"
                      onClick={() => {
                        setActiveFollowUpMark(null);
                        setFollowUpText('');
                      }}
                    >
                      취소
                    </button>
                  </form>
                ) : (
                  <button
                    type="button"
                    className="ask-followup-open"
                    onClick={() => setActiveFollowUpMark(group.markId)}
                  >
                    이 대목에 이어 묻기
                  </button>
                )}
              </div>
            )}
          </div>
        );
      })}

      {/* 요청 중인 질문 목록 (4.3 4번 줄) */}
      {pendingEntries.map(([turnId, pendingInfo]) => {
        const timerText = formatElapsedTimer(pendingInfo.askedAt, now);

        return (
          <div key={turnId} className="ask-pending">
            <span className="ask-pending-label">
              {pendingInfo.queued ? '줄 서는 중' : `요청 중 (${timerText})`}
            </span>
            {pendingInfo.quote && (
              <span className="ask-pending-quote">
                “{pendingInfo.quote.length > 50
                  ? pendingInfo.quote.slice(0, 50) + '…'
                  : pendingInfo.quote}”
              </span>
            )}
          </div>
        );
      })}
    </section>
  );
}
