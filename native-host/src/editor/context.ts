import { createContext } from 'react';

export interface EditorNote { id: string; title: string; created: string; agenda?: string; tier?: unknown; }
export interface EditorHost {
  native: boolean;
  writeImage(directory: string, name: string, bytes: Uint8Array): Promise<unknown>;
  openFile(path: string): Promise<void>;
  openPath?(path: string): Promise<void>;
  openURL(url: string): Promise<void>;
  confirm(message: string): Promise<boolean>;
  warn(message: string): Promise<void>;
}
export interface EditorFeatures {
  notes?: EditorNote[]; currentId?: string; showRecords?: boolean; noteDirectory?: string;
  children?: { id: string; title: string }[];
  comments?: Record<string, string>;
  openNote?(id: string): void;
  openChild?(id: string): void;
  createChild?(): Promise<{ id: string; title: string } | null>;
  comment?(id: string, body: string, quote: string): Promise<unknown>;
  openComment?(id: string): void;
  host?: EditorHost;
}
export const EditorContext = createContext<EditorFeatures>({});
