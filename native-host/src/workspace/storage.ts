import { asRow, asText } from './types.ts';
import type { RecentWorkspace, StoragePort, WorkspaceState } from './types.ts';

export const KEYS = {
  last: 'withlog.lastWorkspace.v1',
  recent: 'withlog.workspaces.v1',
  home: 'withlog.workspaceHome.v1',
  prefs: 'withlog.prefs.v1',
  tabs: 'withlog.tabs.v1',
} as const;

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
  const path = value.replace(/\/+$/, '');
  return path.startsWith('/') ? path : null;
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
