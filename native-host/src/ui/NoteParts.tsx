import { useCallback, useEffect, useRef, useState } from "react";
import type { Note, WorkspaceState } from "../workspace/types.ts";
import { asRow, asText } from "../workspace/types.ts";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { EditorHandle } from "../editor/Editor.tsx";
import { Confirm } from "./Modal.tsx";
import { ActionMenu } from "./ActionMenu.tsx";
import type { ActionMenuItem } from "./ActionMenu.tsx";
import { Icon } from "./Icon.tsx";
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
import { openNoteRail, CLAUDE_ENABLED } from "@claude-plug/state";
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
  onClose,
  closeLabel,
}: {
  note: Note;
  controller: WorkspaceController;
  parent?: string;
  mode?: NoteMode;
  /** 노트 여러 개를 나란히 띄운 칸에서 그 칸을 닫는 동작입니다. 있으면 정보줄 오른쪽 끝에 닫기 단추가 붙습니다. */
  onClose?: () => void;
  closeLabel?: string;
}) {
  const state = controller.getSnapshot(),
    [evaluating, setEvaluating] = useState(false),
    [history, setHistory] = useState(false),
    [deleting, setDeleting] = useState(false),
    [menuOpen, setMenuOpen] = useState(false),
    [copiedId, setCopiedId] = useState(false),
    [copiedItem, setCopiedItem] = useState<string | null>(null),
    [moveError, setMoveError] = useState(""),
    moreButton = useRef<HTMLButtonElement>(null),
    closeMenu = useCallback(() => setMenuOpen(false), []);
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
  const score = asRow(live.evaluation)
      ? String(asRow(live.evaluation)?.score)
      : "",
    canPopOut =
      !parent &&
      controller.isNative() &&
      controller.context.kind === "main" &&
      mode !== "preview",
    moveToWindow = () =>
      void controller
        .moveNoteWindow(note.id)
        .then((result) =>
          setMoveError(
            result === "opened"
              ? ""
              : result === "unsaved"
                ? "저장하지 못한 글이 있어서 새 창으로 옮기지 않았어요. 충돌 배너를 먼저 풀어 주세요."
                : result === "asking"
                  ? "답을 기다리는 물음이 있어서 새 창으로 옮기지 않았어요. 답이 온 뒤에 옮겨 주세요."
                  : "새 창을 열지 못했어요. 앱을 다시 켜고 해 보세요.",
          ),
        );
  const copyId = () =>
    void navigator.clipboard.writeText(chain).then(() => {
      setCopiedId(true);
      window.setTimeout(() => setCopiedId(false), 1500);
    });
  // 자주 쓰지 않는 동작은 "더 보기" 메뉴로 모으고, 지우기는 구분선 아래 빨간색으로 떼어 놓는다.
  const menuItems: ActionMenuItem[] = [
    ...(note.format === "markdown"
      ? [
          {
            key: "body-copy",
            label: copiedItem === "body" ? "✓ 복사했어요" : "본문 복사",
            icon: "copy" as const,
            className: "act-body-copy",
            onSelect: () => {
              void navigator.clipboard.writeText(
                stripMarkers(controller.readBuffer(key, live.body)),
              );
              setCopiedItem("body");
              window.setTimeout(() => setCopiedItem(null), 1500);
            },
          },
        ]
      : []),
    ...(!parent && state.workspace?.path
      ? [
          {
            key: "window-link",
            label: copiedItem === "link" ? "✓ 복사했어요" : "새 창 링크 복사",
            icon: "link" as const,
            className: "act-window-link",
            onSelect: () => {
              void navigator.clipboard.writeText(
                `aisurada://doc/${encodeURIComponent(note.id)}?ws=${encodeURIComponent(state.workspace!.path)}&window=1`,
              );
              setCopiedItem("link");
              window.setTimeout(() => setCopiedItem(null), 1500);
            },
          },
        ]
      : []),
    "separator" as const,
    {
      key: "delete",
      label: "삭제",
      icon: "trash" as const,
      danger: true,
      className: "act-delete",
      onSelect: () => setDeleting(true),
    },
  ];
  return (
    <>
      <header className="doc-meta-bar">
      <div className="doc-meta">
        <div className="doc-meta-left">
          <span className="doc-meta-time">
            <Icon name="clock" size={12} />
            {TYPE_NAMES[asText(note.type) || "free"] || asText(note.type)} ·{" "}
            {note.date} {note.time}
          </span>
          {Boolean(note.format && note.format !== "markdown") && (
            <span className="format-badge">{String(note.format).toUpperCase()}</span>
          )}
          {live.tier === "record" && <span className="record-badge">기록</span>}
          {!parent && (
            <button
              type="button"
              className="fav-toggle ico-btn"
              aria-label="즐겨찾기"
              data-tip="즐겨찾기"
              aria-pressed={live.favorite === true}
              onClick={() =>
                void controller.data.favorite(note.id, live.favorite !== true)
              }
            >
              <Icon name="star" />
            </button>
          )}
        </div>
        <div className="doc-meta-right">
          <button
            type="button"
            className="doc-meta-copy-id ico-btn has-text"
            aria-label={parent ? "경로 복사" : "ID 복사"}
            data-tip={
              copiedId ? "복사했어요" : (parent ? "경로 복사 · " : "ID 복사 · ") + (chain.length > 14 ? chain.slice(0, 10) + "…" : chain)
            }
            onClick={copyId}
          >
            <Icon name={copiedId ? "check" : "copy"} />
            <span>{parent ? "경로" : "ID"}</span>
          </button>
          {!parent && (
            <button
              type="button"
              className={`doc-eval-badge ico-btn${score ? " has-text" : ""}`}
              aria-label={score ? `평가 ${score}점` : "평가"}
              data-tip="평가"
              onClick={() => setEvaluating(!evaluating)}
            >
              <Icon name="evaluate" />
              {score && <span>{score}</span>}
            </button>
          )}
          {note.format === "markdown" && (
            <button
              type="button"
              className="doc-meta-history ico-btn"
              aria-label="이력"
              data-tip="이력"
              onClick={() => setHistory(true)}
            >
              <Icon name="history" />
            </button>
          )}
          {mode === "agenda" && (
            <button
              type="button"
              className="ico-btn"
              aria-label="탭으로 띄우기"
              data-tip="탭으로 띄우기"
              onClick={() => {
                go(controller, docAddress(note.id));
              }}
            >
              <Icon name="tab" />
            </button>
          )}
          {canPopOut && (
            <button
              type="button"
              className="pane-pop ico-btn"
              aria-label="새 창으로"
              data-tip="새 창으로"
              onClick={moveToWindow}
            >
              <Icon name="popout" />
            </button>
          )}
          <button
            type="button"
            ref={moreButton}
            className="doc-meta-more ico-btn"
            aria-label="더 보기"
            data-tip="더 보기"
            data-tip-align="end"
            aria-haspopup="menu"
            aria-expanded={menuOpen}
            onClick={() => setMenuOpen(!menuOpen)}
          >
            <Icon name="more" />
          </button>
          {onClose && (
            <>
              <i className="doc-meta-sep" aria-hidden="true" />
              <button
                type="button"
                className="doc-meta-close ico-btn"
                aria-label={closeLabel || "닫기"}
                data-tip="닫기"
                data-tip-align="end"
                onClick={onClose}
              >
                <Icon name="close" />
              </button>
            </>
          )}
        </div>
      </div>
      {(!parent && (agenda || !!note.template) || sessionChips(live).length > 0) && (
        <div className="doc-context-chips">
          {!parent &&
            agenda &&
            (mode === "preview" ? (
              <span className="doc-agenda-chip">{agenda.title}</span>
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
          <NoteSessionChips
            note={live}
            onOpenRail={CLAUDE_ENABLED ? (sessId) => {
              openNoteRail(note.id, sessId);
              if (mode === "agenda" || mode === "preview") {
                if (mode === "preview") controller.showPreview(null);
                go(controller, docAddress(note.id));
              }
            } : undefined}
            onOpenTerminal={CLAUDE_ENABLED && controller.isNative() ? (sessId) => {
              // 칩의 세션을 앱 터미널 탭에서 이어 연다. 이어 열기 명령은 세션을 연 CLI에 맞게 호스트가 고른다
              if (mode === "preview") controller.showPreview(null);
              go(controller, "/terminal/" + encodeURIComponent(sessId));
            } : undefined}
          />
        </div>
      )}
      {menuOpen && (
        <ActionMenu
          label="더 보기"
          items={menuItems}
          anchor={moreButton}
          onClose={closeMenu}
        />
      )}
    </header>
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
  const rangeRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    rangeRef.current?.focus();
  }, []);

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

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        e.stopImmediatePropagation?.();
        close();
      }
    };
    window.addEventListener("keydown", handleKeyDown, true);
    return () => window.removeEventListener("keydown", handleKeyDown, true);
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
        if (e.key === "Escape") {
          e.preventDefault();
          e.stopPropagation();
          close();
        }
        if (e.key === "Enter" && (e.metaKey || e.ctrlKey)) {
          e.preventDefault();
          save();
        }
      }}
    >
      <label>
        평가 {score}
        <input
          ref={rangeRef}
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
        <button className="doc-eval-cancel" onClick={close}>취소</button>
        <button
          className="doc-eval-clear"
          onClick={() => {
            void controller.data.evaluate(note.id, null);
            close();
          }}
        >
          평가 지우기
        </button>
        <button className="doc-eval-save" onClick={save}>저장</button>
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
    <section className="note-tags doc-tags">
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
        className="tag-input"
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
          if (e.key === "Backspace" && !value && own.length && !e.repeat) {
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
        <button
          className="dlp-toggle"
          onClick={() => setOpen(!open)}
          aria-expanded={open}
        >
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
            <div className="dlp-sugrow" key={r.note.id}>
              <span>{names[r.kind]}</span>{" "}
              <button onClick={() => openNote(r.note)}>
                {displayTitle(r.note, state.prefs.titleDate)}
              </button>
              <small>{r.reason}</small>
              <button
                className="dlp-connectbtn"
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
      setEdit("");
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
          <div className="comment-compose">
            <textarea
              className="comment-input"
              aria-label="새 코멘트"
              placeholder="코멘트를 남겨요"
              value={body}
              onChange={(e) => setBody(e.target.value)}
              onKeyDown={(e) => keyboard(e, send)}
            />
            <button className="btn-primary" disabled={!body.trim()} onClick={send}>
              추가
            </button>
          </div>
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

/** 노트의 sessionAgent 줄에서 읽은 CLI 이름. 줄이 없거나 claude면 null이다(Claude는 줄을 적지 않는 약속이다. 칩에는 그래도 이름을 보인다). */
function sessionAgentId(value: unknown): string | null {
  const agent = asText(value)?.trim();
  return agent && agent !== "claude" ? agent : null;
}

/** 칩에 보여 주는 CLI 이름. 모르는 CLI는 줄에 적힌 이름 그대로 보인다. */
function sessionAgentLabel(agent: string): string {
  if (agent === "claude") return "Claude Code";
  if (agent === "codex") return "Codex";
  if (agent === "gemini") return "Gemini CLI";
  return agent;
}

/** 노트 상단 정보줄 아래의 세션 칩. 시험에서도 그려 보려고 내보낸다. */
export function NoteSessionChips({
  note,
  onOpenRail,
  onOpenTerminal,
}: {
  note: Note;
  onOpenRail?: (sessionId: string) => void;
  onOpenTerminal?: (sessionId: string) => void;
}) {
  const createdSess = asText(note.sessionId)?.trim() || "";
  const editedSess = asText(note.editedSessionId)?.trim() || "";

  if (!createdSess && !editedSess) return null;

  type ChipInfo = {
    role: "one" | "created" | "edited";
    label: string;
    id: string;
    /** Claude가 아닌 CLI의 세션이면 그 id(codex, gemini …). Claude는 줄이 없어 null이고, 칩에는 「Claude Code」로 보인다. */
    agent: string | null;
  };

  const createdAgent = sessionAgentId(note.sessionAgent);
  const editedAgent = sessionAgentId(note.editedSessionAgent);
  const chips: ChipInfo[] = [];
  if (createdSess && editedSess && createdSess !== editedSess) {
    chips.push({ role: "created", label: "작성 세션", id: createdSess, agent: createdAgent });
    chips.push({ role: "edited", label: "편집 세션", id: editedSess, agent: editedAgent });
  } else {
    // 같은 세션이면 만든 쪽 기록을 우선한다. 만든 세션 줄이 없을 때만 편집 쪽을 본다
    const singleId = editedSess || createdSess;
    chips.push({ role: "one", label: "세션", id: singleId, agent: createdSess ? createdAgent : editedAgent });
  }

  return (
    <span className="doc-meta-sessions">
      {chips.map((chip) => (
        <SessionChipItem key={chip.role + chip.id} chip={chip} onOpenRail={onOpenRail} onOpenTerminal={onOpenTerminal} />
      ))}
    </span>
  );
}

function SessionChipItem({
  chip,
  onOpenRail,
  onOpenTerminal,
}: {
  chip: { role: string; label: string; id: string; agent: string | null };
  onOpenRail?: (sessionId: string) => void;
  onOpenTerminal?: (sessionId: string) => void;
}) {
  const [copied, setCopied] = useState(false);

  const handleCopy = () => {
    void navigator.clipboard.writeText(chip.id);
    setCopied(true);
    setTimeout(() => setCopied(false), 1600);
  };

  return (
    <span className="doc-meta-id" data-session-role={chip.role} data-session-agent={chip.agent ?? "claude"}>
      <span className="doc-meta-id-agent">{sessionAgentLabel(chip.agent ?? "claude")}</span>
      <span className="doc-meta-id-label">{chip.label}</span>
      <span className="doc-meta-id-val">{chip.id}</span>
      {onOpenTerminal && (
        <button
          type="button"
          className="doc-meta-id-term"
          title="앱 터미널 탭에서 이 세션을 이어 연다"
          onClick={() => onOpenTerminal(chip.id)}
        >
          터미널
        </button>
      )}
      {onOpenRail && (
        <button
          type="button"
          className="doc-meta-id-open"
          onClick={() => onOpenRail(chip.id)}
        >
          <svg
            viewBox="0 0 16 16"
            width="11"
            height="11"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
            aria-hidden="true"
          >
            <rect x="3" y="3" width="10" height="10" rx="1.5" />
            <path d="M3 6.5h10" />
          </svg>
          열기
        </button>
      )}
      <button
        type="button"
        className="doc-meta-id-copy"
        onClick={handleCopy}
      >
        <svg
          viewBox="0 0 16 16"
          width="11"
          height="11"
          fill="none"
          stroke="currentColor"
          strokeWidth="1.5"
          strokeLinecap="round"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          <rect x="5.5" y="5.5" width="7.5" height="7.5" rx="1.5" />
          <path d="M3.5 10.5V4a1.5 1.5 0 0 1 1.5-1.5h6.5" />
        </svg>
        {copied ? "복사됨" : "복사"}
      </button>
    </span>
  );
}
