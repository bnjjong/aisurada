import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import { Icon } from "./Icon.tsx";
import { addLayer, topLayer } from "./layers.ts";

/**
 * 모달 크기를 기억해 두는 localStorage 열쇠. 같은 종류의 창은 같은 크기로 다시 열리도록 클래스 이름(없으면 제목)으로 짓는다.
 * 제목에는 태스크 키 같은 바뀌는 글이 들 수 있어서 클래스 이름을 먼저 쓴다.
 */
export function modalSizeKey(className: string, title: string): string {
  const kind = className.split(/\s+/).filter(Boolean).sort().join(".");
  return `aisurada.modalSize.${kind || title}`;
}

/** 손잡이로 줄일 수 있는 가장 작은 크기. 처음 크기가 이보다 작은 창은 처음 크기까지만 줄인다. */
const MIN_WIDTH = 480;
const MIN_HEIGHT = 300;

/**
 * 공용 모달. 기본으로 크기를 바꿀 수 있다: 오른쪽 아래 손잡이를 끌어 늘이고 줄이며(두 번 누르면 처음 크기), 제목 줄의 단추로 전체 화면을 켠다.
 * 바꾼 크기는 [modalSizeKey]로 기억해 같은 종류의 창을 다시 열 때 쓴다. 확인·경고처럼 작은 창은 `resizable={false}`로 끈다.
 */
export function Modal({
  title,
  close,
  children,
  className = "",
  wide = false,
  headerActions,
  resizable = true,
  defaultWidth,
  defaultHeight,
  storageKey,
}: {
  title: string;
  close(): void;
  children: ReactNode;
  className?: string;
  wide?: boolean;
  headerActions?: ReactNode;
  resizable?: boolean;
  defaultWidth?: number;
  defaultHeight?: number;
  storageKey?: string;
}) {
  const token = useRef(Symbol()),
    callbacks = useRef(close);
  callbacks.current = close;
  const box = useRef<HTMLElement>(null);
  const [full, setFull] = useState(false);
  const sizeKey = resizable ? (storageKey ?? modalSizeKey(className, title)) : undefined;
  const [size, setSize] = useState<{ width: number; height?: number } | null>(() => {
    if (!resizable) return null;
    if (sizeKey) {
      try {
        const saved = localStorage.getItem(sizeKey);
        if (saved) {
          const parsed = JSON.parse(saved);
          if (typeof parsed?.width === "number") {
            return {
              width: parsed.width,
              height: typeof parsed.height === "number" ? parsed.height : undefined,
            };
          }
        }
      } catch {}
    }
    if (defaultWidth) {
      return { width: defaultWidth, height: defaultHeight };
    }
    return null;
  });

  const startResize = (e: React.PointerEvent<HTMLElement>) => {
    const area = box.current;
    if (!resizable || full || !area) return;
    e.preventDefault();
    const grip = e.currentTarget,
      pointer = e.pointerId,
      x0 = e.clientX,
      y0 = e.clientY,
      start = area.getBoundingClientRect(),
      clamp = (value: number, min: number, max: number) =>
        Math.max(min, Math.min(max, Math.round(value))),
      minW = Math.min(MIN_WIDTH, Math.round(start.width)),
      minH = Math.min(MIN_HEIGHT, Math.round(start.height)),
      maxW = Math.max(minW, window.innerWidth - 32),
      maxH = Math.max(minH, window.innerHeight - 32);

    grip.setPointerCapture(pointer);
    let moved = false;
    const move = (ev: PointerEvent) => {
      moved = true;
      area.style.width = clamp(start.width + 2 * (ev.clientX - x0), minW, maxW) + "px";
      area.style.height = clamp(start.height + 2 * (ev.clientY - y0), minH, maxH) + "px";
    };
    const end = () => {
      grip.removeEventListener("pointermove", move);
      grip.removeEventListener("pointerup", end);
      grip.removeEventListener("pointercancel", end);
      // 누르기만 하고 끌지 않았으면(두 번 누르기의 첫 누름 따위) 지금 크기를 기억하지 않는다
      if (!moved) return;
      const r = area.getBoundingClientRect();
      const nextSize = {
        width: clamp(r.width, minW, maxW),
        height: clamp(r.height, minH, maxH),
      };
      setSize(nextSize);
      if (sizeKey) {
        try {
          localStorage.setItem(sizeKey, JSON.stringify(nextSize));
        } catch {}
      }
    };
    grip.addEventListener("pointermove", move);
    grip.addEventListener("pointerup", end);
    grip.addEventListener("pointercancel", end);
  };
  // 손잡이를 두 번 누르면 기억한 크기를 버리고 처음 크기로 돌아간다
  const resetSize = () => {
    // 끄는 동안 직접 바꾼 인라인 크기를 처음 크기로 맞춘다. React는 기억한 크기와 처음 크기가 같으면 스타일을 다시 쓰지 않는다
    const area = box.current;
    if (area) {
      area.style.width = defaultWidth ? `${defaultWidth}px` : "";
      area.style.height = defaultWidth && defaultHeight ? `${defaultHeight}px` : "";
    }
    if (sizeKey) {
      try {
        localStorage.removeItem(sizeKey);
      } catch {}
    }
    setSize(defaultWidth ? { width: defaultWidth, height: defaultHeight } : null);
  };
  useEffect(() => {
    const remove = addLayer(token.current),
      previous = document.activeElement as HTMLElement | null;
    const focusFirst = () => {
      if (!box.current) return;
      const focusables = box.current.querySelectorAll<HTMLElement>(
        'input:not([disabled]):not([type="hidden"]), textarea:not([disabled]), select:not([disabled]), button:not([disabled]), [tabindex]:not([tabindex="-1"])',
      );
      if (focusables.length > 0) {
        focusables[0].focus({ preventScroll: true });
      } else {
        box.current.focus({ preventScroll: true });
      }
    };
    const rId = requestAnimationFrame(focusFirst);
    const key = (e: KeyboardEvent) => {
      if (topLayer() !== token.current) return;
      if (e.key === "Escape" && !e.defaultPrevented) {
        e.preventDefault();
        e.stopPropagation();
        callbacks.current();
        return;
      }
      if (e.key === "Tab" && box.current) {
        const focusables = Array.from(
          box.current.querySelectorAll<HTMLElement>(
            'input:not([disabled]):not([type="hidden"]), textarea:not([disabled]), select:not([disabled]), button:not([disabled]), [tabindex]:not([tabindex="-1"])',
          ),
        ).filter(
          (el) => el.offsetParent !== null || el === document.activeElement,
        );
        if (!focusables.length) {
          e.preventDefault();
          return;
        }
        const first = focusables[0];
        const last = focusables[focusables.length - 1];
        if (e.shiftKey) {
          if (
            document.activeElement === first ||
            !box.current.contains(document.activeElement)
          ) {
            e.preventDefault();
            last.focus();
          }
        } else {
          if (
            document.activeElement === last ||
            !box.current.contains(document.activeElement)
          ) {
            e.preventDefault();
            first.focus();
          }
        }
      }
    };
    window.addEventListener("keydown", key);
    return () => {
      cancelAnimationFrame(rId);
      remove();
      window.removeEventListener("keydown", key);
      if (previous?.isConnected) previous.focus({ preventScroll: true });
    };
  }, []);
  return createPortal(
    <div
      className={`ui-modal-backdrop ${className.includes("note-preview") ? "note-preview-overlay" : ""} ${wide ? "ui-modal-wide" : ""}`}
      onPointerDown={(e) => {
        if (e.target === e.currentTarget && topLayer() === token.current)
          close();
      }}
    >
      <section
        ref={box}
        tabIndex={-1}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className={`ui-modal docmodal ${className} ${resizable ? "ui-modal-resizable" : ""} ${full ? "ui-modal-full" : ""}`.trim()}
        style={
          full
            ? {
                width: "calc(100vw - 32px)",
                height: "calc(100vh - 32px)",
                maxWidth: "none",
                maxHeight: "none",
              }
            : size
            ? {
                width: `${size.width}px`,
                ...(size.height ? { height: `${size.height}px` } : {}),
                maxWidth: "calc(100vw - 32px)",
                maxHeight: "calc(100vh - 32px)",
              }
            : undefined
        }
      >
        <header>
          <h2>{title}</h2>
          <div className="ui-modal-controls">
            {resizable && (
              <button
                type="button"
                className="modal-ctrl-btn"
                title={full ? "크기 되돌리기" : "전체 화면"}
                aria-label={full ? "크기 되돌리기" : "전체 화면"}
                onClick={() => setFull((f) => !f)}
              >
                <Icon name={full ? "restore" : "maximize"} />
              </button>
            )}
            {headerActions ?? (
              <button
                type="button"
                className={`modal-ctrl-btn modal-ctrl-close ${className.includes("nhist") ? "nhist-close" : ""}`.trim()}
                title="닫기"
                aria-label={`${title} 닫기`}
                onClick={close}
              >
                <Icon name="close" />
              </button>
            )}
          </div>
        </header>
        {/* 크기를 조절하는 모달은 스크롤 영역(ui-modal-body)과 손잡이 띠(ui-modal-gripbar)를 나눠서, 손잡이가 내용 위에 겹치거나 스크롤을 따라 올라오지 않게 한다 */}
        {resizable ? <div className="ui-modal-body">{children}</div> : children}
        {resizable && !full && (
          <div className="ui-modal-gripbar">
            <span
              className="preview-grip"
              title="끌어서 크기 조절 · 두 번 누르면 처음 크기"
              aria-hidden="true"
              onPointerDown={startResize}
              onDoubleClick={resetSize}
            >
              <svg viewBox="0 0 12 12" width="12" height="12" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round">
                <path d="M10.5 3.5L3.5 10.5M10.5 7.5L7.5 10.5" />
              </svg>
            </span>
          </div>
        )}
      </section>
    </div>,
    document.body,
  );
}
export function Confirm({
  title,
  description,
  accept,
  close,
  label = "삭제",
}: {
  title: string;
  description?: string;
  accept(): void | Promise<void>;
  close(): void;
  label?: string;
}) {
  return (
    <Modal title={title} close={close} resizable={false}>
      <p>{description}</p>
      <footer>
        <button onClick={close}>취소</button>
        <button
          className="button-primary"
          onClick={() => {
            close();
            void accept();
          }}
        >
          {label}
        </button>
      </footer>
    </Modal>
  );
}
