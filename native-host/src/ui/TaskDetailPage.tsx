// 태스크 상세: Jira 이슈 화면 구조다. 가운데는 설명·완료 기준·연결·활동, 오른쪽은 세부 정보와 Ready For Develop 조건이다.
// 상태는 상태 메뉴에서 바꾼다. 메뉴에는 사람이 바꿀 수 있는 곳만 나오고, 조건이 모자란 곳은 까닭과 함께 막혀 보인다(엔진이 알려 준다).
// 러너(읽기 전용)의 실행은 「실행」 영역에서 보고 시작하고 멈추고 질문에 답한다. 채팅은 이 화면에서 필요할 때 여는 선택 기능인데 아직 없다(3b).
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { usePopoverDismiss } from "./usePopoverDismiss.ts";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { WorkspaceState } from "../workspace/types.ts";
import { asText } from "../workspace/types.ts";
import { docAddress, projectAddress } from "../navigation/routes.ts";
import { TaskReportSources } from "./ManagerIntake.tsx";
import { Confirm, Modal } from "./Modal.tsx";
import { Icon } from "./Icon.tsx";
import { ReadMarkdown } from "./ReadMarkdown.tsx";
import { PriorityMark, StatusPill, TypeMark } from "./TasksPage.tsx";
import { TaskFormModal } from "./TaskFormModal.tsx";
import { RunnerPanel } from "./RunnerPanel.tsx";
import type { RunnerPanelPreset } from "./RunnerPanel.tsx";
import { useRunner } from "./runnerStore.ts";
import { editedLine } from "./resourcesClient.ts";
import {
  LINK_CHOICES, activityText, activityWho, addComment, checkedLine, deleteTask, explainError, formFromTask, getTask, githubUrl, linkCandidates, linkTasks, listTasks,
  parseRefInput, priorityLabel, relationLabel, repoChoices, statusTitle, timeLabel, toggleCriterion, transitionTask, typeLabel, updateTask,
} from "./tasksClient.ts";
import type { ActivityEvent, RelationType, RepoChoice, TaskDetail, TaskForm, TaskPriority, TaskRow, TaskStatus, TaskType } from "./tasksClient.ts";
import { listGithubRepos, useGithub } from "./githubStore.ts";
import type { GithubRepo } from "./githubStore.ts";

type ActivityTab = "all" | "comments" | "history";

/** 시험과 눈으로 보는 확인에서 엔진 없이 처음 모양을 정할 때 쓴다(앱은 주지 않는다). */
export interface TaskDetailPreset {
  task?: TaskDetail;
  tab?: ActivityTab;
  menuOpen?: boolean;
  runner?: RunnerPanelPreset;
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
  const [editingTitle, setEditingTitle] = useState(false);
  const [tempTitle, setTempTitle] = useState("");
  const [editingBody, setEditingBody] = useState(false);
  const [tempBody, setTempBody] = useState("");
  const [addingRef, setAddingRef] = useState(false);
  const [selectedNoteId, setSelectedNoteId] = useState("");
  const [refInput, setRefInput] = useState("");
  const [refError, setRefError] = useState("");
  const [sendingStatus, setSendingStatus] = useState(false);
  const [statusMessage, setStatusMessage] = useState("");
  const runner = useRunner();
  const github = useGithub();
  const [githubRepos, setGithubRepos] = useState<GithubRepo[]>([]);

  useEffect(() => {
    if (github.status?.connected) {
      listGithubRepos(controller.callHost).then(
        (list) => setGithubRepos(list.repositories),
        () => {}
      );
    }
  }, [github.status?.connected, controller]);

  const runnerVersion = runner.version;

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

  // 러너가 이 태스크의 상태를 바꾸면(질문, 보고, 실패) 호스트가 사건을 알린다. 그때 다시 읽는다
  useEffect(() => {
    if (runnerVersion > 0) void reload();
  }, [runnerVersion, reload]);

  const repos = useMemo(() => repoChoices(ws.projects), [ws.projects]);
  const notes = useMemo(() => ws.notes.map((n) => ({ id: n.id, title: asText(n.title) })), [ws.notes]);
  const back = () => controller.navigate("/tasks");

  /** 한 가지 일을 하고, 실패하면 까닭을 보이고 새로 읽는다. */
  const run = async (job: () => Promise<TaskDetail | void>) => {
    setBusy(true);
    try {
      const next = await job();
      if (next) setTask(current => current?.id === next.id && !next.intakeSources
        ? { ...next, intakeSources: current.intakeSources } : next);
      else await reload();
      setError("");
      return true;
    } catch (e) {
      setError(explainError(e));
      await reload();
      return false;
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

  const defaultBranch = task.repo ? (githubRepos.find((r) => r.fullName === task.repo)?.defaultBranch || "main") : "main";
  const branchChoices = ["main", "develop", "master"];
  if (defaultBranch && !branchChoices.includes(defaultBranch)) branchChoices.unshift(defaultBranch);
  if (task.baseBranch && !branchChoices.includes(task.baseBranch)) branchChoices.push(task.baseBranch);

  const saveField = async (patch: Partial<TaskForm>) => {
    if (!task) return;
    const form = { ...formFromTask(task), ...patch };
    return run(() => updateTask(call, task, form));
  };

  const slackSource = task.refs.find((ref) => ref.kind === "slack");
  const firstReport = task.intakeSources?.find(source => source.taskOrigin === "created" && source.manager === task.manager);
  const statusDestination = firstReport?.source ? firstReport.source.kind === "telegram" && !!firstReport.destination
    : !!(firstReport?.url || slackSource);
  const openCommentLink = (url: string) => {
    if (/^\/doc\/[^/?#]+$/.test(url)) controller.navigate(url);
    else if (controller.isNative() && /^https?:\/\//i.test(url)) void openUrl(url).catch(() => {});
  };
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
        {editingTitle ? (
          <form
            className="task-inline-title-form"
            onSubmit={(e) => {
              e.preventDefault();
              const t = tempTitle.trim();
              if (t && t !== task.title) void saveField({ title: t });
              setEditingTitle(false);
            }}
          >
            <input
              ref={(el) => el?.focus()}
              className="task-inline-title-input"
              value={tempTitle}
              onChange={(e) => setTempTitle(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Escape") setEditingTitle(false);
              }}
              onBlur={() => {
                const t = tempTitle.trim();
                if (t && t !== task.title) void saveField({ title: t });
                setEditingTitle(false);
              }}
              disabled={busy}
            />
          </form>
        ) : (
          <h1
            className="task-title task-editable"
            onClick={() => {
              setTempTitle(task.title);
              setEditingTitle(true);
            }}
            title="클릭하여 제목 수정"
          >{task.title}</h1>
        )}
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
            <RunnerPanel task={task} controller={controller} preset={preset?.runner} />
            <section className="task-sec" aria-label="설명">
              <h2>
                설명
                {!editingBody && (
                  <button
                    type="button"
                    className="task-inline-btn"
                    onClick={() => {
                      setTempBody(task.body);
                      setEditingBody(true);
                    }}
                  >
                    <Icon name="edit" /> 편집
                  </button>
                )}
              </h2>
              {editingBody ? (
                <div className="task-inline-body-form">
                  <textarea
                    ref={(el) => el?.focus()}
                    className="task-inline-textarea"
                    value={tempBody}
                    onChange={(e) => setTempBody(e.target.value)}
                    placeholder="태스크 설명(마크다운)…"
                    rows={6}
                    onKeyDown={(e) => {
                      if ((e.metaKey || e.ctrlKey) && e.key === "Enter") {
                        e.preventDefault();
                        if (tempBody !== task.body) void saveField({ body: tempBody });
                        setEditingBody(false);
                      } else if (e.key === "Escape") {
                        setEditingBody(false);
                      }
                    }}
                    disabled={busy}
                  />
                  <div className="task-inline-actions">
                    <button
                      type="button"
                      className="task-btn-primary"
                      onClick={() => {
                        if (tempBody !== task.body) void saveField({ body: tempBody });
                        setEditingBody(false);
                      }}
                      disabled={busy}
                    >
                      저장
                    </button>
                    <button
                      type="button"
                      onClick={() => setEditingBody(false)}
                      disabled={busy}
                    >
                      취소
                    </button>
                    <small className="task-hint-keys">⌘+Enter로 저장, Esc로 취소</small>
                  </div>
                </div>
              ) : (
                <div
                  className="task-body-preview task-editable"
                  onClick={() => {
                    setTempBody(task.body);
                    setEditingBody(true);
                  }}
                  title="클릭하여 설명 편집"
                >
                  {task.body.trim() ? (
                    <ReadMarkdown body={task.body} external />
                  ) : (
                    <p className="task-empty task-clickable">설명이 없어요. 클릭하여 적을 수 있어요.</p>
                  )}
                </div>
              )}
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
                <button type="button" onClick={() => setAddingRef((v) => !v)}>
                  <Icon name={addingRef ? "close" : "plus"} /> {addingRef ? "닫기" : "자원 추가"}
                </button>
              </h2>
              {task.refs.length === 0 ? (
                <p className="task-empty">
                  연결한 노트나 GitHub 글이 없어요.{" "}
                  {!addingRef && (
                    <button type="button" className="task-inline-link-btn" onClick={() => setAddingRef(true)}>
                      여기서 바로 추가할 수 있어요.
                    </button>
                  )}
                </p>
              ) : (
                <ul className="task-links">
                  {task.refs.map((r) => (
                    <li key={`${r.kind}:${r.target}`}>
                      <span className="task-chip-plain">{r.kind === "note" ? "노트" : r.kind === "slack" ? "Slack" : "GitHub"}</span>
                      {r.kind === "note" ? (
                        <button type="button" className="task-link-key" onClick={() => controller.navigate(docAddress(r.target))}>
                          {noteTitle(r.target) || r.title || r.target}
                        </button>
                      ) : (
                        <a className="task-link-key" href={r.kind === "slack" ? r.target : githubUrl(r.target)} target="_blank" rel="noreferrer">
                          {r.target.replace("https://github.com/", "")}
                        </a>
                      )}
                      <button
                        type="button"
                        aria-label={`${r.title || r.target} 연결 풀기`}
                        disabled={busy}
                        onClick={() => void saveField({ refs: task.refs.filter((x) => x !== r) })}
                      >
                        ×
                      </button>
                    </li>
                  ))}
                </ul>
              )}
              {addingRef && (
                <div className="task-ref-inline-box">
                  <div className="task-ref-add">
                    <select aria-label="노트 고르기" value={selectedNoteId} onChange={(e) => setSelectedNoteId(e.target.value)} disabled={busy}>
                      <option value="">노트에서 고르기</option>
                      {notes.map((n) => (
                        <option key={n.id} value={n.id}>
                          {n.title || n.id}
                        </option>
                      ))}
                    </select>
                    <button
                      type="button"
                      disabled={busy || !selectedNoteId}
                      onClick={() => {
                        const n = notes.find((x) => x.id === selectedNoteId);
                        if (!n) return;
                        const nextRefs = [...task.refs];
                        if (!nextRefs.some((r) => r.kind === "note" && r.target === n.id)) {
                          nextRefs.push({ kind: "note", target: n.id, title: n.title });
                          void saveField({ refs: nextRefs });
                        }
                        setSelectedNoteId("");
                      }}
                    >
                      노트 연결
                    </button>
                  </div>
                  <div className="task-ref-add">
                    <input
                      aria-label="GitHub 이슈나 PR"
                      placeholder="GitHub 이슈·PR 주소 또는 owner/name#53"
                      value={refInput}
                      onChange={(e) => setRefInput(e.target.value)}
                      disabled={busy}
                      onKeyDown={(e) => {
                        if (e.key === "Enter") {
                          e.preventDefault();
                          const parsed = parseRefInput(refInput);
                          if (!parsed) {
                            setRefError("GitHub 이슈·PR 주소나 owner/name#번호를 적어 주세요.");
                            return;
                          }
                          setRefError("");
                          const nextRefs = [...task.refs];
                          if (!nextRefs.some((r) => r.kind === parsed.kind && r.target === parsed.target)) {
                            nextRefs.push(parsed);
                            void saveField({ refs: nextRefs });
                          }
                          setRefInput("");
                        }
                      }}
                    />
                    <button
                      type="button"
                      disabled={busy || !refInput.trim()}
                      onClick={() => {
                        const parsed = parseRefInput(refInput);
                        if (!parsed) {
                          setRefError("GitHub 이슈·PR 주소나 owner/name#번호를 적어 주세요.");
                          return;
                        }
                        setRefError("");
                        const nextRefs = [...task.refs];
                        if (!nextRefs.some((r) => r.kind === parsed.kind && r.target === parsed.target)) {
                          nextRefs.push(parsed);
                          void saveField({ refs: nextRefs });
                        }
                        setRefInput("");
                      }}
                    >
                      GitHub 연결
                    </button>
                  </div>
                  {refError && <p role="alert" className="task-error-text">{refError}</p>}
                </div>
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
                    <FeedItem key={`${e.at}-${i}`} event={e} onLink={openCommentLink} />
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
                  <StatusMenu task={task} busy={busy} onPick={(to) => void run(() => transitionTask(call, task, to))} />
                </dd>
                <dt>유형</dt>
                <dd>
                  <InlineTypeSelect
                    type={task.type}
                    busy={busy}
                    onChange={(t) => void saveField({ type: t })}
                  />
                </dd>
                <dt>우선순위</dt>
                <dd>
                  <InlinePrioritySelect
                    priority={task.priority}
                    busy={busy}
                    onChange={(p) => void saveField({ priority: p })}
                  />
                </dd>
                <dt>저장소</dt>
                <dd>
                  <InlineRepoEdit
                    repo={task.repo}
                    repos={repos}
                    busy={busy}
                    onChange={(p) => void saveField(p)}
                  />
                </dd>
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
                <dd>
                  <InlineBaseBranchEdit
                    baseBranch={task.baseBranch}
                    defaultBranch={defaultBranch}
                    branchChoices={branchChoices}
                    busy={busy}
                    onChange={(b) => void saveField({ base: b })}
                  />
                </dd>
                <dt>라벨</dt>
                <dd>
                  <InlineLabelsEdit
                    labels={task.labels}
                    busy={busy}
                    onChange={(l) => void saveField({ labelsText: l })}
                  />
                </dd>
                {task.manager && <><dt>매니저</dt><dd>{task.manager}</dd></>}
                <dt>만든 때</dt>
                <dd>{timeLabel(task.createdAt)}</dd>
                <dt>고친 때</dt>
                <dd>
                  {timeLabel(task.editedAt)}
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
            <TaskReportSources entries={task.intakeSources ?? []} firstRef={slackSource} controller={controller} busy={busy}
              onSaveFirst={source => saveField({ refs: task.refs.map(ref => ref === slackSource ? { ...ref, ...source } : ref) })}>
              {task.manager && statusDestination && runner.supported && <button type="button" disabled={sendingStatus} onClick={async () => {
                setSendingStatus(true);
                setStatusMessage("");
                try {
                  await controller.callHost("manager_send_status", { taskKey: task.key });
                  setStatusMessage("원래 제보 대화에 상태를 알렸어요.");
                } catch (err) {
                  setStatusMessage(explainError(err));
                } finally {
                  setSendingStatus(false);
                }
              }}>제보 대화에 상태 알리기</button>}
              {statusMessage && <p role="status">{statusMessage}</p>}
            </TaskReportSources>
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
  usePopoverDismiss({
    ref: box,
    enabled: open,
    onClose: () => setOpen(false),
  });
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

function FeedItem({ event, onLink }: { event: ActivityEvent; onLink(url: string): void }) {
  return (
    <li className={`task-feed-item task-feed-${event.kind}`}>
      <div>
        <strong>{activityWho(event)}</strong>가 {activityText(event)}
        {event.kind === "comment" && event.body && (
          <div className="task-comment-body"><ReadMarkdown body={event.body} comment onLink={onLink} /></div>
        )}
      </div>
      <time dateTime={event.at}>{timeLabel(event.at)}</time>
    </li>
  );
}

/** 다른 태스크와 연결을 건다. 막고 있음·막혀 있음·관련 가운데 이 태스크 기준으로 고른다. */
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

/** 세부 정보 사이드바: 인라인 유형(Type) 선택 */
function InlineTypeSelect({ type, busy, onChange }: { type: TaskType; busy: boolean; onChange: (t: TaskType) => void }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  usePopoverDismiss({
    ref: box,
    enabled: open,
    onClose: () => setOpen(false),
  });

  const choices: TaskType[] = ["feature", "bug", "chore"];
  return (
    <div className="task-inline-picker" ref={box}>
      <button
        type="button"
        className="task-inline-pill-btn"
        disabled={busy}
        onClick={() => setOpen((o) => !o)}
        title="클릭하여 유형 변경"
      >
        <TypeMark type={type} /> <span>{typeLabel(type)}</span>
        <span className="task-inline-caret" aria-hidden="true">▾</span>
      </button>
      {open && (
        <div className="task-menu task-popover-sm" role="menu" aria-label="유형 변경">
          {choices.map((c) => (
            <button
              key={c}
              type="button"
              role="menuitem"
              className={`task-menu-item task-menu-item-row ${c === type ? "is-active" : ""}`}
              onClick={() => {
                setOpen(false);
                if (c !== type) onChange(c);
              }}
            >
              <TypeMark type={c} /> <span>{typeLabel(c)}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/** 세부 정보 사이드바: 인라인 우선순위(Priority) 선택 */
function InlinePrioritySelect({ priority, busy, onChange }: { priority: TaskPriority; busy: boolean; onChange: (p: TaskPriority) => void }) {
  const [open, setOpen] = useState(false);
  const box = useRef<HTMLDivElement>(null);
  usePopoverDismiss({
    ref: box,
    enabled: open,
    onClose: () => setOpen(false),
  });

  const choices: TaskPriority[] = ["high", "medium", "low"];
  return (
    <div className="task-inline-picker" ref={box}>
      <button
        type="button"
        className="task-inline-pill-btn"
        disabled={busy}
        onClick={() => setOpen((o) => !o)}
        title="클릭하여 우선순위 변경"
      >
        <PriorityMark priority={priority} /> <span>{priorityLabel(priority)}</span>
        <span className="task-inline-caret" aria-hidden="true">▾</span>
      </button>
      {open && (
        <div className="task-menu task-popover-sm" role="menu" aria-label="우선순위 변경">
          {choices.map((c) => (
            <button
              key={c}
              type="button"
              role="menuitem"
              className={`task-menu-item task-menu-item-row ${c === priority ? "is-active" : ""}`}
              onClick={() => {
                setOpen(false);
                if (c !== priority) onChange(c);
              }}
            >
              <PriorityMark priority={c} /> <span>{priorityLabel(c)}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

/** 세부 정보 사이드바: 인라인 저장소(Repo) 편집 */
function InlineRepoEdit({
  repo,
  repos,
  busy,
  onChange,
}: {
  repo?: string | null;
  repos: RepoChoice[];
  busy: boolean;
  onChange: (patch: { repo: string; project: string }) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [val, setVal] = useState(repo ?? "");

  const submit = () => {
    const trimmed = val.trim();
    if (trimmed !== (repo ?? "")) {
      const choice = repos.find((r) => r.repo === trimmed);
      onChange({ repo: trimmed, project: choice?.project ?? "" });
    }
    setEditing(false);
  };

  if (editing) {
    return (
      <form
        className="task-inline-field-form"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <input
          ref={(el) => el?.focus()}
          className="task-inline-input"
          list="inline-repo-list"
          value={val}
          placeholder="owner/repo"
          onChange={(e) => setVal(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") setEditing(false);
          }}
          onBlur={submit}
          disabled={busy}
        />
        <datalist id="inline-repo-list">
          {repos.map((r) => (
            <option key={r.repo} value={r.repo}>
              {r.label}
            </option>
          ))}
        </datalist>
      </form>
    );
  }

  return (
    <div className="task-inline-editable-row">
      {repo ? (
        <a href={`https://github.com/${repo}`} target="_blank" rel="noreferrer">
          {repo}
        </a>
      ) : (
        <span className="task-muted-text">—</span>
      )}
      <button
        type="button"
        className="task-inline-icon-btn"
        title="저장소 변경"
        disabled={busy}
        onClick={() => {
          setVal(repo ?? "");
          setEditing(true);
        }}
      >
        <Icon name="edit" />
      </button>
    </div>
  );
}

/** 세부 정보 사이드바: 인라인 기준 브랜치(Base Branch) 편집 */
function InlineBaseBranchEdit({
  baseBranch,
  defaultBranch = "main",
  branchChoices = ["main", "develop", "master"],
  busy,
  onChange,
}: {
  baseBranch?: string | null;
  defaultBranch?: string;
  branchChoices?: string[];
  busy: boolean;
  onChange: (b: string) => void;
}) {
  const [editing, setEditing] = useState(false);
  const [val, setVal] = useState(baseBranch ?? "");

  const submit = (overrideVal?: string) => {
    const target = overrideVal !== undefined ? overrideVal : val;
    const trimmed = target.trim();
    if (trimmed !== (baseBranch ?? "")) {
      onChange(trimmed);
    }
    setEditing(false);
  };

  if (editing) {
    return (
      <div className="task-inline-branch-edit-wrap">
        <form
          className="task-inline-field-form"
          onSubmit={(e) => {
            e.preventDefault();
            submit();
          }}
        >
          <input
            ref={(el) => el?.focus()}
            className="task-inline-input"
            list="inline-base-branch-list"
            value={val}
            placeholder={`저장소 기본 (${defaultBranch})`}
            onChange={(e) => setVal(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === "Escape") setEditing(false);
            }}
            disabled={busy}
          />
          <datalist id="inline-base-branch-list">
            {branchChoices.map((b) => (
              <option key={b} value={b}>
                {b === defaultBranch ? `${b} (저장소 기본)` : b}
              </option>
            ))}
          </datalist>
          <div className="task-inline-mini-actions">
            <button type="submit" className="task-btn-primary task-btn-xs" disabled={busy}>
              저장
            </button>
            <button type="button" className="task-btn-xs" onClick={() => setEditing(false)} disabled={busy}>
              취소
            </button>
          </div>
        </form>
        <div className="task-inline-branch-chips">
          <span className="task-inline-chip-label">추천:</span>
          {branchChoices.map((b) => (
            <button
              key={b}
              type="button"
              className={`task-inline-branch-chip ${val === b ? "is-selected" : ""}`}
              disabled={busy}
              onClick={() => {
                setVal(b);
                submit(b);
              }}
            >
              {b}
            </button>
          ))}
          {val && (
            <button
              type="button"
              className="task-inline-branch-chip task-chip-reset"
              disabled={busy}
              onClick={() => {
                setVal("");
                submit("");
              }}
              title="저장소 기본 브랜치로 초기화"
            >
              기본으로
            </button>
          )}
        </div>
      </div>
    );
  }

  return (
    <div
      className="task-inline-editable-row task-clickable"
      title="클릭하여 기준 브랜치 변경 (비우면 저장소 기본)"
      onClick={() => {
        setVal(baseBranch ?? "");
        setEditing(true);
      }}
    >
      {baseBranch ? (
        <code>{baseBranch}</code>
      ) : (
        <span className="task-default-branch-display">
          <code>{defaultBranch}</code>
          <small className="task-default-badge">저장소 기본</small>
        </span>
      )}
      <span className="task-inline-edit-icon" aria-hidden="true">
        <Icon name="edit" />
      </span>
    </div>
  );
}

/** 세부 정보 사이드바: 인라인 라벨(Labels) 편집 */
function InlineLabelsEdit({ labels, busy, onChange }: { labels: string[]; busy: boolean; onChange: (labelsText: string) => void }) {
  const [editing, setEditing] = useState(false);
  const [val, setVal] = useState(labels.join(", "));

  const submit = () => {
    const trimmed = val.trim();
    if (trimmed !== labels.join(", ")) {
      onChange(trimmed);
    }
    setEditing(false);
  };

  if (editing) {
    return (
      <form
        className="task-inline-field-form"
        onSubmit={(e) => {
          e.preventDefault();
          submit();
        }}
      >
        <input
          ref={(el) => el?.focus()}
          className="task-inline-input"
          value={val}
          placeholder="라벨 (쉼표 구분)"
          onChange={(e) => setVal(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Escape") setEditing(false);
          }}
          onBlur={submit}
          disabled={busy}
        />
      </form>
    );
  }

  return (
    <div
      className="task-inline-editable-row task-clickable"
      title="클릭하여 라벨 변경"
      onClick={() => {
        setVal(labels.join(", "));
        setEditing(true);
      }}
    >
      {labels.length > 0 ? (
        <ul className="task-labels">
          {labels.map((l) => (
            <li key={l}>{l}</li>
          ))}
        </ul>
      ) : (
        <span className="task-muted-text">—</span>
      )}
      <span className="task-inline-edit-icon" aria-hidden="true">
        <Icon name="edit" />
      </span>
    </div>
  );
}
