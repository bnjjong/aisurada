// The note-level facts the ask parts need and the part 3 note screen supplies through NoteAskFrame.
import { createContext } from 'react';

export interface AskContextValue {
  /** The absolute path of the note's folder. Unknown for an old single-file note. */
  baseDir?: string;
  /** The note id (the sub note id for a sub note). It is the store key. */
  noteId?: string;
}

export const AskContext = createContext<AskContextValue>({});
