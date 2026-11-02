import { asRow, asText } from './types.ts';
import type { RecentWorkspace, StoragePort, WorkspaceState } from './types.ts';

export const KEYS = {
  last: 'withlog.lastWorkspace.v1',
  recent: 'withlog.workspaces.v1',
  home: 'withlog.workspaceHome.v1',
  prefs: 'withlog.prefs.v1',
  tabs: 'withlog.tabs.v1',
  noteWindow: 'withlog.noteWindow.v1',
  sampleReport: 'withlog.sampleReport.v1',
  connMode: 'withlog.connMode.v3',
  connFilter: 'withlog.connFilter.v2',
  sampleReportEdits: 'withlog.reportEdits.v1',
  sampleReportMemos: 'withlog.reportMemos.v1',
  sampleTemplates: 'withlog.templates.v1',
  recentNotesPrefix: 'withlog.recentNotes.v1:',
} as const;

export interface AppPreferences {
  theme: 'light' | 'dark';
  weekLayout: 'swimlane' | 'stack';
  railWidth: number;
  detailListWidth: number;
  docWidth: number;
  noteRailWidth: number;
  previewWidth: number;
  previewHeight: number | null;
  askCardWidth: number;
  askCardHeight: number | null;
  composeWidth: number;
  composeInputHeight: number | null;
  docSortMode: 'structure' | 'time';
  docSortDir: 'desc' | 'asc';
  titleDatePrefix: boolean;
  showRecordTier: boolean;
  mermaidShowSource: boolean;
  askTimeoutSeconds: number;
  sessionPreviewOpen: boolean;
  collapsedProjects: string[];
  expandedSessionProjects: string[];
  [key: string]: unknown;
}

export const DEFAULT_PREFERENCES: AppPreferences = {
  theme: 'light',
  weekLayout: 'swimlane',
  railWidth: 264,
  detailListWidth: 340,
  docWidth: 820,
  noteRailWidth: 392,
  previewWidth: 720,
  previewHeight: null,
  askCardWidth: 360,
  askCardHeight: null,
  composeWidth: 280,
  composeInputHeight: null,
  docSortMode: 'structure',
  docSortDir: 'desc',
  titleDatePrefix: true,
  showRecordTier: false,
  mermaidShowSource: false,
  askTimeoutSeconds: 3600,
  sessionPreviewOpen: true,
  collapsedProjects: [],
  expandedSessionProjects: [],
};

function clampInt(val: unknown, min: number, max: number, def: number): number {
  if (typeof val !== 'number' || !Number.isFinite(val)) return def;
  return Math.max(min, Math.min(max, Math.round(val)));
}

function clampNullableInt(val: unknown, min: number, max: number): number | null {
  if (val === null || val === undefined) return null;
  if (typeof val !== 'number' || !Number.isFinite(val)) return null;
  return Math.max(min, Math.min(max, Math.round(val)));
}

export function normalizePreferences(raw: unknown): { state: AppPreferences; version: number } {
  const row = asRow(raw);
  const stateRow = asRow(row?.state) || {};
  const rawVersion = typeof row?.version === 'number' ? row.version : 0;
  if (rawVersion < 1 && stateRow.askTimeoutSeconds === 1200) {
    delete stateRow.askTimeoutSeconds;
  }
  const merged: AppPreferences = { ...DEFAULT_PREFERENCES, ...stateRow };

  merged.theme = stateRow.theme === 'dark' ? 'dark' : 'light';
  merged.weekLayout = stateRow.weekLayout === 'stack' || stateRow.weekLayout === 'dates' ? 'stack' : 'swimlane';
  merged.railWidth = clampInt(stateRow.railWidth, 200, 420, 264);
  merged.detailListWidth = clampInt(stateRow.detailListWidth, 260, 560, 340);
  merged.docWidth = clampInt(stateRow.docWidth, 640, 1600, 820);
  merged.noteRailWidth = clampInt(stateRow.noteRailWidth, 392, 960, 392);
  merged.previewWidth = clampInt(stateRow.previewWidth, 480, 1600, 720);
  merged.previewHeight = clampNullableInt(stateRow.previewHeight, 320, 2400);
  merged.askCardWidth = clampInt(stateRow.askCardWidth, 280, 720, 360);
  merged.askCardHeight = clampNullableInt(stateRow.askCardHeight, 160, 1600);
  merged.composeWidth = clampInt(stateRow.composeWidth, 240, 720, 280);
  merged.composeInputHeight = clampNullableInt(stateRow.composeInputHeight, 80, 1200);
  merged.docSortMode = stateRow.docSortMode === 'time' ? 'time' : 'structure';
  merged.docSortDir = stateRow.docSortDir === 'asc' ? 'asc' : 'desc';
  merged.titleDatePrefix = typeof stateRow.titleDatePrefix === 'boolean' ? stateRow.titleDatePrefix : (typeof stateRow.titleDate === 'boolean' ? stateRow.titleDate : true);
  merged.showRecordTier = typeof stateRow.showRecordTier === 'boolean' ? stateRow.showRecordTier : (typeof stateRow.showRecords === 'boolean' ? stateRow.showRecords : false);
  merged.mermaidShowSource = typeof stateRow.mermaidShowSource === 'boolean' ? stateRow.mermaidShowSource : (typeof stateRow.diagrams === 'boolean' ? stateRow.diagrams : false);

  // Backward compatibility aliases for existing UI bindings
  (merged as any).titleDate = merged.titleDatePrefix;
  (merged as any).showRecords = merged.showRecordTier;
  (merged as any).diagrams = merged.mermaidShowSource;
  (merged as any).agendaSort = merged.docSortMode;
  (merged as any).agendaDesc = merged.docSortDir === 'desc';
  (merged as any).previewSize = merged.previewWidth ? { width: merged.previewWidth, height: merged.previewHeight || 480 } : null;

  const validTimeouts = [600, 1200, 1800, 3600, 7200];
  merged.askTimeoutSeconds = typeof stateRow.askTimeoutSeconds === 'number' && validTimeouts.includes(stateRow.askTimeoutSeconds)
    ? stateRow.askTimeoutSeconds
    : 3600;

  merged.sessionPreviewOpen = typeof stateRow.sessionPreviewOpen === 'boolean' ? stateRow.sessionPreviewOpen : true;
  merged.collapsedProjects = Array.isArray(stateRow.collapsedProjects)
    ? stateRow.collapsedProjects.filter((s): s is string => typeof s === 'string')
    : [];
  merged.expandedSessionProjects = Array.isArray(stateRow.expandedSessionProjects)
    ? stateRow.expandedSessionProjects.filter((s): s is string => typeof s === 'string')
    : [];

  return { state: merged, version: 1 };
}

export function readNoteWindowGeometry(storage: StoragePort): { width: number; height: number; x?: number; y?: number } | null {
  const row = asRow(readJSON(storage, KEYS.noteWindow));
  const width = typeof row?.width === 'number' && row.width > 0 ? row.width : null;
  const height = typeof row?.height === 'number' && row.height > 0 ? row.height : null;
  if (!width || !height) return null;
  const x = typeof row?.x === 'number' && Number.isFinite(row.x) ? row.x : undefined;
  const y = typeof row?.y === 'number' && Number.isFinite(row.y) ? row.y : undefined;
  return { width, height, ...(x !== undefined ? { x } : {}), ...(y !== undefined ? { y } : {}) };
}

export function writeNoteWindowGeometry(
  storage: StoragePort,
  geom: { width: number; height: number; x?: number; y?: number }
) {
  if (geom.width <= 0 || geom.height <= 0) return;
  const val: Record<string, number> = { width: geom.width, height: geom.height };
  if (typeof geom.x === 'number' && Number.isFinite(geom.x)) val.x = geom.x;
  if (typeof geom.y === 'number' && Number.isFinite(geom.y)) val.y = geom.y;
  writeJSON(storage, KEYS.noteWindow, val);
}

export const writeNoteGeometry = writeNoteWindowGeometry;
export const readNoteGeometry = readNoteWindowGeometry;

export function readPreferences(storage: StoragePort): AppPreferences {
  const json = readJSON(storage, KEYS.prefs);
  const normalized = normalizePreferences(json);
  const row = asRow(json);
  if (row && row.version !== 1) {
    writeJSON(storage, KEYS.prefs, { state: normalized.state, version: 1 });
  }
  return normalized.state;
}

export function writePreferences(
  storage: StoragePort,
  prefs: Partial<Omit<AppPreferences, 'weekLayout'> & { weekLayout?: 'swimlane' | 'stack' | 'agendas' | 'dates' | string; titleDate?: boolean; showRecords?: boolean; diagrams?: boolean; agendaSort?: 'structure' | 'time'; agendaDesc?: boolean; previewSize?: { width: number; height: number } | null }>,
  isMain = true
) {
  if (!isMain) return; // 노트 창은 설정을 쓰지 않는다 (15.7, 16.3)
  const current = readPreferences(storage);
  const normalizedUpdate: Record<string, unknown> = { ...prefs };
  if (typeof prefs.titleDate === 'boolean' && prefs.titleDatePrefix === undefined) {
    normalizedUpdate.titleDatePrefix = prefs.titleDate;
  }
  if (typeof prefs.titleDatePrefix === 'boolean') {
    normalizedUpdate.titleDate = prefs.titleDatePrefix;
  }
  if (typeof prefs.showRecords === 'boolean' && prefs.showRecordTier === undefined) {
    normalizedUpdate.showRecordTier = prefs.showRecords;
  }
  if (typeof prefs.showRecordTier === 'boolean') {
    normalizedUpdate.showRecords = prefs.showRecordTier;
  }
  if (typeof prefs.diagrams === 'boolean' && prefs.mermaidShowSource === undefined) {
    normalizedUpdate.mermaidShowSource = prefs.diagrams;
  }
  if (typeof prefs.mermaidShowSource === 'boolean') {
    normalizedUpdate.diagrams = prefs.mermaidShowSource;
  }
  if (prefs.agendaSort !== undefined && prefs.docSortMode === undefined) {
    normalizedUpdate.docSortMode = prefs.agendaSort;
  }
  if (typeof prefs.agendaDesc === 'boolean' && prefs.docSortDir === undefined) {
    normalizedUpdate.docSortDir = prefs.agendaDesc ? 'desc' : 'asc';
  }
  if (prefs.weekLayout === 'dates') {
    normalizedUpdate.weekLayout = 'stack';
  } else if (prefs.weekLayout === 'agendas') {
    normalizedUpdate.weekLayout = 'swimlane';
  }
  const next = { ...current, ...normalizedUpdate };
  writeJSON(storage, KEYS.prefs, { state: next, version: 1 });
}

export function readJSON(storage: StoragePort, key: string): unknown {
  try { return JSON.parse(storage.getItem(key) || 'null'); }
  catch { return null; }
}

export function writeJSON(storage: StoragePort, key: string, value: unknown) {
  try { storage.setItem(key, JSON.stringify(value)); }
  catch { /* Unavailable storage must not prevent opening a workspace. */ }
}

export function lastWorkspace(storage: StoragePort): { path: string; name: string } | null {
  const value = asRow(readJSON(storage, KEYS.last));
  const path = asText(value?.path);
  return path ? { path, name: asText(value?.name) } : null;
}

export function recentWorkspaces(storage: StoragePort): RecentWorkspace[] {
  const value = readJSON(storage, KEYS.recent);
  if (!Array.isArray(value)) return [];
  const seen = new Set<string>();
  const result: RecentWorkspace[] = [];
  for (const item of value) {
    const row = asRow(item), path = asText(row?.path);
    if (!path || seen.has(path)) continue;
    seen.add(path);
    result.push({ path, name: asText(row?.name), opened: asText(row?.opened),
      ...(row?.created === true ? { created: true } : {}), ...(row?.sample === true ? { sample: true } : {}) });
  }
  return result.slice(0, 6);
}

export function addRecent(storage: StoragePort, item: Omit<RecentWorkspace, 'opened'>): RecentWorkspace[] {
  // Read on every mutation so another entry point's update is not overwritten.
  const next = [{ ...item, opened: '방금 전' }, ...recentWorkspaces(storage).filter(r => r.path !== item.path)].slice(0, 6);
  writeJSON(storage, KEYS.recent, next);
  return next;
}

export function removeRecent(storage: StoragePort, path: string): RecentWorkspace[] {
  const next = recentWorkspaces(storage).filter(r => r.path !== path);
  writeJSON(storage, KEYS.recent, next);
  return next;
}

export function homePath(value: unknown): string | null {
  if (typeof value !== 'string') return null;
  const path = value.trim().replace(/\/+$/, '');
  if (!path.startsWith('/') || path === '~/Documents') return null;
  return path;
}

export function readTheme(storage: StoragePort): WorkspaceState['theme'] {
  const theme = asRow(asRow(readJSON(storage, KEYS.prefs))?.state)?.theme;
  return theme === 'dark' || theme === 'light' ? theme : 'system';
}

export function savedOpenIds(storage: StoragePort, root: string): Set<string> {
  const value = asRow(readJSON(storage, KEYS.tabs));
  const state = asRow(value?.state);
  if (value?.version !== 1 || state?.workspaceRoot !== root || !Array.isArray(state.openIds)) return new Set();
  return new Set(state.openIds.filter((id): id is string => typeof id === 'string' && id.length > 0));
}
