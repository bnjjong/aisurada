// 승인이 필요한 변경의 수를 레일의 배지와 스킬·에이전트 페이지가 같이 보도록 한 곳에 둔다(#53).
// 수는 엔진(`list-resource-changes`)이 알려 주고, 이 모듈은 부르고 기억할 뿐이다. 알림(창이 닫혀 있어도 알리는 쪽)은 호스트가 따로 한다.
import { useSyncExternalStore } from "react";
import { loadCounts } from "./approvalsClient.ts";
import type { ApprovalCounts } from "./approvalsClient.ts";
import type { EngineCall } from "./resourcesClient.ts";

export interface ApprovalsSnapshot {
  counts: ApprovalCounts;
  /** 한 번이라도 읽었는지 */
  loaded: boolean;
}

/** 배경에서 수를 다시 읽는 간격(밀리초). 엔진을 부를 때마다 자바가 뜨므로 길게 잡고, 창이 눈에 띌 때와 알림이 올 때는 바로 읽는다. */
export const POLL_MS = 60_000;

const EMPTY: ApprovalsSnapshot = { counts: { pending: 0, unapproved: 0 }, loaded: false };
let snapshot: ApprovalsSnapshot = EMPTY;
const listeners = new Set<() => void>();

function publish(next: ApprovalsSnapshot) {
  if (next.loaded === snapshot.loaded && next.counts.pending === snapshot.counts.pending && next.counts.unapproved === snapshot.counts.unapproved) return;
  snapshot = next;
  for (const listener of [...listeners]) listener();
}

export const subscribeApprovals = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};
export const getApprovalsSnapshot = () => snapshot;
export const useApprovalCounts = (): ApprovalCounts =>
  useSyncExternalStore(subscribeApprovals, getApprovalsSnapshot, getApprovalsSnapshot).counts;

/** 시험이 서로 새지 않게 처음 상태로 돌린다. */
export function resetApprovalsStore() {
  snapshot = EMPTY;
}

/** 패널이 이미 읽은 수를 그대로 반영한다(다시 부르지 않는다). */
export function setApprovalCounts(counts: ApprovalCounts) {
  publish({ counts, loaded: true });
}

/** 엔진에서 수를 읽는다. 못 읽으면(엔진이 낡았거나 vault가 없음) 이전 수를 그대로 둔다. */
export async function refreshApprovalCounts(call: EngineCall): Promise<void> {
  try {
    setApprovalCounts(await loadCounts(call));
  } catch {
    // 배지는 있어도 없어도 되는 안내다. 오류를 화면에 띄우지 않는다
  }
}

/**
 * 배경에서 수를 따라가게 한다: 지금 한 번 읽고, 창이 눈에 띌 때마다, 그리고 {@link POLL_MS}마다 읽는다. 끄는 함수를 돌려준다.
 * 승인 알림이 오면 호출한 쪽이 {@link refreshApprovalCounts}를 따로 부른다.
 */
export function startApprovalsWatch(call: EngineCall, target: { addEventListener: (t: string, f: () => void) => void; removeEventListener: (t: string, f: () => void) => void }): () => void {
  const read = () => void refreshApprovalCounts(call);
  read();
  target.addEventListener("focus", read);
  const timer = setInterval(read, POLL_MS);
  return () => {
    target.removeEventListener("focus", read);
    clearInterval(timer);
  };
}
