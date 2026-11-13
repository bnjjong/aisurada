// 태스크 상세의 「실행」 영역: 러너가 이 태스크로 하는 일을 보여 주고, 사람이 할 일(지금 실행, 중지, 질문에 답하기)을 둔다.
// - Ready For Develop: 「지금 실행」
// - 일하는 중: 일곱 걸음 중 어디인지, 걸린 시간, 「중지」
// - Waiting Answer: 러너의 질문과 답 칸. 답하면 같은 claude 세션으로 이어 조사한다
// - 지난 실행들: 결과, 걸린 시간, 토큰, 진행 기록
import { useEffect, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { RunEvent, RunPhase, RunRecord } from "./runnerStore.ts";
import {
  STEPS, activeRunOf, answerTask, formatCost, formatDuration, formatTokens, loadRuns, runTask, stepIndex, stopTask, useRunner, waitingRunOf,
} from "./runnerStore.ts";
import type { RunnerSnapshot } from "./runnerStore.ts";
import type { TaskDetail } from "./tasksClient.ts";

const PHASE_LABEL: Record<RunPhase, string> = {
  running: "일하는 중",
  "waiting-answer": "답을 기다림",
  done: "보고함",
  failed: "실패",
  cancelled: "중지함",
  abandoned: "이어 가지 않음",
};

/** 시험과 눈으로 보는 확인에서 호스트 없이 처음 모양을 정할 때 쓴다(앱은 주지 않는다). */
export interface RunnerPanelPreset {
  snapshot?: RunnerSnapshot;
  runs?: RunRecord[];
  events?: RunEvent[];
  /** 「일하는 중」의 걸린 시간을 고정한다(밀리초 기준 현재 시각) */
  now?: number;
  openEvents?: boolean;
}

/** 유닉스 밀리초를 「11월 13일 21:30」로. */
export function clock(ms: number): string {
  if (!ms) return "";
  const d = new Date(ms);
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getMonth() + 1}월 ${d.getDate()}일 ${p(d.getHours())}:${p(d.getMinutes())}`;
}

export function RunnerPanel({ task, controller, preset }: { task: TaskDetail; controller: WorkspaceController; preset?: RunnerPanelPreset }) {
  const host = controller.callHost;
  const live = useRunner();
  const snap = preset?.snapshot ?? live;
  const [runs, setRuns] = useState<RunRecord[]>(preset?.runs ?? []);
  const [events, setEvents] = useState<RunEvent[]>(preset?.events ?? []);
  const [answer, setAnswer] = useState("");
  const [busy, setBusy] = useState(false);
  const [tick, setTick] = useState(() => preset?.now ?? Date.now());
  const active = activeRunOf(snap, task.key);
  const waiting = waitingRunOf(snap, task.key);

  useEffect(() => {
    if (preset?.runs) return;
    let gone = false;
    void loadRuns(host, task.key).then((r) => {
      if (gone) return;
      setRuns(r.runs);
      setEvents(r.events);
    });
    return () => {
      gone = true;
    };
  }, [host, task.key, task.status, snap.version, preset?.runs]);

  // 일하는 동안 걸린 시간이 흘러가게 한다
  useEffect(() => {
    if (!active || preset?.now) return;
    const id = window.setInterval(() => setTick(Date.now()), 1000);
    return () => window.clearInterval(id);
  }, [active?.runId, preset?.now]);

  if (!snap.supported) return null;
  const canRun = task.status === "ready-for-develop";
  if (!canRun && !active && !waiting && runs.length === 0) return null;

  const act = async (job: () => Promise<boolean>) => {
    setBusy(true);
    try {
      return await job();
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="task-sec runner-panel" aria-label="실행">
      <h2>실행</h2>
      {snap.error && <p role="alert" className="task-error">{snap.error}</p>}

      {waiting && (
        <div className="runner-ask" role="group" aria-label="러너의 질문">
          <p className="runner-ask-label">러너가 답을 기다려요</p>
          <blockquote>{waiting.question}</blockquote>
          <form
            onSubmit={(e) => {
              e.preventDefault();
              const text = answer.trim();
              if (!text) return;
              void act(async () => {
                const ok = await answerTask(host, task.key, text);
                if (ok) setAnswer("");
                return ok;
              });
            }}
          >
            <textarea
              aria-label="질문에 대한 답"
              rows={3}
              placeholder="답을 적어요. 모르면 모른다고 적어도 돼요"
              value={answer}
              onChange={(e) => setAnswer(e.target.value)}
            />
            <div className="runner-row">
              <button type="submit" className="button-primary" disabled={busy || !answer.trim()}>
                답하기
              </button>
              <span className="task-hint">답하면 같은 세션으로 이어서 조사해요.</span>
              <span className="task-spacer" />
              <button type="button" className="task-danger" disabled={busy} onClick={() => void act(() => stopTask(host, task.key))}>
                중지
              </button>
            </div>
          </form>
        </div>
      )}

      {active && (
        <div className="runner-now" role="group" aria-label="일하는 중">
          <ol className="runner-steps" aria-label="걸음">
            {STEPS.map((s, i) => {
              const at = stepIndex(active.step);
              return (
                <li key={s} className={i < at ? "is-done" : i === at ? "is-current" : ""} aria-current={i === at ? "step" : undefined}>
                  {s}
                </li>
              );
            })}
          </ol>
          <div className="runner-row">
            <span>
              <strong>{active.step || "시작"}</strong> 걸음 · {formatDuration(Math.max(0, tick - active.startedAt) / 1000)}째
              {active.turns > 0 ? ` · claude ${active.turns}번 부름` : ""}
            </span>
            <span className="task-spacer" />
            <button type="button" className="task-danger" disabled={busy} onClick={() => void act(() => stopTask(host, task.key))}>
              중지
            </button>
          </div>
        </div>
      )}

      {canRun && !active && (
        <div className="runner-row">
          <button type="button" className="button-primary" disabled={busy || !snap.claudeAvailable} onClick={() => void act(() => runTask(host, task.key))}>
            지금 실행
          </button>
          <span className="task-hint">
            {snap.settings.autoPickup ? "자동으로 집기가 켜져 있어서 곧 집어요. 급하면 눌러요." : "자동으로 집기가 꺼져 있어요. 누르면 지금 읽기 전용으로 조사를 시작해요."}
          </span>
        </div>
      )}

      {runs.length > 0 && (
        <>
          <h3>지난 실행</h3>
          <ul className="runner-runs">
            {runs.map((r, i) => (
              <li key={r.runId}>
                <div className="runner-row">
                  <span className={`runner-phase runner-phase-${r.phase}`}>{PHASE_LABEL[r.phase]}</span>
                  <span className="task-hint">{clock(r.startedAt)}</span>
                  <span className="task-hint">
                    {formatDuration(r.workedSecs)} · 질문 {r.questions}개 · 토큰 {formatTokens(r.inputTokens)}/{formatTokens(r.outputTokens)} · {formatCost(r.costUsd)}
                  </span>
                </div>
                {r.summary && <p className="runner-summary">{r.summary}</p>}
                {r.error && <p className="runner-fail">{r.error}</p>}
                {i === 0 && events.length > 0 && (
                  <details open={preset?.openEvents}>
                    <summary>진행 기록 {events.length}</summary>
                    <ol className="runner-events">
                      {events.map((e, n) => (
                        <li key={`${e.at}-${n}`}>
                          <span className="runner-event-step">{e.step}</span> {e.text}
                        </li>
                      ))}
                    </ol>
                  </details>
                )}
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  );
}
