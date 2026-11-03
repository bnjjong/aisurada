import { useEffect, useRef } from "react";
import type { ReactNode } from "react";
import { createPortal } from "react-dom";
import { addLayer, topLayer } from "./layers.ts";
export function Modal({
  title,
  close,
  children,
  className = "",
  wide = false,
}: {
  title: string;
  close(): void;
  children: ReactNode;
  className?: string;
  wide?: boolean;
}) {
  const token = useRef(Symbol()),
    callbacks = useRef(close);
  callbacks.current = close;
  const box = useRef<HTMLElement>(null);
  useEffect(() => {
    const remove = addLayer(token.current),
      previous = document.activeElement as HTMLElement | null;
    box.current?.focus({ preventScroll: true });
    const key = (e: KeyboardEvent) => {
      if (
        e.key === "Escape" &&
        !e.defaultPrevented &&
        topLayer() === token.current
      ) {
        e.preventDefault();
        e.stopPropagation();
        callbacks.current();
      }
    };
    window.addEventListener("keydown", key);
    return () => {
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
        className={`ui-modal docmodal ${className}`}
      >
        <header>
          <h2>{title}</h2>
          <button
            aria-label={`${title} 닫기`}
            className={className.includes("nhist") ? "nhist-close" : undefined}
            onClick={close}
          >
            {className.includes("nhist") ? "이력 닫기" : "닫기"}
          </button>
        </header>
        {children}
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
