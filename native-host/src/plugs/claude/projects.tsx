import React, { useEffect, useState, useSyncExternalStore } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import type { WorkspaceController } from "../../workspace/controller.ts";
import type { WorkspaceState, Agenda, Row } from "../../workspace/types.ts";
import { asText } from "../../workspace/types.ts";
import { HUE_COLORS } from "../../ui/model.ts";

export interface ClaudeSession {
  sessionId: string;
  cwd: string;
  startedAt: number;
  kind: "interactive" | "background" | "unknown";
  name?: string;
  id?: string;
  pid?: number;
  status: "running" | "blocked" | "idle" | "unknown";
  title?: string;
  branch?: string;
  processAlive?: boolean;
  heldByAgent?: boolean;
  endedAt?: number;
  isEndedRow?: boolean;
}

interface AgentsResponse {
  ok: boolean;
  stdout: string;
  error?: string | null;
  elapsedMs: number;
}

interface SidecarResult {
  status: number;
  stdout: string;
  stderr: string;
}

interface SessionStoreState {
  status: "idle" | "loading" | "ok" | "failed";
  sessions: ClaudeSession[];
  skippedCount: number;
  endedDroppedCount: number;
  error: string | null;
  readAt: number | null;
}

let storeState: SessionStoreState = {
  status: "idle",
  sessions: [],
  skippedCount: 0,
  endedDroppedCount: 0,
  error: null,
  readAt: null,
};

const listeners = new Set<() => void>();

function notify() {
  listeners.forEach((l) => l());
}

function getSnapshot() {
  return storeState;
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

/**
 * 6.1 한 번 읽는 차례를 수행합니다.
 */
export async function refreshSessions(vaultPath?: string) {
  if (storeState.status === "loading") {
    return;
  }

  if (!isTauri()) {
    storeState = {
      ...storeState,
      status: "failed",
      error: "이 화면에서는 세션 목록을 볼 수 없어요(앱에서만 돼요).",
    };
    notify();
    return;
  }

  storeState = {
    ...storeState,
    status: "loading",
  };
  notify();

  let rawListRes: AgentsResponse;
  try {
    rawListRes = await invoke<AgentsResponse>("list_claude_agents");
  } catch (err: any) {
    storeState = {
      ...storeState,
      status: "failed",
      error: `세션 목록을 못 받았어요: ${err?.message || String(err)}`,
    };
    notify();
    return;
  }

  if (!rawListRes.ok) {
    storeState = {
      ...storeState,
      status: "failed",
      error: rawListRes.error || "세션 목록을 못 받았어요.",
    };
    notify();
    return;
  }

  let rawArray: any[];
  try {
    rawArray = JSON.parse(rawListRes.stdout);
  } catch {
    storeState = {
      ...storeState,
      status: "failed",
      error: "claude 가 준 세션 목록을 못 읽었어요.",
    };
    notify();
    return;
  }

  if (!Array.isArray(rawArray)) {
    storeState = {
      ...storeState,
      status: "failed",
      error: "claude 가 준 세션 목록이 배열이 아니에요.",
    };
    notify();
    return;
  }

  // 6.3 줄마다 세션 1개로 좁힌다
  const validRows: ClaudeSession[] = [];
  for (const item of rawArray) {
    if (!item || typeof item !== "object") continue;
    const { sessionId, cwd, startedAt } = item;
    if (typeof sessionId !== "string" || !sessionId.trim()) continue;
    if (typeof cwd !== "string" || !cwd.trim()) continue;
    if (typeof startedAt !== "number" || !Number.isFinite(startedAt)) continue;

    // kind
    let kind: ClaudeSession["kind"] = "unknown";
    if (item.kind === "interactive" || item.kind === "background") {
      kind = item.kind;
    }

    // status / state (6.3)
    let status: ClaudeSession["status"] = "unknown";
    const rawState = typeof item.state === "string" ? item.state : null;
    const rawStatus = typeof item.status === "string" ? item.status : null;
    const effectiveStatus = rawState || rawStatus;
    if (effectiveStatus === "working" || effectiveStatus === "busy") {
      status = "running";
    } else if (effectiveStatus === "blocked") {
      status = "blocked";
    } else if (effectiveStatus === "idle") {
      status = "idle";
    }

    const session: ClaudeSession = {
      sessionId: sessionId.trim(),
      cwd: cwd.trim(),
      startedAt,
      kind,
      status,
      name: typeof item.name === "string" && item.name.trim() ? item.name.trim() : undefined,
      id: typeof item.id === "string" && item.id.trim() ? item.id.trim() : undefined,
      pid: typeof item.pid === "number" && Number.isFinite(item.pid) ? item.pid : undefined,
    };
    validRows.push(session);
  }

  const skippedCount = rawArray.length - validRows.length;

  // 5. 4개를 함께 부르고 모두 끝나기를 기다린다 (6.1)
  const pidsToQuery = validRows
    .map((r) => r.pid)
    .filter((p): p is number => typeof p === "number");

  const titlesQueries: string[][] = [];
  for (let i = 0; i < validRows.length; i += 200) {
    const chunk = validRows.slice(i, i + 200).map((r) => r.sessionId);
    titlesQueries.push(chunk);
  }

  const [aliveRes, titlesRes, heldRes, endedRes] = await Promise.allSettled([
    // A. alive_pids (6.4)
    pidsToQuery.length > 0
      ? invoke<number[]>("alive_pids", { pids: pidsToQuery })
      : Promise.resolve([]),
    // B. session-titles (6.6)
    titlesQueries.length > 0
      ? Promise.all(
          titlesQueries.map((chunk) =>
            invoke<SidecarResult>("sidecar_call", { argv: ["session-titles", ...chunk] })
              .then((r) => (r.status === 0 ? JSON.parse(r.stdout) : null))
              .catch(() => null)
          )
        )
      : Promise.resolve([]),
    // C. held_agent_sessions (6.6)
    vaultPath
      ? invoke<string[]>("held_agent_sessions", { vault: vaultPath }).catch(() => [])
      : Promise.resolve([]),
    // D. recent-sessions (6.5)
    invoke<SidecarResult>("sidecar_call", { argv: ["recent-sessions", "days=7"] })
      .then((r) => (r.status === 0 ? JSON.parse(r.stdout) : null))
      .catch(() => null),
  ]);

  // A. alive_pids 반영
  const alivePids = new Set<number>(
    aliveRes.status === "fulfilled" && Array.isArray(aliveRes.value) ? aliveRes.value : []
  );
  if (aliveRes.status === "fulfilled") {
    for (const r of validRows) {
      if (typeof r.pid === "number") {
        r.processAlive = alivePids.has(r.pid);
      }
    }
  }

  // B. session-titles 반영
  if (titlesRes.status === "fulfilled" && Array.isArray(titlesRes.value)) {
    for (const resp of titlesRes.value) {
      if (resp && Array.isArray(resp.sessions)) {
        for (const item of resp.sessions) {
          if (!item || !item.sessionId) continue;
          const match = validRows.find((r) => r.sessionId === item.sessionId);
          if (match) {
            if (typeof item.title === "string" && item.title.trim()) {
              match.title = item.title.trim();
            }
            if (typeof item.branch === "string" && item.branch.trim()) {
              match.branch = item.branch.trim();
            }
          }
        }
      }
    }
  }

  // C. held_agent_sessions 반영
  const heldSet = new Set<string>(
    heldRes.status === "fulfilled" && Array.isArray(heldRes.value) ? heldRes.value : []
  );
  for (const r of validRows) {
    if (heldSet.has(r.sessionId)) {
      r.heldByAgent = true;
    }
  }

  // D. 끝난 행 덧붙이기 (6.5)
  const endedRows: ClaudeSession[] = [];
  let endedDroppedCount = 0;
  if (endedRes.status === "fulfilled" && endedRes.value) {
    const endedData = endedRes.value;
    const rawDropped = Number(endedData.dropped);
    endedDroppedCount = Number.isFinite(rawDropped) && rawDropped > 0 ? rawDropped : 0;

    const existingIds = new Set(validRows.map((r) => r.sessionId));
    if (Array.isArray(endedData.sessions)) {
      for (const e of endedData.sessions) {
        if (!e || typeof e !== "object") continue;
        const sId = typeof e.sessionId === "string" ? e.sessionId.trim() : null;
        const sCwd = typeof e.cwd === "string" ? e.cwd.trim() : null;
        if (!sId || !sCwd) continue;
        if (existingIds.has(sId)) continue; // 목록 행에 이미 있는 세션 뺀다

        const lastActiveAt = typeof e.lastActiveAt === "number" ? e.lastActiveAt : Date.now();
        const startedAt = typeof e.startedAt === "number" ? e.startedAt : lastActiveAt;

        endedRows.push({
          sessionId: sId,
          cwd: sCwd,
          kind: "interactive",
          startedAt,
          status: "unknown",
          endedAt: lastActiveAt,
          title: typeof e.title === "string" && e.title.trim() ? e.title.trim() : undefined,
          branch: typeof e.branch === "string" && e.branch.trim() ? e.branch.trim() : undefined,
          isEndedRow: true,
        });
      }
    }
  }

  const allSessions = [...validRows, ...endedRows];

  storeState = {
    status: "ok",
    sessions: allSessions,
    skippedCount,
    endedDroppedCount,
    error: null,
    readAt: Date.now(),
  };
  notify();
}

/**
 * PROJECTS 목록 훅
 */
export function useProjectsList(): ClaudeSession[] {
  const store = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  return store.sessions;
}

export function useSessionStore() {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

function stripTrailingSlash(path: string): string {
  return path.endsWith("/") ? path.slice(0, -1) : path;
}

/**
 * 6.8 아젠다 매칭: cwd와 repo 매칭
 */
function matchAgenda(sessionCwd: string, agendas: Agenda[]): Agenda | null {
  const normCwd = stripTrailingSlash(sessionCwd);
  let bestMatch: Agenda | null = null;
  let bestLen = -1;

  for (const a of agendas) {
    const rawRepo = asText(a.repo);
    if (!rawRepo || !rawRepo.trim()) continue;
    const normRepo = stripTrailingSlash(rawRepo.trim());
    if (normCwd === normRepo || normCwd.startsWith(normRepo + "/")) {
      if (normRepo.length > bestLen) {
        bestLen = normRepo.length;
        bestMatch = a;
      }
    }
  }

  return bestMatch;
}

/**
 * 지난 시간 상대 표기 (6.10)
 */
function formatRelativeTime(timestamp: number, baseTime: number): string {
  const diffSec = Math.max(0, Math.floor((baseTime - timestamp) / 1000));
  if (diffSec < 60) return "방금 전";
  const diffMin = Math.floor(diffSec / 60);
  if (diffMin < 60) return `${diffMin}분 전`;
  const diffHour = Math.floor(diffMin / 60);
  if (diffHour < 24) return `${diffHour}시간 전`;
  const diffDay = Math.floor(diffHour / 24);
  return `${diffDay}일 전`;
}

/**
 * 6장 PROJECTS 레일 메인 컴포넌트
 */
export function RailProjects({
  controller,
  state,
}: {
  controller?: WorkspaceController;
  state?: WorkspaceState;
}): React.JSX.Element {
  const store = useSessionStore();
  const [copiedId, setCopiedId] = useState<string | null>(null);
  const [expandedOlder, setExpandedOlder] = useState<Record<string, boolean>>({});

  const vaultPath = state?.workspace?.path || "";

  useEffect(() => {
    if (store.status === "idle") {
      void refreshSessions(vaultPath);
    }

    const onFocus = () => void refreshSessions(vaultPath);
    const onVisibility = () => {
      if (document.visibilityState === "visible") {
        void refreshSessions(vaultPath);
      }
    };

    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [vaultPath]);

  const readAt = store.readAt || Date.now();
  const sevenDaysAgo = readAt - 7 * 86_400_000;

  const ws = state?.workspace;
  const projects: Row[] = ws?.projects || [];
  const agendas: Agenda[] = ws?.agendas || [];

  // 아젠다별 세션 맵핑 (6.8)
  const agendaSessionsMap = new Map<string, ClaudeSession[]>();
  const unmatchedSessions: ClaudeSession[] = [];

  for (const s of store.sessions) {
    const matched = matchAgenda(s.cwd, agendas);
    if (matched) {
      const list = agendaSessionsMap.get(matched.id) || [];
      list.push(s);
      agendaSessionsMap.set(matched.id, list);
    } else {
      unmatchedSessions.push(s);
    }
  }

  // 아젠다 내 세션 정렬: 놓는 시각 내림차순, 같으면 sessionId 오름차순 (6.8)
  for (const list of agendaSessionsMap.values()) {
    list.sort((a, b) => {
      const timeA = a.endedAt || a.startedAt;
      const timeB = b.endedAt || b.startedAt;
      if (timeB !== timeA) return timeB - timeA;
      return a.sessionId.localeCompare(b.sessionId);
    });
  }

  // 프로젝트별 그룹화 (6.8, 6.9)
  const LOOSE_KEY = "sess:(미분류)";
  const rawExpanded = state?.prefs?.expandedSessionProjects;
  const currentExpanded: string[] = Array.isArray(rawExpanded) ? rawExpanded : [];
  const expandedProjects = new Set<string>(currentExpanded);

  const projectGroups: {
    key: string;
    title: string;
    hueColor: string;
    agendas: { agenda: Agenda; recent: ClaudeSession[]; older: ClaudeSession[] }[];
    totalCount: number;
    olderCount: number;
  }[] = [];

  // 1. 등록된 프로젝트들
  for (const p of projects) {
    const pSlug = asText(p.slug);
    const pTitle = asText(p.title);
    const pHue = asText(p.hue);
    const pKey = `sess:${pSlug}`;

    const projAgendas = agendas.filter((a) => a.project === pSlug);
    const agendaData: { agenda: Agenda; recent: ClaudeSession[]; older: ClaudeSession[] }[] = [];
    let projTotal = 0;
    let projOlder = 0;

    for (const a of projAgendas) {
      const sessions = agendaSessionsMap.get(a.id) || [];
      if (sessions.length === 0) continue;

      const recent: ClaudeSession[] = [];
      const older: ClaudeSession[] = [];
      for (const s of sessions) {
        const time = s.endedAt || s.startedAt;
        if (time >= sevenDaysAgo) {
          recent.push(s);
        } else {
          older.push(s);
        }
      }

      projTotal += sessions.length;
      projOlder += older.length;
      agendaData.push({ agenda: a, recent, older });
    }

    if (projTotal > 0) {
      projectGroups.push({
        key: pKey,
        title: pTitle,
        hueColor: HUE_COLORS[pHue] || "var(--accent)",
        agendas: agendaData,
        totalCount: projTotal,
        olderCount: projOlder,
      });
    }
  }

  // 2. 미분류 프로젝트
  const unassignedAgendas = agendas.filter((a) => !projects.some((p) => p.slug === a.project));
  const unassignedData: { agenda: Agenda; recent: ClaudeSession[]; older: ClaudeSession[] }[] = [];
  let unassignedTotal = 0;
  let unassignedOlder = 0;

  for (const a of unassignedAgendas) {
    const sessions = agendaSessionsMap.get(a.id) || [];
    if (sessions.length === 0) continue;

    const recent: ClaudeSession[] = [];
    const older: ClaudeSession[] = [];
    for (const s of sessions) {
      const time = s.endedAt || s.startedAt;
      if (time >= sevenDaysAgo) {
        recent.push(s);
      } else {
        older.push(s);
      }
    }

    unassignedTotal += sessions.length;
    unassignedOlder += older.length;
    unassignedData.push({ agenda: a, recent, older });
  }

  if (unassignedTotal > 0) {
    projectGroups.push({
      key: LOOSE_KEY,
      title: "미분류",
      hueColor: "var(--fg-hint)",
      agendas: unassignedData,
      totalCount: unassignedTotal,
      olderCount: unassignedOlder,
    });
  }

  const toggleProject = (key: string) => {
    if (!controller) return;
    const current = Array.isArray(state?.prefs?.expandedSessionProjects)
      ? (state!.prefs!.expandedSessionProjects as string[])
      : [];
    const next = current.includes(key)
      ? current.filter((k) => k !== key)
      : [...current, key];
    controller.preference({ expandedSessionProjects: next });
  };

  const copyResumeCommand = (sessionId: string) => {
    const cmd = `claude --resume ${sessionId}`;
    void navigator.clipboard?.writeText(cmd);
    setCopiedId(sessionId);
    setTimeout(() => {
      setCopiedId((curr) => (curr === sessionId ? null : curr));
    }, 1600);
  };

  const currentRoute = state?.route || (typeof location !== "undefined" ? location.pathname : "");

  return (
    <section data-rail-section="projects" className="rail-projects-section">
      <header className="sess-rail-head">
        <h2>PROJECTS</h2>
        <button
          className="nav-add"
          aria-label="세션 목록 새로고침"
          disabled={store.status === "loading"}
          onClick={() => void refreshSessions(vaultPath)}
        >
          ↻
        </button>
      </header>

      {/* 실패 줄 (6.7) */}
      {store.error && <div className="sess-note sess-note-bad">{store.error}</div>}

      {/* 프로젝트 그룹 렌더링 (6.8, 6.9, 6.10) */}
      {projectGroups.map((proj) => {
        const isExpanded = expandedProjects.has(proj.key);
        const showOlder = !!expandedOlder[proj.key];

        return (
          <div className="sess-project" key={proj.key}>
            <button
              className="sess-toggle"
              onClick={() => toggleProject(proj.key)}
              aria-expanded={isExpanded}
            >
              <span className="sess-caret">{isExpanded ? "▾" : "▸"}</span>
              <span className="sess-dot" style={{ backgroundColor: proj.hueColor }} />
              <span className="sess-proj-title">{proj.title}</span>
              <span className="sess-count">{proj.totalCount}</span>
            </button>

            {isExpanded && (
              <div className="sess-project-content">
                {proj.agendas.map(({ agenda, recent, older }) => {
                  const visibleSessions = showOlder ? [...recent, ...older] : recent;
                  if (visibleSessions.length === 0) return null;

                  return (
                    <div className="sess-agenda-group" key={agenda.id}>
                      <div className="sess-agenda-head">
                        <span
                          className="sess-dot"
                          style={{ color: HUE_COLORS[asText(agenda.hue)] || "var(--accent)" }}
                        >
                          ●
                        </span>
                        <span className="sess-agenda-name">{agenda.title}</span>
                      </div>

                      {visibleSessions.map((sess) => {
                        const targetPath = `/session/${encodeURIComponent(sess.sessionId)}`;
                        const isActive = currentRoute === targetPath;
                        const time = sess.endedAt || sess.startedAt;
                        const timeText = formatRelativeTime(time, readAt);

                        let kindLabel = "알 수 없음";
                        if (sess.isEndedRow) {
                          kindLabel = "끝남";
                        } else if (sess.kind === "background") {
                          kindLabel = "백그라운드";
                        } else if (sess.kind === "interactive") {
                          kindLabel = "터미널";
                        }

                        return (
                          <div className="sess-row" key={sess.sessionId}>
                            <button
                              className={`sess-item ${isActive ? "is-active" : ""}`}
                              onClick={() => {
                                if (controller) {
                                  controller.navigate(targetPath);
                                } else {
                                  window.history.pushState(null, "", targetPath);
                                  window.dispatchEvent(new PopStateEvent("popstate"));
                                }
                              }}
                            >
                              <div className="sess-title">
                                {sess.status === "running" && <span className="sess-run" />}
                                {sess.title || sess.name || "(이름 없는 세션)"}
                              </div>
                              <div className="sess-meta">
                                <span className="sess-time">{timeText}</span>
                                <span className="sess-sep"> · </span>
                                <span
                                  className={`sess-kind ${sess.isEndedRow ? "sess-ended" : ""}`}
                                >
                                  {kindLabel}
                                </span>
                                {sess.status === "blocked" && (
                                  <>
                                    <span className="sess-sep"> · </span>
                                    <span className="sess-blocked">멈춤</span>
                                  </>
                                )}
                                {sess.branch && (
                                  <>
                                    <span className="sess-sep"> · </span>
                                    <span className="sess-branch">{sess.branch}</span>
                                  </>
                                )}
                              </div>
                            </button>
                            <button
                              className="sess-copy"
                              title="이어 가기 명령 복사"
                              onClick={() => copyResumeCommand(sess.sessionId)}
                            >
                              {copiedId === sess.sessionId ? "복사됨" : "복사"}
                            </button>
                          </div>
                        );
                      })}
                    </div>
                  );
                })}

                {proj.olderCount > 0 && (
                  <button
                    className="sess-more"
                    onClick={() =>
                      setExpandedOlder((prev) => ({
                        ...prev,
                        [proj.key]: !prev[proj.key],
                      }))
                    }
                  >
                    {showOlder
                      ? "오래된 세션 숨기기"
                      : `오래된 세션 ${proj.olderCount}개 보기`}
                  </button>
                )}
              </div>
            )}
          </div>
        );
      })}

      {/* 안 붙은 세션 줄 (6.8, 6.13) */}
      {unmatchedSessions.length > 0 && (
        <div className="sess-note">
          아젠다에 안 붙은 세션 {unmatchedSessions.length}개 — 그 저장소 아젠다에 경로를 적으면 여기 붙어요.
        </div>
      )}

      {/* 모양이 아니라 건너뛴 줄 (6.3) */}
      {store.skippedCount > 0 && (
        <div className="sess-note">모양이 아니라 건너뛴 줄 {store.skippedCount}개</div>
      )}

      {/* D15 결함 고침: 자른 끝난 세션 줄 (6.5, 6.13) */}
      {store.endedDroppedCount > 0 && (
        <div className="sess-note" data-note="ended-dropped">
          이레 안에 끝난 세션 중 오래된 {store.endedDroppedCount}개는 안 보여요. 한 번에 읽는 수를 넘었어요.
        </div>
      )}
    </section>
  );
}
