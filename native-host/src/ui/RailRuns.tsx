// 레일 「에이전트」 탭의 「최근 실행」: 러너가 태스크마다 claude를 돌린 실행을 최신순으로 보여 준다(진행 중이 맨 위).
// 줄의 윗줄은 태스크 제목, 아랫줄은 상태·때·비용이다. 줄을 누르면 그 태스크의 상세로 가고, 대화는 거기 「실행」 영역의 「대화 보기」로 연다.
// 마우스를 올리면 질문·보고 앞부분·실패 까닭이 설명으로 보인다. 목록은 호스트가 알리는 러너 사건마다 `runnerStore`가 새로 읽는다.
import { useEffect, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import { taskAddress } from "../navigation/routes.ts";
import { PHASE_LABEL, RECENT_RUNS, runSubline, runTip, runTone, useRunner } from "./runnerStore.ts";
import { listTasks } from "./tasksClient.ts";

/** 태스크 키 → 제목. 실행 기록에는 제목이 없어서 태스크 목록에서 키로 맞춘다. 읽지 못하면 키만 보인다. */
function useTaskTitles(controller: WorkspaceController, keys: string): Record<string, string> {
  const runner = useRunner();
  const [titles, setTitles] = useState<Record<string, string>>({});
  useEffect(() => {
    if (!keys) return;
    let gone = false;
    void listTasks(controller.callEngine)
      .then(({ tasks }) => {
        if (!gone) setTitles(Object.fromEntries(tasks.map((t) => [t.key, t.title])));
      })
      .catch(() => {});
    return () => {
      gone = true;
    };
    // 목록은 러너 사건(version)이 올 때와 보이는 실행이 바뀔 때만 다시 읽는다
  }, [keys, runner.version, controller.callEngine]);
  return titles;
}

export function RailRuns({ controller, taskKey, now }: { controller: WorkspaceController; taskKey?: string; now?: number }) {
  const runner = useRunner();
  const runs = runner.recent.slice(0, RECENT_RUNS);
  const keys = runs.map((r) => r.taskKey).join(",");
  const titles = useTaskTitles(controller, keys);
  const at = now ?? Date.now();
  return (
    <section className="rail-runs" aria-label="최근 실행">
      <header>
        <span>최근 실행</span>
        <small>{runs.length}</small>
      </header>
      {runs.length === 0 ? (
        <p className="rr-empty">아직 러너가 돌린 실행이 없어요. Ready For Develop인 태스크를 러너가 집으면 여기에 쌓여요.</p>
      ) : (
        <ul>
          {runs.map((r) => (
            <li key={r.runId} className={`rr t-${runTone(r)}${taskKey === r.taskKey ? " is-open" : ""}${r.sessionId ? "" : " no-chat"}`}>
              <button
                type="button"
                className="rr-btn"
                title={runTip(r)}
                aria-label={`${r.taskKey} ${titles[r.taskKey] ?? ""} ${PHASE_LABEL[r.phase]}`.replace(/\s+/g, " ").trim()}
                onClick={() => controller.navigate(taskAddress(r.taskKey))}
              >
                <i className="rr-dot" aria-hidden="true" />
                <span className="rr-text">
                  <span className="rr-l1">
                    <b>{r.taskKey}</b>
                    {titles[r.taskKey] && <span className="rr-t">{titles[r.taskKey]}</span>}
                  </span>
                  <span className="rr-l2">{runSubline(r, at)}</span>
                </span>
              </button>
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}
