import { useEffect, useRef, useState } from "react";
import type { Note, WorkspaceState } from "../workspace/types.ts";
import { asRow, asText } from "../workspace/types.ts";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { EditorHandle } from "../editor/Editor.tsx";
import { Confirm } from "./Modal.tsx";
import { History } from "./History.tsx";
import { ReadMarkdown } from "./ReadMarkdown.tsx";
import {
  TYPE_NAMES,
  displayTitle,
  tags,
  tagCandidates,
  recommendations,
  linkCandidates,
  shortDate,
  visibleNotes,
} from "./model.ts";
import { stripMarkers } from "../editor/grammar.ts";
import { agendaAddress, docAddress } from "../navigation/routes.ts";
export type NoteMode = "tab" | "window" | "agenda" | "preview" | "sub";
const go = (c: WorkspaceController, path: string) => {
  if (c.context.kind === "note")
    void c.callHost("show_in_main", { path, rail: null, sub: null });
  else c.navigate(path);
};
export function NoteMeta({
  note,
  controller,
  parent,
  mode = "tab",
}: {
  note: Note;
  controller: WorkspaceController;
  parent?: string;
  mode?: NoteMode;
}) {
  const state = controller.getSnapshot(),
    [mins, setMins] = useState(Math.max(0, Number(note.mins) || 0)),
    [identity, setIdentity] = useState(false),
    [evaluating, setEvaluating] = useState(false),
    [history, setHistory] = useState(false),
    [deleting, setDeleting] = useState(false),
    [moveError, setMoveError] = useState("");
  const live = controller.data.note({ id: note.id, parent }) || note,
    key = parent ? parent + ":" + note.id : note.id,
    agenda = state.workspace?.agendas.find((a) => a.id === note.agendaId),
    template = state.workspace?.templates.find((t) => t.id === note.template),
    project = state.workspace?.projects.find((p) => p.slug === agenda?.project),
    chain = parent
      ? controller.data.chain(parent)?.split("/").concat(note.id).join("/") ||
        note.id
      : note.id;
  const remove = async () => {
    if (parent) {
      const result = await controller.data.deleteSubNote(parent, note.id);
      if (result.ok) controller.removeChildCards(parent, note.id);
      controller.popSubNote();
    } else {
      if (mode === "preview") controller.showPreview(null);
      await controller.data.deleteDoc(note.id);
    }
  };
  return (
    <>
      <div className="doc-meta">
        <span>
          {TYPE_NAMES[asText(note.type) || "free"] || asText(note.type)} ·{" "}
          {note.date} {note.time}
        </span>
        {note.format !== "markdown" && (
          <span className="format-badge">{note.format.toUpperCase()}</span>
        )}
        {live.tier === "record" && <span className="record-badge">기록</span>}
        {!parent && (
          <>
            <button
              className="fav-toggle"
              aria-label="즐겨찾기"
              aria-pressed={live.favorite === true}
              onClick={() =>
                void controller.data.favorite(note.id, live.favorite !== true)
              }
            >
              {live.favorite === true ? "★" : "☆"}
            </button>
            <button
              className="doc-eval-badge"
              onClick={() => setEvaluating(!evaluating)}
            >
              {asRow(live.evaluation)
                ? String(asRow(live.evaluation)?.score)
                : "평가"}
            </button>
            {mode === "preview" ? (
              Number(note.mins) > 0 && <span>{String(note.mins)}분</span>
            ) : (
              <label>
                소요 시간{" "}
                <input
                  className="doc-meta-mins-input"
                  type="number"
                  min={0}
                  value={mins}
                  onChange={(e) =>
                    setMins(
                      Math.max(0, Math.round(Number(e.target.value) || 0)),
                    )
                  }
                  onBlur={() => {
                    if (mins !== Number(live.mins || 0))
                      void controller.data.updateNote(
                        { id: note.id },
                        { mins },
                      );
                  }}
                />
                분
              </label>
            )}
          </>
        )}
        {note.format === "markdown" && (
          <>
            <button
              className="doc-meta-body-copy"
              onClick={() =>
                void navigator.clipboard.writeText(
                  stripMarkers(controller.readBuffer(key, live.body)),
                )
              }
            >
              본문 복사
            </button>
            <button
              className="doc-meta-history"
              onClick={() => setHistory(true)}
            >
              이력
            </button>
          </>
        )}
        <button onClick={() => setIdentity(!identity)}>
          {parent ? "경로" : "ID"}
        </button>
        {!parent &&
          controller.isNative() &&
          controller.context.kind === "main" && (
            <button
              onClick={() =>
                void controller
                  .moveNoteWindow(note.id)
                  .then((result) =>
                    setMoveError(
                      result === "opened"
                        ? ""
                        : result === "unsaved"
                          ? "저장하지 못한 글이 있어서 새 창으로 옮기지 않았어요. 충돌 배너를 먼저 풀어 주세요."
                          : "새 창을 열지 못했어요. 앱을 다시 켜고 해 보세요.",
                    ),
                  )
              }
            >
              새 창으로
            </button>
          )}
        {(mode === "agenda" || mode === "preview") && (
          <button
            onClick={() => {
              if (mode === "preview") controller.showPreview(null);
              go(controller, docAddress(note.id));
            }}
          >
            탭으로 띄우기
          </button>
        )}
        <button onClick={() => setDeleting(true)}>삭제</button>
      </div>
      {identity && (
        <div className="doc-id-chip">
          <span>
            {parent ? "경로" : "ID"} <code>{chain}</code>
          </span>
          <button onClick={() => void navigator.clipboard.writeText(chain)}>
            복사
          </button>
          {!parent && state.workspace?.path && (
            <button
              onClick={() =>
                void navigator.clipboard.writeText(
                  `withlog://doc/${encodeURIComponent(note.id)}?ws=${encodeURIComponent(state.workspace!.path)}&window=1`,
                )
              }
            >
              새 창 링크
            </button>
          )}
        </div>
      )}
      <div className="doc-context-chips">
        {!parent &&
          agenda &&
          (mode === "preview" ? (
            <span>{agenda.title}</span>
          ) : (
            <button onClick={() => go(controller, agendaAddress(agenda.id))}>
              {project ? asText(project.title) + " / " : ""}
              {agenda.title}
            </button>
          ))}
        {!!note.template &&
          !parent &&
          (template ? (
            <button
              onClick={() =>
                go(
                  controller,
                  "/templates?open=" + encodeURIComponent(asText(template.id)),
                )
              }
            >
              {asText(template.name)}
            </button>
          ) : (
            <span>알 수 없음 ({asText(note.template)})</span>
          ))}
        {sessionChips(live).map((s) => (
          <span className="session-chip" key={s.id}>
            {s.label} <code>{s.id}</code>
            <button onClick={() => void navigator.clipboard.writeText(s.id)}>
              복사
            </button>
          </span>
        ))}
      </div>
      {moveError && <p role="status">{moveError}</p>}
      {evaluating && (
        <Evaluation
          note={live}
          controller={controller}
          close={() => setEvaluating(false)}
        />
      )}{" "}
      {history && (
        <History
          id={note.id}
          keyValue={key}
          controller={controller}
          fallback={live.body}
          close={() => setHistory(false)}
        />
      )}{" "}
      {deleting && (
        <Confirm
          title={parent ? "하위 페이지를 삭제할까요?" : "휴지통으로 옮길까요?"}
          description={
            parent ? "휴지통을 거치지 않고 바로 지워집니다." : note.title
          }
          close={() => setDeleting(false)}
          accept={remove}
          label={parent ? "삭제" : "휴지통으로"}
        />
      )}
    </>
  );
}
export function sessionChips(note: Note) {
  const a = asText(note.sessionId),
    b = asText(note.editedSessionId);
  return a && b && a !== b
    ? [
        { id: a, label: "작성 세션" },
        { id: b, label: "편집 세션" },
      ]
    : a || b
      ? [{ id: b || a, label: "세션" }]
      : [];
}
function Evaluation({
  note,
  controller,
  close,
}: {
  note: Note;
  controller: WorkspaceController;
  close(): void;
}) {
  const [score, setScore] = useState(
      Number(asRow(note.evaluation)?.score ?? 70),
    ),
    [memo, setMemo] = useState(asText(asRow(note.evaluation)?.note));
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const outside = (e: PointerEvent) => {
      if (
        !box.current?.contains(e.target as Node) &&
        !(e.target as Element).closest(".doc-eval-badge")
      )
        close();
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [close]);
  const save = () => {
    void controller.data.evaluate(note.id, score, memo);
    close();
  };
  return (
    <div
      className="doc-eval-pop"
      ref={box}
      onKeyDown={(e) => {
        if (e.key === "Escape") close();
        if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
          e.preventDefault();
          save();
        }
      }}
    >
      <label>
        평가 {score}
        <input
          className="doc-eval-range"
          type="range"
          min={0}
          max={100}
          value={score}
          onChange={(e) => setScore(Number(e.target.value))}
        />
      </label>
      <textarea
        className="doc-eval-note"
        aria-label="평가 메모"
        maxLength={1000}
        value={memo}
        onChange={(e) => setMemo(e.target.value)}
      />
      <div>
        <button onClick={close}>취소</button>
        <button
          onClick={() => {
            void controller.data.evaluate(note.id, null);
            close();
          }}
        >
          평가 지우기
        </button>
        <button onClick={save}>저장</button>
      </div>
    </div>
  );
}
export function Tags({
  note,
  parent,
  controller,
  state,
}: {
  note: Note;
  parent?: string;
  controller: WorkspaceController;
  state: WorkspaceState;
}) {
  const [value, setValue] = useState(""),
    [focused, setFocused] = useState(false),
    own = tags(note);
  const add = (text: string) => {
    const tag = text.trim();
    if (tag && own.length < 50 && !own.includes(tag))
      void controller.data.updateNote(
        { id: note.id, parent },
        { tags: [...own, tag] },
      );
    setValue("");
  };
  return (
    <section className="note-tags">
      <h3>태그</h3>
      <div>
        {own.map((t) => (
          <span className="tag-chip" key={t}>
            {t}
            <button
              aria-label={`${t} 태그 삭제`}
              onClick={() =>
                void controller.data.updateNote(
                  { id: note.id, parent },
                  { tags: own.filter((x) => x !== t) },
                )
              }
            >
              ×
            </button>
          </span>
        ))}
      </div>
      <input
        aria-label="태그 추가"
        placeholder="태그 추가"
        value={value}
        onChange={(e) => setValue(e.target.value)}
        onFocus={() => setFocused(true)}
        onBlur={() => setFocused(false)}
        onKeyDown={(e) => {
          if (e.nativeEvent.isComposing) return;
          if (e.key === "Enter" || e.key === ",") {
            e.preventDefault();
            add(value);
          }
          if (e.key === "Backspace" && !value && own.length) {
            e.preventDefault();
            void controller.data.updateNote(
              { id: note.id, parent },
              { tags: own.slice(0, -1) },
            );
          }
        }}
      />
      {focused && (
        <div className="tag-candidates">
          {tagCandidates(state.workspace?.notes || [], own, value).map((t) => (
            <button
              key={t}
              onMouseDown={(e) => e.preventDefault()}
              onClick={() => add(t)}
            >
              {t}
            </button>
          ))}
        </div>
      )}
    </section>
  );
}
export function Links({
  note,
  controller,
  state,
  mode,
}: {
  note: Note;
  controller: WorkspaceController;
  state: WorkspaceState;
  mode: NoteMode;
}) {
  const [open, setOpen] = useState(false),
    [adding, setAdding] = useState(false),
    [kind, setKind] = useState("direct"),
    [query, setQuery] = useState(""),
    [tag, setTag] = useState("");
  const all = visibleNotes(
      state.workspace?.notes || [],
      state.prefs.showRecords,
    ),
    out = state.vault.relations.filter((r) => r.from === note.id),
    incoming = state.vault.relations.filter((r) => r.to === note.id),
    candidates = linkCandidates(note, all, kind, query, tag),
    names: Record<string, string> = {
      direct: "직접 흐름",
      indirect: "영향",
      attach: "참고",
    };
  const openNote = (n: Note) => {
    if (mode === "preview") controller.showPreview(n.id);
    else
      go(
        controller,
        agendaAddress(n.agendaId) + "?open=" + encodeURIComponent(n.id),
      );
  };
  return (
    <section className="note-links">
      <header>
        <button onClick={() => setOpen(!open)} aria-expanded={open}>
          연결 {out.length + incoming.length}
        </button>
        <button
          onClick={() => {
            setAdding(!adding);
            setOpen(true);
          }}
        >
          연결 추가
        </button>
      </header>
      {open && (
        <>
          <h4>추천 연결</h4>
          {recommendations(note, all, state.vault.relations).map((r) => (
            <div key={r.note.id}>
              <span>{names[r.kind]}</span>{" "}
              <button onClick={() => openNote(r.note)}>
                {displayTitle(r.note, state.prefs.titleDate)}
              </button>
              <small>{r.reason}</small>
              <button
                onClick={() =>
                  void controller.data.link(note.id, r.note.id, r.kind)
                }
              >
                연결
              </button>
            </div>
          ))}
          {[
            ["이 노트에서 이어진 기록", out],
            ["이 노트를 가리키는 기록", incoming],
          ].map(([title, list]) => (
            <div key={String(title)}>
              <h4>{String(title)}</h4>
              {(list as typeof out).map((r) => {
                const n = state.workspace?.notes.find(
                  (n) => n.id === (r.from === note.id ? r.to : r.from),
                );
                return (
                  n && (
                    <p key={r.from + r.to + r.kind}>
                      {names[r.kind]}{" "}
                      <button onClick={() => openNote(n)}>
                        {displayTitle(n, state.prefs.titleDate)}
                      </button>
                      {r.from === note.id && (
                        <button
                          aria-label="연결 끊기"
                          onClick={() =>
                            void controller.data.link(
                              note.id,
                              n.id,
                              r.kind,
                              true,
                            )
                          }
                        >
                          ×
                        </button>
                      )}
                    </p>
                  )
                );
              })}
            </div>
          ))}
          {adding && (
            <div className="link-picker">
              <select
                aria-label="연결 종류"
                value={kind}
                onChange={(e) => setKind(e.target.value)}
              >
                {Object.entries(names).map(([v, l]) => (
                  <option key={v} value={v}>
                    {l}
                  </option>
                ))}
              </select>
              <input
                aria-label="연결할 노트 찾기"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
              <div>
                {[
                  "",
                  ...new Set([
                    ...tags(note),
                    ...tagCandidates(all, tags(note), "", 6),
                  ]),
                ].map((t) => (
                  <button
                    key={t}
                    aria-pressed={tag === t}
                    onClick={() => setTag(t)}
                  >
                    {t || "모든 태그"}
                  </button>
                ))}
              </div>
              {candidates.slice(0, 30).map((n) => (
                <button
                  className="link-candidate"
                  key={n.id}
                  onClick={() => void controller.data.link(note.id, n.id, kind)}
                >
                  {displayTitle(n, state.prefs.titleDate)}
                </button>
              ))}
              {candidates.length > 30 && (
                <p>관련성 높은 30개만 표시합니다. 검색어를 좁혀 주세요.</p>
              )}
            </div>
          )}
        </>
      )}
    </section>
  );
}
export function NotesBelow({
  note,
  parent,
  controller,
  state,
  handle,
  activeComment,
  openChild,
}: {
  note: Note;
  parent?: string;
  controller: WorkspaceController;
  state: WorkspaceState;
  handle: React.RefObject<EditorHandle | null>;
  activeComment: string;
  openChild(id: string): void;
}) {
  const key = parent ? parent + ":" + note.id : note.id,
    folder =
      !!parent ||
      (!!state.workspace?.path &&
        !!note.path &&
        !/\.(md|json|html)$/i.test(asText(note.path))),
    comments = state.vault.comments[key],
    children = state.vault.subNotes[note.id] || [];
  return (
    <>
      {folder && (
        <section className="subnotes">
          <header>
            <h3>후속 노트 {children.length}</h3>
            <button
              onClick={() =>
                void controller.data.createSubNote(note.id).then((r) => {
                  if (r.ok && asText(r.value?.id))
                    openChild(asText(r.value?.id));
                })
              }
            >
              추가
            </button>
          </header>
          <nav className="child-note-list" aria-label="후속 노트">
            {children.map((child) => (
              <button
                className="subnote-row"
                key={child.id}
                onClick={() => openChild(child.id)}
              >
                <strong>{child.title}</strong>
                <small>{shortDate(child.created)}</small>
              </button>
            ))}
          </nav>
        </section>
      )}
      {folder && (
        <Comments
          key={key}
          note={note}
          parent={parent}
          controller={controller}
          comments={comments}
          handle={handle}
          activeComment={activeComment}
        />
      )}
    </>
  );
}
function Comments({
  note,
  parent,
  controller,
  comments,
  handle,
  activeComment,
}: {
  note: Note;
  parent?: string;
  controller: WorkspaceController;
  comments: WorkspaceState["vault"]["comments"][string] | undefined;
  handle: React.RefObject<EditorHandle | null>;
  activeComment: string;
}) {
  const [body, setBody] = useState(""),
    [edit, setEdit] = useState(""),
    [editBody, setEditBody] = useState(""),
    [deleting, setDeleting] = useState(""),
    [flash, setFlash] = useState("");
  const nodes = useRef(new Map<string, HTMLDivElement>());
  useEffect(() => {
    if (!activeComment) return;
    const id = activeComment.split("|")[0];
    nodes.current
      .get(id)
      ?.scrollIntoView({ block: "center", behavior: "smooth" });
    setFlash(id);
    const timer = setTimeout(() => setFlash(""), 1400);
    return () => clearTimeout(timer);
  }, [activeComment]);
  const send = () => {
    if (!body.trim()) return;
    void controller.data.comment({ id: note.id, parent }, "create", body);
    setBody("");
  };
  const keyboard = (e: React.KeyboardEvent, save: () => void) => {
    if (e.key === "Escape") {
      e.stopPropagation();
      e.preventDefault();
    }
    if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
      e.preventDefault();
      save();
    }
  };
  return (
    <section className="note-comment-thread">
      <h3>코멘트 {comments?.comments.length || 0}</h3>
      {comments?.state === "unreadable" ? (
        <p role="alert">
          코멘트 파일(comments.json)을 읽을 수 없어요. 덮어써서 잃지 않도록
          편집을 잠시 멈췄어요.
        </p>
      ) : (
        <>
          <div className="comment-list">
            {comments?.comments.length ? (
              comments.comments.map((c) => {
                const id = asText(c.id),
                  quote = asText(asRow(c.anchor)?.quote);
                return (
                  <div
                    className={`comment-row ${flash === id ? "comment-flash" : ""}`}
                    ref={(node) => {
                      if (node) nodes.current.set(id, node);
                      else nodes.current.delete(id);
                    }}
                    key={id}
                  >
                    <header>
                      <time>{shortDate(c.createdAt)}</time>
                      {!!c.editedAt && <small>수정됨</small>}
                      <button
                        onClick={() => {
                          setEdit(id);
                          setEditBody(asText(c.body));
                        }}
                      >
                        고치기
                      </button>
                      <button onClick={() => setDeleting(id)}>지우기</button>
                    </header>
                    {quote && (
                      <button
                        className="comment-quote"
                        onClick={() => handle.current?.comment(id)}
                      >
                        {quote}
                      </button>
                    )}
                    {edit === id ? (
                      <>
                        <textarea
                          aria-label="코멘트 고치기"
                          value={editBody}
                          onChange={(e) => setEditBody(e.target.value)}
                          onKeyDown={(e) =>
                            keyboard(e, () => {
                              void controller.data.comment(
                                { id: note.id, parent },
                                "update",
                                editBody,
                                id,
                              );
                              setEdit("");
                            })
                          }
                        />
                        <button onClick={() => setEdit("")}>취소</button>
                        <button
                          onClick={() => {
                            void controller.data.comment(
                              { id: note.id, parent },
                              "update",
                              editBody,
                              id,
                            );
                            setEdit("");
                          }}
                        >
                          저장
                        </button>
                      </>
                    ) : (
                      <div className="comment-body">
                        <ReadMarkdown body={asText(c.body)} />
                      </div>
                    )}
                  </div>
                );
              })
            ) : (
              <p>아직 코멘트가 없어요.</p>
            )}
          </div>
          <textarea
            aria-label="새 코멘트"
            placeholder="코멘트를 남겨요"
            value={body}
            onChange={(e) => setBody(e.target.value)}
            onKeyDown={(e) => keyboard(e, send)}
          />
          <button disabled={!body.trim()} onClick={send}>
            추가
          </button>
        </>
      )}
      {deleting && (
        <Confirm
          title="코멘트를 삭제할까요?"
          description="이 코멘트가 사라져요. 휴지통을 거치지 않고 바로 지워집니다."
          close={() => setDeleting("")}
          accept={async () => {
            const r = await controller.data.comment(
              { id: note.id, parent },
              "delete",
              "",
              deleting,
            );
            if (r.ok) handle.current?.removeComment(deleting);
          }}
        />
      )}
    </section>
  );
}
