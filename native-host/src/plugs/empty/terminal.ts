// Empty implementation of @claude-plug/terminal for Part 4-free builds.
// Adheres to Chapter 17 (17.6) of aisurada Part 3 screen specifications.

export type SessionShellPlan =
  | { kind: 'plain' }
  | { kind: 'refuse'; notice?: string }
  | { kind: 'session'; start: (cols: number, rows: number, onData: (bytes: Uint8Array) => void) => Promise<{ fellBack: boolean }> };

export async function planSessionShell(
  _tabId: string,
  _sessionId?: string
): Promise<SessionShellPlan> {
  return { kind: 'plain' };
}
