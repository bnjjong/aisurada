// 러너의 화면 쪽 상태. 태스크 목록, 상세, 상단 막대가 같은 상태를 보도록 한 곳에 둔다.
// 태스크를 집고 claude에게 맡기고 보고·질문을 남기는 일은 모두 호스트(Rust, `runner/`)가 한다.
// 이 모듈은 호스트 명령을 부르고, 호스트가 알려 주는 사건(`ais://runner`)을 받아 기억할 뿐이다.
import { useEffect, useState, useSyncExternalStore } from "react";

export type HostCall = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export type RunPhase = "running" | "waiting-answer" | "done" | "failed" | "cancelled" | "abandoned";

/** 실행 기록 한 건(호스트의 `RunRecord`). */
export interface RunRecord {
  runId: string;
  taskKey: string;
  repo: string;
  sessionId: string;
  phase: RunPhase;
  /** 받기 집기 판단 받아오기 일하기 내보내기 정리 */
  step: string;
  startedAt: number;
  endedAt: number | null;
  turns: number;
  questions: number;
  question: string | null;
  answer: string | null;
  workdir: string | null;
  workedSecs: number;
  costUsd: number;
  inputTokens: number;
  outputTokens: number;
  summary: string | null;
  error: string | null;
}

export interface RunEvent {
  at: number;
  step: string;
  text: string;
}

export interface RunnerSettings {
  /** 켜면 Ready For Develop가 된 태스크를 러너가 스스로 집는다 */
  autoPickup: boolean;
  concurrency: number;
  model: string;
}

export interface RunnerSnapshot {
  settings: RunnerSettings;
  claudeAvailable: boolean;
  /** 일하는 스레드 수. 앱을 닫을 때 묻는 숫자다 */
  running: number;
  active: RunRecord[];
  waiting: RunRecord[];
  /** 모든 태스크를 가로지른 최근 실행(레일의 「최근 실행」). 끝나지 않은 것이 앞이고 그다음은 최신순이다 */
  recent: RunRecord[];
  /** 호스트에서 한 번이라도 읽었는지 */
  loaded: boolean;
  /** 이 빌드에 러너가 있는지(claude 없는 빌드에는 명령이 없다) */
  supported: boolean;
  /** 호스트가 사건을 알릴 때마다 오른다. 태스크 화면이 이 값이 바뀌면 다시 읽는다 */
  version: number;
  /** 마지막 일이 낸 오류 글. 성공하면 빈 글 */
  error: string;
}

export const MIN_CONCURRENCY = 1;
export const MAX_CONCURRENCY = 6;

export const DEFAULT_RUNNER_SETTINGS: RunnerSettings = { autoPickup: false, concurrency: 3, model: "sonnet" };

/** 레일의 「최근 실행」에 보이는 건수. */
export const RECENT_RUNS = 5;
/** 세션 화면이 러너 세션을 찾으려고 한 번에 읽는 건수(호스트의 상한과 같다). */
const SESSION_LOOKUP_RUNS = 500;

/** 일곱 걸음. 호스트가 기록에 남기는 이름과 같다. */
export const STEPS = ["받기", "집기", "판단", "받아오기", "일하기", "내보내기", "정리"] as const;

const EMPTY: RunnerSnapshot = {
  settings: DEFAULT_RUNNER_SETTINGS,
  claudeAvailable: true,
  running: 0,
  active: [],
  waiting: [],
  recent: [],
  loaded: false,
  supported: true,
  version: 0,
  error: "",
};

// ---------------------------------------------------------------- 읽기

const isRecord = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
const PHASES = new Set<string>(["running", "waiting-answer", "done", "failed", "cancelled", "abandoned"]);
const num = (v: unknown, fallback = 0) => (typeof v === "number" && Number.isFinite(v) ? v : fallback);
const str = (v: unknown) => (typeof v === "string" ? v : "");
const maybeStr = (v: unknown) => (typeof v === "string" && v ? v : null);

/** 호스트 행을 실행 기록으로 읽는다. 모양이 틀리면 null이다. */
export function toRun(v: unknown): RunRecord | null {
  if (!isRecord(v) || typeof v.runId !== "string" || !v.runId || typeof v.taskKey !== "string" || !v.taskKey) return null;
  const phase = typeof v.phase === "string" && PHASES.has(v.phase) ? (v.phase as RunPhase) : "running";
  return {
    runId: v.runId,
    taskKey: v.taskKey,
    repo: str(v.repo),
    sessionId: str(v.sessionId),
    phase,
    step: str(v.step),
    startedAt: num(v.startedAt),
    endedAt: typeof v.endedAt === "number" ? v.endedAt : null,
    turns: num(v.turns),
    questions: num(v.questions),
    question: maybeStr(v.question),
    answer: maybeStr(v.answer),
    workdir: maybeStr(v.workdir),
    workedSecs: num(v.workedSecs),
    costUsd: num(v.costUsd),
    inputTokens: num(v.inputTokens),
    outputTokens: num(v.outputTokens),
    summary: maybeStr(v.summary),
    error: maybeStr(v.error),
  };
}

export function toRuns(v: unknown): RunRecord[] {
  return Array.isArray(v) ? v.flatMap((r) => toRun(r) ?? []) : [];
}

export function toEvents(v: unknown): RunEvent[] {
  if (!Array.isArray(v)) return [];
  return v.flatMap((e) => (isRecord(e) && typeof e.text === "string" ? [{ at: num(e.at), step: str(e.step), text: e.text }] : []));
}

/** 호스트 설정을 읽는다. 칸이 모자라거나 틀리면 그 칸만 기본값이다. */
export function toSettings(v: unknown): RunnerSettings {
  const d = DEFAULT_RUNNER_SETTINGS;
  if (!isRecord(v)) return d;
  return {
    autoPickup: typeof v.autoPickup === "boolean" ? v.autoPickup : d.autoPickup,
    concurrency: Math.min(MAX_CONCURRENCY, Math.max(MIN_CONCURRENCY, Math.round(num(v.concurrency, d.concurrency)))),
    model: typeof v.model === "string" ? v.model : d.model,
  };
}

// ---------------------------------------------------------------- 상태

let snapshot: RunnerSnapshot = EMPTY;
const listeners = new Set<() => void>();

function set(next: Partial<RunnerSnapshot>) {
  snapshot = { ...snapshot, ...next };
  for (const l of listeners) l();
}

export const getRunnerSnapshot = () => snapshot;
export const subscribeRunner = (l: () => void) => {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
};
export const useRunner = () => useSyncExternalStore(subscribeRunner, getRunnerSnapshot, getRunnerSnapshot);

/** 시험용: 상태를 처음으로 되돌리거나 원하는 모양으로 놓는다. */
export function resetRunner(next: Partial<RunnerSnapshot> = {}) {
  snapshot = { ...EMPTY, ...next };
  for (const l of listeners) l();
}

function explain(e: unknown): string {
  const text = e instanceof Error ? e.message : typeof e === "string" ? e : "";
  return text.replace(/^Error: /, "") || "러너를 부르지 못했어요";
}

/** 호스트에 이 명령이 없다는 오류인지(claude를 뺀 빌드, 브라우저 미리보기). */
export function isMissingCommand(e: unknown): boolean {
  const text = explain(e);
  return /not found|not allowed|unknown command|command .* (not|isn)|없는 명령|알 수 없는/i.test(text);
}

// ---------------------------------------------------------------- 호스트 부르기

/** 지금 모습을 읽는다. 실패해도 화면을 깨지 않고 오류 글만 남긴다. */
export async function refreshRunner(call: HostCall): Promise<void> {
  try {
    const v = await call<unknown>("runner_status");
    if (!isRecord(v)) throw new Error("러너 응답을 읽지 못했어요");
    // 최근 실행은 보조다: 못 읽으면 이전 목록을 두고 러너 상태는 그대로 알린다
    const recent = await loadRecentRuns(call, RECENT_RUNS);
    set({
      settings: toSettings(v.settings),
      claudeAvailable: v.claudeAvailable !== false,
      running: num(v.running),
      active: toRuns(v.active),
      waiting: toRuns(v.waiting),
      recent: recent ?? snapshot.recent,
      loaded: true,
      supported: true,
      error: "",
    });
  } catch (e) {
    // 열린 vault가 없을 때도 여기로 온다: 지원 여부는 명령이 없다는 오류일 때만 끈다
    set({ loaded: true, supported: !isMissingCommand(e), error: isMissingCommand(e) ? "" : explain(e) });
  }
}

/** 호스트가 「러너가 바뀌었다」고 알렸다. 다시 읽고 태스크 화면이 따라오게 version을 올린다. */
export function runnerChanged(call: HostCall): Promise<void> {
  set({ version: snapshot.version + 1 });
  return refreshRunner(call);
}

/** 어떤 일이 실패하면 그 오류 글을 남기고 다시 던지지 않는다. 성공이면 true. */
async function attempt(call: HostCall, work: () => Promise<unknown>): Promise<boolean> {
  try {
    await work();
    set({ error: "" });
    await refreshRunner(call);
    return true;
  } catch (e) {
    set({ error: explain(e) });
    return false;
  }
}

export const runTask = (call: HostCall, taskKey: string) => attempt(call, () => call("runner_run", { taskKey }));
export const stopTask = (call: HostCall, taskKey: string) => attempt(call, () => call("runner_stop", { taskKey }));
export const answerTask = (call: HostCall, taskKey: string, text: string) => attempt(call, () => call("runner_answer", { taskKey, text }));
export const saveRunnerSettings = (call: HostCall, settings: RunnerSettings) =>
  attempt(call, () => call("runner_settings_set", { settings }));

/** 태스크 하나의 실행 기록과 가장 최근 실행의 이벤트. */
export async function loadRuns(call: HostCall, taskKey: string): Promise<{ runs: RunRecord[]; events: RunEvent[] }> {
  try {
    const v = await call<unknown>("runner_runs", { taskKey });
    if (!isRecord(v)) return { runs: [], events: [] };
    return { runs: toRuns(v.runs), events: toEvents(v.events) };
  } catch {
    return { runs: [], events: [] };
  }
}

/** 모든 태스크를 가로지른 최근 실행(끝나지 않은 것이 앞). 못 읽으면 null이라 부르는 쪽이 이전 것을 둔다. */
export async function loadRecentRuns(call: HostCall, limit: number): Promise<RunRecord[] | null> {
  try {
    const v = await call<unknown>("runner_recent_runs", { limit });
    return Array.isArray(v) ? toRuns(v) : null;
  } catch {
    return null;
  }
}

/** 이 세션 id를 연 러너 실행의 태스크 키. 러너가 연 세션이 아니면 null. */
export async function taskOfSession(call: HostCall, sessionId: string): Promise<string | null> {
  // 세션이 아직 안 만들어진 실행은 sessionId가 비어 있다: 빈 값으로 그런 실행을 찾아내면 안 된다
  if (!sessionId) return null;
  const all = await loadRecentRuns(call, SESSION_LOOKUP_RUNS);
  return all?.find((r) => r.sessionId === sessionId)?.taskKey ?? null;
}

/**
 * 세션 id가 러너가 연 세션이면 그 태스크 키, 아니면 null. 세션 화면이 입력창을 닫고 「태스크로」 칩을 보일 때 쓴다.
 * 레일에 이미 읽어 둔 최근 실행에서 먼저 찾고, 없을 때만 호스트에 더 많이 물어본다.
 */
export function useRunnerTaskOfSession(call: HostCall | undefined, sessionId: string): string | null {
  const snap = useRunner();
  const known = sessionId ? (snap.recent.find((r) => r.sessionId === sessionId)?.taskKey ?? null) : null;
  const [found, setFound] = useState<{ id: string; key: string | null }>({ id: "", key: null });
  useEffect(() => {
    if (!sessionId || known || !call || !snap.supported) return;
    let gone = false;
    void taskOfSession(call, sessionId).then((key) => {
      if (!gone) setFound({ id: sessionId, key });
    });
    return () => {
      gone = true;
    };
  }, [sessionId, known, call, snap.supported, snap.version]);
  return known ?? (found.id === sessionId ? found.key : null);
}

// ---------------------------------------------------------------- 보여 주기

export const PHASE_LABEL: Record<RunPhase, string> = {
  running: "일하는 중",
  "waiting-answer": "답을 기다림",
  done: "보고함",
  failed: "실패",
  cancelled: "중지함",
  abandoned: "이어 가지 않음",
};

/** 걸음의 순서(0부터). 모르는 이름이면 -1. */
export const stepIndex = (step: string) => STEPS.indexOf(step as (typeof STEPS)[number]);

/** 일한 시간을 「3분 12초」처럼. */
export function formatDuration(secs: number): string {
  const s = Math.max(0, Math.round(secs));
  if (s < 60) return `${s}초`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}분 ${s % 60}초`;
  return `${Math.floor(m / 60)}시간 ${m % 60}분`;
}

/** 토큰 수를 「12.3k」처럼. */
export function formatTokens(n: number): string {
  return n >= 1_000_000 ? `${(n / 1_000_000).toFixed(1)}M` : n >= 1000 ? `${(n / 1000).toFixed(1)}k` : String(n);
}

export const formatCost = (usd: number) => (usd <= 0 ? "0달러" : usd < 0.01 ? "0.01달러 미만" : `${usd.toFixed(2)}달러`);

/** 이 태스크를 지금 돌고 있는(또는 끊겨 이어 갈) 실행. 없으면 undefined */
export const activeRunOf = (s: RunnerSnapshot, taskKey: string) => s.active.find((r) => r.taskKey === taskKey);
/** 이 태스크의 사람의 답을 기다리는 실행. 없으면 undefined */
export const waitingRunOf = (s: RunnerSnapshot, taskKey: string) => s.waiting.find((r) => r.taskKey === taskKey);

/** 지난 때를 「방금」 「3분 전」 「1시간 전」 「어제」 「2일 전」처럼. 한 달이 넘으면 「2달 전」. */
export function sinceKo(ms: number, now: number): string {
  const s = Math.max(0, Math.round((now - ms) / 1000));
  if (s < 60) return "방금";
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}분 전`;
  const h = Math.floor(m / 60);
  if (h < 24) return `${h}시간 전`;
  const d = Math.floor(h / 24);
  if (d === 1) return "어제";
  return d < 30 ? `${d}일 전` : `${Math.floor(d / 30)}달 전`;
}

/** 레일 「최근 실행」 줄의 점 색 단계. */
export type RunTone = "busy" | "wait" | "ok" | "err" | "off";
export const runTone = (r: RunRecord): RunTone =>
  r.phase === "running" ? "busy" : r.phase === "waiting-answer" ? "wait" : r.phase === "done" ? "ok" : r.phase === "failed" ? "err" : "off";

/** 줄의 아랫줄: 「상태 · 때 · 비용 · 대화 없음」. 일하는 중이면 때는 「3분째」다. */
export function runSubline(r: RunRecord, now: number): string {
  const when = r.phase === "running"
    ? Math.floor((now - r.startedAt) / 60000) < 1 ? "방금 시작" : `${Math.floor((now - r.startedAt) / 60000)}분째`
    : sinceKo(r.phase === "waiting-answer" ? r.startedAt : (r.endedAt ?? r.startedAt), now);
  const parts = [PHASE_LABEL[r.phase], when];
  if (r.endedAt !== null && r.costUsd > 0) parts.push(formatCost(r.costUsd));
  if (!r.sessionId) parts.push("대화 없음");
  return parts.join(" · ");
}

/** 줄에 마우스를 올리면 보이는 설명: 걸음, 질문, 보고 앞부분, 실패 까닭. */
export function runTip(r: RunRecord): string {
  switch (r.phase) {
    case "running": {
      const n = stepIndex(r.step);
      return `일하는 중 · ${r.step} 걸음${n >= 0 ? ` (${n + 1}/${STEPS.length})` : ""}`;
    }
    case "waiting-answer":
      return `러너의 질문: ${r.question ?? "답을 기다리고 있어요"}`;
    case "done":
      return r.summary ? `보고 앞부분: ${r.summary}` : "보고함";
    case "failed":
      return `실패 까닭: ${r.error ?? "알 수 없어요"}`;
    case "cancelled":
      return "사람이 중지했어요";
    default:
      return "태스크를 손으로 옮겨서 러너가 이어 가지 않아요";
  }
}

/** 러너 표시의 색 단계. 막대와 레일의 러너 표시가 같이 쓴다. */
export const runnerTone = (s: RunnerSnapshot): "is-warn" | "is-busy" | "is-wait" | "is-idle" =>
  !s.claudeAvailable ? "is-warn" : s.active.length > 0 ? "is-busy" : s.waiting.length ? "is-wait" : "is-idle";

/** 막대의 한 줄 요약. */
export function runnerSummary(s: RunnerSnapshot): string {
  if (!s.claudeAvailable) return "claude를 못 찾았어요";
  const parts: string[] = [];
  if (s.active.length) parts.push(`일하는 중 ${s.active.length}`);
  if (s.waiting.length) parts.push(`답 기다림 ${s.waiting.length}`);
  if (!parts.length) return s.settings.autoPickup ? "집을 일을 기다려요" : "쉬는 중";
  return parts.join(" · ");
}
