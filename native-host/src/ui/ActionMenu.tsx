import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { RefObject } from "react";
import { createPortal } from "react-dom";
import { Icon } from "./Icon.tsx";
import type { IconName } from "./Icon.tsx";

export type ActionMenuItem =
  | "separator"
  | {
      key: string;
      label: string;
      icon: IconName;
      /** 이름 오른쪽에 흐리게 붙는 짧은 값(예: 복사할 ID의 앞부분). */
      hint?: string;
      /** 지우기처럼 되돌리기 어려운 동작은 빨간색으로 그립니다. */
      danger?: boolean;
      /** 항목에 덧붙이는 클래스. 시험과 스타일이 이 이름으로 항목을 찾습니다. */
      className?: string;
      onSelect(): void;
    };

/**
 * 노트 정보줄의 "더 보기(⋯)" 같은 단추가 펼치는 작은 메뉴입니다.
 * 자주 쓰지 않는 동작을 줄 위에서 치우되, 지우기는 구분선 아래 빨간색으로 떼어 놓으려고 만들었습니다.
 * [anchor]는 메뉴를 연 단추이고, 메뉴는 그 단추의 오른쪽 끝에 맞춰 바로 아래에 뜹니다.
 * 정보줄 안의 단추 스타일이 메뉴 항목으로 번지지 않도록 문서 맨 바깥(body)에 그리며,
 * 그래서 화면이 스크롤되거나 창 크기가 바뀌면 위치가 어긋나지 않게 메뉴를 닫습니다.
 * 바깥을 누르거나 Esc를 누르면 [onClose]를 부르고, 위아래 화살표로 항목을 옮깁니다.
 * [anchor]를 다시 누르는 것은 바깥 누르기로 치지 않아서, 그 단추가 스스로 여닫게 둡니다.
 */
export function ActionMenu({
  label,
  items,
  anchor,
  onClose,
}: {
  label: string;
  items: ActionMenuItem[];
  anchor: RefObject<HTMLElement | null>;
  onClose(): void;
}) {
  const box = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState<{ top: number; right: number } | null>(
    null,
  );

  useLayoutEffect(() => {
    const rect = anchor.current?.getBoundingClientRect();
    if (rect)
      setPlace({
        top: rect.bottom + 6,
        right: Math.max(8, window.innerWidth - rect.right),
      });
  }, [anchor]);

  useEffect(() => {
    if (!place) return;
    box.current?.querySelector<HTMLButtonElement>("button")?.focus();
    const outside = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!box.current?.contains(target) && !anchor.current?.contains(target))
        onClose();
    };
    const keys = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
        anchor.current?.focus();
        return;
      }
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      const buttons = [
        ...(box.current?.querySelectorAll<HTMLButtonElement>("button") ?? []),
      ];
      if (!buttons.length) return;
      e.preventDefault();
      const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const next =
        e.key === "ArrowDown"
          ? (at + 1) % buttons.length
          : (at - 1 + buttons.length) % buttons.length;
      buttons[next].focus();
    };
    document.addEventListener("pointerdown", outside);
    window.addEventListener("keydown", keys, true);
    window.addEventListener("resize", onClose);
    window.addEventListener("scroll", onClose, true);
    return () => {
      document.removeEventListener("pointerdown", outside);
      window.removeEventListener("keydown", keys, true);
      window.removeEventListener("resize", onClose);
      window.removeEventListener("scroll", onClose, true);
    };
  }, [anchor, onClose, place]);

  return createPortal(
    <div
      className="act-menu"
      role="menu"
      aria-label={label}
      ref={box}
      style={
        place
          ? { top: place.top, right: place.right }
          : { visibility: "hidden" }
      }
    >
      {items.map((item, i) =>
        item === "separator" ? (
          <hr key={"s" + i} role="separator" />
        ) : (
          <button
            key={item.key}
            type="button"
            role="menuitem"
            className={`act-item${item.danger ? " is-danger" : ""}${item.className ? " " + item.className : ""}`}
            onClick={() => {
              onClose();
              item.onSelect();
            }}
          >
            <Icon name={item.icon} />
            <span>{item.label}</span>
            {item.hint && <small>{item.hint}</small>}
          </button>
        ),
      )}
    </div>,
    document.body,
  );
}
