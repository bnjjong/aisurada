// 설정 화면의 「노트」. 노트 목록과 본문에 무엇을 보일지 정한다.
import type { WorkspaceController } from "../workspace/controller.ts";
import type { WorkspaceState } from "../workspace/types.ts";
import { displayTitle } from "./model.ts";
import { SettingsGroup, SettingsPane, SettingsRow, SettingsSwitch } from "./SettingsParts.tsx";

export function NoteSettings({
  controller,
  state,
}: {
  controller: WorkspaceController;
  state: WorkspaceState;
}) {
  const example = displayTitle(
    { title: "예시 노트", date: state.workspace?.today || "2026-11-01" },
    state.prefs.titleDatePrefix ?? state.prefs.titleDate,
  );

  return (
    <SettingsPane title="노트" desc="노트 목록과 본문에 무엇을 보일지 정해요.">
      <SettingsGroup title="목록과 제목">
        <SettingsRow
          settingKey="titleDatePrefix"
          className="set-switch"
          title="제목 앞 날짜"
          desc={
            <>
              노트 목록과 즐겨찾기와 빠른 검색과 연결 화면에 적용돼요.
              <span className="set-example-tag">예 · {example}</span>
            </>
          }
          control={
            <SettingsSwitch
              checked={!!state.prefs.titleDatePrefix}
              onChange={(checked) => controller.preference({ titleDatePrefix: checked })}
            />
          }
        />
        <SettingsRow
          settingKey="showRecordTier"
          className="set-switch"
          title="기록 노트"
          desc="목록과 검색에 기록 노트를 포함해요."
          control={
            <SettingsSwitch
              checked={!!state.prefs.showRecordTier}
              onChange={(checked) => controller.preference({ showRecordTier: checked })}
            />
          }
        />
      </SettingsGroup>
      <SettingsGroup title="본문">
        <SettingsRow
          settingKey="mermaidShowSource"
          className="set-switch"
          title="다이어그램"
          desc="노트의 Mermaid 코드를 그림과 함께 펼쳐요."
          control={
            <SettingsSwitch
              checked={!!state.prefs.mermaidShowSource}
              onChange={(checked) => controller.preference({ mermaidShowSource: checked })}
            />
          }
        />
      </SettingsGroup>
    </SettingsPane>
  );
}
