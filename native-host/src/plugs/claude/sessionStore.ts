// Claude 세션 목록의 화면 쪽 저장소. 세션 화면, 노트 옆 대화창, 터미널 이어 열기가 같이 쓴다.
// 호스트의 `list_claude_agents`와 엔진의 `session-titles`로 한 번 읽어 모아 두고, 읽는 쪽은 구독한다.
// (예전에는 왼쪽 레일의 PROJECTS 패널이 이 파일에 같이 있었다. 패널은 없어졌고 저장소만 남았다.)
import { useEffect, useSyncExternalStore } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";

import type { ClaudeSession } from './sessionTypes.ts';
export type { ClaudeSession };

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
let refreshInFlight: Promise<void> | null = null;
export function refreshSessions(vaultPath?: string): Promise<void> {
  if (refreshInFlight) return refreshInFlight;
  const task = refreshSessionList(vaultPath);
  refreshInFlight = task;
  void task.finally(() => { if (refreshInFlight === task) refreshInFlight = null; });
  return task;
}
async function refreshSessionList(vaultPath?: string) {

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

    // 6.3: a text is a "비지 않은 글" and is kept as it is; it is not trimmed (a leading space would change the key)
    const session: ClaudeSession = {
      sessionId,
      cwd,
      startedAt,
      kind,
      status,
      name: typeof item.name === "string" && item.name.trim() ? item.name : undefined,
      id: typeof item.id === "string" && item.id.trim() ? item.id : undefined,
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
    // C. held_agent_sessions (6.6): only when a vault is open and there is at least one row
    vaultPath && validRows.length > 0
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

/** 읽어 둔 세션 목록. 구독하므로 목록이 바뀌면 다시 그려진다. */
export function useSessionList(): ClaudeSession[] {
  const store = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  return store.sessions;
}

/** Puts a list into the store. For the node tests; the real list comes from `refreshSessions`. */
export function setSessionStoreState(next: Partial<SessionStoreState>): void {
  storeState = { ...storeState, ...next };
  notify();
}

export function getSessionStoreSnapshot(): SessionStoreState {
  return storeState;
}

export function useSessionStore() {
  return useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
}

/**
 * 세션 목록을 쓰는 화면이 쥐는 새로고침 규칙. 한 번도 못 읽었으면 지금 읽고, 창이 다시 눈에 띄면(포커스, 보이게 됨) 다시 읽는다.
 * 왼쪽 레일의 PROJECTS 패널이 하던 일을 세션 목록을 실제로 쓰는 화면으로 옮긴 것이다.
 */
export function useSessionListSync(vault?: string): void {
  const store = useSessionStore();
  useEffect(() => {
    if (store.status === "idle") void refreshSessions(vault);
  }, [store.status, vault]);
  useEffect(() => {
    const onFocus = () => void refreshSessions(vault);
    const onVisibility = () => {
      if (document.visibilityState === "visible") void refreshSessions(vault);
    };
    window.addEventListener("focus", onFocus);
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      window.removeEventListener("focus", onFocus);
      document.removeEventListener("visibilitychange", onVisibility);
    };
  }, [vault]);
}
