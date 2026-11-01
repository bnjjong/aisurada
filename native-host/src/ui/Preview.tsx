import { useRef, useState } from "react";
import type { WorkspaceState } from "../workspace/types.ts";
import type { WorkspaceController } from "../workspace/controller.ts";
import { Modal } from "./Modal.tsx";
import { NoteReading } from "../navigation/NoteReading.tsx";
import { displayTitle } from "./model.ts";
export function Preview({
  state,
  controller,
}: {
  state: WorkspaceState;
  controller: WorkspaceController;
}) {
  const note = state.workspace?.notes.find((n) => n.id === state.preview),
    [full, setFull] = useState(false),
    box = useRef<HTMLDivElement>(null),
    resizing = useRef(false);
  if (!note) return null;
  const captured =
      state.previewOpened?.id === note.id ? state.previewOpened : note,
    agenda = state.workspace?.agendas.find((a) => a.id === captured.agendaId),
    size = state.prefs.previewSize;
  return (
    <Modal
      title={displayTitle(note, state.prefs.titleDate)}
      close={() => controller.showPreview(null)}
      className={`note-preview ${full ? "preview-full" : ""}`}
      wide={full}
    >
      <div
        ref={box}
        className="preview-resize"
        style={
          !full && size ? { width: size.width, height: size.height } : undefined
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
          if (
            r &&
            (Math.round(r.width) !== size?.width ||
              Math.round(r.height) !== size?.height)
          )
            controller.preference({
              previewSize: {
                width: Math.round(r.width),
                height: Math.round(r.height),
              },
            });
        }}
      >
        <div className="preview-tools">
          {document.querySelector(".report-dialog") && (
            <button onClick={() => controller.showPreview(null)}>회고로</button>
          )}
          <button onClick={() => setFull(!full)}>
            {full ? "크기 되돌리기" : "전체 화면"}
          </button>
        </div>
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
