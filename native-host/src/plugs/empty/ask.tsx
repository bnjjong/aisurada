// Empty implementation of @claude-plug/ask for Part 4-free builds.
// Adheres to Chapter 17 (17.4) of withlog Part 3 screen specifications.
import type { ReactNode } from 'react';
import type { Note } from '../../workspace/types.ts';

/**
 * Note body port providing writing capabilities for main and sub notes (Chapter 10, 17.4).
 * Adheres to noteBodyPort contract with 13 fields.
 */
export interface NoteBodyPort {
  kind?: 'main' | 'sub';
  id?: string;
  parent?: string;
  write(body: string): Promise<void> | void;
  saveDraft?(body: string): void;
  read?(): string;
  autosaveBody?(body?: string): Promise<void> | void;
  currentBody?(): string;
  commitTitle?(title?: string): Promise<boolean | void> | void;
  protect?(enabled?: boolean): void;
  flush?(mode?: string): Promise<any> | void;
  discard?(): Promise<void> | void;
  keep?(): Promise<any> | void;
  restore?(body: string): Promise<any> | void;
  conflicted?(): boolean;
  editing?(): boolean;
}

export interface AskEditorProps {
  onAskAi?: (quote: string, question: string, turnId: string, images?: File[]) => void;
  askDocId?: string;
  onAskFollowUp?: (question: string, underlineId: string, images?: File[]) => void;
}

export interface NoteAskInput {
  doc: Note;
  port: NoteBodyPort;
  baseDir?: string;
  vault?: string;
  currentDraft: () => string;
  timeoutSeconds?: () => number;
  repo?: string;
  root?: string;
  rootSessionId?: string;
  openDoc?: (id: string) => boolean | void;
  onFollowUp?: (question: string, underlineId: string, images?: File[]) => void;
}

export interface NoteAskOutput {
  editor: AskEditorProps;
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

export function EditorAskPlugins(_props?: {
  docId?: string;
  onAsk?: (quote: string, question: string, turnId: string, images?: File[]) => void;
  onFollowUp?: (question: string, underlineId: string, images?: File[]) => void;
  onOpenCompose?: (quote: string, rect: { left: number; top: number }, markId?: string) => void;
}): null {
  return null;
}

export interface AskComposeState {
  left: number;
  top: number;
  quote: string;
  markId?: string;
}

export interface AskComposePopoverProps {
  state: AskComposeState;
  onSubmit: (question: string, images?: File[]) => void;
  onCancel: () => void;
}

export function AskComposePopover(_props: AskComposePopoverProps): null {
  return null;
}

export interface AskHoverCardProps {
  docId: string;
  turnId: string;
  onFollowUp?: (question: string, underlineId: string, images?: File[]) => void;
  onFocusInput?: () => void;
  onClose?: () => void;
  pinned?: boolean;
  boundaryRef?: React.RefObject<HTMLElement | null>;
}

export function AskHoverCard(_props: AskHoverCardProps): null {
  return null;
}

// Backwards-compatible stub
export function NoteAskPanel(_props?: any): null {
  return null;
}
