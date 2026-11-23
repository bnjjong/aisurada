// 설정 화면의 「AI」. Claude 연결과 대기 시간 선택을 보인다.
import type { WorkspaceController } from "../workspace/controller.ts";
import type { WorkspaceState } from "../workspace/types.ts";
import { ClaudeConnect } from "@claude-plug/connect";
import { CLAUDE_ENABLED } from "@claude-plug/state";

export function AiSettings({
  controller,
  state,
}: {
  controller: WorkspaceController;
  state: WorkspaceState;
}) {
  if (!CLAUDE_ENABLED) return null;
  return (
    <>
      <ClaudeConnect />
      <label className="set-switch set-ask-timeout">
        <span>
          <strong>AI 응답 대기 시간</strong>
          <small>노트에서 AI에게 질문하고 응답을 기다리는 최대 시간이에요.</small>
        </span>
        <select
          className="set-ask-timeout-select"
          value={state.prefs.askTimeoutSeconds ?? 3600}
          onChange={(e) => controller.preference({ askTimeoutSeconds: Number(e.target.value) })}
        >
          <option value={600}>10분</option>
          <option value={1200}>20분</option>
          <option value={1800}>30분</option>
          <option value={3600}>1시간 (기본값)</option>
          <option value={7200}>2시간</option>
        </select>
      </label>
    </>
  );
}
