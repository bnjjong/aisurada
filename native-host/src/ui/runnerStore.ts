// 러너의 화면 쪽 상태. 태스크 목록, 상세, 상단 막대가 같은 상태를 보도록 한 곳에 둔다.
// 태스크를 집고 claude에게 맡기고 보고·질문을 남기는 일은 모두 호스트(Rust, `runner/`)가 한다.
// 이 모듈은 호스트 명령을 부르고, 호스트가 알려 주는 사건(`ais://runner`)을 받아 기억할 뿐이다.
import { useSyncExternalStore } from "react";

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

/** 일곱 걸음. 호스트가 기록에 남기는 이름과 같다. */
export const STEPS = ["받기", "집기", "판단", "받아오기", "일하기", "내보내기", "정리"] as const;

const EMPTY: RunnerSnapshot = {
  settings: DEFAULT_RUNNER_SETTINGS,
  claudeAvailable: true,
  running: 0,
  active: [],
  waiting: [],
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
    set({
      settings: toSettings(v.settings),
      claudeAvailable: v.claudeAvailable !== false,
      running: num(v.running),
      active: toRuns(v.active),
      waiting: toRuns(v.waiting),
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

// ---------------------------------------------------------------- 보여 주기

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

/** 막대의 한 줄 요약. */
export function runnerSummary(s: RunnerSnapshot): string {
  if (!s.claudeAvailable) return "claude를 못 찾았어요";
  const parts: string[] = [];
  if (s.active.length) parts.push(`일하는 중 ${s.active.length}`);
  if (s.waiting.length) parts.push(`답 기다림 ${s.waiting.length}`);
  if (!parts.length) return s.settings.autoPickup ? "집을 일을 기다려요" : "쉬는 중";
  return parts.join(" · ");
}
