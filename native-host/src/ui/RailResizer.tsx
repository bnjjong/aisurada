import type { KeyboardEvent, PointerEvent } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import {
  clampRailWidth,
  RAIL_WIDTH_DEFAULT,
  RAIL_WIDTH_MAX,
  RAIL_WIDTH_MIN,
} from "./model.ts";

/** 키보드로 한 번에 움직이는 폭. Shift를 함께 누르면 세 배다. */
const KEY_STEP = 16;

/**
 * 좌측 메뉴(레일)의 폭을 끌어서 조절하는 손잡이.
 *
 * 폭은 `.workspace-screen`의 `--rail-width`로 건다. 끄는 동안에는 그 값만 바꿔서 화면을
 * 다시 그리지 않고, 손을 떼면 [controller]로 설정 `railWidth`에 한 번만 저장한다.
 * 더블클릭하면 기본 폭으로 돌아가고, 포커스를 준 뒤 ←/→로도 조절할 수 있다.
 */
export function RailResizer({
  controller,
  width,
}: {
  controller: WorkspaceController;
  width: number;
}) {
  const apply = (handle: HTMLElement, value: number) =>
    handle
      .closest<HTMLElement>(".workspace-screen")
      ?.style.setProperty("--rail-width", `${value}px`);

  const pointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const handle = event.currentTarget,
      startX = event.clientX;
    let current = width;
    document.body.classList.add("is-resizing");
    const move = (e: globalThis.PointerEvent) => {
      current = clampRailWidth(width + e.clientX - startX);
      apply(handle, current);
    };
    const end = () => {
      document.removeEventListener("pointermove", move);
      document.removeEventListener("pointercancel", end);
      document.body.classList.remove("is-resizing");
      if (current !== width) controller.preference({ railWidth: current });
    };
    document.addEventListener("pointermove", move);
    document.addEventListener("pointerup", end, { once: true });
    document.addEventListener("pointercancel", end, { once: true });
  };

  const keyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? KEY_STEP * 3 : KEY_STEP;
    const next =
      event.key === "ArrowLeft"
        ? width - step
        : event.key === "ArrowRight"
          ? width + step
          : event.key === "Home"
            ? RAIL_WIDTH_MIN
            : event.key === "End"
              ? RAIL_WIDTH_MAX
              : null;
    if (next === null) return;
    // 캘린더의 날짜 이동 같은 창 단위 단축키로 번지지 않게 여기서 멈춘다.
    event.preventDefault();
    event.stopPropagation();
    const value = clampRailWidth(next);
    if (value !== width) controller.preference({ railWidth: value });
  };

  return (
    <div
      className="rail-resize"
      role="separator"
      aria-orientation="vertical"
      aria-label="메뉴 폭 조절"
      aria-valuemin={RAIL_WIDTH_MIN}
      aria-valuemax={RAIL_WIDTH_MAX}
      aria-valuenow={width}
      tabIndex={0}
      title="끌어서 폭 조절 · 더블클릭하면 기본 폭"
      onPointerDown={pointerDown}
      onKeyDown={keyDown}
      onDoubleClick={() => {
        if (width !== RAIL_WIDTH_DEFAULT)
          controller.preference({ railWidth: RAIL_WIDTH_DEFAULT });
      }}
    />
  );
}
