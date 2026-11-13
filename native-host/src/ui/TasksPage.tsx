// 태스크 페이지: Jira처럼 보드(상태별 열)와 목록으로 본다. 태스크는 「만들기」 양식이나 보드의 한 줄 입력으로 시작하고,
// 상태를 바꾸는 규칙은 엔진이 지킨다(상세 페이지의 상태 메뉴). 채팅은 상세 페이지에서 필요할 때 연다(아직 없다).
import { useCallback, useEffect, useMemo, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { WorkspaceState } from "../workspace/types.ts";
import { asText } from "../workspace/types.ts";
import { taskAddress } from "../navigation/routes.ts";
import { Page } from "./OtherPages.tsx";
import { TaskFormModal } from "./TaskFormModal.tsx";
import { RunnerBar } from "./RunnerBar.tsx";
import { activeRunOf, runTask, stopTask, useRunner } from "./runnerStore.ts";
import type { RunnerSnapshot } from "./runnerStore.ts";
import {
  BOARD_COLUMNS, NO_FILTER, SHELF, TYPES, explainError, filterChoices, filterTasks, groupByStatus, isActiveFilter, keyNumber,
  listTaskTrash, listTasks, priorityLabel, repoChoices, restoreTask, statusTitle, taskMeta, typeLabel,
} from "./tasksClient.ts";
import type { TaskFilter, TaskRow, TaskStatus, TaskTrashRow, TaskType } from "./tasksClient.ts";

type View = "board" | "list";
const VIEW_KEY = "aisurada.tasks.view";

function readView(): View {
  try {
    return window.localStorage.getItem(VIEW_KEY) === "list" ? "list" : "board";
  } catch {
    return "board";
  }
}

/** 상태 알약. 색은 상태가 정한다(styles.css의 `.task-pill-*`). */
export function StatusPill({ status, label }: { status: TaskStatus; label?: string }) {
  return (
    <span className={`task-pill task-pill-${status}`}>
      <i aria-hidden="true" />
      {label ?? statusTitle(status)}
    </span>
  );
}

/** 유형 표식(기능, 버그, 잡무). */
export function TypeMark({ type }: { type: TaskType }) {
  return (
    <span className={`task-type task-type-${type}`} role="img" aria-label={typeLabel(type)} title={typeLabel(type)}>
      {type === "feature" ? "+" : type === "bug" ? "●" : "✓"}
    </span>
  );
}

/** 우선순위 화살표. */
export function PriorityMark({ priority }: { priority: TaskRow["priority"] }) {
  const mark = priority === "high" ? "↑" : priority === "low" ? "↓" : "=";
  return (
    <span className={`task-priority task-priority-${priority}`} role="img" aria-label={`우선순위 ${priorityLabel(priority)}`} title={`우선순위 ${priorityLabel(priority)}`}>
      {mark}
    </span>
  );
}

/** 시험과 눈으로 보는 확인에서 엔진 없이 처음 모양을 정할 때 쓴다(앱은 주지 않는다). */
export interface TasksPreset {
  tasks?: TaskRow[];
  trash?: TaskTrashRow[];
  view?: View;
  shelfOpen?: boolean;
  runner?: RunnerSnapshot;
}

export function TasksPage({ state, controller, preset }: { state: WorkspaceState; controller: WorkspaceController; preset?: TasksPreset }) {
  const call = controller.callEngine;
  const ws = state.workspace!;
  const [tasks, setTasks] = useState<TaskRow[] | null>(preset?.tasks ?? null);
  const [trash, setTrash] = useState<TaskTrashRow[]>(preset?.trash ?? []);
  const [skipped, setSkipped] = useState<string[]>([]);
  const [error, setError] = useState("");
  const [view, setViewState] = useState<View>(preset?.view ?? readView());
  const [filter, setFilter] = useState<TaskFilter>(NO_FILTER);
  const [creating, setCreating] = useState<{ title: string } | null>(null);
  const [shelfOpen, setShelfOpen] = useState(preset?.shelfOpen ?? false);
  const [quick, setQuick] = useState("");
  const live = useRunner();
  const runner = preset?.runner ?? live;
  const host = controller.callHost;

  const setView = (v: View) => {
    setViewState(v);
    try {
      window.localStorage.setItem(VIEW_KEY, v);
    } catch {
      // 저장하지 못해도 보기는 바뀐다
    }
  };

  const reload = useCallback(async () => {
    try {
      const [list, gone] = await Promise.all([listTasks(call), listTaskTrash(call)]);
      setTasks(list.tasks);
      setSkipped(list.skipped);
      setTrash(gone);
      setError("");
    } catch (e) {
      setError(explainError(e));
      setTasks((prev) => prev ?? []);
    }
  }, [call]);

  // 러너나 다른 창이 바꾼 것을 따라가도록 창이 다시 눈에 띌 때 다시 읽는다
  useEffect(() => {
    void reload();
    const onFocus = () => void reload();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [reload]);

  // 러너가 집거나 끝내면 호스트가 사건을 알린다. 그때마다 목록을 다시 읽어 상태 열이 따라가게 한다
  useEffect(() => {
    if (live.version > 0) void reload();
  }, [live.version, reload]);

  const all = tasks ?? [];
  const visible = useMemo(() => filterTasks(all, filter), [all, filter]);
  const grouped = useMemo(() => groupByStatus(visible), [visible]);
  const choices = useMemo(() => filterChoices(all), [all]);
  const repos = useMemo(() => repoChoices(ws.projects), [ws.projects]);
  const notes = useMemo(() => ws.notes.map((n) => ({ id: n.id, title: asText(n.title) })), [ws.notes]);
  const open = (key: string) => controller.navigate(taskAddress(key));
  const set = <K extends keyof TaskFilter>(key: K, value: TaskFilter[K]) => setFilter((f) => ({ ...f, [key]: value }));
  const shelfCount = SHELF.reduce((n, s) => n + grouped[s].length, 0);

  const card = (t: TaskRow) => (
    <li key={t.key} className={`task-card${t.blockedBy.length ? " is-blocked" : ""}`} data-task-key={t.key}>
      <button type="button" className="task-card-title" onClick={() => open(t.key)}>
        {t.title}
      </button>
      {t.labels.length > 0 && (
        <ul className="task-labels" aria-label="라벨">
          {t.labels.map((l) => (
            <li key={l}>{l}</li>
          ))}
        </ul>
      )}
      {t.blockedBy.length > 0 && <p className="task-blocked">{t.blockedBy.join(", ")}에 막혀 있어요</p>}
      {runner.supported && activeRunOf(runner, t.key) && (
        <span className="task-run-chip">
          <i aria-hidden="true" />
          러너 · {activeRunOf(runner, t.key)?.step || "시작"}
        </span>
      )}
      {runner.supported && t.status === "ready-for-develop" && t.blockedBy.length === 0 && !activeRunOf(runner, t.key) && (
        <button type="button" className="task-run" disabled={!runner.claudeAvailable} onClick={() => void runTask(host, t.key)}>
          ▶ 지금 실행
        </button>
      )}
      {runner.supported && t.status === "in-progress" && activeRunOf(runner, t.key) && (
        <button type="button" className="task-run" onClick={() => void stopTask(host, t.key)}>
          ■ 중지
        </button>
      )}
      <div className="task-card-foot">
        <TypeMark type={t.type} />
        <span className="task-key">{t.key}</span>
        {t.criteriaTotal > 0 && (
          <span className="task-progress" title={`완료 기준 ${t.criteriaDone}/${t.criteriaTotal}`}>
            {t.criteriaDone}/{t.criteriaTotal}
          </span>
        )}
        <span className="task-spacer" />
        <PriorityMark priority={t.priority} />
      </div>
    </li>
  );

  const column = (status: TaskStatus) => (
    <section key={status} className="task-col" aria-label={statusTitle(status)}>
      <header>
        <StatusPill status={status} />
        <span className="task-count">{grouped[status].length}</span>
      </header>
      <ul className="task-cards">{grouped[status].map(card)}</ul>
      {status === "draft" && (
        <form
          className="task-quick"
          onSubmit={(e) => {
            e.preventDefault();
            if (!quick.trim()) return;
            setCreating({ title: quick.trim() });
            setQuick("");
          }}
        >
          <input aria-label="요약만 쓰고 Enter" placeholder="요약을 쓰고 Enter" value={quick} onChange={(e) => setQuick(e.target.value)} />
        </form>
      )}
    </section>
  );

  const table = (rows: TaskRow[]) => (
    <table className="task-table">
      <thead>
        <tr>
          <th scope="col">유형</th>
          <th scope="col">키</th>
          <th scope="col">요약</th>
          <th scope="col">상태</th>
          <th scope="col">우선</th>
          <th scope="col">저장소</th>
          <th scope="col">연결·진행</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((t) => (
          <tr key={t.key} data-task-key={t.key}>
            <td>
              <TypeMark type={t.type} />
            </td>
            <td className="task-key">{t.key}</td>
            <td>
              <button type="button" className="task-card-title" onClick={() => open(t.key)}>
                {t.title}
              </button>
            </td>
            <td>
              <StatusPill status={t.status} />
            </td>
            <td>
              <PriorityMark priority={t.priority} />
            </td>
            <td>{t.repo ?? ""}</td>
            <td className="task-meta">{taskMeta(t)}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );

  return (
    <Page title="태스크" className="task-page">
      <p className="task-intro">
        일을 티켓으로 적어 두면 러너가 집어 claude에게 맡겨요. 지금 러너는 저장소와 노트를 읽기만 하고, 조사 결과를 댓글로 남기거나 사람에게만 풀 수 있는 질문을 해요. 코드를 고치고 PR을 올리는 일은 아직 없어요. 완료 기준은 만들 때 꼭 적어요.
      </p>
      <RunnerBar call={host} preset={preset?.runner} />
      <div className="task-toolbar">
        <div className="task-tabs" role="tablist" aria-label="보기">
          <button type="button" role="tab" aria-selected={view === "board"} onClick={() => setView("board")}>
            보드
          </button>
          <button type="button" role="tab" aria-selected={view === "list"} onClick={() => setView("list")}>
            목록
          </button>
        </div>
        <span className="task-spacer" />
        <button type="button" onClick={() => void reload()}>
          새로고침
        </button>
        <button type="button" className="button-primary" onClick={() => setCreating({ title: "" })}>
          + 만들기
        </button>
      </div>
      <div className="task-filters">
        <input
          type="search"
          aria-label="태스크 검색"
          placeholder="요약이나 키(TSK-14)로 찾기"
          value={filter.query}
          onChange={(e) => set("query", e.target.value)}
        />
        <select aria-label="저장소 거르기" value={filter.repo} onChange={(e) => set("repo", e.target.value)}>
          <option value="">모든 저장소</option>
          {choices.repos.map((r) => (
            <option key={r} value={r}>
              {r}
            </option>
          ))}
        </select>
        <select aria-label="유형 거르기" value={filter.type} onChange={(e) => set("type", e.target.value as TaskType | "")}>
          <option value="">모든 유형</option>
          {TYPES.map((t) => (
            <option key={t.id} value={t.id}>
              {t.label}
            </option>
          ))}
        </select>
        <select aria-label="라벨 거르기" value={filter.label} onChange={(e) => set("label", e.target.value)}>
          <option value="">모든 라벨</option>
          {choices.labels.map((l) => (
            <option key={l} value={l}>
              {l}
            </option>
          ))}
        </select>
        <label className="task-check">
          <input type="checkbox" checked={filter.blockedOnly} onChange={(e) => set("blockedOnly", e.target.checked)} />
          막힌 것만
        </label>
        {isActiveFilter(filter) && (
          <button type="button" onClick={() => setFilter(NO_FILTER)}>
            거르기 지우기
          </button>
        )}
      </div>
      {error && <p role="alert" className="task-error">{error}</p>}
      {skipped.length > 0 && (
        <details className="task-skipped">
          <summary>읽지 못한 파일 {skipped.length}개</summary>
          <ul>
            {skipped.map((s) => (
              <li key={s}>{s}</li>
            ))}
          </ul>
        </details>
      )}
      {tasks === null ? (
        <p className="task-empty">불러오는 중이에요…</p>
      ) : view === "board" ? (
        <>
          <div className="task-board">{BOARD_COLUMNS.map(column)}</div>
          <div className="task-shelf">
            <span className="task-hint">접어 둔 상태</span>
            {SHELF.map((s) => (
              <StatusPill key={s} status={s} label={`${statusTitle(s)} ${grouped[s].length}`} />
            ))}
            <button type="button" disabled={shelfCount === 0} onClick={() => setShelfOpen((o) => !o)}>
              {shelfOpen ? "접기" : "펼치기"}
            </button>
          </div>
          {shelfOpen && shelfCount > 0 && table(SHELF.flatMap((s) => grouped[s]).sort((a, b) => keyNumber(b.key) - keyNumber(a.key)))}
          {all.length === 0 && <p className="task-empty">아직 태스크가 없어요. 위의 「+ 만들기」나 Draft 열의 한 줄 입력으로 시작해요.</p>}
        </>
      ) : visible.length === 0 ? (
        <p className="task-empty">{all.length === 0 ? "아직 태스크가 없어요. 위의 「+ 만들기」로 시작해요." : "조건에 맞는 태스크가 없어요."}</p>
      ) : (
        table(visible)
      )}
      {trash.length > 0 && (
        <section className="task-trash" aria-label="휴지통">
          <h2>지운 것 {trash.length}</h2>
          <ul>
            {trash.map((t) => (
              <li key={t.id}>
                <span className="task-key">{t.key}</span> {t.title}
                <button
                  type="button"
                  onClick={async () => {
                    try {
                      await restoreTask(call, t.key);
                      await reload();
                    } catch (e) {
                      setError(explainError(e));
                    }
                  }}
                >
                  되살리기
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}
      <p className="task-hint task-legend">상태는 태스크를 열어 상태 메뉴에서 바꿔요.</p>
      {creating && (
        <TaskFormModal
          call={call}
          initialTitle={creating.title}
          repos={repos}
          notes={notes}
          close={() => setCreating(null)}
          saved={(task) => {
            setCreating(null);
            void reload();
            open(task.key);
          }}
        />
      )}
    </Page>
  );
}
