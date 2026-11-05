export type WindowContext = { kind: 'main' } | { kind: 'note'; docId: string };

export function windowContext(pathname: string): WindowContext {
  const match = /^\/note-window\/([^/]+)$/.exec(pathname);
  if (match) {
    try { return { kind: 'note', docId: decodeURIComponent(match[1]) }; }
    catch { /* A malformed address belongs to the main router. */ }
  }
  return { kind: 'main' };
}

export interface StoragePort {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
}

export interface FileEntry {
  name: string;
  isFile: boolean;
  isDirectory: boolean;
  isSymlink: boolean;
}

export interface FilePort {
  exists(path: string): Promise<boolean>;
  readDir(path: string): Promise<FileEntry[]>;
  readTextFile(path: string): Promise<string>;
  writeTextFile(path: string, text: string): Promise<void>;
  mkdir(path: string, options: { recursive: boolean }): Promise<void>;
  rename(from: string, to: string): Promise<void>;
  remove(path: string, options?: { recursive: boolean }): Promise<void>;
}

export interface HostPort {
  invoke<T>(command: string, args?: Record<string, unknown>): Promise<T>;
  documentDir(): Promise<string>;
  chooseDirectory(): Promise<string | null>;
  confirmLeave(message: string): Promise<boolean>;
}

// Editors and report forms register their pending work here when introduced.
export interface PendingWork {
  flushReports(): Promise<void>;
  flushEditors(): Promise<void>;
  waitForWrites(timeoutMs: number): Promise<boolean>;
  unsavedNotes(): number;
  flushNote?(id: string): Promise<void>;
  unsavedForNote?(id: string): number;
}

export interface WorkspaceServices {
  host: HostPort;
  files: FilePort;
  storage: StoragePort;
  watch(root: string, changed: (paths: string[]) => void): Promise<() => void>;
  noteWindowsChanged?(changed: (ids: string[]) => void): Promise<() => void>;
  selfWritesChanged?(changed: (event: { root: string; relPath: string; from: string }) => void): Promise<() => void>;
  windowLabel?: string;
  sanitizeHTML?(html: string): string;
  menu?(received: (id: string) => void): Promise<() => void>;
  navigateRequested?(received: (nav: { path: string; rail?: string | null; sub?: string | null }) => void): Promise<() => void>;
  deepLinks?: import('../interaction/links.ts').DeepLinkSource;
  developmentLinks?(received: (urls: string[]) => void): () => void;
  closeWindow?(): Promise<void>;
  closeRequested?(handler: () => void): Promise<() => void>;
  /** The note window's logical size and position, each time it is moved or resized. */
  noteGeometryChanged?(changed: (geometry: { width: number; height: number; x: number; y: number }) => void): Promise<() => void>;
  native?: boolean;
  mac?: boolean;
  navigate(path: string, replace: boolean): void;
  pending: PendingWork;
  now(): Date;
  uuid(): string;
  sleep(ms: number): Promise<void>;
  warn(message: string, error?: unknown): void;
}

export type Row = Record<string, unknown>;
export interface Agenda extends Row { id: string; slug: string; title: string; }
export interface Note extends Row {
  id: string;
  title: string;
  body: string;
  agenda: string;
  agendaId: string;
  format: string;
  created: string;
  date: string;
  time: string;
}
export interface Workspace {
  path: string;
  name: string;
  sample: boolean;
  today: string;
  agendas: Agenda[];
  projects: Row[];
  notes: Note[];
  templates: Row[];
  reports: Row[];
  trash: Row[];
  skipped: Row[];
  conflicts: Row[];
  noteReadFailed: boolean;
}

export interface RecentWorkspace { path: string; name: string; opened: string; created?: boolean; sample?: boolean; }
/** 설정에서 고르는 화면 모양. 물성(tactile), 격자(grid), 유리(glass), 모던(modern), 이전 모양(classic). */
export type Skin = 'tactile' | 'grid' | 'glass' | 'modern' | 'classic';

export interface WorkspaceState {
  prefs: import('../ui/model.ts').Preferences;
  settingsOpen: boolean;
  form: { kind: 'note' | 'agenda' | 'project'; id?: string; agenda?: string; project?: string } | null;
  hiddenAgendas: string[];
  previewOpened: Note | null;
  booting: boolean;
  busy: boolean;
  route: string;
  workspace: Workspace | null;
  notice: string | null;
  fatal: string | null;
  recent: RecentWorkspace[];
  home: string;
  theme: 'light' | 'dark' | 'system';
  /** 화면 모양. 밝기(theme)와는 따로 고른다. */
  skin: Skin;
  tabs: TabsSnapshot;
  noteWindows: string[];
  searchOpen: boolean;
  preview: string | null;
  subNotes: { root: string; ids: string[]; arrival: string | null } | null;
  vault: import('../data/vault.ts').VaultMemory;
  noteCloseDialog?: { reasons: string[] } | null;
}

export const asRow = (value: unknown): Row | null =>
  value !== null && typeof value === 'object' && !Array.isArray(value) ? value as Row : null;
export const asText = (value: unknown): string => typeof value === 'string' ? value : '';
export const rows = (value: unknown, envelope?: string): Row[] => {
  const source = Array.isArray(value) ? value : envelope ? asRow(value)?.[envelope] : null;
  return Array.isArray(source) ? source.map(asRow).filter((r): r is Row => r !== null) : [];
};
export const workspaceName = (path: string): string => path.replace(/\/+$/, '').split('/').pop() || '워크스페이스';
import type { TabsSnapshot } from '../navigation/tabs.ts';
