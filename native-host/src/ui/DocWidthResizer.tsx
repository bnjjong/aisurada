import type { KeyboardEvent, PointerEvent } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";

export const DOC_WIDTH_DEFAULT = 820;
export const DOC_WIDTH_MIN = 640;
export const DOC_WIDTH_MAX = 1600;
const KEY_STEP = 16;

export function clampDocWidth(value: unknown): number {
  const n = typeof value === "number" ? value : Number(value);
  if (!Number.isFinite(n)) return DOC_WIDTH_DEFAULT;
  return Math.min(DOC_WIDTH_MAX, Math.max(DOC_WIDTH_MIN, Math.round(n)));
}

/**
 * 부록 F: 노트 본문 시트의 폭을 끌어서 조절하는 손잡이 (DocWidthResizer).
 *
 * - 시트 오른쪽 끝에서 8px~24px 바깥의 16px 띠로 위치합니다.
 * - 시트가 가운데에 정렬되어 있으므로 마우스 dx 이동 시 2*dx 계수로 양쪽이 균등하게 벌어집니다.
 * - 640px부터 1600px 사이로 조절되며 기본값은 820px입니다.
 * - 더블클릭하면 기본 폭(820px)으로 돌아갑니다.
 */
export function DocWidthResizer({
  controller,
  width,
}: {
  controller?: WorkspaceController;
  width: number;
}) {
  const apply = (handle: HTMLElement, value: number) => {
    const sheet = handle.closest<HTMLElement>(".note-reading");
    if (sheet) {
      sheet.style.setProperty("--doc-width", `${value}px`);
    }
  };

  const pointerDown = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    const handle = event.currentTarget;
    const startX = event.clientX;
    const startWidth = clampDocWidth(width);
    let current = startWidth;

    document.body.classList.add("col-resizing");

    const move = (e: globalThis.PointerEvent) => {
      // 가운데 정렬된 시트이므로 오른쪽 끝을 dx 옮기면 전체 폭은 2*dx 변해야 1:1로 따라옵니다.
      const dx = e.clientX - startX;
      current = clampDocWidth(startWidth + 2 * dx);
      apply(handle, current);
    };

    const end = () => {
      document.removeEventListener("pointermove", move);
      document.removeEventListener("pointercancel", end);
      document.body.classList.remove("col-resizing");
      if (current !== width && controller) {
        controller.preference({ docWidth: current });
      }
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
            ? DOC_WIDTH_MIN
            : event.key === "End"
              ? DOC_WIDTH_MAX
              : null;
    if (next === null) return;
    event.preventDefault();
    event.stopPropagation();
    const value = clampDocWidth(next);
    if (value !== width && controller) {
      controller.preference({ docWidth: value });
    }
  };

  return (
    <div
      className="doc-width-resize"
      role="separator"
      aria-orientation="vertical"
      aria-label="본문 너비 조절"
      aria-description="드래그해서 너비 조절 · 더블클릭 시 기본값"
      title="드래그해서 본문 너비 조절 (더블클릭: 기본값 820px)"
      aria-valuemin={DOC_WIDTH_MIN}
      aria-valuemax={DOC_WIDTH_MAX}
      aria-valuenow={width}
      tabIndex={0}
      onPointerDown={pointerDown}
      onKeyDown={keyDown}
      onDoubleClick={() => {
        if (width !== DOC_WIDTH_DEFAULT && controller) {
          controller.preference({ docWidth: DOC_WIDTH_DEFAULT });
        }
      }}
    >
      <span className="doc-width-resize-line" aria-hidden="true" />
      <span className="doc-width-resize-pill" aria-hidden="true">
        <span className="doc-width-resize-dots" />
      </span>
    </div>
  );
}
