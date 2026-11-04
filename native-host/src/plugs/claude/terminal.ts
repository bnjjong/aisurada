// Implementation of @claude-plug/terminal for Part 4 Claude-enabled builds.
// Full implementation in Step 9 (Chapter 10).

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
