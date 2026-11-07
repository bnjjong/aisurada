import { useRef, useState } from "react";
import type { WorkspaceState } from "../workspace/types.ts";
import type { WorkspaceController } from "../workspace/controller.ts";
import { Modal } from "./Modal.tsx";
import { NoteReading } from "../navigation/NoteReading.tsx";
import { displayTitle } from "./model.ts";
import { docAddress } from "../navigation/routes.ts";

export function Preview({
  state,
  controller,
}: {
  state: WorkspaceState;
  controller: WorkspaceController;
}) {
  const note = state.workspace?.notes.find((n) => n.id === state.preview),
    [full, setFull] = useState(false),
    [moveError, setMoveError] = useState(""),
    box = useRef<HTMLDivElement>(null),
    resizing = useRef(false);
  if (!note) return null;
  const captured =
      state.previewOpened?.id === note.id ? state.previewOpened : note,
    agenda = state.workspace?.agendas.find((a) => a.id === captured.agendaId),
    pw = Number(state.prefs.previewWidth) || 720,
    ph = typeof state.prefs.previewHeight === "number" ? state.prefs.previewHeight : null;
  const title = displayTitle(note, state.prefs.titleDatePrefix ?? (state.prefs as any).titleDate ?? true);

  const headerActions = (
    <>
      <button
        type="button"
        className="modal-ctrl-btn"
        title="탭으로 띄우기"
        aria-label="탭으로 띄우기"
        onClick={() => {
          controller.showPreview(null);
          const path = docAddress(note.id);
          if (controller.context.kind === "note")
            void controller.callHost("show_in_main", {
              path,
              rail: null,
              sub: null,
            });
          else controller.navigate(path);
        }}
      >
        <svg
          viewBox="0 0 16 16"
          width="14"
          height="14"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.4"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <rect x="2" y="3" width="12" height="10" rx="1.5" />
          <path d="M2 6.5h12" />
          <path d="M6 3v3.5" />
        </svg>
      </button>
      {controller.isNative() && controller.context.kind === "main" && (
        <button
          type="button"
          className="modal-ctrl-btn pane-pop"
          title="새 창으로"
          aria-label="새 창으로"
          onClick={() => {
            void controller.moveNoteWindow(note.id).then((result) => {
              if (result !== "opened") {
                setMoveError(
                  result === "unsaved"
                    ? "저장하지 못한 글이 있어서 새 창으로 옮기지 않았어요. 충돌 배너를 먼저 풀어 주세요."
                    : "새 창을 열지 못했어요. 앱을 다시 켜고 해 보세요.",
                );
              }
            });
          }}
        >
          <svg
            viewBox="0 0 16 16"
            width="14"
            height="14"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.4"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <path d="M7 3H3.5A1.5 1.5 0 0 0 2 4.5v8A1.5 1.5 0 0 0 3.5 14h8a1.5 1.5 0 0 0 1.5-1.5V9" />
            <path d="M10 2h4v4" />
            <path d="M7.5 8.5L14 2" />
          </svg>
        </button>
      )}
      <button
        type="button"
        className="modal-ctrl-btn"
        title={full ? "크기 되돌리기" : "전체 화면"}
        aria-label={full ? "크기 되돌리기" : "전체 화면"}
        onClick={() => setFull(!full)}
      >
        {full ? (
          <svg
            viewBox="0 0 16 16"
            width="14"
            height="14"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.4"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <path d="M5.5 3.5V2.5a1 1 0 0 1 1-1h7a1 1 0 0 1 1 1v7a1 1 0 0 1-1 1h-1" />
            <rect x="2.5" y="5.5" width="8" height="8" rx="1.2" />
          </svg>
        ) : (
          <svg
            viewBox="0 0 16 16"
            width="14"
            height="14"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.4"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <rect x="2.5" y="2.5" width="11" height="11" rx="1.5" />
          </svg>
        )}
      </button>
      <button
        type="button"
        className="modal-ctrl-btn modal-ctrl-close"
        title="닫기"
        aria-label={`${title} 닫기`}
        onClick={() => controller.showPreview(null)}
      >
        <svg
          viewBox="0 0 16 16"
          width="14"
          height="14"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="M4 4l8 8M12 4l-8 8" />
        </svg>
      </button>
    </>
  );

  return (
    <Modal
      title={title}
      close={() => controller.showPreview(null)}
      className={`note-preview docmodal ${full ? "preview-full" : ""}`}
      wide={full}
      headerActions={headerActions}
    >
      <div
        ref={box}
        className="preview-resize"
        style={
          !full ? ({ width: pw, ...(ph ? { height: ph } : {}) } as React.CSSProperties) : undefined
        }
        onPointerDown={(e) => {
          const r = e.currentTarget.getBoundingClientRect();
          resizing.current =
            !full && e.clientX > r.right - 20 && e.clientY > r.bottom - 20;
        }}
        onPointerUp={() => {
          if (!resizing.current) return;
          resizing.current = false;
          const r = box.current?.getBoundingClientRect();
          if (r) {
            const width = Math.max(480, Math.min(1600, Math.round(r.width)));
            const height = Math.max(320, Math.min(2400, Math.round(r.height)));
            if (width !== pw || height !== (ph || 480)) {
              controller.preference({
                previewWidth: width,
                previewHeight: height,
              });
            }
          }
        }}
      >
        {moveError && <p role="status" className="read-failure">{moveError}</p>}
        <NoteReading note={captured} controller={controller} mode="preview" />
      </div>
      <footer>
        <button
          disabled={!agenda}
          onClick={() => {
            controller.showPreview(null);
            const path =
              "/agenda/" +
              encodeURIComponent(agenda!.id) +
              "?open=" +
              encodeURIComponent(note.id);
            if (controller.context.kind === "note")
              void controller.callHost("show_in_main", {
                path,
                rail: null,
                sub: null,
              });
            else controller.navigate(path);
          }}
        >
          아젠다에서 열기
        </button>
        <button onClick={() => controller.showPreview(null)}>닫기</button>
      </footer>
    </Modal>
  );
}
