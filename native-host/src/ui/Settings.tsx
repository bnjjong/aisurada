import type { WorkspaceController } from "../workspace/controller.ts";
import type { WorkspaceState } from "../workspace/types.ts";
import { Modal } from "./Modal.tsx";
import { displayTitle } from "./model.ts";
export function Settings({
  controller,
  state,
}: {
  controller: WorkspaceController;
  state: WorkspaceState;
}) {
  return (
    <Modal
      title="설정"
      close={() => controller.settings(false)}
      className="settings-modal"
    >
      <p>이 기기에만 적용되는 설정이에요. 노트 파일은 바뀌지 않아요.</p>
      {(
        [
          [
            "titleDatePrefix",
            "제목 앞 날짜",
            "노트 목록과 즐겨찾기와 빠른 검색과 연결 화면에 적용돼요.",
          ],
          ["showRecordTier", "기록 노트", "목록과 검색에 기록 노트를 포함해요."],
          [
            "mermaidShowSource",
            "다이어그램",
            "노트의 Mermaid 코드를 그림과 함께 펼쳐요.",
          ],
        ] as const
      ).map(([key, label, hint]) => (
        <label className="set-switch" key={key}>
          <span>
            <strong>{label}</strong>
            <small>{hint}</small>
          </span>
          <input
            type="checkbox"
            checked={!!state.prefs[key as keyof typeof state.prefs]}
            onChange={(e) => controller.preference({ [key]: e.target.checked })}
          />
        </label>
      ))}
      <p className="settings-example">
        {displayTitle(
          { title: "예시 노트", date: state.workspace?.today || "2026-11-01" },
          state.prefs.titleDatePrefix ?? state.prefs.titleDate,
        )}
      </p>
    </Modal>
  );
}
