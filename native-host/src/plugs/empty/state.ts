// Empty implementation of @claude-plug/state for Part 4-free builds.
// Adheres to Chapter 17 (17.3) of withlog Part 3 screen specifications.

export const CLAUDE_ENABLED = false;

const EMPTY_ASK_STATE = {
  pending: {} as Record<string, Record<string, { queued?: boolean }>>,
  unseen: {} as Record<string, string>,
};

export function useAskState<T = typeof EMPTY_ASK_STATE>(
  selector: (state: typeof EMPTY_ASK_STATE) => T = (s) => s as unknown as T
): T {
  return selector(EMPTY_ASK_STATE);
}

const EMPTY_RAIL_STATE = {
  rails: {} as Record<string, string>,
  openRail: null as ((noteKey: string, sessionId: string) => void) | null,
  closeRail: (_noteKey?: string) => {},
};

export function useNoteRail<T = typeof EMPTY_RAIL_STATE>(
  selector: (state: typeof EMPTY_RAIL_STATE) => T = (s) => s as unknown as T
): T {
  return selector(EMPTY_RAIL_STATE);
}

export function noteRailNow(): typeof EMPTY_RAIL_STATE {
  return EMPTY_RAIL_STATE;
}

export function waitingAnswers(_noteKeys?: Set<string>): number {
  return 0;
}

export function unsavedAnswers(): number {
  return 0;
}
