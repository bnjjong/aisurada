// 태스크 상세: Jira 이슈 화면 구조다. 가운데는 설명·완료 기준·연결·활동, 오른쪽은 세부 정보와 Ready For Develop 조건이다.
// 상태는 상태 메뉴에서 바꾼다. 메뉴에는 사람이 바꿀 수 있는 곳만 나오고, 조건이 모자란 곳은 까닭과 함께 막혀 보인다(엔진이 알려 준다).
// 채팅은 이 화면에서 필요할 때 여는 선택 기능인데 아직 없다(3단계).
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { WorkspaceState } from "../workspace/types.ts";
import { asText } from "../workspace/types.ts";
import { docAddress, projectAddress } from "../navigation/routes.ts";
import { Confirm, Modal } from "./Modal.tsx";
import { Icon } from "./Icon.tsx";
import { ReadMarkdown } from "./ReadMarkdown.tsx";
import { PriorityMark, StatusPill, TypeMark } from "./TasksPage.tsx";
import { TaskFormModal } from "./TaskFormModal.tsx";
import { editedLine } from "./resourcesClient.ts";
import {
  LINK_CHOICES, activityText, activityWho, addComment, checkedLine, deleteTask, explainError, getTask, githubUrl, linkCandidates, linkTasks, listTasks,
  priorityLabel, relationLabel, repoChoices, statusTitle, timeLabel, toggleCriterion, transitionTask, typeLabel,
} from "./tasksClient.ts";
import type { ActivityEvent, RelationType, TaskDetail, TaskRow, TaskStatus } from "./tasksClient.ts";

type ActivityTab = "all" | "comments" | "history";

/** 시험과 눈으로 보는 확인에서 엔진 없이 처음 모양을 정할 때 쓴다(앱은 주지 않는다). */
export interface TaskDetailPreset {
  task?: TaskDetail;
  tab?: ActivityTab;
  menuOpen?: boolean;
}

export function TaskDetailPage({
  taskKey,
  state,
  controller,
  preset,
}: {
  taskKey: string;
  state: WorkspaceState;
  controller: WorkspaceController;
  preset?: TaskDetailPreset;
}) {
  const call = controller.callEngine;
  const ws = state.workspace!;
  const [task, setTask] = useState<TaskDetail | null>(preset?.task ?? null);
  const [missing, setMissing] = useState(false);
  const [error, setError] = useState("");
  const [editing, setEditing] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [linking, setLinking] = useState(false);
  const [tab, setTab] = useState<ActivityTab>(preset?.tab ?? "all");
  const [comment, setComment] = useState("");
  const [busy, setBusy] = useState(false);

  const reload = useCallback(async () => {
    try {
      setTask(await getTask(call, taskKey));
      setMissing(false);
      setError("");
    } catch (e) {
      const message = explainError(e);
      if (/못 찾았다|모양이 아니다/.test(message)) setMissing(true);
      else setError(message);
    }
  }, [call, taskKey]);

  useEffect(() => {
    if (!preset?.task) setTask(null);
    void reload();
    const onFocus = () => void reload();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [reload]);

  const repos = useMemo(() => repoChoices(ws.projects), [ws.projects]);
  const notes = useMemo(() => ws.notes.map((n) => ({ id: n.id, title: asText(n.title) })), [ws.notes]);
  const back = () => controller.navigate("/tasks");

  /** 한 가지 일을 하고, 실패하면 까닭을 보이고 새로 읽는다. */
  const run = async (job: () => Promise<TaskDetail | void>) => {
    setBusy(true);
    try {
      const next = await job();
      if (next) setTask(next);
      else await reload();
      setError("");
    } catch (e) {
      setError(explainError(e));
      await reload();
    } finally {
      setBusy(false);
    }
  };

  if (missing) {
    return (
      <section className="page-scroll task-page">
        <div className="route-page task-page">
          <h1>태스크를 찾을 수 없어요</h1>
          <p className="task-empty">{taskKey}는 없거나 휴지통에 있어요.</p>
          <button type="button" onClick={back}>태스크 목록으로</button>
        </div>
      </section>
    );
  }
  if (!task) {
    return (
      <section className="page-scroll task-page">
        <div className="route-page task-page">
          <p className="task-empty">{error || "불러오는 중이에요…"}</p>
        </div>
      </section>
    );
  }

  const projectTitle = task.project ? asText(ws.projects.find((p) => p.slug === task.project)?.title) || task.project : "";
  const noteTitle = (id: string) => ws.notes.find((n) => n.id === id)?.title;
  const shown = task.activity.filter((e) => (tab === "all" ? true : tab === "comments" ? e.kind === "comment" : e.kind !== "comment")).slice().reverse();
  const edited = editedLine(task);

  return (
    <section className="page-scroll task-page task-detail">
      <div className="route-page task-page">
        <nav className="task-crumb" aria-label="위치">
          <button type="button" onClick={back}>
            <Icon name="back" /> 태스크
          </button>
          <span aria-hidden="true">›</span>
          <TypeMark type={task.type} />
          <strong>{task.key}</strong>
        </nav>
        <h1 className="task-title">{task.title}</h1>
        <div className="task-actions">
          <StatusMenu task={task} busy={busy} defaultOpen={preset?.menuOpen} onPick={(to) => void run(() => transitionTask(call, task, to))} />
          <button type="button" onClick={() => setEditing(true)}>
            편집
          </button>
          <button type="button" className="task-danger" onClick={() => setConfirmDelete(true)}>
            삭제
          </button>
        </div>
        {error && <p role="alert" className="task-error">{error}</p>}

        <div className="task-cols">
          <div className="task-main">
            <section className="task-sec" aria-label="설명">
              <h2>설명</h2>
              {task.body.trim() ? <ReadMarkdown body={task.body} external /> : <p className="task-empty">설명이 없어요. 「편집」에서 적을 수 있어요.</p>}
            </section>

            <section className="task-sec" aria-label="완료 기준">
              <h2>
                완료 기준 <span className="task-count">{task.criteriaDone}/{task.criteriaTotal}</span>
              </h2>
              <ul className="task-criteria-list">
                {task.criteria.map((c, i) => (
                  <li key={i} className={c.done ? "is-done" : ""}>
                    <label>
                      <input type="checkbox" checked={c.done} disabled={busy} onChange={() => void run(() => toggleCriterion(call, task, i))} />
                      <span>{c.text}</span>
                    </label>
                    {c.done && <small>{checkedLine(c)}</small>}
                  </li>
                ))}
              </ul>
              <p className="task-hint">만들 때 꼭 적는 기준이에요. 일이 끝났는지 이 항목으로 확인해요.</p>
            </section>

            <section className="task-sec" aria-label="연결된 태스크">
              <h2>
                연결된 태스크 <span className="task-count">{task.links.length}</span>
                <button type="button" onClick={() => setLinking(true)}>
                  <Icon name="link" /> 태스크 연결
                </button>
              </h2>
              {task.links.length === 0 ? (
                <p className="task-empty">연결이 없어요.</p>
              ) : (
                <ul className="task-links">
                  {task.links.map((l) => (
                    <li key={`${l.type}:${l.key}`}>
                      <span className="task-chip-plain">{relationLabel(l.type)}</span>
                      <button type="button" className="task-link-key" onClick={() => controller.navigate(`/task/${encodeURIComponent(l.key)}`)}>
                        {l.key}
                      </button>
                      <span className="task-link-title">{l.title}</span>
                      <StatusPill status={l.status} />
                      <button type="button" aria-label={`${l.key} 연결 풀기`} disabled={busy} onClick={() => void run(async () => { await linkTasks(call, task.key, l.key, l.type as RelationType, true); })}>
                        ×
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              <p className="task-hint">막혀 있는 태스크는 막은 태스크가 Done이 되기 전에는 러너가 집지 않아요.</p>
            </section>

            <section className="task-sec" aria-label="연결한 자원">
              <h2>
                연결한 자원 <span className="task-count">{task.refs.length}</span>
              </h2>
              {task.refs.length === 0 ? (
                <p className="task-empty">연결한 노트나 GitHub 글이 없어요. 「편집」에서 더할 수 있어요.</p>
              ) : (
                <ul className="task-links">
                  {task.refs.map((r) => (
                    <li key={`${r.kind}:${r.target}`}>
                      <span className="task-chip-plain">{r.kind === "note" ? "노트" : "GitHub"}</span>
                      {r.kind === "note" ? (
                        <button type="button" className="task-link-key" onClick={() => controller.navigate(docAddress(r.target))}>
                          {noteTitle(r.target) || r.title || r.target}
                        </button>
                      ) : (
                        <a className="task-link-key" href={githubUrl(r.target)} target="_blank" rel="noreferrer">
                          {r.target.replace("https://github.com/", "")}
                        </a>
                      )}
                    </li>
                  ))}
                </ul>
              )}
            </section>

            <section className="task-sec" aria-label="활동">
              <h2>활동</h2>
              <div className="task-tabs" role="tablist" aria-label="활동 종류">
                {(
                  [
                    ["all", "전체"],
                    ["comments", "댓글"],
                    ["history", "이력"],
                  ] as const
                ).map(([id, label]) => (
                  <button key={id} type="button" role="tab" aria-selected={tab === id} onClick={() => setTab(id)}>
                    {label}
                  </button>
                ))}
              </div>
              <form
                className="task-comment-form"
                onSubmit={(e) => {
                  e.preventDefault();
                  const body = comment.trim();
                  if (!body) return;
                  void run(async () => {
                    await addComment(call, task.key, body);
                    setComment("");
                  });
                }}
              >
                <input aria-label="댓글" placeholder="댓글 추가…" value={comment} onChange={(e) => setComment(e.target.value)} />
                <button type="submit" disabled={busy || !comment.trim()}>
                  남기기
                </button>
              </form>
              {shown.length === 0 ? (
                <p className="task-empty">아직 없어요.</p>
              ) : (
                <ol className="task-feed">
                  {shown.map((e, i) => (
                    <FeedItem key={`${e.at}-${i}`} event={e} />
                  ))}
                </ol>
              )}
            </section>
          </div>

          <aside className="task-side" aria-label="세부 정보">
            <section className="task-sec">
              <h2>세부 정보</h2>
              <dl className="task-dl">
                <dt>상태</dt>
                <dd>
                  <StatusPill status={task.status} />
                </dd>
                <dt>유형</dt>
                <dd>
                  <TypeMark type={task.type} /> {typeLabel(task.type)}
                </dd>
                <dt>우선순위</dt>
                <dd>
                  <PriorityMark priority={task.priority} /> {priorityLabel(task.priority)}
                </dd>
                <dt>저장소</dt>
                <dd>{task.repo ? <a href={`https://github.com/${task.repo}`} target="_blank" rel="noreferrer">{task.repo}</a> : "—"}</dd>
                {task.project && (
                  <>
                    <dt>프로젝트</dt>
                    <dd>
                      <button type="button" className="task-link-key" onClick={() => controller.navigate(projectAddress(task.project!))}>
                        {projectTitle}
                      </button>
                    </dd>
                  </>
                )}
                <dt>기준 브랜치</dt>
                <dd>{task.baseBranch ? <code>{task.baseBranch}</code> : "저장소 기본"}</dd>
                <dt>라벨</dt>
                <dd>
                  {task.labels.length ? (
                    <ul className="task-labels">
                      {task.labels.map((l) => (
                        <li key={l}>{l}</li>
                      ))}
                    </ul>
                  ) : (
                    "—"
                  )}
                </dd>
                <dt>만든 때</dt>
                <dd>{task.createdAt ? timeLabel(task.createdAt) : "—"}</dd>
                <dt>고친 때</dt>
                <dd>
                  {task.editedAt ? timeLabel(task.editedAt) : "—"}
                  {edited && <small className="task-edited">{edited}</small>}
                </dd>
              </dl>
            </section>
            <section className="task-sec">
              <h2>Ready For Develop 조건</h2>
              <ul className="task-ready">
                {task.readiness.map((r) => (
                  <li key={r.item} className={r.ok ? "is-ok" : "is-no"}>
                    <span aria-hidden="true">{r.ok ? "✓" : "○"}</span> {r.label}
                  </li>
                ))}
              </ul>
              <p className="task-hint">세 가지가 차야 상태 메뉴에서 Ready For Develop을 고를 수 있어요.</p>
            </section>
          </aside>
        </div>
      </div>

      {editing && (
        <TaskFormModal
          call={call}
          existing={task}
          repos={repos}
          notes={notes}
          close={() => setEditing(false)}
          saved={(next) => {
            setEditing(false);
            setTask(next);
            void reload();
          }}
        />
      )}
      {linking && (
        <LinkModal
          call={call}
          task={task}
          close={() => setLinking(false)}
          done={() => {
            setLinking(false);
            void reload();
          }}
        />
      )}
      {confirmDelete && (
        <Confirm
          title={`${task.key}을 지울까요?`}
          description="휴지통으로 옮겨요. 태스크 페이지의 「지운 것」에서 되살릴 수 있어요. 이 태스크가 막고 있던 태스크는 풀려요."
          label="휴지통으로"
          close={() => setConfirmDelete(false)}
          accept={async () => {
            try {
              await deleteTask(call, task.key);
              back();
            } catch (e) {
              setError(explainError(e));
            }
          }}
        />
      )}
    </section>
  );
}

/** 상태 메뉴. 사람이 바꿀 수 있는 곳만 나오고, 조건이 모자란 곳은 까닭과 함께 막혀 보인다. */
function StatusMenu({ task, busy, defaultOpen = false, onPick }: { task: TaskDetail; busy: boolean; defaultOpen?: boolean; onPick: (to: TaskStatus) => void }) {
  const [open, setOpen] = useState(defaultOpen);
  const box = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const outside = (e: MouseEvent) => {
      if (box.current && !box.current.contains(e.target as Node)) setOpen(false);
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") setOpen(false);
    };
    document.addEventListener("mousedown", outside);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("mousedown", outside);
      document.removeEventListener("keydown", key);
    };
  }, [open]);
  return (
    <div className="task-status-menu" ref={box}>
      <button type="button" className="task-status-button" aria-haspopup="menu" aria-expanded={open} disabled={busy} onClick={() => setOpen((o) => !o)}>
        <StatusPill status={task.status} />
        <span aria-hidden="true">▾</span>
      </button>
      {open && (
        <div className="task-menu" role="menu" aria-label="상태 바꾸기">
          <h3>상태 바꾸기</h3>
          {task.transitions.length === 0 ? (
            <p className="task-hint">사람이 바꿀 수 있는 곳이 없어요.</p>
          ) : (
            task.transitions.map((o) => (
              <button
                key={o.to}
                type="button"
                role="menuitem"
                className="task-menu-item"
                disabled={!o.ok}
                onClick={() => {
                  setOpen(false);
                  onPick(o.to);
                }}
              >
                <StatusPill status={o.to} />
                {!o.ok && o.reason && <small>{o.reason}</small>}
              </button>
            ))
          )}
        </div>
      )}
    </div>
  );
}

function FeedItem({ event }: { event: ActivityEvent }) {
  return (
    <li className={`task-feed-item task-feed-${event.kind}`}>
      <div>
        <strong>{activityWho(event)}</strong>가 {activityText(event)}
        {event.kind === "comment" && event.body && <p className="task-comment-body">{event.body}</p>}
      </div>
      <time dateTime={event.at}>{timeLabel(event.at)}</time>
    </li>
  );
}

/** 다른 태스크와 연결을 건다. 막고 있음·막혀 있음·관련·중복 가운데 이 태스크 기준으로 고른다. */
function LinkModal({ call, task, close, done }: { call: Parameters<typeof getTask>[0]; task: TaskDetail; close: () => void; done: () => void }) {
  const [rows, setRows] = useState<TaskRow[] | null>(null);
  const [relation, setRelation] = useState<RelationType>("blocked-by");
  const [other, setOther] = useState("");
  const [error, setError] = useState("");
  useEffect(() => {
    listTasks(call).then((r) => setRows(r.tasks), (e) => setError(explainError(e)));
  }, [call]);
  const candidates = rows ? linkCandidates(rows, task) : [];
  return (
    <Modal title={`${task.key}에 태스크 연결`} close={close} className="ndf task-link-form">
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          if (!other) return;
          try {
            await linkTasks(call, task.key, other, relation);
            done();
          } catch (err) {
            setError(explainError(err));
          }
        }}
      >
        <label>
          종류
          <select aria-label="연결 종류" value={relation} onChange={(e) => setRelation(e.target.value as RelationType)}>
            {LINK_CHOICES.map((c) => (
              <option key={c.id} value={c.id}>
                {c.label}
              </option>
            ))}
          </select>
        </label>
        <label>
          태스크
          <select aria-label="연결할 태스크" value={other} onChange={(e) => setOther(e.target.value)}>
            <option value="">{rows === null ? "불러오는 중…" : candidates.length ? "태스크를 골라 주세요" : "연결할 수 있는 태스크가 없어요"}</option>
            {candidates.map((t) => (
              <option key={t.key} value={t.key}>
                {t.key} · {t.title} ({statusTitle(t.status)})
              </option>
            ))}
          </select>
        </label>
        <p className="task-hint">「막혀 있음」은 고른 태스크가 Done이 되기 전에는 이 태스크를 러너가 집지 않는다는 뜻이에요.</p>
        {error && <p role="alert" className="task-error">{error}</p>}
        <footer>
          <button type="button" onClick={close}>
            취소
          </button>
          <button type="submit" className="button-primary" disabled={!other}>
            연결
          </button>
        </footer>
      </form>
    </Modal>
  );
}
