import {
  useEffect,
  useMemo,
  useRef,
  useState,
  useSyncExternalStore,
} from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { Note } from "../workspace/types.ts";
import { asText } from "../workspace/types.ts";
import { Editor } from "./Editor.tsx";
import type { EditorHandle } from "./Editor.tsx";
import { EditorSession } from "./session.ts";
import { editorHost } from "./platform.ts";
import { ConflictDiff } from "./ConflictDiff.tsx";
import { Tags, Links, NotesBelow } from "../ui/NoteParts.tsx";
import type { NoteMode } from "../ui/NoteParts.tsx";
import { ReadMarkdown } from "../ui/ReadMarkdown.tsx";
import { openEditorLink } from "./media.tsx";
import type { EditorHeading } from "./Editor.tsx";
import type { AskEditorProps } from "../plugs/empty/ask.tsx";

export function NoteEditor({
  note,
  parent,
  controller,
  mode = "tab",
  askEditor,
  onDraftBuffer,
}: {
  note: Note;
  parent?: string;
  controller: WorkspaceController;
  mode?: NoteMode;
  askEditor?: AskEditorProps;
  onDraftBuffer?: (fn: () => string) => void;
}) {
  const [session] = useState(
    () => new EditorSession(controller, { id: note.id, parent }, note.body),
  );
  useEffect(() => {
    onDraftBuffer?.(() => session.body());
  }, [session, onDraftBuffer]);
  const snapshot = useSyncExternalStore(session.subscribe, session.getSnapshot);
  const state = controller.getSnapshot(),
    data = controller.data,
    key = parent ? `${parent}:${note.id}` : note.id;
  const [activeComment, setActiveComment] = useState(""),
    handle = useRef<EditorHandle | null>(null);
  const [difference, setDifference] = useState<string | null>(null);
  const [headings, setHeadings] = useState<EditorHeading[]>([]),
    [section, setSection] = useState(""),
    [toc, setToc] = useState(false);
  const host = useMemo(editorHost, []),
    children = state.vault.subNotes[note.id] || [];
  const directory =
    state.workspace?.path &&
    asText(note.path) &&
    (parent || !/\.(md|json|html)$/i.test(asText(note.path)))
      ? `${state.workspace.path}/${asText(note.path)}`
      : undefined;
  useEffect(() => {
    session.receive(note.body);
  }, [note.body, session]);
  useEffect(() => () => session.dispose(), [session]);
  useEffect(
    () =>
      controller.registerChildRemover(note.id, (id) =>
        handle.current?.removeChild(id),
      ),
    [note.id, controller],
  );
  useEffect(() => {
    const blur = () => {
      void session.flush();
    };
    addEventListener("blur", blur);
    return () => removeEventListener("blur", blur);
  }, [session]);
  useEffect(() => {
    if (!directory) return;
    const release = data.retainSubNotes(note.id);
    void data.loadSubNotes(note.id);
    void data.loadComments({ id: note.id, parent });
    return release;
  }, [note.id, parent, data]);
  const openChild = (id: string) => {
    const chain = data.chain(note.id)?.split("/") || [note.id];
    if (mode === "preview") {
      controller.showPreview(null);
      if (controller.context.kind === "note") {
        void controller.callHost("show_in_main", {
          path: "/doc/" + encodeURIComponent(note.id),
          rail: null,
          sub: id,
        });
        return;
      }
      controller.openSubNotes(chain[0], [...chain.slice(1), id]);
      controller.navigate("/doc/" + encodeURIComponent(note.id));
      return;
    }
    controller.openSubNotes(chain[0], [...chain.slice(1), id]);
  };
  const comments = state.vault.comments[key]?.comments || [];
  const features = {
    currentId: note.id,
    notes: state.workspace?.notes.map((note) => ({
      id: note.id,
      title: note.title,
      created: note.created,
      tier: note.tier,
      agenda: state.workspace?.agendas.find(
        (agenda) => agenda.id === note.agendaId,
      )?.title,
    })),
    children,
    noteDirectory: directory,
    host,
    showRecords: state.prefs.showRecords,
    comments: Object.fromEntries(
      comments.map((comment) => [asText(comment.id), asText(comment.body)]),
    ),
    openNote: controller.showPreview,
    openChild,
    createChild: directory
      ? async () => {
          const result = await data.createSubNote(note.id);
          const id = asText(result.value?.id);
          return result.ok && id
            ? { id, title: asText(result.value?.title) || "새 후속 노트" }
            : null;
        }
      : undefined,
    comment: async (id: string, body: string, quote: string) => {
      const result = await data.comment(
        { id: note.id, parent },
        "create",
        body,
        id,
        quote,
      );
      if (result.ok) {
        setActiveComment(id + "|" + Date.now());
        await data.loadComments({ id: note.id, parent });
      }
    },
    openComment: (id: string) => {
      setActiveComment(id + "|" + Date.now());
      void data.loadComments({ id: note.id, parent });
    },
  };
  return (
    <>
      {note.format !== "html" && (
        <div className="note-editor-status">
          <span role="status" className={`vb-savestate ${snapshot.state}`}>
            {snapshot.status}
          </span>
        </div>
      )}
      {snapshot.state === "conflict" && (
        <div className="doc-conflict-banner" role="alert">
          <p>
            이 문서가 외부(MCP·동기화)에서도 바뀌었어요. 어느 쪽을 살릴지 고르기
            전에는 자동 저장을 멈춰 둘게요.
          </p>
          <div>
            {note.format !== "json" && (
              <button
                data-conflict-action="diff"
                onClick={() => setDifference(session.body())}
              >
                차이 보기
              </button>
            )}
            <button
              data-conflict-action="load"
              onClick={() => void session.discard()}
              disabled={snapshot.discarding}
            >
              디스크 버전 불러오기
            </button>
            <button
              data-conflict-action="keep"
              onClick={() => void session.keep()}
              disabled={snapshot.discarding}
            >
              내 편집 유지
            </button>
          </div>
        </div>
      )}
      {snapshot.discarding && <p role="status">디스크 버전을 불러오는 중…</p>}
      {note.format === "html" ? (
        <div className="doc-prose">
          <ReadMarkdown
            html={note.body}
            directory={directory}
            native={host.native}
            onLink={(url) => {
              if (url.startsWith("#") && !/^#(?:doc|sub):/.test(url))
                return false;
              void openEditorLink(url, features);
            }}
          />
        </div>
      ) : (
        <Editor
          body={snapshot.mirror.body}
          format={note.format}
          features={features}
          label={`${note.title} 본문 편집`}
          onChange={session.change}
          mirror={snapshot.mirror}
          onMirrorPending={session.pause}
          onMirrorConflict={session.conflict}
          onReady={(value) => {
            handle.current = value;
          }}
          onHeadings={setHeadings}
          onSection={setSection}
          diagramsOpen={state.prefs.diagrams}
          askDocId={askEditor?.askDocId || note.id}
          onAskAi={askEditor?.onAskAi}
          onAskFollowUp={askEditor?.onAskFollowUp}
        />
      )}
      {headings.length >= 3 && (
        <nav
          className={`note-toc ${toc ? "expanded" : ""}`}
          aria-label="목차"
          onMouseEnter={() => setToc(true)}
          onMouseLeave={() => setToc(false)}
          onFocus={() => setToc(true)}
        >
          {headings.slice(1).map((h) => (
            <button
              aria-current={section === h.key}
              key={h.key}
              onClick={() => {
                setToc(true);
                handle.current?.heading(h.key);
              }}
            >
              {toc ? h.text : "━"}
            </button>
          ))}
        </nav>
      )}
      {difference !== null && (
        <ConflictDiff
          left={difference}
          readRight={session.diskBody}
          close={() => setDifference(null)}
          discard={() => void session.discard()}
          keep={() => void session.keep()}
        />
      )}
      <Tags note={note} parent={parent} controller={controller} state={state} />
      {!parent && (
        <Links note={note} controller={controller} state={state} mode={mode} />
      )}
      <NotesBelow
        note={note}
        parent={parent}
        controller={controller}
        state={state}
        handle={handle}
        activeComment={activeComment}
        openChild={openChild}
      />
    </>
  );
}
