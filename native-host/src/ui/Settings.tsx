import type { WorkspaceController } from "../workspace/controller.ts";
import type { Skin, WorkspaceState } from "../workspace/types.ts";
import { ClaudeConnect } from "@claude-plug/connect";
import { Modal } from "./Modal.tsx";
import { displayTitle } from "./model.ts";
/** 설정에서 고를 수 있는 화면 모양. 순서가 곧 화면에 나오는 순서다. */
const SKIN_OPTIONS: readonly { id: Skin; label: string; hint: string }[] = [
  {
    id: "tactile",
    label: "물성",
    hint: "눌리는 것은 떠 있고, 쓰는 것은 파여 있는 둥근 모양이에요.",
  },
  {
    id: "grid",
    label: "격자",
    hint: "4px 격자와 또렷한 먹선, 각진 모서리로 정확하게 그린 모양이에요.",
  },
  {
    id: "glass",
    label: "유리",
    hint: "반투명 유리 패널과 은은한 빛번짐으로 가볍게 그린 모양이에요.",
  },
  {
    id: "modern",
    label: "모던",
    hint: "얇은 선과 차분한 회색, 작은 곡률로 정돈한 모양이에요.",
  },
  {
    id: "classic",
    label: "기본",
    hint: "얇은 테두리와 평평한 면으로 그린 이전 모양이에요.",
  },
];

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
      <fieldset className="set-skin">
        <legend>화면 모양</legend>
        <div className="set-skin-grid">
        {SKIN_OPTIONS.map((option) => (
          <label className="set-skin-option" key={option.id}>
            <input
              type="radio"
              name="skin"
              value={option.id}
              checked={state.skin === option.id}
              onChange={() => controller.preference({ skin: option.id })}
            />
            <span className="skin-preview" data-skin-preview={option.id} aria-hidden="true">
              <i />
              <b />
            </span>
            <span className="set-skin-text">
              <strong>{option.label}</strong>
              <small>{option.hint}</small>
            </span>
          </label>
        ))}
        </div>
      </fieldset>
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
      <ClaudeConnect />
    </Modal>
  );
}
