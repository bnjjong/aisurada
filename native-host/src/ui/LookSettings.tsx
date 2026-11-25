// 설정 화면의 「모양」. 화면 모양 카드와 화면 크기 단추를 보인다.
import type { WorkspaceController } from "../workspace/controller.ts";
import type { Skin, WorkspaceState } from "../workspace/types.ts";
import { UI_SCALES } from "../workspace/typography.ts";
import { SettingsPane } from "./SettingsParts.tsx";
import { SKIN_OPTIONS, findSkinOption, type SkinOption } from "../workspace/skin.ts";

export { SKIN_OPTIONS, type SkinOption } from "../workspace/skin.ts";

/**
 * 화면 모양 단추(라디오 카드 한 장) 컴포넌트
 */
export function SkinOptionCard({
  option,
  checked,
  onChange,
}: {
  option: SkinOption;
  checked: boolean;
  onChange: (id: Skin) => void;
}) {
  return (
    <label className="set-skin-option">
      <input
        type="radio"
        name="skin"
        value={option.id}
        checked={checked}
        onChange={() => onChange(option.id)}
      />
      <span className="skin-preview" data-skin-preview={option.id} aria-hidden="true">
        <i />
        <b />
      </span>
      <span className="set-skin-text">
        <strong>{option.label}</strong>
      </span>
    </label>
  );
}

/**
 * 스킨 선택 라디오 단추 목록 그리드 컴포넌트
 */
export function SkinPicker({
  skin,
  onChange,
}: {
  skin: Skin;
  onChange: (skin: Skin) => void;
}) {
  const currentSkin = findSkinOption(skin);

  return (
    <div className="set-group" data-setting="skin">
      <h4>화면 모양</h4>
      <div className="set-skin-grid" role="radiogroup" aria-label="화면 모양">
        {SKIN_OPTIONS.map((option) => (
          <SkinOptionCard
            key={option.id}
            option={option}
            checked={skin === option.id}
            onChange={onChange}
          />
        ))}
      </div>
      <p className="set-skin-selected-desc">
        <strong>{currentSkin.label}</strong> · {currentSkin.hint}
      </p>
    </div>
  );
}

export function LookSettings({
  controller,
  state,
}: {
  controller: Pick<WorkspaceController, "preference">;
  state: Pick<WorkspaceState, "skin" | "typography">;
}) {
  return (
    <SettingsPane title="모양" desc="앱이 어떻게 보이는지 정해요. 밝게·어둡게는 레일의 ◐ 단추가 그대로 맡아요.">
      <SkinPicker
        skin={state.skin}
        onChange={(skin) => controller.preference({ skin })}
      />

      <div className="set-group" data-setting="scale">
        <h4>화면 크기</h4>
        <div className="set-card set-card-padded set-typo">
          <div className="typo-row">
            <small className="set-scale-hint">
              글자와 단추를 앱 전체에서 함께 키우거나 줄여요. ⌘ + / ⌘ - / ⌘ 0 키로도 바꿀 수 있어요. 지금은 {state.typography.scale}%예요.
            </small>
            <div className="typo-scale" role="group" aria-label="화면 크기">
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
        </div>
      </div>
    </SettingsPane>
  );
}
