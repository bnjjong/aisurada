import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { Note } from "../workspace/types.ts";
import type { WorkspaceController } from "../workspace/controller.ts";
import { rememberNoteScroll } from "./scroll.ts";
import { NoteEditor } from "../editor/NoteEditor.tsx";
import { NoteMeta } from "../ui/NoteParts.tsx";
import type { NoteMode } from "../ui/NoteParts.tsx";
import { useNoteAsk, NoteAskFrame, AskComposePopover } from "@claude-plug/ask";
import type { NoteBodyPort, AskComposeState } from "@claude-plug/ask";
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
  const draftGetter = useRef<() => string>(() => live.body);
  const [composeState, setComposeState] = useState<AskComposeState | null>(null);

  const port = useMemo<NoteBodyPort>(() => ({
    kind: parent ? "sub" : "main",
    id: note.id,
    parent,
    write: async (body: string) => {
      await controller?.data.saveBody({ id: note.id, parent }, body);
    },
    saveDraft: (body: string) => {
      controller?.data.edit({ id: note.id, parent }, { body });
    },
    read: () => draftGetter.current(),
  }), [controller, note.id, parent]);

  const askWire = useNoteAsk({
    doc: live,
    port,
    baseDir: noteDir || "",
    currentDraft: () => draftGetter.current(),
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
          <NoteAskFrame baseDir={noteDir} noteId={note.id}>
            <NoteEditor
              key={`${parent || ""}:${note.id}`}
              note={live}
              parent={parent}
              controller={controller}
              mode={context}
              askEditor={askWire?.editor}
              onDraftBuffer={(fn) => {
                draftGetter.current = fn;
              }}
            />
          </NoteAskFrame>
          {askWire?.panel}
          {composeState && (
            <AskComposePopover
              state={composeState}
              onSubmit={(question, images) => {
                const turnId = `ak${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`;
                askWire?.editor?.onAskAi?.(composeState.quote, question, turnId, images);
                setComposeState(null);
              }}
              onCancel={() => setComposeState(null)}
            />
          )}
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
