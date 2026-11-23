// 설정 화면의 「모양」. 화면 모양 카드와 화면 크기 단추를 보인다.
import type { WorkspaceController } from "../workspace/controller.ts";
import type { Skin, WorkspaceState } from "../workspace/types.ts";
import { UI_SCALES } from "../workspace/typography.ts";

/** 설정에서 고를 수 있는 화면 모양. 순서가 곧 화면에 나오는 순서다. */
export const SKIN_OPTIONS: readonly { id: Skin; label: string; hint: string }[] = [
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

export function LookSettings({
  controller,
  state,
}: {
  controller: Pick<WorkspaceController, "preference">;
  state: Pick<WorkspaceState, "skin" | "typography">;
}) {
  return (
    <>
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
      <fieldset className="set-typo">
        <div className="typo-row">
          <strong id="typo-scale-label">화면 크기</strong>
          <small>
            글자와 버튼을 앱 전체에서 함께 키우거나 줄여요. ⌘ + / ⌘ - / ⌘ 0 키로도 바꿀 수 있어요. 지금은 {state.typography.scale}%예요.
          </small>
          <div className="typo-scale" role="group" aria-labelledby="typo-scale-label">
            {UI_SCALES.map((scale) => (
              <button
                type="button"
                key={scale}
                aria-pressed={state.typography.scale === scale}
                onClick={() => controller.preference({ uiScale: scale })}
              >
                {scale}%
              </button>
            ))}
          </div>
        </div>
      </fieldset>
    </>
  );
}
