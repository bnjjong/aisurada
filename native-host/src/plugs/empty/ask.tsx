// Empty implementation of @claude-plug/ask for Part 4-free builds.
// Adheres to Chapter 17 (17.4) of withlog Part 3 screen specifications.
import type { ReactNode } from 'react';

export interface NoteAskInput {
  doc?: any;
  port?: any;
  baseDir?: string;
  currentDraft?: () => string;
  onFollowUp?: (question: string, underlineId: string, images?: any[]) => void;
}

export interface NoteAskOutput {
  editor: any;
  panel: ReactNode;
}

export function useNoteAsk(_props?: NoteAskInput): NoteAskOutput {
  return {
    editor: {},
    panel: null,
  };
}

export function NoteAskFrame({ children }: { baseDir?: string; noteId?: string; children?: ReactNode }): ReactNode {
  return children ?? null;
}

export function EditorAskPlugins(_props?: any): null {
  return null;
}

export function AskComposePopover(_props?: {
  state?: any;
  onSubmit?: (text: string) => void;
  onCancel?: () => void;
}): null {
  return null;
}

export function AskHoverCard(_props?: any): null {
  return null;
}

// Backwards-compatible stub
export function NoteAskPanel(_props?: any): null {
  return null;
}
