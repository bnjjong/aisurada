// 엔진 호출 및 스토어 공통 틀(chassis).
// 1) 화면이 엔진을 부르는 공통 타입(`EngineCall`)과 엔진 오류 해석(`explainEngineError`, `extractErrorMessage`).
// 2) React `useSyncExternalStore` 기반의 판에 박힌 리스너·스냅샷 관리 틀(`createExternalStore`).
import { useSyncExternalStore } from "react";
import type { Call } from "../data/commands.ts";

/** 엔진 호출 한 번. 문자열 목록(명령줄 길)이나 호출 객체(`engine_call`이 맡은 명령)를 받는다. */
export type EngineCall = (request: string[] | Call) => Promise<unknown>;

/** 임의의 값이 순수 객체 레코드인지 확인한다. */
export function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === "object" && v !== null && !Array.isArray(v);
}

/** 오류 객체 또는 값에서 오류 문자열(stderr/message/String)을 뽑아낸다. */
export function extractErrorMessage(e: unknown): string {
  if (isRecord(e) && typeof e.stderr === "string") return e.stderr;
  if (e instanceof Error) return e.message;
  return String(e);
}

/**
 * 엔진 오류를 사람이 읽을 수 있는 한국어 설명으로 바꾼다.
 * AIS-412(동시 쓰기 충돌) 및 AIS-### 에러 코드를 기본으로 풀고, 호출부의 추가 매핑을 지원한다.
 */
export function explainEngineError(
  e: unknown,
  customClassifier?: (stderr: string) => string | undefined,
): string {
  const stderr = extractErrorMessage(e);
  if (customClassifier) {
    const custom = customClassifier(stderr);
    if (custom) return custom;
  }
  if (/AIS-412/.test(stderr)) {
    return "다른 곳(AI나 다른 창)에서 먼저 고쳤어요. 목록을 새로 불러온 뒤 다시 고쳐 주세요.";
  }
  const m = /AIS-\d+\s+(.*)/s.exec(stderr);
  return (m ? m[1] : stderr).trim() || "알 수 없는 오류가 났어요.";
}

/** React useSyncExternalStore와 결합된 외부 스토어 계약 */
export interface ExternalStore<T> {
  subscribe: (listener: () => void) => () => void;
  getSnapshot: () => T;
  publish: (nextOrUpdater: T | ((prev: T) => T)) => void;
  useStore: () => T;
}

/**
 * React 18/19 외부 스토어 공통 틀.
 * 리스너 구독 관리, 스냅샷 변경 감지, useSyncExternalStore 훅을 일원화한다.
 */
export function createExternalStore<T>(
  initialSnapshot: T,
  areEqual: (prev: T, next: T) => boolean = (a, b) => a === b,
): ExternalStore<T> {
  let snapshot = initialSnapshot;
  const listeners = new Set<() => void>();

  const subscribe = (listener: () => void) => {
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  };

  const getSnapshot = () => snapshot;

  const publish = (nextOrUpdater: T | ((prev: T) => T)) => {
    const next = typeof nextOrUpdater === "function"
      ? (nextOrUpdater as (prev: T) => T)(snapshot)
      : nextOrUpdater;
    if (areEqual(snapshot, next)) return;
    snapshot = next;
    for (const listener of [...listeners]) {
      listener();
    }
  };

  const useStore = () => useSyncExternalStore(subscribe, getSnapshot, getSnapshot);

  return {
    subscribe,
    getSnapshot,
    publish,
    useStore,
  };
}
