import { useEffect, useRef, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { Note, Row, WorkspaceState } from "../workspace/types.ts";
import { asText } from "../workspace/types.ts";
import { Modal, Confirm } from "./Modal.tsx";
import {
  visibleNotes,
  displayTitle,
  tags,
  TYPE_NAMES,
  scheduleNotice,
} from "./model.ts";
import { Editor } from "../editor/Editor.tsx";
import { ReadMarkdown } from "./ReadMarkdown.tsx";
import { EMPTY_TEMPLATE } from "./Forms.tsx";
import { weekMonday } from "../navigation/routes.ts";
export function Page({
  title,
  children,
}: {
  title: string;
  children?: React.ReactNode;
}) {
  return (
    <section className="page-scroll">
      <div className="route-page">
        <h1>{title}</h1>
        {children}
      </div>
    </section>
  );
}
export function Favorites({
  state,
  controller,
}: {
  state: WorkspaceState;
  controller: WorkspaceController;
}) {
  const [query, setQuery] = useState(""),
    [sort, setSort] = useState("date"),
    [desc, setDesc] = useState(true),
    [group, setGroup] = useState("category");
  const all = visibleNotes(
      state.workspace!.notes,
      state.prefs.showRecords,
    ).filter((n) => n.favorite === true),
    filtered = all
      .filter((n) => n.title.toLowerCase().includes(query.trim().toLowerCase()))
      .sort(
        (a, b) =>
          (desc ? -1 : 1) *
          (sort === "title"
            ? a.title.localeCompare(b.title, "ko") || a.id.localeCompare(b.id)
            : a.created.localeCompare(b.created) || a.id.localeCompare(b.id)),
      );
  const names = [
    ...new Set(
      filtered.flatMap((n) =>
        group === "category"
          ? [asText(n.favoriteCategory) || "미분류"]
          : group === "tags"
            ? tags(n).length
              ? tags(n)
              : ["태그 없음"]
            : [""],
      ),
    ),
  ].sort((a, b) =>
    ["미분류", "태그 없음"].includes(a)
      ? 1
      : ["미분류", "태그 없음"].includes(b)
        ? -1
        : a.localeCompare(b, "ko"),
  );
  return (
    <Page title="즐겨찾기">
      {!all.length ? (
        <p>아직 즐겨찾기한 노트가 없어요.</p>
      ) : (
        <>
          <div className="list-tools">
            <input
              aria-label="즐겨찾기 제목 찾기"
              value={query}
              onChange={(e) => setQuery(e.target.value)}
            />
            <select
              aria-label="즐겨찾기 정렬"
              value={sort}
              onChange={(e) => setSort(e.target.value)}
            >
              <option value="date">날짜</option>
              <option value="title">제목</option>
            </select>
            <button onClick={() => setDesc(!desc)}>
              {desc ? "내림차순" : "오름차순"}
            </button>
            <select
              aria-label="즐겨찾기 묶음"
              value={group}
              onChange={(e) => setGroup(e.target.value)}
            >
              <option value="none">안 묶음</option>
              <option value="category">카테고리</option>
              <option value="tags">태그</option>
            </select>
          </div>
          {!filtered.length && <p>“{query}”와 맞는 즐겨찾기가 없어요.</p>}
          {names.map((name) => (
            <section key={name}>
              <h2>{name}</h2>
              {filtered
                .filter((n) =>
                  group === "none" || group === "category"
                    ? group === "none" ||
                      (asText(n.favoriteCategory) || "미분류") === name
                    : tags(n).length
                      ? tags(n).includes(name)
                      : name === "태그 없음",
                )
                .map((n) => (
                  <FavoriteRow
                    key={n.id}
                    note={n}
                    all={all}
                    state={state}
                    controller={controller}
                  />
                ))}
            </section>
          ))}
        </>
      )}
    </Page>
  );
}
function FavoriteRow({
  note,
  all,
  state,
  controller,
}: {
  note: Note;
  all: Note[];
  state: WorkspaceState;
  controller: WorkspaceController;
}) {
  const [value, setValue] = useState(asText(note.favoriteCategory)),
    [open, setOpen] = useState(false);
  const set = (label: string) => {
    setValue(label);
    setOpen(false);
    void controller.data.favorite(note.id, label);
  };
  return (
    <div className="fav-row">
      <button
        className="fav-open"
        onClick={() => controller.showPreview(note.id)}
      >
        {displayTitle(note, state.prefs.titleDate)}
      </button>
      <button onClick={() => setOpen(!open)}>
        {asText(note.favoriteCategory) || "카테고리 없음"}
      </button>
      <button
        aria-label={`${note.title} 즐겨찾기 해제`}
        onClick={() => void controller.data.favorite(note.id, false)}
      >
        ★
      </button>
      {open && (
        <div className="category-picker">
          <input
            aria-label="카테고리 이름"
            value={value}
            onChange={(e) => setValue(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Enter") set(value.trim());
            }}
          />
          <button onClick={() => set(value.trim())}>카테고리 추가</button>
          <button onClick={() => set("")}>카테고리 없음</button>
          {[
            ...new Set(
              all.map((n) => asText(n.favoriteCategory)).filter(Boolean),
            ),
          ].map((c) => (
            <button key={c} onClick={() => set(c)}>
              {c}
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
export function Connections({
  state,
  controller,
}: {
  state: WorkspaceState;
  controller: WorkspaceController;
}) {
  const [mode, setMode] = useState(() => {
      try {
        return localStorage.getItem("withlog.connMode.v3") === "list"
          ? "list"
          : "map";
      } catch {
        return "map";
      }
    }),
    [only, setOnly] = useState(() => {
      try {
        return localStorage.getItem("withlog.connFilter.v2") === "1";
      } catch {
        return false;
      }
    }),
    [query, setQuery] = useState(""),
    [selected, setSelected] = useState("");
  const all = visibleNotes(
      state.workspace!.notes,
      state.prefs.showRecords,
    ).filter((n) => !state.hiddenAgendas.includes(n.agendaId)),
    relations = state.vault.relations.filter(
      (r) => all.some((n) => n.id === r.from) && all.some((n) => n.id === r.to),
    ),
    notes = all.filter(
      (n) =>
        n.title.includes(mode === "map" ? query.trim() : query) &&
        (!only ||
          mode !== "list" ||
          relations.some((r) => r.from === n.id || r.to === n.id)),
    );
  useEffect(() => {
    if (selected && !notes.some((n) => n.id === selected)) setSelected("");
  }, [notes.map((n) => n.id).join("|"), selected]);
  const detail = notes.find((n) => n.id === selected);
  return (
    <Page title="연결">
      <div className="list-tools">
        {["map", "list"].map((m, i) => (
          <button
            aria-pressed={mode === m}
            key={m}
            onClick={() => {
              setMode(m);
              try {
                localStorage.setItem("withlog.connMode.v3", m);
              } catch {}
            }}
          >
            {["지도", "목록"][i]}
          </button>
        ))}
        <input
          aria-label="연결 찾기"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
        />
        {mode === "list" && (
          <label>
            <input
              type="checkbox"
              checked={only}
              onChange={(e) => {
                setOnly(e.target.checked);
                try {
                  localStorage.setItem(
                    "withlog.connFilter.v2",
                    e.target.checked ? "1" : "0",
                  );
                } catch {}
              }}
            />
            연결된 노트만
          </label>
        )}
        <button onClick={() => controller.filterAgendas([])}>모두 표시</button>
        {state.workspace!.agendas.map((a) => (
          <button
            aria-pressed={!state.hiddenAgendas.includes(a.id)}
            key={a.id}
            onClick={() =>
              controller.filterAgendas(
                state.hiddenAgendas.includes(a.id)
                  ? state.hiddenAgendas.filter((id) => id !== a.id)
                  : [...state.hiddenAgendas, a.id],
              )
            }
          >
            {a.title}
          </button>
        ))}
      </div>
      {mode === "list" ? (
        <div>
          {notes.map((n) => (
            <div className="connection-row" key={n.id}>
              <strong>{displayTitle(n, state.prefs.titleDate)}</strong>
              <span>
                {
                  relations.filter((r) => r.from === n.id || r.to === n.id)
                    .length
                }
                개 연결
              </span>
              <button
                className="cx-detail-open"
                onClick={() => controller.showPreview(n.id)}
              >
                문서 열기
              </button>
            </div>
          ))}
        </div>
      ) : (
        <div
          className="connection-map"
          onClick={(e) => {
            if (
              e.target === e.currentTarget ||
              e.target === e.currentTarget.querySelector("svg")
            )
              setSelected("");
          }}
          onKeyDown={(e) => {
            if (e.key === "Escape") {
              e.preventDefault();
              e.stopPropagation();
              setSelected("");
            }
          }}
        >
          <svg viewBox="0 0 900 500" role="img" aria-label="노트 연결 지도">
            {relations.map((r) => {
              const a = notes.findIndex((n) => n.id === r.from),
                b = notes.findIndex((n) => n.id === r.to);
              if (a < 0 || b < 0) return null;
              const p = (i: number) => ({
                x:
                  450 +
                  Math.cos((i / Math.max(1, notes.length)) * Math.PI * 2) * 320,
                y:
                  250 +
                  Math.sin((i / Math.max(1, notes.length)) * Math.PI * 2) * 180,
              });
              return (
                <line
                  key={r.from + r.to + r.kind}
                  x1={p(a).x}
                  y1={p(a).y}
                  x2={p(b).x}
                  y2={p(b).y}
                />
              );
            })}
            {notes.map((n, i) => {
              const x =
                  450 +
                  Math.cos((i / Math.max(1, notes.length)) * Math.PI * 2) * 320,
                y =
                  250 +
                  Math.sin((i / Math.max(1, notes.length)) * Math.PI * 2) * 180;
              return (
                <g
                  className="cg-node"
                  role="button"
                  tabIndex={0}
                  aria-label={n.title}
                  key={n.id}
                  transform={`translate(${x},${y})`}
                  onClick={() => setSelected(selected === n.id ? "" : n.id)}
                  onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                      e.preventDefault();
                      setSelected(selected === n.id ? "" : n.id);
                    }
                  }}
                >
                  <circle r={selected === n.id ? 14 : 10} />
                  <text x={16} y={4}>
                    {displayTitle(n, state.prefs.titleDate).slice(0, 24)}
                  </text>
                </g>
              );
            })}
          </svg>
          {detail && (
            <aside className="connection-detail">
              <h2>{displayTitle(detail, state.prefs.titleDate)}</h2>
              <p>
                {detail.date} · {detail.time}
              </p>
              <button
                className="cx-detail-open"
                onClick={() => controller.showPreview(detail.id)}
              >
                문서 열기
              </button>
            </aside>
          )}
        </div>
      )}
    </Page>
  );
}
export function Templates({
  state,
  controller,
  initialOpen,
}: {
  state: WorkspaceState;
  controller: WorkspaceController;
  initialOpen: string;
}) {
  const templates = state.workspace!.templates,
    [open, setOpen] = useState(() =>
      templates.some((t) => t.id === initialOpen)
        ? initialOpen
        : asText(templates[0]?.id),
    );
  useEffect(() => {
    if (!templates.some((t) => t.id === open))
      setOpen(asText(templates[0]?.id));
  }, [templates.map((t) => t.id).join("|")]);
  const selected = templates.find((t) => t.id === open);
  return (
    <Page title="템플릿">
      <button
        className="tpl-add"
        onClick={() =>
          void controller.data
            .saveTemplate({
              name: "새 템플릿",
              type: "plan",
              desc: "",
              body: EMPTY_TEMPLATE,
            })
            .then((r) => {
              if (r.ok) setOpen(asText(r.value?.id));
            })
        }
      >
        새 템플릿
      </button>
      <div className="templates-layout">
        <aside>
          {[
            [
              "기본 템플릿",
              templates.filter((t) => t.category !== "ritual" && t.builtin),
            ],
            [
              "데일리 리포트 템플릿",
              templates.filter((t) => t.category === "ritual"),
            ],
            [
              "내 템플릿",
              templates.filter((t) => t.category !== "ritual" && !t.builtin),
            ],
          ].map(([label, list]) => (
            <section key={String(label)}>
              <h2>
                {String(label)} {(list as Row[]).length}
              </h2>
              {(list as Row[]).map((t) => (
                <button
                  key={asText(t.id)}
                  aria-expanded={open === t.id}
                  onClick={() => setOpen(asText(t.id))}
                >
                  {asText(t.name)}
                </button>
              ))}
            </section>
          ))}
        </aside>
        {selected ? (
          <TemplateEditor
            key={open}
            template={selected}
            controller={controller}
            state={state}
          />
        ) : (
          <p>저장된 템플릿이 없어요.</p>
        )}
      </div>
    </Page>
  );
}
function TemplateEditor({
  template,
  controller,
  state,
}: {
  template: Row;
  controller: WorkspaceController;
  state: WorkspaceState;
}) {
  const fields = (t: Row) => ({
      name: asText(t.name),
      type: asText(t.type) || "plan",
      desc: asText(t.desc),
      body: asText(t.body),
    }),
    [value, setValue] = useState(() => fields(template)),
    [mirror, setMirror] = useState({
      revision: 0,
      body: asText(template.body),
      external: false,
    }),
    draft = useRef(value),
    last = useRef(JSON.stringify(fields(template))),
    dirty = useRef(false),
    timer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined),
    epoch = useRef(controller.data.epoch());
  draft.current = value;
  const send = () => {
    clearTimeout(timer.current);
    if (!dirty.current || epoch.current !== controller.data.epoch()) return;
    dirty.current = false;
    last.current = JSON.stringify(draft.current);
    const v = draft.current,
      input = {
        name: v.name || "제목 없는 템플릿",
        desc: v.desc,
        body: v.body,
      };
    void controller.data.saveTemplate(
      template.category === "ritual"
        ? input
        : { ...input, id: asText(template.id), type: v.type },
      template.category === "ritual" ? asText(template.id) : undefined,
    );
  };
  const update = (next: Partial<typeof value>) => {
    draft.current = { ...draft.current, ...next };
    setValue(draft.current);
    dirty.current = true;
    clearTimeout(timer.current);
    timer.current = setTimeout(send, 2000);
  };
  useEffect(() => () => send(), []);
  useEffect(() => {
    const next = fields(template),
      signature = JSON.stringify(next);
    if (signature === last.current) return;
    const external = !dirty.current;
    last.current = signature;
    dirty.current = false;
    clearTimeout(timer.current);
    setValue(next);
    draft.current = next;
    setMirror((old) => ({
      revision: old.revision + 1,
      body: next.body,
      external,
    }));
  }, [template]);
  return (
    <section className="template-editor">
      <header>
        <strong>{template.builtin ? "기본 · " : ""}자동 저장</strong>
        <code>{asText(template.id)}</code>
        <button
          onClick={() =>
            void navigator.clipboard.writeText(asText(template.id))
          }
        >
          복사
        </button>
        {!template.builtin && template.category !== "ritual" && (
          <button
            onClick={() => {
              clearTimeout(timer.current);
              dirty.current = false;
              void controller.data.deleteTemplate(asText(template.id));
            }}
          >
            삭제
          </button>
        )}
      </header>
      <label>
        이름
        <input
          className="tpl-name-input"
          value={value.name}
          onChange={(e) => update({ name: e.target.value })}
        />
      </label>
      {template.category !== "ritual" && (
        <label>
          유형
          <select
            value={value.type}
            onChange={(e) => update({ type: e.target.value })}
          >
            {Object.entries(TYPE_NAMES).map(([v, l]) => (
              <option key={v} value={v}>
                {l}
              </option>
            ))}
          </select>
        </label>
      )}
      <label>
        설명
        <textarea
          value={value.desc}
          onChange={(e) => update({ desc: e.target.value })}
        />
      </label>
      <Editor
        body={value.body}
        mirror={mirror}
        onChange={(body) => update({ body })}
        features={{
          notes: state.workspace!.notes,
          showRecords: state.prefs.showRecords,
        }}
      />
    </section>
  );
}
const shift = (date: string, n: number) => {
  const d = new Date(date + "T12:00:00Z");
  d.setUTCDate(d.getUTCDate() + n);
  return d.toISOString().slice(0, 10);
};
export function reportIds(date: string, notes: Note[], reports: Row[]) {
  const day = new Date(date + "T12:00:00Z").getUTCDay(),
    has = notes.some((n) => n.date === date),
    ids: string[] = [];
  for (const kind of ["morning", "evening"]) {
    const id = "rv-" + kind + "-" + date;
    if (reports.some((r) => r.id === id) || (has && day > 0 && day < 6))
      ids.push(id);
  }
  const weekly = "rv-weekly-" + date;
  if (
    day === 1 &&
    (reports.some((r) => r.id === weekly) ||
      notes.some((n) => n.date >= date && n.date < shift(date, 7)))
  )
    ids.push(weekly);
  return ids;
}
function reportBody(id: string, state: WorkspaceState) {
  const date = id.slice(-10),
    weekly = id.startsWith("rv-weekly"),
    kind = id.startsWith("rv-morning")
      ? "모닝 리포트"
      : weekly
        ? "주간 리포트"
        : "마무리 회고",
    notes = visibleNotes(
      state.workspace!.notes,
      state.prefs.showRecords,
    ).filter((n) =>
      weekly ? n.date >= date && n.date < shift(date, 7) : n.date === date,
    );
  return (
    `# ${date} ${kind}\n\n` +
    (notes.length
      ? notes
          .map(
            (n) =>
              `- [${displayTitle(n, state.prefs.titleDate)}](#doc:${n.id})`,
          )
          .join("\n")
      : "아직 이 기간의 기록이 없어요.")
  );
}
function ReportRead({
  body,
  controller,
}: {
  body: string;
  controller: WorkspaceController;
}) {
  return (
    <ReadMarkdown
      {...(/<\/?[a-z][^>]*>/i.test(body) ? { html: body } : { body })}
      onLink={(url) => {
        if (url.startsWith("#doc:")) controller.showPreview(url.slice(5));
      }}
    />
  );
}
export function Review({
  state,
  controller,
}: {
  state: WorkspaceState;
  controller: WorkspaceController;
}) {
  const [query, setQuery] = useState(""),
    [status, setStatus] = useState(""),
    [dismiss, setDismiss] = useState(false),
    warned = useRef(false);
  useEffect(() => {
    if (query || !controller.isNative()) return;
    let live = true;
    const load = () =>
      void controller.callEngine(["schedule", "status", "--json"]).then(
        (value) => {
          if (live) setStatus(scheduleNotice(value));
        },
        (error) => {
          if (!warned.current) {
            warned.current = true;
            console.warn("리포트 작업 상태를 읽지 못했어요", error);
          }
        },
      );
    const visible = () => {
      if (document.visibilityState === "visible") load();
    };
    load();
    addEventListener("focus", load);
    document.addEventListener("visibilitychange", visible);
    return () => {
      live = false;
      removeEventListener("focus", load);
      document.removeEventListener("visibilitychange", visible);
    };
  }, [query]);
  const today = state.workspace!.today;
  let previous = shift(today, -1);
  while ([0, 6].includes(new Date(previous + "T12:00:00Z").getUTCDay()))
    previous = shift(previous, -1);
  const main = [
      `rv-evening-${previous}`,
      `rv-morning-${today}`,
      `rv-weekly-${shift(weekMonday(today), -7)}`,
    ],
    ids = [
      ...new Set([
        ...main,
        ...state
          .workspace!.reports.map((r) => asText(r.id))
          .sort()
          .reverse(),
      ]),
    ].filter((id) => {
      const r = state.workspace!.reports.find((r) => r.id === id);
      return (reportBody(id, state) + asText(r?.body) + asText(r?.memo))
        .toLowerCase()
        .includes(query.toLowerCase());
    });
  return (
    <Page title="데일리 리포트">
      <input
        aria-label="리포트 검색"
        placeholder="기록 검색"
        value={query}
        onChange={(e) => setQuery(e.target.value)}
      />
      {status && !dismiss && (
        <p role="status">
          {status}
          <button onClick={() => setDismiss(true)}>닫기</button>
        </p>
      )}
      {ids.map((id) => (
        <ReportCard
          key={id}
          id={id}
          state={state}
          controller={controller}
          initiallyOpen={main.includes(id)}
        />
      ))}
    </Page>
  );
}
function ReportCard({
  id,
  state,
  controller,
  initiallyOpen,
}: {
  id: string;
  state: WorkspaceState;
  controller: WorkspaceController;
  initiallyOpen: boolean;
}) {
  const report = state.workspace!.reports.find((r) => r.id === id),
    body = asText(report?.body) || reportBody(id, state),
    [open, setOpen] = useState(initiallyOpen),
    [editing, setEditing] = useState(false),
    [initial, setInitial] = useState(""),
    [draft, setDraft] = useState("");
  return (
    <section className="report-row">
      <header>
        <button aria-expanded={open} onClick={() => setOpen(!open)}>
          {id}
        </button>
      </header>
      {open && (
        <>
          {editing ? (
            <>
              <Editor
                body={initial}
                onChange={setDraft}
                features={{
                  notes: state.workspace!.notes,
                  showRecords: state.prefs.showRecords,
                }}
              />
              <button onClick={() => setEditing(false)}>취소</button>
              <button
                onClick={() => {
                  controller.data.saveReport(id, draft, asText(report?.memo));
                  setEditing(false);
                }}
              >
                저장
              </button>
            </>
          ) : (
            <>
              <ReportRead body={body} controller={controller} />
              <button
                onClick={() => {
                  setInitial(body);
                  setDraft(body);
                  setEditing(true);
                }}
              >
                편집
              </button>
              <button
                onClick={() =>
                  controller.data.saveReport(id, "", asText(report?.memo))
                }
              >
                AI 원본
              </button>
            </>
          )}
          <label>
            메모
            <textarea
              aria-label={`${id} 메모`}
              value={asText(report?.memo)}
              onChange={(e) =>
                controller.data.saveReport(
                  id,
                  asText(report?.body),
                  e.target.value,
                )
              }
            />
          </label>
        </>
      )}
    </section>
  );
}
export function ReportDialog({
  id,
  state,
  controller,
  close,
}: {
  id: string;
  state: WorkspaceState;
  controller: WorkspaceController;
  close(): void;
}) {
  const report = state.workspace!.reports.find((r) => r.id === id);
  return (
    <Modal title={id} close={close} className="report-dialog">
      <ReportRead
        body={asText(report?.body) || reportBody(id, state)}
        controller={controller}
      />
      <footer>
        <button
          onClick={() => {
            close();
            controller.navigate("/review");
          }}
        >
          데일리 리포트로
        </button>
      </footer>
    </Modal>
  );
}
export function Trash({
  state,
  controller,
}: {
  state: WorkspaceState;
  controller: WorkspaceController;
}) {
  const [confirm, setConfirm] = useState<"empty" | string>("");
  const items = [...state.workspace!.trash].sort((a, b) =>
    asText(b.deletedAt || b.trashedAt).localeCompare(
      asText(a.deletedAt || a.trashedAt),
    ),
  );
  return (
    <Page title="휴지통">
      {!!items.length && (
        <button className="trash-empty-btn" onClick={() => setConfirm("empty")}>
          휴지통 비우기
        </button>
      )}
      {items.length ? (
        items.map((r, i) => {
          const id = asText(r.trashId),
            agenda =
              state.workspace!.agendas.find((a) => a.id === r.agendaId) ||
              state.workspace!.agendas.find((a) => a.slug === r.agenda);
          return (
            <div className="trash-row" key={id || i}>
              <strong className="trash-row-title">
                {displayTitle(r, state.prefs.titleDate)}
              </strong>
              <small>
                {agenda?.title || asText(r.agenda)} ·{" "}
                {TYPE_NAMES[asText(r.type)]} ·{" "}
                {asText(r.deletedAt || r.trashedAt)}
              </small>
              <button
                onClick={() => void controller.data.trashAction(id, "restore")}
              >
                복구
              </button>
              <button className="trash-purge" onClick={() => setConfirm(id)}>
                완전 삭제
              </button>
            </div>
          );
        })
      ) : (
        <p>휴지통이 비어 있어요.</p>
      )}
      {confirm && (
        <Confirm
          title={
            confirm === "empty" ? "휴지통을 비울까요?" : "완전히 삭제할까요?"
          }
          description="이 작업은 되돌릴 수 없어요."
          close={() => setConfirm("")}
          accept={async () => {
            if (confirm === "empty") await controller.data.emptyTrash();
            else await controller.data.trashAction(confirm, "purge");
          }}
        />
      )}
    </Page>
  );
}
export function Guide() {
  const examples = [
    [
      "1. CLI 스킬",
      `withlog의 노트는 wl-cli로 읽고 씁니다.\n목록: wl-cli list-docs limit=20\n읽기: wl-cli get-doc <노트 id>\n쓰기 전 토큰을 받고 f1=<precondition>을 함께 보냅니다.`,
    ],
    [
      "2. MCP 서버 설정 예시",
      JSON.stringify(
        {
          mcpServers: {
            withlog: {
              command: "node",
              args: ["~/withlog/dist-mcp/withlog-mcp.js"],
            },
          },
        },
        null,
        2,
      ),
    ],
    ["3. Claude에게 말하기", "withlog에서 오늘 작업한 노트를 찾아 요약해 줘."],
  ];
  return (
    <Page title="연동 가이드">
      {examples.map(([title, code]) => (
        <section className="guide-step" key={title}>
          <h2>{title}</h2>
          <pre>{code}</pre>
          <button onClick={() => void navigator.clipboard.writeText(code)}>
            복사
          </button>
        </section>
      ))}
    </Page>
  );
}
