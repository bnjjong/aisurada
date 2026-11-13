// 태스크 페이지 위의 러너 막대: 러너가 지금 무엇을 하는지, 스스로 집을지, 동시에 몇 개까지 돌릴지.
// 러너 자체는 호스트가 돈다(`runner/`). 이 막대는 상태를 보여 주고 설정을 호스트에 적을 뿐이다.
import type { HostCall, RunnerSnapshot } from "./runnerStore.ts";
import { MAX_CONCURRENCY, MIN_CONCURRENCY, runnerSummary, saveRunnerSettings, useRunner } from "./runnerStore.ts";

export function RunnerBar({ call, preset }: { call: HostCall; preset?: RunnerSnapshot }) {
  const live = useRunner();
  const s = preset ?? live;
  if (!s.supported) return null;
  const busy = s.active.length > 0;
  const tone = !s.claudeAvailable ? "is-warn" : busy ? "is-busy" : s.waiting.length ? "is-wait" : "is-idle";
  const change = (patch: Partial<RunnerSnapshot["settings"]>) => void saveRunnerSettings(call, { ...s.settings, ...patch });
  return (
    <div className="runner-bar" role="group" aria-label="러너">
      <span className={`runner-pill ${tone}`}>
        <i aria-hidden="true" />
        러너 · {runnerSummary(s)}
      </span>
      <label className="task-check" title="Ready For Develop가 된 태스크를 러너가 스스로 집어요. 끄면 태스크의 「지금 실행」을 눌렀을 때만 돌아요.">
        <input type="checkbox" checked={s.settings.autoPickup} onChange={(e) => change({ autoPickup: e.target.checked })} />
        Ready가 되면 스스로 집기
      </label>
      <label className="task-check">
        동시에
        <select
          aria-label="동시에 돌릴 수"
          value={s.settings.concurrency}
          onChange={(e) => change({ concurrency: Number(e.target.value) })}
        >
          {Array.from({ length: MAX_CONCURRENCY - MIN_CONCURRENCY + 1 }, (_, i) => MIN_CONCURRENCY + i).map((n) => (
            <option key={n} value={n}>
              {n}개
            </option>
          ))}
        </select>
      </label>
      {s.error && <span role="alert" className="runner-error">{s.error}</span>}
      {!s.claudeAvailable && <span className="task-hint">claude 실행 파일을 찾으면 러너가 일해요(AISURADA_CLAUDE_BIN으로 알려 줄 수도 있어요).</span>}
    </div>
  );
}
