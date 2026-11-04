// Implementation of @claude-plug/terminal for Part 4 Claude-enabled builds.
// Adheres to Chapter 10 (10.9, 10.10, 10.11) of withlog Part 4 Claude integration specifications.
import { Channel, invoke } from '@tauri-apps/api/core';
import { refreshSessions, getSessionStoreSnapshot } from './projects.tsx';

export type SessionShellPlan =
  | { kind: 'plain' }
  | { kind: 'refuse'; notice?: string }
  | { kind: 'session'; start: (cols: number, rows: number, onData: (bytes: Uint8Array) => void) => Promise<{ fellBack: boolean }> };

export const LIVE_ELSEWHERE_NOTICE = '이 세션은 지금 다른 터미널에서 돌고 있어요. 그 터미널에서 이어 가 주세요. 여기서 이어 열면 두 곳이 한 대화에 써서 대화가 갈라져요.';
export const AGENT_BUSY_NOTICE = 'withlog 가 이 세션에서 답을 쓰고 있어요. 답이 끝나면 탭을 다시 열어 주세요.';

function decode(base64: string): Uint8Array {
  const raw = atob(base64);
  const bytes = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
  return bytes;
}

export interface PlanHost {
  alive(key: string): Promise<boolean>;
  vault(): Promise<string | null>;
  /** Reads the list again; a failure leaves the list the screen already holds. */
  refresh(vault: string | null): Promise<void>;
  rows(): Array<{ sessionId: string; processAlive?: boolean; heldByAgent?: boolean; id?: string; cwd: string }>;
  firstCwd(sessionId: string): Promise<string | null>;
  release(vault: string, sessionId: string): Promise<string | null>;
  start(key: string, cols: number, rows: number, action: Record<string, unknown>, onData: (bytes: Uint8Array) => void): Promise<{ fellBack: boolean }>;
}

/**
 * 10.9: what the terminal cell does for `term:<session id>`.
 * 1. A shell alive on the key: plain, and the host reattaches. Nothing is planted.
 * 2. With a vault, the list is read again and the session's first folder is read with it; without one only the held list is used.
 * 3. A row alive with no short id and not held by an agent: refuse.
 * 4. A row alive with a short id: attach.
 * 5. Otherwise resume, with the first folder or the row's folder (none for an agent-held row).
 * 6. For a resume with a vault, `release_agent_session` is asked; `busy` refuses, anything else passes.
 * 7. The session shell.
 */
export async function planSessionShellWith(host: PlanHost, tabId: string, sessionId?: string): Promise<SessionShellPlan> {
  if (!sessionId) return { kind: 'plain' };
  if (await host.alive(tabId).catch(() => false)) return { kind: 'plain' };
  const vault = await host.vault().catch(() => null);
  let firstCwd: string | null = null;
  if (vault) {
    const [, cwd] = await Promise.all([host.refresh(vault).catch(() => {}), host.firstCwd(sessionId).catch(() => null)]);
    firstCwd = cwd;
  }
  const row = host.rows().find((item) => item.sessionId === sessionId);
  let action: Record<string, unknown>;
  if (row && row.processAlive === true && row.heldByAgent !== true && !row.id) return { kind: 'refuse', notice: LIVE_ELSEWHERE_NOTICE };
  if (row && row.processAlive === true && row.id) {
    action = { mode: 'attach', sessionId: row.id };
  } else {
    const cwd = firstCwd ?? (row && row.heldByAgent !== true ? row.cwd : undefined);
    action = { mode: 'resume', sessionId, ...(cwd ? { cwd } : {}) };
    if (vault) {
      const released = await host.release(vault, sessionId).catch(() => null);
      if (released === 'busy') return { kind: 'refuse', notice: AGENT_BUSY_NOTICE };
    }
  }
  return { kind: 'session', start: (cols, rows, onData) => host.start(tabId, cols, rows, action, onData) };
}

const tauriPlanHost: PlanHost = {
  alive: (key) => invoke<boolean>('pty_alive', { key }),
  vault: () => invoke<string | null>('vault_root'),
  refresh: (vault) => refreshSessions(vault ?? undefined),
  rows: () => getSessionStoreSnapshot().sessions,
  firstCwd: async (sessionId) => {
    const reply = await invoke<{ status: number; stdout: string }>('sidecar_call', { argv: ['session-titles', sessionId] });
    if (reply.status !== 0) return null;
    const cwd = JSON.parse(reply.stdout)?.sessions?.[0]?.cwd;
    return typeof cwd === 'string' && cwd ? cwd : null;
  },
  release: (vault, sessionId) => invoke<string>('release_agent_session', { vault, sessionId }).catch(() => null),
  start: (key, cols, rows, action, onData) => {
    const channel = new Channel<string>();
    channel.onmessage = (chunk) => {
      try {
        onData(decode(chunk));
      } catch {
        /* a chunk that is not base64 is dropped */
      }
    };
    return invoke<{ fellBack: boolean }>('pty_start_session', { key, cols, rows, sessionAction: action, onData: channel });
  },
};

export async function planSessionShell(tabId: string, sessionId?: string): Promise<SessionShellPlan> {
  return planSessionShellWith(tauriPlanHost, tabId, sessionId);
}
