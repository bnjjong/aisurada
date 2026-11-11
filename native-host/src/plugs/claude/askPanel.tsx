// AI Q&A panel rendered below note body.
// Adheres to Chapter 4 (4.1, 4.2, 4.3, 4.4, 4.9, 4.12) of aisurada Part 4 Claude integration specifications.
import { useEffect, useState } from 'react';
import { AskTurnView, FollowUpRow, PendingLine } from './askParts.tsx';
import { groupTurns } from './askText.ts';
import type { AskThread, AskTurn } from './askText.ts';
import { clearNoteUnseen, dismissAbandoned, useAskState } from './state.ts';
import type { AbandonedAsk } from './state.ts';

export type { AskThread, AskTurn };

interface AskPanelProps {
  /** The store key: the note id, or the sub note id. */
  noteKey: string;
  onFollowUp?: (question: string, underlineId: string, images?: File[]) => void;
}

/** 4.9: the text of an abandoned line: [head, second line]. */
export function abandonedLines(item: AbandonedAsk): [string, string] {
  if (item.queued) return ['보내지 못한 물음이 있어요. 앞 물음을 기다리는 사이에 앱이 닫혔어요.', '이 물음은 AI에게 가지 않았어요. 같은 대목에서 다시 물어 주세요.'];
  if (item.runtime === 'http') return ['답을 못 받은 물음이 있어요. 앱이 닫히는 사이에 끝났어요.', 'HTTP 대화라 터미널에서 못 열어요.'];
  return ['답을 못 받은 물음이 있어요. 앱이 닫히는 사이에 끝났어요.', `claude --resume ${item.sessionId}`];
}

function askedAtText(at: number): string {
  return new Date(at).toLocaleString('ko-KR', { year: 'numeric', month: 'long', day: 'numeric', hour: 'numeric', minute: '2-digit' });
}

export function AskPanel({ noteKey, onFollowUp }: AskPanelProps) {
  const thread = useAskState((state) => state.threads[noteKey] ?? null) as AskThread | null;
  const pendingByMark = useAskState((state) => state.pending[noteKey]);
  const abandoned = useAskState((state) => state.abandoned[noteKey]);
  const loadError = useAskState((state) => state.loadErrors[noteKey] ?? null);
  const runtime = useAskState((state) => state.runtime);
  const [open, setOpen] = useState<Set<string>>(() => new Set());

  const turns = thread?.turns ?? [];
  const pendingEntries = Object.entries(pendingByMark ?? {}).sort((a, b) => a[1].at - b[1].at);
  const abandonedList = abandoned ?? [];

  // 4.3 「본 것으로 적기」: once the panel draws a turn, the unseen mark of the note goes off, and again each time
  // the turn count grows, or an answer that arrives while the note is open keeps the tab dot lit.
  useEffect(() => {
    if (turns.length > 0) clearNoteUnseen(noteKey);
  }, [noteKey, turns.length]);

  // 4.3: nothing at all to draw means no panel.
  if (turns.length === 0 && pendingEntries.length === 0 && abandonedList.length === 0 && !loadError) return null;

  const groups = groupTurns(turns);
  const grouped = new Set(groups.map((group) => group.markId));
  const waitingOnly = pendingEntries.filter(([markId]) => !grouped.has(markId));

  const toggle = (markId: string, on: boolean) =>
    setOpen((prev) => {
      const next = new Set(prev);
      if (on) next.add(markId);
      else next.delete(markId);
      return next;
    });

  return (
    <section className="ask-panel">
      <div className="ask-panel-header">
        <h3 className="ask-panel-title">AI 문답</h3>
        {thread?.sessionId && runtime !== 'http' && (
          <span className="ask-panel-resume" title="터미널에서 이 대화를 이어 갈 때 쓰는 명령이에요.">claude --resume {thread.sessionId}</span>
        )}
      </div>

      {loadError && <div className="ask-load-error">이 노트의 문답 기록을 못 읽었어요. {loadError}</div>}

      {groups.map((group) => {
        const record = pendingByMark?.[group.markId];
        return (
          <div key={group.markId} className="ask-mark-group" data-mark-id={group.markId}>
            {group.turns.map((turn) => (
              <AskTurnView key={turn.id} turn={turn} fold />
            ))}
            {record ? (
              <PendingLine record={record} markId={group.markId} noteKey={noteKey} />
            ) : onFollowUp && open.has(group.markId) ? (
              <FollowUpRow
                autoFocus
                onSend={(question, images) => {
                  onFollowUp(question, group.markId, images);
                  toggle(group.markId, false);
                }}
                onClose={() => toggle(group.markId, false)}
              />
            ) : onFollowUp ? (
              <button type="button" className="ask-followup-open" onClick={() => toggle(group.markId, true)}>이 대목에 이어 묻기</button>
            ) : null}
          </div>
        );
      })}

      {waitingOnly.map(([markId, record]) => (
        <PendingLine key={markId} record={record} markId={markId} noteKey={noteKey} />
      ))}

      {abandonedList.map((item) => {
        const [head, second] = abandonedLines(item);
        return (
          <div key={item.markId} className="ask-abandoned">
            <div className="ask-abandoned-head">
              <span>{head}</span>
              <button type="button" className="ask-abandoned-x" aria-label="이 줄 지우기" title="이 줄 지우기" onClick={() => dismissAbandoned(noteKey, item.markId)}>×</button>
            </div>
            {second.startsWith('claude --resume') ? <code className="ask-abandoned-cmd">{second}</code> : <div>{second}</div>}
            <div className="ask-abandoned-at">{askedAtText(item.at)} 에 물었어요</div>
          </div>
        );
      })}
    </section>
  );
}
