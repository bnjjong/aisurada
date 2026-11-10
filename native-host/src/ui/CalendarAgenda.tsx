import { useEffect, useRef, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { Note, WorkspaceState, Agenda } from "../workspace/types.ts";
import { asText } from "../workspace/types.ts";
import { calendarView, weekMonday } from "../navigation/routes.ts";
import {
  visibleNotes,
  TYPE_NAMES,
  TYPES,
  displayTitle,
  railAgendaOrder,
} from "./model.ts";
import { AgendaFilter } from "./Rail.tsx";
import { NoteReading } from "../navigation/NoteReading.tsx";
import { Confirm, Modal } from "./Modal.tsx";
import { Icon } from "./Icon.tsx";
import { DiffTable } from "./History.tsx";
export const addDays = (date: string, n: number) => {
  const d = new Date(date + "T12:00:00Z");
  d.setUTCDate(d.getUTCDate() + n);
  return d.toISOString().slice(0, 10);
};
export const minute = (time: string) =>
  Number(time.slice(0, 2)) * 60 + Number(time.slice(3, 5));
const clockAt = (m: number) =>
  String(Math.floor(m / 60)).padStart(2, "0") + ":" + String(m % 60).padStart(2, "0");
export const timeAt = (n: number) =>
  clockAt(Math.max(0, Math.min(1425, Math.round(Math.round(n / 15) * 15))));
// 12.4: arrows move exactly 15 minutes. A drag moves the dragged distance rounded to
// 15 minutes (14:37 dragged 30 minutes is 15:07). The start is held between 00:00 and
// 23:00. C193: a held value on the other side of the pressed direction is not written.
export const movedTime = (time: string, delta: number, snap: boolean): string | null => {
  const from = minute(time), top = 1380;
  const next = Math.max(0, Math.min(top, from + (snap ? Math.round(delta / 15) * 15 : delta)));
  return (delta > 0 && next < from) || (delta < 0 && next > from) ? null : clockAt(next);
};
// 1분당 px 배율(1.2px)과 일정 하나가 일 보기에서 자리를 잡는 분(60분 = 72px)
const SCALE = 1.2;
const SLOT = 60;
const CARD_HEIGHT = SLOT * SCALE; // 72px
export function lanes(notes: Note[]) {
  const ends: number[] = [],
    map = new Map<string, number>(),
    counts = new Map<string, number>();
  const sorted = [...notes].sort((a, b) => a.time.localeCompare(b.time));
  for (const n of sorted) {
    const start = minute(n.time),
      i = ends.findIndex((end) => end <= start);
    const lane = i < 0 ? ends.length : i;
    ends[lane] = start + SLOT;
    map.set(n.id, lane);
  }
  let cluster: Note[] = [],
    clusterEnd = -1;
  for (const n of sorted) {
    const start = minute(n.time);
    if (cluster.length > 0 && start >= clusterEnd) {
      const groupCount =
        Math.max(...cluster.map((x) => map.get(x.id) ?? 0)) + 1;
      for (const x of cluster) counts.set(x.id, groupCount);
      cluster = [];
    }
    cluster.push(n);
    clusterEnd = Math.max(clusterEnd, start + SLOT);
  }
  if (cluster.length > 0) {
    const groupCount =
      Math.max(...cluster.map((x) => map.get(x.id) ?? 0)) + 1;
    for (const x of cluster) counts.set(x.id, groupCount);
  }
  return { map, count: ends.length, counts };
}
export function canPlaceAt(
  note: Note,
  notes: Note[],
  layout: ReturnType<typeof lanes>,
  time: string,
) {
  const start = minute(time),
    end = start + SLOT;
  return !notes.some(
    (other) =>
      other.id !== note.id &&
      layout.map.get(other.id) === layout.map.get(note.id) &&
      start < minute(other.time) + SLOT &&
      minute(other.time) < end,
  );
}
export function Calendar({
  state,
  controller,
  query,
}: {
  state: WorkspaceState;
  controller: WorkspaceController;
  query: URLSearchParams;
}) {
  const ws = state.workspace!,
    view = state.tabs.calendar || calendarView(query, null, ws.today),
    invalid =
      !!query.get("mode") &&
      !["day", "week", "month"].includes(query.get("mode")!),
    mode = invalid ? "week" : view.mode,
    all = visibleNotes(ws.notes, state.prefs.showRecords).filter(
      (n) => !state.hiddenAgendas.includes(n.agendaId),
    ),
    [now, setNow] = useState(new Date()),
    [dragged, setDragged] = useState<{ id: string; delta: number } | null>(
      null,
    ),
    drag = useRef<{ id: string; y: number; moved: boolean } | null>(null);
  useEffect(() => {
    const timer = setInterval(() => setNow(new Date()), 60000);
    return () => clearInterval(timer);
  }, []);
  const route = (m: string, d = view.date, w = view.week) =>
    controller.navigate(`/calendar?mode=${m}&date=${d}&week=${w}`, true);
  const shift = (dir: number) => {
    if (mode === "month") {
      const d = new Date(view.date + "T12:00:00Z");
      d.setUTCDate(1);
      d.setUTCMonth(d.getUTCMonth() + dir);
      route(mode, d.toISOString().slice(0, 10));
    } else if (mode === "week")
      route(mode, view.date, addDays(view.week, dir * 7));
    else route(mode, addDays(view.date, dir));
  };
  const dayNotes = all.filter((n) => n.date === view.date),
    layout = lanes(dayNotes),
    start = Math.max(
      0,
      Math.min(8, ...dayNotes.map((n) => Math.floor(minute(n.time) / 60) - 1)),
    ),
    end = Math.min(
      24,
      Math.max(19, ...dayNotes.map((n) => Math.ceil(minute(n.time) / 60) + 2)),
    );
  const month = new Date(view.date + "T12:00:00Z");
  month.setUTCDate(1);
  const monthStart = month.toISOString().slice(0, 10),
    gridStart = weekMonday(monthStart);
  const saveTime = (note: Note, time: string) => {
    if (time !== note.time)
      void controller.data.updateNote({ id: note.id }, { time });
  };
  return (
    <section className="page-scroll">
      <div className="calendar-page">
        <h1>캘린더</h1>
        <div className="topbar calendar-controls">
          <div className="segment">
            {(["month", "week", "day"] as const).map((m, i) => (
              <button
                key={m}
                aria-pressed={!invalid && mode === m}
                onClick={() => route(m)}
              >
                {["월", "주", "일"][i]}
              </button>
            ))}
          </div>
          <button onClick={() => shift(-1)}>이전</button>
          <span>
            {mode === "week"
              ? view.week + " ~ " + addDays(view.week, 6)
              : mode === "month"
                ? view.date.slice(0, 7)
                : view.date}
          </span>
          <button onClick={() => shift(1)}>다음</button>
          <button onClick={() => route(mode, ws.today, weekMonday(ws.today))}>
            오늘
          </button>
          <input
            type="date"
            aria-label="기준 날짜"
            value={mode === "week" ? view.week : view.date}
            onChange={(e) => {
              if (e.target.value)
                route(mode, e.target.value, weekMonday(e.target.value));
            }}
          />
          <AgendaFilter state={state} controller={controller} />
          <button onClick={() => controller.form({ kind: "note" })}>
            새 문서
          </button>
        </div>
        {mode === "day" ? (
          <>
            {!dayNotes.length ? (
              <p>이 날의 기록이 없어요</p>
            ) : (
              <div
                className="day-timeline"
                style={{ height: (end - start) * CARD_HEIGHT }}
              >
                {Array.from({ length: end - start }, (_, i) => (
                  <div className="hour-line" style={{ top: i * CARD_HEIGHT }} key={i}>
                    <time>{String(start + i).padStart(2, "0")}:00</time>
                  </div>
                ))}
                {view.date === ws.today && (
                  <div
                    className="now-line"
                    style={{
                      top:
                        (now.getHours() * 60 + now.getMinutes() - start * 60) *
                        SCALE,
                    }}
                  >
                    지금
                  </div>
                )}
                {dayNotes.map((n) => {
                  const lane = layout.map.get(n.id) || 0;
                  const laneCount = Math.max(
                    1,
                    layout.counts?.get(n.id) || layout.count,
                  );
                  return (
                    <button
                      className="tl-block"
                      data-calendar-note={n.id}
                      key={n.id}
                      style={{
                        top: (minute(n.time) - start * 60) * SCALE,
                        left: `calc(55px + ${(lane * 90) / laneCount}%)`,
                        width: `calc(${90 / laneCount}% - 14px)`,
                        height: CARD_HEIGHT,
                        transform:
                          dragged?.id === n.id
                            ? `translateY(${dragged.delta}px)`
                            : undefined,
                      }}
                      onClick={(e) => {
                        if (e.detail === 0) controller.showPreview(n.id);
                      }}
                      onKeyDown={(e) => {
                        if (e.key === "Enter" || e.key === " ") {
                          e.preventDefault();
                          controller.showPreview(n.id);
                        } else if (e.key === "ArrowUp" || e.key === "ArrowDown") {
                          e.preventDefault();
                          const next = movedTime(
                            n.time,
                            e.key === "ArrowUp" ? -15 : 15,
                            false,
                          );
                          if (next) saveTime(n, next);
                        }
                      }}
                      onPointerDown={(e) => {
                        if (e.button === 0) {
                          drag.current = { id: n.id, y: e.clientY, moved: false };
                          if (e.nativeEvent.isTrusted)
                            e.currentTarget.setPointerCapture(e.pointerId);
                        }
                      }}
                      onPointerMove={(e) => {
                        const d = drag.current;
                        if (d?.id === n.id && Math.abs(e.clientY - d.y) > 4) {
                          d.moved = true;
                          setDragged({ id: n.id, delta: e.clientY - d.y });
                        }
                      }}
                      onPointerCancel={() => {
                        drag.current = null;
                        setDragged(null);
                      }}
                      onPointerUp={(e) => {
                        const d = drag.current;
                        if (!d || d.id !== n.id) return;
                        drag.current = null;
                        setDragged(null);
                        if (e.currentTarget.hasPointerCapture(e.pointerId))
                          e.currentTarget.releasePointerCapture(e.pointerId);
                        if (!d.moved) {
                          controller.showPreview(n.id);
                          return;
                        }
                        e.currentTarget.style.pointerEvents = "none";
                        const hit = document
                            .elementFromPoint(e.clientX, e.clientY)
                            ?.closest<HTMLElement>("[data-calendar-note]"),
                          other = dayNotes.find(
                            (x) =>
                              x.id === hit?.dataset.calendarNote && x.id !== n.id,
                          );
                        e.currentTarget.style.pointerEvents = "";
                        if (other) {
                          saveTime(n, other.time);
                          saveTime(other, n.time);
                        } else {
                          const next = movedTime(
                            n.time,
                            (e.clientY - d.y) / SCALE,
                            true,
                          );
                          if (next && canPlaceAt(n, dayNotes, layout, next))
                            saveTime(n, next);
                        }
                      }}
                    >
                      <time>{n.time}</time>
                      <strong>{n.title || "제목 없는 노트"}</strong>
                    </button>
                  );
                })}
              </div>
            )}
          </>
        ) : mode === "week" ? (
          <>
            <div className="week-layout-controls">
              <button
                aria-pressed={state.prefs.weekLayout === "agendas" || state.prefs.weekLayout === "swimlane"}
                onClick={() => controller.preference({ weekLayout: "swimlane" })}
              >
                아젠다 레인
              </button>
              <button
                aria-pressed={state.prefs.weekLayout === "dates" || state.prefs.weekLayout === "stack"}
                onClick={() => controller.preference({ weekLayout: "stack" })}
              >
                날짜 열
              </button>
            </div>
            {state.prefs.weekLayout === "dates" || state.prefs.weekLayout === "stack" ? (
              <div className="week-columns">
                {Array.from({ length: 7 }, (_, i) => addDays(view.week, i)).map(
                  (date) => (
                    <section key={date}>
                      <h3>{date.slice(5)}</h3>
                      {all
                        .filter((n) => n.date === date)
                        .map((n) => (
                          <button
                            className="mchip"
                            key={n.id}
                            onClick={() => controller.showPreview(n.id)}
                          >
                            {n.time} {n.title}
                          </button>
                        ))}
                    </section>
                  ),
                )}
              </div>
            ) : (
              <div className="week-agendas">
                {railAgendaOrder(ws.agendas, ws.notes, ws.projects)
                  .filter((a) =>
                    all.some(
                      (n) =>
                        n.agendaId === a.id &&
                        n.date >= view.week &&
                        n.date <= addDays(view.week, 6),
                    ),
                  )
                  .map((a) => (
                    <section key={a.id}>
                      <h3>{a.title}</h3>
                      {all
                        .filter(
                          (n) =>
                            n.agendaId === a.id &&
                            n.date >= view.week &&
                            n.date <= addDays(view.week, 6),
                        )
                        .map((n) => (
                          <button
                            className="mchip"
                            key={n.id}
                            onClick={() => controller.showPreview(n.id)}
                          >
                            {n.date.slice(5)} {n.time} {n.title}
                          </button>
                        ))}
                    </section>
                  ))}
                {!all.some(
                  (n) => n.date >= view.week && n.date <= addDays(view.week, 6),
                ) && <p>이번 주엔 기록된 아젠다가 없어요.</p>}
              </div>
            )}
          </>
        ) : (
          <>
            <div className="month-weekdays">
              {["월", "화", "수", "목", "금", "토", "일"].map((day) => (
                <span key={day}>{day}</span>
              ))}
            </div>
            <div className="month-grid">
              {Array.from({ length: 42 }, (_, i) => addDays(gridStart, i)).map(
                (date) => (
                  <MonthCell
                    key={date}
                    date={date}
                    current={date.startsWith(view.date.slice(0, 7))}
                    notes={all.filter((n) => n.date === date)}
                    open={(id) => controller.showPreview(id)}
                    day={() => route("day", date)}
                  />
                ),
              )}
            </div>
          </>
        )}
      </div>
    </section>
  );
}
function MonthCell({
  date,
  current,
  notes,
  open,
  day,
}: {
  date: string;
  current: boolean;
  notes: Note[];
  open(id: string): void;
  day(): void;
}) {
  const box = useRef<HTMLDivElement>(null),
    [limit, setLimit] = useState(3);
  useEffect(() => {
    if (!box.current) return;
    const observer = new ResizeObserver((entries) =>
      setLimit(
        Math.max(1, Math.floor((entries[0].contentRect.height - 60) / 25)),
      ),
    );
    observer.observe(box.current);
    return () => observer.disconnect();
  }, []);
  const shown = notes.length > limit ? Math.max(0, limit - 1) : limit;
  return (
    <div
      ref={box}
      className={`month-cell ${current ? "" : "outside-month"}`}
      onClick={() => {
        if (current) day();
      }}
    >
      <button
        className="mcell-date"
        disabled={!current}
        onClick={(e) => {
          e.stopPropagation();
          day();
        }}
      >
        {Number(date.slice(8))}
      </button>
      {current && (
        <>
          {notes.slice(0, shown).map((n) => (
            <button
              className="mchip"
              key={n.id}
              onClick={(e) => {
                e.stopPropagation();
                open(n.id);
              }}
            >
              {n.title}
            </button>
          ))}
          {notes.length > shown && (
            <button
              onClick={(e) => {
                e.stopPropagation();
                day();
              }}
            >
              +{notes.length - shown}건 더
            </button>
          )}
        </>
      )}
    </div>
  );
}
export function AgendaPage({
  state,
  controller,
  id,
  open,
}: {
  state: WorkspaceState;
  controller: WorkspaceController;
  id: string;
  open: string[];
}) {
  const ws = state.workspace!,
    agenda =
      ws.agendas.find((a) => a.id === id) ||
      ws.agendas.find((a) => a.slug === id),
    [deleting, setDeleting] = useState(false),
    [error, setError] = useState(""),
    [widths, setWidths] = useState<Record<string, number>>({});
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
    controller.navigate(
      "/agenda/" +
        encodeURIComponent(id) +
        (ids.length ? "?open=" + ids.map(encodeURIComponent).join(",") : ""),
      true,
    );
  if (!agenda)
    return (
      <section className="page-scroll">
        <h1>이 아젠다를 못 찾았어요</h1>
      </section>
    );
  const notes = visibleNotes(
      ws.notes.filter((n) => n.agendaId === agenda.id),
      state.prefs.showRecordTier ?? (state.prefs as any).showRecords ?? false,
    ),
    newestFirst = state.prefs.docSortDir === "desc" || Boolean((state.prefs as any).agendaDesc),
    sortMode: "structure" | "time" =
      (state.prefs.docSortMode || (state.prefs as any).agendaSort) === "time"
        ? "time"
        : "structure",
    sorted = [...notes].sort(
      (a, b) =>
        (newestFirst ? -1 : 1) *
        (a.created || "").localeCompare(b.created || ""),
    );
  const toggle = (nid: string) =>
    setOpen(
      open.includes(nid) ? open.filter((id) => id !== nid) : [...open, nid],
    );
  const row = (n: Note) => (
    <div className="doc-listitem" key={n.id}>
      <button onClick={() => toggle(n.id)}>
        <strong className="dli-title">
          {displayTitle(n, state.prefs.titleDatePrefix ?? (state.prefs as any).titleDate ?? true)}
        </strong>
        <small>
          {n.date} · {n.time}{" "}
          {Boolean(n.format && n.format !== "markdown") && String(n.format).toUpperCase()}
        </small>
      </button>
      <button
        className="fav-toggle"
        aria-pressed={n.favorite === true}
        aria-label={`${n.title} 즐겨찾기`}
        onClick={() => void controller.data.favorite(n.id, n.favorite !== true)}
      >
        {n.favorite ? "★" : "☆"}
      </button>
    </div>
  );
  return (
    <div className="agenda-route">
      <section className="agenda-list page-scroll">
        <div className="detail-head">
          <button
            type="button"
            className="ico-btn"
            aria-label="아젠다 목록으로"
            data-tip="아젠다 목록으로"
            data-tip-align="start"
            onClick={() => controller.navigate("/calendar")}
          >
            <Icon name="back" />
          </button>
          <h1 className="sr-only">{agenda.title}</h1>
          <h2>{agenda.title}</h2>
          <button
            type="button"
            className="ico-btn"
            aria-label="이슈 보기"
            data-tip="이슈 보기"
            disabled
          >
            <Icon name="issue" />
          </button>
          <button
            type="button"
            className="ico-btn ag-add button-primary"
            aria-label="문서 추가"
            data-tip="문서 추가"
            onClick={() => controller.form({ kind: "note", agenda: agenda.id })}
          >
            <Icon name="plus" />
          </button>
        </div>
        <p>{asText(agenda.repo)}</p>
        <p>{asText(agenda.summary)}</p>
        <div className="agenda-sort">
          <div className="ag-seg" role="group" aria-label="아젠다 정렬">
            {(
              [
                ["structure", "구조", "tree"],
                ["time", "시간순", "clock"],
              ] as const
            ).map(([mode, name, icon]) => (
              <button
                key={mode}
                type="button"
                aria-pressed={sortMode === mode}
                aria-label={name}
                data-tip={name}
                onClick={() => controller.preference({ docSortMode: mode })}
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
            onClick={() =>
              controller.preference({
                docSortDir: newestFirst ? "asc" : "desc",
              })
            }
          >
            <Icon name="sort" />
          </button>
          <span className="ag-sp" />
          <button
            type="button"
            className="ico-btn ag-ov-nav"
            aria-label="개요"
            data-tip="개요"
            data-tip-align="end"
            onClick={() => setOpen([])}
          >
            <Icon name="grid" />
          </button>
        </div>
        <div className="doclist">
          {sortMode === "structure"
            ? TYPES.map((type) => {
                const group = sorted.filter((n) => (n.type || "free") === type);
                return group.length ? (
                  <section key={type}>
                    <h3>
                      {TYPE_NAMES[type]} {group.length}
                    </h3>
                    {group.map(row)}
                  </section>
                ) : null;
              })
            : sorted.map(row)}
        </div>
        <div className="agenda-danger">
          <span>위험 구역</span>
          <button type="button" onClick={() => setDeleting(true)}>
            <Icon name="trash" />
            아젠다 삭제
          </button>
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
              onPointerDown={(e) => {
                e.preventDefault();
                const x = e.clientX,
                  start =
                    e.currentTarget.parentElement!.getBoundingClientRect()
                      .width;
                const move = (e: PointerEvent) =>
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
          <h2>{agenda.title}</h2>
          <p>
            {asText(agenda.repo)} · 노트 {notes.length}개
          </p>
          <p>{asText(agenda.summary)}</p>
          {!ws.sample && (
            <Facts agenda={agenda} state={state} controller={controller} />
          )}{" "}
          {!!notes.length && (
            <div className="note-composition">
              {TYPES.map((t) => (
                <span key={t}>
                  {TYPE_NAMES[t]} {notes.filter((n) => n.type === t).length}
                </span>
              ))}
            </div>
          )}
          <h3>히스토리</h3>
          {[...notes]
            .sort((a, b) => (a.created || "").localeCompare(b.created || ""))
            .map((n) => (
              <button
                className="agenda-history-note"
                key={n.id}
                onClick={() => toggle(n.id)}
              >
                {n.time} {displayTitle(n, state.prefs.titleDatePrefix ?? (state.prefs as any).titleDate ?? true)}
              </button>
            ))}
        </section>
      )}
      {deleting && (
        <Confirm
          title={
            notes.length
              ? "아젠다와 노트를 삭제할까요?"
              : "아젠다를 삭제할까요?"
          }
          description={`노트 ${ws.notes.filter((n) => n.agendaId === agenda.id).length}개를 휴지통으로 옮깁니다.`}
          close={() => setDeleting(false)}
          accept={async () => {
            const r = await controller.data.deleteEntity("agenda", agenda.id);
            if (r.ok) controller.navigate("/calendar", true);
            else setError(r.reason || "아젠다를 지우지 못했어요");
          }}
        />
      )}
      {error && (
        <Modal title="아젠다를 지우지 못했어요" close={() => setError("")}>
          <p>{error}</p>
        </Modal>
      )}
    </div>
  );
}
function Facts({
  agenda,
  state,
  controller,
}: {
  agenda: Agenda;
  state: WorkspaceState;
  controller: WorkspaceController;
}) {
  const facts = state.vault.facts[agenda.id],
    [editing, setEditing] = useState(false),
    [draft, setDraft] = useState(""),
    [base, setBase] = useState(""),
    [clash, setClash] = useState(false),
    [difference, setDifference] = useState(false),
    [busy, setBusy] = useState(false);
  useEffect(() => {
    void controller.data.loadFacts(agenda.id);
  }, [agenda.id]);
  const save = async () => {
    setBusy(true);
    const r = await controller.data.saveFacts(agenda.id, draft, base);
    setBusy(false);
    if (r.ok) {
      setEditing(false);
      setClash(false);
    } else if (r.conflict) {
      setBase(asText(r.value?.facts));
      setClash(true);
    }
  };
  return (
    <section className="agenda-facts ag-facts">
      <header className="ag-facts-head">
        <h3>사실층</h3>
        {facts && !facts.tooLargeToEdit && !editing && (
          <button
            className="btn"
            onClick={() => {
              setDraft(asText(facts.facts));
              setBase(asText(facts.facts));
              setEditing(true);
              setClash(false);
            }}
          >
            {facts.facts ? "고치기" : "적기"}
          </button>
        )}
      </header>
      {!facts ? (
        <p>사실층을 읽는 중이에요.</p>
      ) : facts.tooLargeToEdit ? (
        <p>
          이 파일이 너무 커서 앱에서 못 고쳐요({String(facts.editLimit)}자를
          넘었어요) {asText(facts.path)}
        </p>
      ) : editing ? (
        <>
          {clash && (
            <p role="alert">
              고치는 사이에 이 파일이 바깥에서 바뀌었어요. 아직 저장하지
              않았어요.{" "}
              <button
                className="ag-facts-diff-btn"
                onClick={() => setDifference(true)}
              >
                차이 보기
              </button>
            </p>
          )}
          <textarea
            className="ag-facts-area"
            value={draft}
            onChange={(e) => setDraft(e.target.value)}
          />
          <p>
            {draft.trim().length} / {String(facts.limit)}자
          </p>
          <div className="ag-facts-foot">
            <button onClick={() => setEditing(false)}>취소</button>
            <button
              className="btn-primary"
              disabled={busy || draft.trim().length > Number(facts.limit)}
              onClick={() => void save()}
            >
              {clash
                ? draft.trim()
                  ? "그래도 저장"
                  : "그래도 지우기"
                : draft.trim()
                  ? "저장"
                  : "사실층 지우기"}
            </button>
          </div>
        </>
      ) : facts.facts ? (
        <p className="facts-text">{asText(facts.facts)}</p>
      ) : (
        <p>아직 적은 사실이 없어요.</p>
      )}
      {difference && (
        <Modal title="사실층 차이" close={() => setDifference(false)} wide>
          <DiffTable left={draft.trim()} right={base} />
          <footer>
            <button onClick={() => setDifference(false)}>닫기</button>
            <button
              onClick={() => {
                setDifference(false);
                setEditing(false);
              }}
            >
              내 글 버리고 닫기
            </button>
            <button
              onClick={() => {
                setDifference(false);
                void save();
              }}
            >
              그래도 저장
            </button>
          </footer>
        </Modal>
      )}
    </section>
  );
}
