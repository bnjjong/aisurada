// GitHub 앱 연결의 화면 쪽 상태. 설정 화면, 프로젝트 폼, 아젠다 화면이 같은 상태를 보도록 한 곳에 둔다.
// GitHub에 닿는 일은 모두 호스트(Rust)가 한다. 이 모듈은 호스트 명령을 부르고 결과를 기억할 뿐이다.
// 개인 키 내용은 화면으로 오지 않는다. 화면은 키 파일의 경로만 호스트에 넘긴다.
import { useSyncExternalStore } from "react";

export type HostCall = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export interface GithubStatus {
  connected: boolean;
  appId?: string | null;
  appName?: string | null;
  appSlug?: string | null;
  /** GitHub 전용 저장소를 받는 폴더. `<cloneRoot>/<owner>/<name>`에 생긴다 */
  cloneRoot: string;
}

export interface GithubRepo {
  fullName: string;
  id: number;
  private: boolean;
  defaultBranch: string;
  htmlUrl: string;
  installationId: number;
}

export interface GithubRepoList {
  repositories: GithubRepo[];
  errors: string[];
}

export interface CloneState {
  phase: "cloning" | "done" | "error";
  /** 받은 폴더(성공했을 때) */
  path?: string;
  /** 이번에 새로 받았으면 true, 이미 있어서 그대로 썼으면 false */
  cloned?: boolean;
  error?: string;
}

export interface GithubSnapshot {
  /** null이면 아직 확인하지 못했다(호스트가 없거나 확인 전) */
  status: GithubStatus | null;
  /** 마지막 확인·연결 시도가 낸 오류 글. 성공하면 빈 글 */
  error: string;
  clones: Record<string, CloneState>;
}

const EMPTY: GithubSnapshot = { status: null, error: "", clones: {} };
let snapshot: GithubSnapshot = EMPTY;
const listeners = new Set<() => void>();
const inFlight = new Map<string, Promise<CloneState>>();

function publish(patch: Partial<GithubSnapshot>) {
  snapshot = { ...snapshot, ...patch };
  for (const listener of [...listeners]) listener();
}

export const subscribeGithub = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};
export const getGithubSnapshot = () => snapshot;
export const useGithub = (): GithubSnapshot => useSyncExternalStore(subscribeGithub, getGithubSnapshot, getGithubSnapshot);

/** 시험이 서로 새지 않게 처음 상태로 돌린다. */
export function resetGithubStore() {
  snapshot = EMPTY;
  inFlight.clear();
}

/** 호스트가 돌려준 오류(문자열이거나 Error)를 사람이 읽을 글로 만든다. */
export function failureText(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  try {
    return JSON.stringify(error);
  } catch {
    return String(error);
  }
}

/** 연결 상태를 다시 읽는다. 호스트를 못 부르면 상태를 null로 두고 오류를 남긴다. */
export async function refreshGithub(call: HostCall): Promise<GithubStatus | null> {
  try {
    const status = await call<GithubStatus>("github_status");
    publish({ status, error: "" });
    return status;
  } catch (e) {
    publish({ status: null, error: failureText(e) });
    return null;
  }
}

/** App ID와 개인 키 파일 경로로 연결한다. 실패하면 오류 글을 남기고 null을 돌려준다. */
export async function connectGithub(call: HostCall, appId: string, pemPath: string): Promise<GithubStatus | null> {
  try {
    const status = await call<GithubStatus>("github_connect", { appId: appId.trim(), pemPath });
    publish({ status, error: "" });
    return status;
  } catch (e) {
    publish({ error: failureText(e) });
    return null;
  }
}

/** 연결을 끊는다: 키체인의 개인 키와 앱 정보를 지운다. 이미 받아 둔 저장소 폴더는 지우지 않는다. */
export async function disconnectGithub(call: HostCall): Promise<boolean> {
  try {
    await call<void>("github_disconnect");
    await refreshGithub(call);
    publish({ clones: {} });
    return true;
  } catch (e) {
    publish({ error: failureText(e) });
    return false;
  }
}

/** 앱 설치로 볼 수 있는 저장소 목록. 설치 하나가 실패해도 나머지는 돌려주고 실패는 `errors`에 담는다. */
export async function listGithubRepos(call: HostCall): Promise<GithubRepoList> {
  return call<GithubRepoList>("github_repositories");
}

/**
 * GitHub 저장소를 앱 전용 폴더에 받는다. 이미 있으면 그대로 쓴다. 같은 저장소를 받는 중이면 그 결과를 같이 기다린다.
 * 연결이 없으면 호스트를 부르지 않고 오류 상태를 남긴다.
 */
export function cloneRepo(call: HostCall, fullName: string): Promise<CloneState> {
  const running = inFlight.get(fullName);
  if (running) return running;
  if (!snapshot.status?.connected) {
    const state: CloneState = { phase: "error", error: "GitHub 연결이 없어 받지 못했어요. 설정에서 GitHub를 먼저 연결해 주세요." };
    publish({ clones: { ...snapshot.clones, [fullName]: state } });
    return Promise.resolve(state);
  }
  publish({ clones: { ...snapshot.clones, [fullName]: { phase: "cloning" } } });
  const job = call<{ path: string; cloned: boolean }>("github_clone", { fullName })
    .then((result): CloneState => ({ phase: "done", path: result.path, cloned: result.cloned }))
    .catch((e): CloneState => ({ phase: "error", error: failureText(e) }))
    .then((state) => {
      inFlight.delete(fullName);
      publish({ clones: { ...snapshot.clones, [fullName]: state } });
      return state;
    });
  inFlight.set(fullName, job);
  return job;
}

/** 받기 결과를 한 줄로 알린다: `docs를 받았어요`, `docs는 이미 있어요`, 실패는 이유까지. */
export function cloneSummary(fullName: string, state: CloneState): string {
  if (state.phase === "cloning") return `${fullName}을(를) 받는 중이에요…`;
  if (state.phase === "error") return `${fullName}을(를) 받지 못했어요: ${state.error ?? "이유를 알 수 없어요."}`;
  return state.cloned ? `${fullName}을(를) 받았어요: ${state.path}` : `${fullName}은(는) 이미 받아 둔 폴더가 있어요: ${state.path}`;
}
