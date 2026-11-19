import { useEffect, useRef, useState } from "react";
import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import { Icon } from "./Icon.tsx";
import { addLayer, topLayer } from "./layers.ts";
export function Modal({
  title,
  close,
  children,
  className = "",
  wide = false,
  headerActions,
  resizable = false,
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
  const [size, setSize] = useState<{ width: number; height?: number } | null>(() => {
    if (!resizable) return null;
    if (storageKey) {
      try {
        const saved = localStorage.getItem(storageKey);
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
      maxW = Math.max(500, window.innerWidth - 32),
      maxH = Math.max(300, window.innerHeight - 32);

    grip.setPointerCapture(pointer);
    const move = (ev: PointerEvent) => {
      area.style.width = clamp(start.width + 2 * (ev.clientX - x0), 480, maxW) + "px";
      area.style.height = clamp(start.height + 2 * (ev.clientY - y0), 300, maxH) + "px";
    };
    const end = () => {
      grip.removeEventListener("pointermove", move);
      grip.removeEventListener("pointerup", end);
      grip.removeEventListener("pointercancel", end);
      const r = area.getBoundingClientRect();
      const nextSize = {
        width: clamp(r.width, 480, maxW),
        height: clamp(r.height, 300, maxH),
      };
      setSize(nextSize);
      if (storageKey) {
        try {
          localStorage.setItem(storageKey, JSON.stringify(nextSize));
        } catch {}
      }
    };
    grip.addEventListener("pointermove", move);
    grip.addEventListener("pointerup", end);
    grip.addEventListener("pointercancel", end);
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
          {headerActions ? (
            <div className="ui-modal-controls">{headerActions}</div>
          ) : (
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
              <button
                type="button"
                className={`modal-ctrl-btn modal-ctrl-close ${className.includes("nhist") ? "nhist-close" : ""}`.trim()}
                title="닫기"
                aria-label={`${title} 닫기`}
                onClick={close}
              >
                <Icon name="close" />
              </button>
            </div>
          )}
        </header>
        {/* 크기를 조절하는 모달은 스크롤 영역(ui-modal-body)과 손잡이 띠(ui-modal-gripbar)를 나눠서, 손잡이가 내용 위에 겹치거나 스크롤을 따라 올라오지 않게 한다 */}
        {resizable ? <div className="ui-modal-body">{children}</div> : children}
        {resizable && !full && (
          <div className="ui-modal-gripbar">
            <span
              className="preview-grip"
              title="끌어서 크기 조절"
              aria-hidden="true"
              onPointerDown={startResize}
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
    <Modal title={title} close={close}>
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
