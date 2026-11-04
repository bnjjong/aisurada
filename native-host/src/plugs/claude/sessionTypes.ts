// The session row the list (6장) builds and the session screen and rail read.
export interface ClaudeSession {
  sessionId: string;
  cwd: string;
  startedAt: number;
  kind: 'interactive' | 'background' | 'unknown';
  name?: string;
  /** The short job id of a background session (6.3). */
  id?: string;
  pid?: number;
  status: 'running' | 'blocked' | 'idle' | 'unknown';
  title?: string;
  branch?: string;
  processAlive?: boolean;
  heldByAgent?: boolean;
  endedAt?: number;
  isEndedRow?: boolean;
}
