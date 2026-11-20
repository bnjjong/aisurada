import { usePendingDecisions, waitingOn } from "./decisionsStore.ts";

/**
 * 노트 목록의 한 줄에 붙는 「답 기다림」 표시입니다. AI가 이 노트(또는 그 후속 노트)에 쓴 「정할 것」이 사람의 답을 기다릴 때만 보입니다.
 * 목록에서 답할 노트를 바로 찾게 하려는 것이고, 수는 `ui/decisionsStore.ts`가 엔진에서 읽어 둔 것입니다.
 */
export function DecisionMark({ noteId }: { noteId: string }) {
  const waiting = waitingOn(usePendingDecisions(), noteId);
  if (!waiting) return null;
  const detail = waiting.answered ? `정할 것 ${waiting.total}개 가운데 ${waiting.answered}개 답함` : `정할 것 ${waiting.total}개`;
  return (
    <span className="decision-mark" title={`AI가 이 노트에서 답을 기다려요 · ${detail}`} aria-label={`답 기다림, ${detail}`}>
      답 기다림
    </span>
  );
}
