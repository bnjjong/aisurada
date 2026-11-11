import { useState, useEffect } from "react";
import type { PointerEvent } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { WorkspaceState, Note } from "../workspace/types.ts";
import { visibleNotes, displayTitle } from "./model.ts";
import { notesAddress } from "../navigation/routes.ts";
import { NoteReading } from "../navigation/NoteReading.tsx";
import { Icon } from "./Icon.tsx";

export function NotesPage({
  state,
  controller,
  open,
}: {
  state: WorkspaceState;
  controller: WorkspaceController;
  open: string[];
}) {
  const ws = state.workspace!;
  const [widths, setWidths] = useState<Record<string, number>>({});
  const [agendaFilter, setAgendaFilter] = useState<string>("all");
  const [query, setQuery] = useState<string>("");

  const selected = ws.notes.filter(
    (n) => open.includes(n.id) && !controller.isPoppedOut(n.id),
  );

  useEffect(() => {
    const floating = open.filter((id) => controller.isPoppedOut(id));
    if (!floating.length) return;
    floating.forEach(controller.focusNoteWindow);
    setOpen(open.filter((id) => !floating.includes(id)));
  }, [open.join(","), state.noteWindows]);

  const setOpen = (ids: string[]) =>
    controller.navigate(notesAddress(ids), true);

  const allVisible = visibleNotes(
    ws.notes,
    state.prefs.showRecordTier ?? (state.prefs as any).showRecords ?? false,
  );

  const filteredByAgenda =
    agendaFilter === "all"
      ? allVisible
      : allVisible.filter((n) => n.agendaId === agendaFilter);

  const filtered = query.trim()
    ? filteredByAgenda.filter((n) =>
        (n.title || "").toLowerCase().includes(query.trim().toLowerCase()),
      )
    : filteredByAgenda;

  const [sortMode, setSortMode] = useState<"time" | "structure">("time");
  const [newestFirst, setNewestFirst] = useState<boolean>(
    state.prefs.docSortDir !== "asc",
  );

  const sorted = [...filtered].sort((a, b) => {
    if (sortMode === "structure") {
      // 아젠다 이름 순 -> 날짜 순
      const agendaA = ws.agendas.find((ag) => ag.id === a.agendaId)?.title || "";
      const agendaB = ws.agendas.find((ag) => ag.id === b.agendaId)?.title || "";
      const agendaDiff = agendaA.localeCompare(agendaB, "ko");
      if (agendaDiff !== 0) return agendaDiff;
    }
    return (
      (newestFirst ? -1 : 1) * (a.created || "").localeCompare(b.created || "")
    );
  });

  const toggle = (nid: string, multi = false) => {
    if (multi) {
      setOpen(open.includes(nid) ? open.filter((id) => id !== nid) : [...open, nid]);
    } else {
      setOpen(open.includes(nid) && open.length === 1 ? [] : [nid]);
    }
  };

  const row = (n: Note) => {
    const noteAgenda = ws.agendas.find((a) => a.id === n.agendaId);
    const isOpen = open.includes(n.id);
    return (
      <div className={`doc-listitem ${isOpen ? "active" : ""}`} key={n.id}>
        <button
          onClick={(e) => toggle(n.id, e.metaKey || e.ctrlKey || e.shiftKey)}
          aria-current={isOpen ? "true" : undefined}
        >
          <strong className="dli-title">
            {displayTitle(
              n,
              state.prefs.titleDatePrefix ?? (state.prefs as any).titleDate ?? true,
            )}
          </strong>
          <small>
            {noteAgenda && (
              <span className="agenda-badge" title={noteAgenda.title}>
                {noteAgenda.title}
              </span>
            )}
            {n.date} · {n.time}{" "}
            {Boolean(n.format && n.format !== "markdown") &&
              String(n.format).toUpperCase()}
          </small>
        </button>
        <button
          className="fav-toggle"
          aria-pressed={n.favorite === true}
          aria-label={`${n.title} 즐겨찾기`}
          onClick={(e) => {
            e.stopPropagation();
            void controller.data.favorite(n.id, n.favorite !== true);
          }}
        >
          {n.favorite ? "★" : "☆"}
        </button>
      </div>
    );
  };

  return (
    <div className="agenda-route">
      <section className="agenda-list page-scroll">
        <div className="detail-head">
          <h1 className="sr-only">전체 노트</h1>
          <h2>전체 노트</h2>
          <button
            type="button"
            className="ico-btn ag-add button-primary"
            aria-label="새 노트"
            data-tip="새 노트"
            onClick={() =>
              controller.form({
                kind: "note",
                agenda: agendaFilter !== "all" ? agendaFilter : undefined,
              })
            }
          >
            <Icon name="plus" />
          </button>
        </div>

        <div className="notes-filters" style={{ margin: "14px 0 6px" }}>
          <select
            className="notes-agenda-select"
            aria-label="아젠다별 필터"
            value={agendaFilter}
            onChange={(e) => setAgendaFilter(e.target.value)}
            style={{
              width: "100%",
              padding: "7px 10px",
              borderRadius: "6px",
              border: "1px solid var(--line)",
              background: "var(--soft)",
              color: "var(--ink)",
              fontSize: "12px",
              marginBottom: "8px",
            }}
          >
            <option value="all">전체 아젠다 ({allVisible.length})</option>
            {ws.agendas.map((a) => {
              const count = allVisible.filter((n) => n.agendaId === a.id).length;
              return (
                <option key={a.id} value={a.id}>
                  {a.title} ({count})
                </option>
              );
            })}
          </select>

          <input
            type="search"
            aria-label="노트 제목 검색"
            placeholder="제목 검색..."
            value={query}
            onChange={(e) => setQuery(e.target.value)}
            style={{
              width: "100%",
              padding: "7px 10px",
              borderRadius: "6px",
              border: "1px solid var(--line)",
              background: "var(--soft)",
              color: "var(--ink)",
              fontSize: "12px",
              boxSizing: "border-box",
            }}
          />
        </div>

        <div className="agenda-sort">
          <div className="ag-seg" role="group" aria-label="노트 정렬">
            {(
              [
                ["time", "시간순", "clock"],
                ["structure", "구조순", "tree"],
              ] as const
            ).map(([mode, name, icon]) => (
              <button
                key={mode}
                type="button"
                aria-pressed={sortMode === mode}
                aria-label={name}
                data-tip={name}
                onClick={() => {
                  setSortMode(mode);
                  controller.preference({ docSortMode: mode });
                }}
              >
                <Icon name={icon} />
              </button>
            ))}
          </div>
          <button
            type="button"
            className={`ico-btn ag-dir${newestFirst ? "" : " is-flipped"}`}
            aria-label={newestFirst ? "최신이 위" : "오래된 것이 위"}
            data-tip={newestFirst ? "최신이 위" : "오래된 것이 위"}
            onClick={() => {
              const next = !newestFirst;
              setNewestFirst(next);
              controller.preference({
                docSortDir: next ? "desc" : "asc",
              });
            }}
          >
            <Icon name="sort" />
          </button>
          <span className="ag-sp" />
          <button
            type="button"
            className="ico-btn ag-ov-nav"
            aria-label="개요 보기"
            data-tip="개요 보기"
            data-tip-align="end"
            onClick={() => setOpen([])}
          >
            <Icon name="grid" />
          </button>
        </div>

        <div className="doclist">
          {sorted.length ? (
            sorted.map(row)
          ) : (
            <p style={{ color: "var(--muted)", fontSize: "12px", padding: "16px 0" }}>
              조건에 맞는 노트가 없어요.
            </p>
          )}
        </div>
      </section>

      {selected.length ? (
        selected.map((n) => (
          <section
            className="agenda-note page-scroll"
            key={n.id}
            style={{ flexBasis: widths[n.id] || 620 }}
          >
            <NoteReading
              note={n}
              controller={controller}
              mode="agenda"
              onClose={selected.length > 1 ? () => toggle(n.id) : undefined}
              closeLabel={`${n.title} 페인 닫기`}
            />
            <div
              className="pane-resize"
              role="separator"
              tabIndex={0}
              aria-orientation="vertical"
              aria-label={`${n.title} 패널 너비 조절`}
              onKeyDown={(e) => {
                if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
                  e.preventDefault();
                  const delta = e.key === "ArrowLeft" ? -24 : 24;
                  setWidths((old) => ({
                    ...old,
                    [n.id]: Math.max(320, (old[n.id] || 620) + delta),
                  }));
                }
              }}
              onDoubleClick={() =>
                setWidths(
                  Object.fromEntries(
                    selected.map((n) => [
                      n.id,
                      Math.max(
                        320,
                        (window.innerWidth - 520) / selected.length,
                      ),
                    ]),
                  ),
                )
              }
              onPointerDown={(e: PointerEvent<HTMLDivElement>) => {
                e.preventDefault();
                const x = e.clientX,
                  start =
                    e.currentTarget.parentElement!.getBoundingClientRect()
                      .width;
                const move = (e: globalThis.PointerEvent) =>
                  setWidths((old) => ({
                    ...old,
                    [n.id]: Math.max(320, start + e.clientX - x),
                  }));
                const end = () => {
                  document.removeEventListener("pointermove", move);
                  document.removeEventListener("pointerup", end);
                };
                document.addEventListener("pointermove", move);
                document.addEventListener("pointerup", end, { once: true });
              }}
            />
          </section>
        ))
      ) : (
        <section className="agenda-overview page-scroll">
          <h2>전체 노트</h2>
          <p style={{ color: "var(--muted)", fontSize: "13px", margin: "12px 0 24px" }}>
            총 <strong>{allVisible.length}</strong>개의 노트가 있습니다.
          </p>
          <div className="note-composition">
            {ws.agendas.map((a) => {
              const count = allVisible.filter((n) => n.agendaId === a.id).length;
              return count > 0 ? (
                <button
                  type="button"
                  key={a.id}
                  className="note-composition-chip"
                  onClick={() => setAgendaFilter(a.id)}
                  title={`${a.title} 아젠다 노트만 보기`}
                >
                  {a.title} <strong>{count}</strong>
                </button>
              ) : null;
            })}
          </div>
        </section>
      )}
    </div>
  );
}
