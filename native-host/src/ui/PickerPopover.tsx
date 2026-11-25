import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { CSSProperties, ReactNode, RefObject } from "react";
import { createPortal } from "react-dom";
import { Icon } from "./Icon.tsx";
import { usePopoverDismiss } from "./usePopoverDismiss.ts";

export function PickerPopover({
  anchor,
  label,
  id,
  onClose,
  children,
}: {
  anchor: RefObject<HTMLElement | null>;
  label: string;
  id?: string;
  onClose(): void;
  children: ReactNode;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState<CSSProperties>();
  // 미리보기 안에서 메뉴에 초점을 옮겨도 Tab 탐색이 대화 상자 안에 머물게 한다.
  const dialog = anchor.current?.closest<HTMLElement>(".note-preview[role='dialog']");
  useLayoutEffect(() => {
    const rect = anchor.current?.getBoundingClientRect();
    if (!rect) return;
    const bounds = dialog?.getBoundingClientRect();
    const left = bounds && dialog ? bounds.left + dialog.clientLeft : 0;
    const top = bounds && dialog ? bounds.top + dialog.clientTop : 0;
    const availableWidth = dialog?.clientWidth ?? window.innerWidth;
    const availableHeight = dialog?.clientHeight ?? window.innerHeight;
    const margin = 8;
    const gap = 6;
    const width = Math.min(280, availableWidth - margin * 2);
    const below = top + availableHeight - rect.bottom - gap - margin;
    const above = rect.top - top - gap - margin;
    const opensAbove = below < 220 && above > below;
    setPlace({
      position: dialog ? "absolute" : "fixed",
      left: Math.max(margin, Math.min(rect.left - left, availableWidth - width - margin)),
      width,
      maxHeight: Math.max(0, opensAbove ? above : below),
      ...(opensAbove
        ? { bottom: top + availableHeight - rect.top + gap }
        : { top: rect.bottom - top + gap }),
    });
  }, [anchor, dialog]);

  usePopoverDismiss({
    ref: box,
    anchorRef: anchor,
    onClose,
    enabled: !!place,
    restoreFocus: true,
    closeOnScroll: true,
    closeOnResize: true,
  });

  useEffect(() => {
    if (!place) return;
    const panel = box.current;
    (panel?.querySelector<HTMLElement>("[data-picker-search]") ?? panel)?.focus({ preventScroll: true });

    const keys = (event: KeyboardEvent) => {
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      const buttons = [...(panel?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
      if (!buttons.length) return;
      event.preventDefault();
      const current = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const next = event.key === "ArrowDown"
        ? (current + 1) % buttons.length
        : current <= 0 ? buttons.length - 1 : current - 1;
      buttons[next].focus();
    };

    window.addEventListener("keydown", keys, true);
    return () => {
      window.removeEventListener("keydown", keys, true);
    };
  }, [place]);

  return createPortal(
    <div className="agenda-picker act-menu" role="dialog" aria-label={label}
      id={id} ref={box} tabIndex={-1} style={place ?? { visibility: "hidden" }}>
      <div className="agenda-picker-title">{label}</div>
      {children}
    </div>,
    dialog ?? document.body,
  );
}

export function PickerSearch({
  label,
  value,
  onChange,
  onSubmit,
}: {
  label: string;
  value: string;
  onChange(value: string): void;
  onSubmit(): void;
}) {
  return (
    <label className="agenda-picker-search">
      <Icon name="search" size={12} />
      <input
        type="text"
        value={value}
        placeholder={label}
        aria-label={label}
        data-picker-search
        onChange={(event) => onChange(event.target.value)}
        onKeyDown={(event) => {
          if (event.key === "Enter" && !event.nativeEvent.isComposing) {
            event.preventDefault();
            onSubmit();
          }
        }}
      />
    </label>
  );
}
