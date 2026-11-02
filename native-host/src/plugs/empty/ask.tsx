// Empty implementation of @claude-plug/ask for Part 4-free builds.
// Adheres to Chapter 17 (17.4) of withlog Part 3 screen specifications.
import type { ReactNode } from 'react';

export function useNoteAsk(_props?: any): any {
  return {
    editor: {},
    panel: null,
    composeOpen: false,
    openCompose: () => {},
    closeCompose: () => {},
    ask: null,
    replyState: 'none',
  };
}

export function NoteAskFrame({ children }: { baseDir?: string; noteId?: string; children?: ReactNode }): ReactNode {
  return children ?? null;
}

export function EditorAskPlugins(_props?: any): null {
  return null;
}

export function AskComposePopover(_props?: any): null {
  return null;
}

export function AskHoverCard(_props?: any): null {
  return null;
}

// Backwards-compatible stub
export function NoteAskPanel(_props?: any): null {
  return null;
}
