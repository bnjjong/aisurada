// AI가 노트에 쓴 「정할 것」(결정 블록) 가운데 사람의 답을 기다리는 노트를 레일 배지와 노트 목록이 같이 보도록 한 곳에 둔다.
// 목록은 엔진(`pending-decisions`)이 알려 주고 이 모듈은 부르고 기억할 뿐이다. 알림(창이 닫혀 있어도 알리는 쪽)은 호스트(`decisions.rs`)가 한다.
import { fromArgv } from "../data/commands.ts";
import { docAddress, subAddress } from "../navigation/routes.ts";
import { createExternalStore, isRecord } from "./engineClient.ts";
import type { EngineCall } from "./engineClient.ts";

export interface PendingDecisionNote {
  /** 메인 노트 id */
  id: string;
  /** 결정 블록이 후속 노트에 있으면 그 id */
  subId: string | null;
  title: string;
  answered: number;
  total: number;
}

export interface DecisionsSnapshot {
  notes: PendingDecisionNote[];
  /** 한 번이라도 읽었는지 */
  loaded: boolean;
}

/** 배경에서 다시 읽는 간격(밀리초). 엔진은 앱 안에서 바로 돌아 싸고, 사람이 「답 다 했음」을 체크하면 배지가 곧 줄어야 해서 짧게 둔다. */
export const POLL_MS = 15_000;

const EMPTY: DecisionsSnapshot = { notes: [], loaded: false };

const count = (v: unknown) => (typeof v === "number" && Number.isFinite(v) && v >= 0 ? Math.floor(v) : 0);

/** 엔진 응답을 읽는다. 모양이 틀린 줄은 건너뛴다. */
export function toPendingNotes(out: unknown): PendingDecisionNote[] {
  if (!isRecord(out) || !Array.isArray(out.notes)) return [];
  return out.notes.flatMap((row): PendingDecisionNote[] => {
    if (!isRecord(row) || typeof row.id !== "string" || !row.id) return [];
    return [{
      id: row.id,
      subId: typeof row.subId === "string" && row.subId ? row.subId : null,
      title: typeof row.title === "string" && row.title.trim() ? row.title : "제목 없음",
      answered: count(row.answered),
      total: count(row.total),
    }];
  });
}

const store = createExternalStore<DecisionsSnapshot>(
  EMPTY,
  (prev, next) => JSON.stringify(prev) === JSON.stringify(next),
);

export const subscribeDecisions = store.subscribe;
export const getDecisionsSnapshot = store.getSnapshot;
/** 답을 기다리는 노트들 */
export const usePendingDecisions = (): PendingDecisionNote[] => store.useStore().notes;

/** 시험이 서로 새지 않게 처음 상태로 돌린다. */
export function resetDecisionsStore() {
  store.publish(EMPTY);
}

/** 이미 읽은 목록을 그대로 반영한다. */
export function setPendingDecisions(notes: PendingDecisionNote[]) {
  store.publish({ notes, loaded: true });
}

/** 엔진에서 목록을 읽는다. 못 읽으면(엔진이 낡았거나 vault가 없음) 이전 목록을 그대로 둔다. */
export async function refreshDecisions(call: EngineCall): Promise<void> {
  try {
    setPendingDecisions(toPendingNotes(await call(fromArgv(["pending-decisions"]))));
  } catch {
    // 배지는 있어도 없어도 되는 안내다. 오류를 화면에 띄우지 않는다
  }
}

/**
 * 배경에서 목록을 따라가게 한다: 지금 한 번 읽고, 창이 눈에 띌 때마다, 그리고 {@link POLL_MS}마다 읽는다. 끄는 함수를 돌려준다.
 * 결정 알림이 오면 호출한 쪽이 {@link refreshDecisions}를 따로 부른다.
 */
export function startDecisionsWatch(call: EngineCall, target: { addEventListener: (t: string, f: () => void) => void; removeEventListener: (t: string, f: () => void) => void }): () => void {
  const read = () => void refreshDecisions(call);
  read();
  target.addEventListener("focus", read);
  const timer = setInterval(read, POLL_MS);
  return () => {
    target.removeEventListener("focus", read);
    clearInterval(timer);
  };
}

/** 기다리는 노트를 여는 화면 주소. 후속 노트면 그 탭을 연다. */
export const decisionAddress = (note: Pick<PendingDecisionNote, "id" | "subId">): string =>
  note.subId ? subAddress(note.id, [note.subId]) : docAddress(note.id);

/** 노트 하나(후속 노트 포함)에서 답을 기다리는 결정 수를 모은다. 기다리는 것이 없으면 null이다. */
export function waitingOn(notes: readonly PendingDecisionNote[], noteId: string): { total: number; answered: number } | null {
  const mine = notes.filter((n) => n.id === noteId);
  if (!mine.length) return null;
  return mine.reduce((sum, n) => ({ total: sum.total + n.total, answered: sum.answered + n.answered }), { total: 0, answered: 0 });
}
