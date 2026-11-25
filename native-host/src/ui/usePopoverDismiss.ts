import { useEffect, type RefObject } from "react";

export interface PopoverDismissOptions {
  /**
   * 팝오버 컨테이너 ref(단일 또는 배열). 이 안을 누른 것은 바깥 누르기로 치지 않는다.
   */
  ref: RefObject<HTMLElement | null> | Array<RefObject<HTMLElement | null>>;
  /**
   * 팝오버를 연 앵커 단추 ref. 단추를 누르는 것은 바깥 누르기로 치지 않는다(단추가 스스로 여닫는다).
   */
  anchorRef?: RefObject<HTMLElement | null>;
  /**
   * 팝오버를 닫을 때 부르는 함수.
   */
  onClose: () => void;
  /**
   * 훅을 켤지 여부 (기본값: true).
   */
  enabled?: boolean;
  /**
   * Esc 키를 누르면 닫을지 여부 (기본값: true).
   */
  closeOnEscape?: boolean;
  /**
   * Esc 키 입력 시 이벤트 기본 동작을 막고 전파를 멈출지 여부 (기본값: true).
   */
  captureEscape?: boolean;
  /**
   * 닫힐 때 앵커 단추로 포커스를 돌려줄지 여부 (기본값: false).
   */
  restoreFocus?: boolean;
  /**
   * 창 크기가 바뀔 때 닫을지 여부 (기본값: false).
   */
  closeOnResize?: boolean;
  /**
   * 창이나 다른 영역이 스크롤될 때 닫을지 여부 (기본값: false).
   * 팝오버 컨테이너 내부의 스크롤은 무시한다.
   */
  closeOnScroll?: boolean;
  /**
   * 눌린 대상이 닫기를 무시해야 하는지 판별하는 함수.
   */
  shouldIgnoreTarget?: (target: Node) => boolean;
}

/**
 * 팝오버 닫기 이벤트 리스너를 등록하고 정리 함수(cleanup)를 반환하는 함수.
 */
export function setupPopoverDismiss({
  ref,
  anchorRef,
  onClose,
  enabled = true,
  closeOnEscape = true,
  captureEscape = true,
  restoreFocus = false,
  closeOnResize = false,
  closeOnScroll = false,
  shouldIgnoreTarget,
}: PopoverDismissOptions): () => void {
  if (!enabled) return () => {};

  const refs = Array.isArray(ref) ? ref : [ref];

  const outside = (e: MouseEvent | PointerEvent) => {
    const target = e.target as Node | null;
    if (!target) return;
    if (shouldIgnoreTarget?.(target)) return;
    if (refs.some((r) => r.current?.contains(target))) return;
    if (anchorRef?.current?.contains(target)) return;
    onClose();
  };

  const onKeyDown = (e: KeyboardEvent) => {
    if (!closeOnEscape || e.key !== "Escape") return;
    if (captureEscape) {
      e.preventDefault();
      e.stopPropagation();
      (e as unknown as { stopImmediatePropagation?: () => void }).stopImmediatePropagation?.();
    }
    onClose();
    if (restoreFocus && anchorRef?.current) {
      anchorRef.current.focus({ preventScroll: true });
    }
  };

  const onResize = () => {
    if (closeOnResize) onClose();
  };

  const onScroll = (e: Event) => {
    if (!closeOnScroll) return;
    const target = e.target as Node | null;
    if (target && refs.some((r) => r.current?.contains(target))) return;
    onClose();
  };

  const pointerEventName =
    typeof window !== "undefined" && "PointerEvent" in window
      ? "pointerdown"
      : "mousedown";

  document.addEventListener(pointerEventName, outside);
  if (closeOnEscape) {
    window.addEventListener("keydown", onKeyDown, true);
  }
  if (closeOnResize) {
    window.addEventListener("resize", onResize);
  }
  if (closeOnScroll) {
    window.addEventListener("scroll", onScroll, true);
  }

  return () => {
    document.removeEventListener(pointerEventName, outside);
    if (closeOnEscape) {
      window.removeEventListener("keydown", onKeyDown, true);
    }
    if (closeOnResize) {
      window.removeEventListener("resize", onResize);
    }
    if (closeOnScroll) {
      window.removeEventListener("scroll", onScroll, true);
    }
  };
}

/**
 * 팝오버, 드롭다운 메뉴, 선택 상자가 바깥 누르기나 Esc 키, 스크롤, 리사이즈로 닫히게 하는 공통 훅.
 */
export function usePopoverDismiss(options: PopoverDismissOptions) {
  useEffect(
    () => setupPopoverDismiss(options),
    [
      options.ref,
      options.anchorRef,
      options.onClose,
      options.enabled,
      options.closeOnEscape,
      options.captureEscape,
      options.restoreFocus,
      options.closeOnResize,
      options.closeOnScroll,
      options.shouldIgnoreTarget,
    ],
  );
}
