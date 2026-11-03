// Empty implementation of @claude-plug/session for Part 4-free builds.
// Adheres to Chapter 17 (17.5) of withlog Part 3 screen specifications.
import React from 'react';

/**
 * Empty implementation of SessionRoute for Part 4-free builds.
 * Displays "이 판에는 Claude 연동이 없어요" as specified in Chapter 17.5.
 */
export function SessionRoute(_props?: any): React.JSX.Element {
  return (
    <section className="empty-state">
      <h1>이 판에는 Claude 연동이 없어요</h1>
      <p>세션 화면은 Claude 연동(4부)을 넣고 지은 판에서 열려요.</p>
    </section>
  );
}

export function NoteSessionRail(_props?: {
  sessionId?: string;
  onClose?: () => void;
  onLeave?: () => void;
  chat?: {
    target: { noteId: string; subId?: string };
    noteMarkdown?: string;
    noteDir?: string;
  };
}): null {
  return null;
}

// Backwards-compatible stubs
export function SessionView({ sessionId }: { sessionId?: string }): React.JSX.Element {
  return <SessionRoute sessionId={sessionId} />;
}

export function SessionRail(_props?: any): null {
  return null;
}

export function useSessionBadge(_docId?: string): null {
  return null;
}
