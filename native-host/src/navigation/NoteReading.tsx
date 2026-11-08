import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { CSSProperties } from "react";
import type { Note } from "../workspace/types.ts";
import type { WorkspaceController } from "../workspace/controller.ts";
import { rememberNoteScroll } from "./scroll.ts";
import { NoteEditor } from "../editor/NoteEditor.tsx";
import { NoteMeta } from "../ui/NoteParts.tsx";
import type { NoteMode } from "../ui/NoteParts.tsx";
import { DocWidthResizer, clampDocWidth } from "../ui/DocWidthResizer.tsx";
import { useNoteAsk, NoteAskFrame, AskComposePopover } from "@claude-plug/ask";
import type { NoteBodyPort, AskComposeState } from "@claude-plug/ask";
import { useNoteRail, noteRailNow } from "@claude-plug/state";
import { NoteSessionRail } from "@claude-plug/session";

export function NoteReading({
  note,
  controller,
  parent,
  root,
  mode,
  onClose,
  closeLabel,
}: {
  note: Note;
  controller?: WorkspaceController;
  parent?: string;
  /** The root note of a sub page chain. The ask target uses it, not the nearest parent (3.13). */
  root?: string;
  mode?: NoteMode;
  /** 노트 여러 개를 나란히 띄운 칸에서 그 칸을 닫는 동작. 정보줄 오른쪽 끝의 닫기 단추로 이어집니다. */
  onClose?: () => void;
  closeLabel?: string;
}) {
  const live = controller?.data.note({ id: note.id, parent }) || note;
  const noteDir = controller?.getSnapshot().workspace?.path && note.path
    ? `${controller.getSnapshot().workspace!.path}/${note.path}`
    : undefined;
  const draftGetter = useRef<() => string>(() => live.body);
  const [composeState, setComposeState] = useState<(AskComposeState & { wrapMark?: (turnId: string) => void }) | null>(null);

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
    autosaveBody: async (body?: string) => {
      const text = body ?? draftGetter.current();
      await controller?.data.saveBody({ id: note.id, parent }, text);
    },
    currentBody: () => draftGetter.current(),
    commitTitle: async (title?: string) => {
      if (title !== undefined) {
        await controller?.data.updateNote({ id: note.id, parent }, { title });
      }
    },
    protect: (enabled = true) => {
      controller?.data.protect({ id: note.id, parent }, enabled);
    },
    flush: async () => {
      await controller?.data.saveBody({ id: note.id, parent }, draftGetter.current());
    },
    discard: async () => {
      if (parent) await controller?.data.loadSubNotes(parent);
      else await controller?.data.reconcile();
    },
    keep: async () => {
      await controller?.data.saveBody({ id: note.id, parent }, draftGetter.current(), true);
    },
    restore: async (body: string) => {
      await controller?.data.saveBody({ id: note.id, parent }, body, true);
    },
    conflicted: () => {
      const key = parent ? `${parent}:${note.id}` : note.id;
      return controller?.data.getSnapshot().conflicted.includes(key) ?? false;
    },
    editing: () => {
      const key = parent ? `${parent}:${note.id}` : note.id;
      return controller?.data.getSnapshot().editing.includes(key) ?? false;
    },
  }), [controller, note.id, parent]);

  const workspace = controller?.getSnapshot().workspace;
  const askWire = useNoteAsk({
    doc: live,
    port,
    baseDir: noteDir || "",
    vault: workspace && !workspace.sample ? workspace.path : "",
    currentDraft: () => draftGetter.current(),
    // 3.12: the wait limit is read at the moment of the press, not when the screen was drawn.
    timeoutSeconds: () => Number(controller?.getSnapshot().prefs.askTimeoutSeconds) || 3600,
    repo: (() => {
      const agenda = workspace?.agendas.find((item) => item.id === live.agendaId);
      return typeof agenda?.repo === "string" && agenda.repo.trim() ? agenda.repo : undefined;
    })(),
    root,
    rootSessionId: parent
      ? (() => {
          const value = controller?.data.note({ id: root ?? parent })?.sessionId;
          return typeof value === "string" && value.trim() ? value.trim() : undefined;
        })()
      : undefined,
    // 4.11: a #doc: link in a model answer opens that note's preview; an unknown note is reported by the answer.
    openDoc: (id: string) => {
      if (!controller || !controller.getSnapshot().workspace?.notes.some((item) => item.id === id)) return false;
      controller.showPreview(id);
      return true;
    },
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

  const isWindow = context === "window";
  const prefDocWidth = clampDocWidth(controller?.getSnapshot().prefs.docWidth);
  const [windowDocWidth, setWindowDocWidth] = useState<number>(prefDocWidth);
  const docWidth = isWindow ? windowDocWidth : prefDocWidth;

  const resizerController = useMemo(() => {
    if (!controller) return undefined;
    if (!isWindow) return controller;
    return {
      ...controller,
      preference: (patch: { docWidth?: number }) => {
        if (typeof patch.docWidth === "number") {
          setWindowDocWidth(clampDocWidth(patch.docWidth));
        }
      },
    } as WorkspaceController;
  }, [controller, isWindow]);

  return (
    <article
      className={`doc-pane note-reading ${live.format === "html" ? "doc-prose" : ""}`}
      style={context !== "preview" ? ({ "--doc-width": `${docWidth}px` } as CSSProperties) : undefined}
    >
      {context !== "preview" && controller && (
        <DocWidthResizer controller={resizerController} width={docWidth} />
      )}
      {controller ? (
        <NoteMeta
          key={note.id}
          note={context === "preview" ? note : live}
          controller={controller}
          parent={parent}
          mode={context}
          onClose={onClose}
          closeLabel={closeLabel}
        />
      ) : (
        <p className="eyebrow">
          {note.date} · {note.time}
        </p>
      )}
      {controller ? (
        <div className="note-body">
          <NoteAskFrame baseDir={noteDir} noteId={note.id}
            cardSize={{ width: Number(controller.getSnapshot().prefs.askCardWidth) || 360, height: typeof controller.getSnapshot().prefs.askCardHeight === "number" ? Number(controller.getSnapshot().prefs.askCardHeight) : null }}
            saveCardSize={(width, height) => controller.preference({ askCardWidth: width, askCardHeight: height })}>
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
              onOpenAskCompose={(quote, rect, wrapMark, markId) => {
                setComposeState({ quote, left: rect.left, top: rect.top, wrapMark, markId });
              }}
            />
          </NoteAskFrame>
          {askWire?.panel}
          {composeState && (
            <NoteAskFrame baseDir={noteDir} noteId={note.id}
            cardSize={{ width: Number(controller.getSnapshot().prefs.askCardWidth) || 360, height: typeof controller.getSnapshot().prefs.askCardHeight === "number" ? Number(controller.getSnapshot().prefs.askCardHeight) : null }}
            saveCardSize={(width, height) => controller.preference({ askCardWidth: width, askCardHeight: height })}>
              <AskComposePopover
                state={composeState}
                onSubmit={(question, images) => {
                  // 2.1: an HTML box asks with its own id and plants no underline; any other question gets a new id,
                  // whose tail is eight hexadecimal digits of a random UUID.
                  const turnId = composeState.markId ?? `ak${Date.now().toString(36)}-${crypto.randomUUID().replace(/-/g, "").slice(0, 8)}`;
                  if (!composeState.markId) composeState.wrapMark?.(turnId);
                  askWire?.editor?.onAskAi?.(composeState.quote, question, turnId, images);
                  setComposeState(null);
                }}
                onCancel={() => setComposeState(null)}
              />
            </NoteAskFrame>
          )}
        </div>
      ) : (
        <>
          <h1>{note.title || "제목 없는 노트"}</h1>
          <pre className="note-body">{note.body}</pre>
        </>
      )}
      {parent && controller && <NoteRail note={note} controller={controller} noteDir={noteDir} root={root ?? parent} />}
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
      <NoteRail note={note} controller={controller} noteDir={noteDir} />
    </div>
  );
}

/**
 * 8.1, 8.2: the session rail of a note or of a sub page. A note's rail is asked for by `openNoteRail(<note id>, <session id>)`; a sub note
 * opens it with its own id, and its chats are those of the pair (root id, sub id).
 */
function NoteRail({
  note,
  controller,
  noteDir,
  root,
}: {
  note: Note;
  controller: WorkspaceController;
  noteDir?: string;
  /** The root note id when this is a sub page. */
  root?: string;
}) {
  const { rails, closeRail } = useNoteRail();
  const sessionId = rails[note.id];
  if (!sessionId) return null;
  const snapshot = controller.getSnapshot();
  return (
    <NoteSessionRail
      // 8.2, 8.5: the rail's state is per note and session; a different session starts a fresh rail.
      key={`${note.id}|${sessionId}`}
      sessionId={sessionId}
      onClose={() => closeRail?.(note.id)}
      chat={{
        target: root ? { noteId: root, subId: note.id } : { noteId: note.id },
        noteMarkdown: note.body,
        noteFormat: note.format,
        noteDir,
        vault: snapshot.workspace && !snapshot.workspace.sample ? snapshot.workspace.path : "",
        timeoutSeconds: () => Number(controller.getSnapshot().prefs.askTimeoutSeconds) || 3600,
        // 8.4: a sub page closes first; a note window hands the address to the main window (the controller does).
        onOpenSession: (id: string) => {
          if (root) controller.openSubNotes?.(root, []);
          controller.navigate(`/session/${encodeURIComponent(id)}`);
        },
        width: Number(snapshot.prefs.noteRailWidth) || 392,
        onWidthChange: (width: number) => controller.preference({ noteRailWidth: width }),
      }}
    />
  );
}
