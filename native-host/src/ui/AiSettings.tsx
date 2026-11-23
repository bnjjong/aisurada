// 설정 화면의 「AI」. Claude 연결과 대기 시간 선택을 보인다.
import type { WorkspaceController } from "../workspace/controller.ts";
import type { WorkspaceState } from "../workspace/types.ts";
import { ClaudeConnect } from "@claude-plug/connect";
import { CLAUDE_ENABLED } from "@claude-plug/state";
import { SettingsGroup, SettingsPane, SettingsRow } from "./SettingsParts.tsx";

export function AiSettings({
  controller,
  state,
}: {
  controller: WorkspaceController;
  state: WorkspaceState;
}) {
  if (!CLAUDE_ENABLED) return null;
  return (
    <SettingsPane title="AI" desc="Claude가 노트를 다루는 범위와 기다리는 시간을 정해요.">
      <div className="set-group">
        <h4>Claude 연결</h4>
        <ClaudeConnect />
      </div>
      <SettingsGroup title="묻기">
        <SettingsRow
          as="label"
          settingKey="ask-timeout"
          className="set-switch set-ask-timeout"
          title="AI 응답 대기 시간"
          desc="노트에서 AI에게 질문하고 응답을 기다리는 최대 시간이에요."
          control={
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
          }
        />
      </SettingsGroup>
    </SettingsPane>
  );
}
