import { useEffect, useMemo, useRef, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import { asRow, asText, rows } from "../workspace/types.ts";
import type { Row } from "../workspace/types.ts";
import { Modal } from "./Modal.tsx";
import { compareBodies } from "../editor/diff.ts";
import type { DiffRow } from "../editor/diff.ts";
import { stripMarkers } from "../editor/grammar.ts";
// 13.4: why the note's present text could not be recorded first, from the first
// commit-vault answer. The table is read top to bottom.
export function restoreReason(commit: { outcome?: unknown; blocked?: unknown } | null | undefined): string {
  if (!commit || commit.outcome === "failed") return "커밋이 실패했어요. 잠시 뒤 다시 해 주세요.";
  if (commit.outcome === "disabled") return "자동 커밋이 꺼져 있거나 vault가 git 저장소가 아니에요.";
  if (commit.blocked === "index-lock") return "vault의 git이 잠겨 있어요(.git/index.lock).";
  if (commit.blocked === "in-progress") return "vault에서 git 병합이나 리베이스가 진행 중이에요.";
  if (commit.blocked === "not-main") return "vault의 git이 main 브랜치에 있지 않아요.";
  return "vault에서 다른 git 작업이 돌고 있어요. 잠시 뒤 다시 해 주세요.";
}
export function DiffTable({
  left,
  right,
  fold = false,
}: {
  left: string;
  right: string;
  fold?: boolean;
}) {
  const diff = useMemo(() => compareBodies(left, right), [left, right]),
    [expanded, setExpanded] = useState<Set<number>>(new Set());
  useEffect(() => setExpanded(new Set()), [left, right]);
  const show = new Set<number>();
  diff.rows.forEach((r, i) => {
    if (r.kind !== "same")
      for (
        let j = Math.max(0, i - 3);
        j <= Math.min(diff.rows.length - 1, i + 3);
        j++
      )
        show.add(j);
  });
  const display: (
    | { row: DiffRow; index: number }
    | { skip: number; index: number }
  )[] = [];
  for (let i = 0; i < diff.rows.length; ) {
    if (!fold || show.has(i)) {
      display.push({ row: diff.rows[i], index: i });
      i++;
    } else {
      const start = i;
      while (i < diff.rows.length && !show.has(i)) i++;
      if (expanded.has(start))
        for (let j = start; j < i; j++)
          display.push({ row: diff.rows[j], index: j });
      else display.push({ skip: i - start, index: start });
    }
  }
  return (
    <>
      <p className="cdiff-count">
        왼쪽에만 {diff.deleted}줄, 오른쪽에만 {diff.added}줄 있어요.
        {diff.skipped
          ? ` 줄 ${diff.skipped}개는 너무 길어서 줄 안까지 갈라 보지 않았습니다.`
          : ""}
      </p>
      {left === right && <p className="cdiff-msg">두 글이 글자까지 같아요.</p>}
      <table className="cdiff-table">
        <thead>
          <tr>
            <th>왼쪽</th>
            <th>오른쪽</th>
            <th>표시</th>
            <th>본문</th>
          </tr>
        </thead>
        <tbody>
          {display.map((item) =>
            "skip" in item ? (
              <tr key={item.index}>
                <td colSpan={4}>
                  <button
                    className="cdiff-fold"
                    onClick={() =>
                      setExpanded(new Set([...expanded, item.index]))
                    }
                  >
                    같은 줄 {item.skip}개를 접었어요. 누르면 펼쳐요
                  </button>
                </td>
              </tr>
            ) : (
              <tr className={`cdiff-row-${item.row.kind}`} key={item.index}>
                <td>{item.row.left}</td>
                <td>{item.row.right}</td>
                <td>
                  {item.row.kind === "del"
                    ? "−"
                    : item.row.kind === "add"
                      ? "+"
                      : ""}
                </td>
                <td>
                  {item.row.pieces
                    ? item.row.pieces.map((p, i) => (
                        <span
                          key={i}
                          className={
                            p.changed ? "cdiff-word-change" : undefined
                          }
                        >
                          {p.text}
                        </span>
                      ))
                    : item.row.text || " "}
                </td>
              </tr>
            ),
          )}
        </tbody>
      </table>
    </>
  );
}
const versionTime = (value: unknown) => {
  const text = asText(value),
    d = new Date(text);
  if (!Number.isFinite(d.getTime())) return text;
  const n = (v: number) => String(v).padStart(2, "0");
  return `${d.getFullYear() === new Date().getFullYear() ? "" : d.getFullYear() + "-"}${n(d.getMonth() + 1)}-${n(d.getDate())} ${n(d.getHours())}:${n(d.getMinutes())}`;
};
export function History({
  id,
  keyValue,
  controller,
  close,
  fallback,
}: {
  id: string;
  keyValue: string;
  controller: WorkspaceController;
  close(): void;
  fallback: string;
}) {
  const [current, setCurrent] = useState(() =>
      controller.readBuffer(keyValue, fallback),
    ),
    [versions, setVersions] = useState<Row[] | null>(null),
    [error, setError] = useState(""),
    [selected, setSelected] = useState(""),
    [mode, setMode] = useState<"current" | "previous">("current"),
    [bodies, setBodies] = useState<Record<string, string>>({}),
    [bodyFailed, setBodyFailed] = useState(false),
    [query, setQuery] = useState(""),
    [matches, setMatches] = useState<string[] | null>(null),
    [searchNote, setSearchNote] = useState(""),
    [notice, setNotice] = useState(""),
    [busy, setBusy] = useState(false),
    [titles, setTitles] = useState<Record<string, string>>({});
  const cache = useRef(new Map<string, Promise<string>>()),
    live = useRef(true);
  const list = async () => {
    try {
      const value = asRow(await controller.callEngine(["history", id]));
      if (!live.current) return;
      const all = rows(value, "versions");
      setVersions(all);
      setSelected((old) =>
        all.some((v) => v.commit === old) ? old : asText(all[0]?.commit),
      );
      const sessions = [
        ...new Set(
          all
            .filter((v) => v.editedBy === "tool")
            .map((v) => asText(v.editedSessionId))
            .filter(Boolean),
        ),
      ];
      if (sessions.length)
        void controller.callEngine(["session-titles", ...sessions]).then(
          (value) => {
            if (live.current)
              setTitles(
                Object.fromEntries(
                  rows(value, "sessions").map((row) => [
                    asText(row.sessionId),
                    asText(row.title),
                  ]),
                ),
              );
          },
          () => {},
        );
    } catch (e) {
      if (live.current) setError("판 목록을 못 읽었어요. " + String(e));
    }
  };
  useEffect(() => {
    void list();
    return () => {
      live.current = false;
    };
  }, [id]);
  useEffect(() => {
    const term = query.trim();
    let active = true;
    const timer = setTimeout(() => {
      if (!term) {
        setMatches(null);
        setSearchNote("");
        return;
      }
      void controller.callEngine(["history", id, "query=" + term]).then(
        (value) => {
          if (!active) return;
          const r = asRow(value);
          const ids = Array.isArray(r?.matches)
            ? r.matches.map((v) =>
                typeof v === "string" ? v : asText(asRow(v)?.commit),
              )
            : [];
          setMatches(ids);
          setSearchNote(`판 ${ids.length}개에 있어요`);
        },
        () => {
          if (active)
            setSearchNote("찾기에 실패했어요. 사이드카가 떠 있는지 봐 주세요");
        },
      );
    }, 250);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [query, id]);
  const index = versions?.findIndex((v) => v.commit === selected) ?? -1,
    older = asText(versions?.[index + 1]?.commit);
  useEffect(() => {
    let active = true;
    setBodyFailed(false);
    for (const commit of [
      selected,
      ...(mode === "previous" && older ? [older] : []),
    ].filter(Boolean)) {
      let work = cache.current.get(commit);
      if (!work) {
        work = controller
          .callEngine(["history", id, "commit=" + commit])
          .then((value) => asText(asRow(value)?.noteBody));
        cache.current.set(commit, work);
      }
      void work.then(
        (body) => {
          if (active) setBodies((old) => ({ ...old, [commit]: body }));
        },
        () => {
          if (active) setBodyFailed(true);
        },
      );
    }
    return () => {
      active = false;
    };
  }, [selected, older, mode, id]);
  const ready =
    selected in bodies && (mode === "current" || !older || older in bodies);
  const restore = async () => {
    if (!ready) return;
    setBusy(true);
    const result = await controller.restoreNote(keyValue, bodies[selected]);
    if (result === "restored") {
      await controller.callEngine(["commit-vault"]).catch(() => {});
      setCurrent(controller.readBuffer(keyValue, fallback));
      await list();
      setNotice(
        `${versionTime(versions?.[index]?.takenAt)} 판으로 되돌렸어요. 되돌리기 전 글은 이력에 남아 있어서 다시 되돌릴 수 있어요.`,
      );
    } else {
      if (result.startsWith("replaced-"))
        setCurrent(controller.readBuffer(keyValue, fallback));
      const commit = controller.lastRestoreCommit;
      const reason = restoreReason(commit);
      setNotice(
        (
          {
            conflict: "바깥에서 이 노트를 고쳐서 되돌리지 못했어요.",
            unsaved: "글을 디스크에 쓰지 못해서 되돌리지 못했어요.",
            "not-recorded":
              "지금 글을 이력에 먼저 남기지 못해서 되돌리지 않았어요. " +
              reason,
            "replaced-unsaved":
              "편집기에는 되돌린 글을 넣었는데 아직 디스크에 못 썼어요.",
            "replaced-conflict":
              "편집기에는 되돌린 글을 넣었는데 그사이 바깥에서 이 노트를 고쳐 충돌이 섰어요.",
            unavailable: "이 노트는 편집기가 없어서 되돌릴 수 없어요.",
          } as Record<string, string>
        )[result] || result,
      );
    }
    setBusy(false);
  };
  return (
    <Modal title="노트 이력" close={close} wide className="nhist">
      {notice && (
        <p className="nhist-notice" role="status">
          {notice}
        </p>
      )}
      <div className="history-layout">
        <aside>
          <input
            className="nhist-search"
            aria-label="이력 찾기"
            placeholder="판 본문 찾기"
            value={query}
            onChange={(e) => setQuery(e.target.value)}
          />
          <p className="nhist-search-note">{searchNote}</p>
          <p>지금 글</p>
          {error ? (
            <p role="alert">{error}</p>
          ) : versions === null ? (
            <p>판 목록을 읽고 있어요.</p>
          ) : !versions.length ? (
            <p>
              이 노트의 판이 아직 없어요. 쓰기가 5분 멈추면 사이드카가 vault를
              커밋하고, 그때부터 판이 쌓여요.
            </p>
          ) : (
            versions
              .filter((v) => !matches || matches.includes(asText(v.commit)))
              .map((v) => (
                <div
                  className={`nhist-item ${v.commit === selected ? "active selected" : ""}`}
                  aria-selected={v.commit === selected}
                  aria-current={v.commit === selected ? "true" : undefined}
                  key={asText(v.commit)}
                  onClick={() => setSelected(asText(v.commit))}
                >
                  <button
                    className="nhist-item-btn"
                    aria-selected={v.commit === selected}
                    aria-current={v.commit === selected ? "true" : undefined}
                    onClick={() => setSelected(asText(v.commit))}
                  >
                    <strong>{versionTime(v.takenAt)}</strong>
                    <small>
                      {v.editedBy === "app"
                        ? "앱"
                        : v.editedBy === "tool"
                          ? `Claude 세션${titles[asText(v.editedSessionId)] ? " 「" + titles[asText(v.editedSessionId)] + "」" : ""}`
                          : "쓴 쪽 모름"}{" "}
                      ·{" "}
                      {v.source === "auto"
                        ? "자동 커밋"
                        : v.source === "manual"
                          ? "손 커밋"
                          : "덮기 직전"}
                    </small>
                  </button>
                </div>
              ))
          )}
        </aside>
        <section>
          <div className="nhist-modes">
            <button
              className="nhist-mode"
              aria-pressed={mode === "current"}
              onClick={() => setMode("current")}
            >
              지금 글과
            </button>
            <button
              className="nhist-mode"
              aria-pressed={mode === "previous"}
              onClick={() => setMode("previous")}
            >
              바로 앞 판과
            </button>
          </div>
          <p>
            본문만 비교하고 머리(편집 도장)는 뺐어요.
            {mode === "previous" && !older
              ? " 가장 오래된 판이라 빈 글과 비교해요."
              : ""}
          </p>
          <div className="nhist-diff">
            {bodyFailed ? (
              <p>이 판의 본문을 못 읽었어요.</p>
            ) : selected && !ready ? (
              <p>판의 본문을 읽고 있어요.</p>
            ) : ready ? (
              <DiffTable
                fold
                left={stripMarkers(
                  mode === "current"
                    ? bodies[selected]
                    : older
                      ? bodies[older]
                      : "",
                )}
                right={stripMarkers(
                  mode === "current" ? current : bodies[selected],
                )}
              />
            ) : null}
          </div>
        </section>
      </div>
      <footer>
        <button
          className="nhist-restore"
          disabled={busy || !ready}
          onClick={() => void restore()}
        >
          {busy ? "되돌리는 중…" : "이 판으로 되돌리기"}
        </button>
      </footer>
    </Modal>
  );
}
