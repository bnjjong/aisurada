import { useEffect, useLayoutEffect, useRef } from "react";
import type { Note } from "../workspace/types.ts";
import type { WorkspaceController } from "../workspace/controller.ts";
import { rememberNoteScroll } from "./scroll.ts";
import { NoteEditor } from "../editor/NoteEditor.tsx";
import { NoteMeta } from "../ui/NoteParts.tsx";
import type { NoteMode } from "../ui/NoteParts.tsx";
import { useNoteAsk, NoteAskFrame, AskComposePopover } from "@claude-plug/ask";
import { useNoteRail, noteRailNow } from "@claude-plug/state";
import { NoteSessionRail } from "@claude-plug/session";

export function NoteReading({
  note,
  controller,
  parent,
  mode,
}: {
  note: Note;
  controller?: WorkspaceController;
  parent?: string;
  mode?: NoteMode;
}) {
  const live = controller?.data.note({ id: note.id, parent }) || note;
  const noteDir = controller?.getSnapshot().workspace?.path && note.path
    ? `${controller.getSnapshot().workspace!.path}/${note.path}`
    : undefined;
  const askWire = useNoteAsk({
    doc: live,
    port: parent ? { kind: "sub", parent, id: note.id } : { kind: "main", id: note.id },
    baseDir: noteDir || "",
    currentDraft: () => live.body,
  });
  useEffect(() => {
    if (parent || !controller) return;
    return controller.recentNotes.enter(
      controller.getSnapshot().workspace?.path || "",
      note.id,
    );
  }, [note.id, parent, controller]);
  const context =
    mode ||
    (parent ? "sub" : controller?.context.kind === "note" ? "window" : "tab");
  return (
    <article
      className={`doc-pane note-reading ${live.format === "html" ? "doc-prose" : ""}`}
    >
      {controller ? (
        <NoteMeta
          key={note.id}
          note={context === "preview" ? note : live}
          controller={controller}
          parent={parent}
          mode={context}
        />
      ) : (
        <p className="eyebrow">
          {note.date} · {note.time}
        </p>
      )}
      {controller ? (
        <div className="note-body">
          <NoteAskFrame noteId={note.id}>
            <NoteEditor
              key={`${parent || ""}:${note.id}`}
              note={live}
              parent={parent}
              controller={controller}
              mode={context}
            />
          </NoteAskFrame>
          {askWire?.panel}
          <AskComposePopover
            state={askWire?.editor}
            onSubmit={() => {}}
            onCancel={() => {}}
          />
        </div>
      ) : (
        <>
          <h1>{note.title || "제목 없는 노트"}</h1>
          <pre className="note-body">{note.body}</pre>
        </>
      )}
    </article>
  );
}

export function NoteScreen({
  note,
  controller,
}: {
  note: Note;
  controller: WorkspaceController;
}) {
  const { rails, closeRail } = useNoteRail();
  const sessionId = rails?.[note.id];
  const noteDir = controller?.getSnapshot().workspace?.path && note.path
    ? `${controller.getSnapshot().workspace!.path}/${note.path}`
    : undefined;
  // Ensure noteRailNow is referenced for deep-link / navigation contracts
  void noteRailNow;
  const surface = useRef<HTMLDivElement>(null);
  useLayoutEffect(() => {
    if (!surface.current) return;
    return rememberNoteScroll(
      surface.current,
      note.id,
      controller.scrollMemory,
      {
        // WebKit requires Window as the receiver of its animation-frame methods.
        now: () => performance.now(),
        frame: (callback) => requestAnimationFrame(callback),
        cancel: (id) => cancelAnimationFrame(id),
      },
    );
  }, [note.id, controller]);
  return (
    <div
      className="doc-scroll"
      ref={surface}
      tabIndex={0}
      aria-label={`${note.title} 본문`}
    >
      <NoteReading note={note} controller={controller} />
      {sessionId && (
        <NoteSessionRail
          sessionId={sessionId}
          onClose={() => closeRail?.(note.id)}
          chat={{
            target: { noteId: note.id },
            noteMarkdown: note.body,
            noteDir,
          }}
        />
      )}
    </div>
  );
}
