import { asRow } from '../workspace/types.ts';
import type { StoragePort, Workspace } from '../workspace/types.ts';
import { KEYS, readJSON, writeJSON } from '../workspace/storage.ts';
import { activeTab, calendarView, docAddress, isNoteTab, parseRoute, routeTab, tabAddress } from './routes.ts';
import type { CalendarView } from './routes.ts';

export interface TabsSnapshot {
  openIds: string[];
  workspaceRoot: string | null;
  lastMain: string;
  calendar: CalendarView | null;
}

export function readTabs(storage: StoragePort): { openIds: string[]; workspaceRoot: string | null } {
  const value = asRow(readJSON(storage, KEYS.tabs)), state = asRow(value?.state);
  if (value?.version !== 1) return { openIds: [], workspaceRoot: null };
  const source = Array.isArray(state?.openIds) ? state.openIds : [];
  return {
    openIds: [...new Set(source.filter((id): id is string => typeof id === 'string' && id.length > 0 && isNoteTab(id)))],
    workspaceRoot: typeof state?.workspaceRoot === 'string' ? state.workspaceRoot : null,
  };
}

export class TabModel {
  private storage: StoragePort;
  private writable: boolean;
  private state: TabsSnapshot;
  private live = new Set<string>();

  constructor(storage: StoragePort, writable: boolean) {
    this.storage = storage;
    this.writable = writable;
    this.state = { ...readTabs(storage), lastMain: '/calendar', calendar: null };
  }

  snapshot(): TabsSnapshot { return this.state; }

  private change(openIds: string[], workspaceRoot = this.state.workspaceRoot) {
    if (workspaceRoot === this.state.workspaceRoot && openIds.length === this.state.openIds.length && openIds.every((id, n) => id === this.state.openIds[n])) return;
    this.state = { ...this.state, openIds, workspaceRoot };
    if (this.writable) writeJSON(this.storage, KEYS.tabs, {
      state: { workspaceRoot, openIds: openIds.filter(isNoteTab) }, version: 1,
    });
  }

  attach(workspace: Workspace) {
    if (!this.writable) return;
    this.live = new Set(workspace.notes.map(note => note.id));
    const root = workspace.sample ? this.state.workspaceRoot : workspace.path;
    const ids = !workspace.sample && root !== this.state.workspaceRoot ? [] : this.state.openIds;
    // C14, C15: an unread note list and the sample leave the real vault's saved tabs alone.
    if (workspace.sample || workspace.noteReadFailed) { this.change(ids, root); return; }
    this.change(ids.filter(id => !isNoteTab(id) || this.live.has(id)), root);
  }

  visible(): string[] { return this.state.openIds.filter(id => !isNoteTab(id) || this.live.has(id)); }

  visit(address: string, workspace: Workspace | null, register = true): string {
    if (!this.writable) return address;
    const route = parseRoute(address);
    if (!workspace) return route.kind === 'launcher' ? address : '/launcher';
    if (route.kind === 'unknown') return '/launcher';
    if (route.kind === 'root') return this.visit('/calendar', workspace, register);
    if (route.kind === 'launcher') return address;
    if (route.kind === 'doc' && !this.live.has(route.id)) {
      return docAddressOrCalendar(this.visible().find(id => isNoteTab(id) && id !== route.id));
    }
    const id = routeTab(route);
    if (register && id !== null && !this.state.openIds.includes(id)) this.change([...this.state.openIds, id]);
    if (id === null) this.state = { ...this.state, lastMain: address };
    if (route.kind === 'calendar') this.state = { ...this.state, calendar: calendarView(route.query, this.state.calendar, workspace.today) };
    return address;
  }

  close(id: string, address: string): string {
    if (!this.writable || !this.state.openIds.includes(id)) return address;
    const visible = this.visible(), index = visible.indexOf(id);
    this.change(this.state.openIds.filter(item => item !== id));
    if (activeTab(address) !== id) return address;
    const neighbor = index >= 0 ? (visible[index + 1] || visible[index - 1]) : visible[0];
    return neighbor ? tabAddress(neighbor) : this.state.lastMain;
  }

  reorder(id: string, index: number) {
    if (!this.writable || !this.state.openIds.includes(id)) return;
    const next = this.state.openIds.filter(item => item !== id);
    next.splice(Math.max(0, Math.min(index, next.length)), 0, id);
    this.change(next);
  }
}

const docAddressOrCalendar = (id: string | undefined) => id ? docAddress(id) : '/calendar';
