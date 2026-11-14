import { asRow } from '../workspace/types.ts';
import type { StoragePort, Workspace } from '../workspace/types.ts';
import { KEYS, readJSON, writeJSON } from '../workspace/storage.ts';
import { activeTab, calendarView, docAddress, isNoteTab, isStoredTab, parseRoute, parseSubTab, routeTab, tabAddress, tabRootId } from './routes.ts';
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
    openIds: [...new Set(source.filter((id): id is string => typeof id === 'string' && id.length > 0 && isStoredTab(id)))],
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
      state: { workspaceRoot, openIds: openIds.filter(isStoredTab) }, version: 1,
    });
  }

  attach(workspace: Workspace) {
    if (!this.writable) return;
    this.live = new Set(workspace.notes.map(note => note.id));
    const root = workspace.sample ? this.state.workspaceRoot : workspace.path;
    const ids = !workspace.sample && root !== this.state.workspaceRoot ? [] : this.state.openIds;
    // C14, C15: an unread note list and the sample leave the real vault's saved tabs alone.
    if (workspace.sample || workspace.noteReadFailed) { this.change(ids, root); return; }
    this.change(ids.filter(id => this.available(id)), root);
  }

  // 노트 탭과 후속 노트 탭은 최상위 노트가 볼트에 있을 때만 살아 있다. 터미널·외부 파일 탭은 늘 살아 있다.
  private available(id: string): boolean {
    const root = tabRootId(id);
    return root === null || this.live.has(root);
  }

  visible(): string[] { return this.state.openIds.filter(id => this.available(id)); }

  // 새 탭의 자리. 후속 노트 탭은 같은 최상위 노트의 탭 무리(상위 노트 탭과 이미 열린 후속 탭들) 바로 뒤에 놓아
  // 상위와 후속을 나란히 오가게 한다. 상위 노트 탭이 닫혀 있고 무리도 없으면 상위 노트 탭을 앞에 함께 연다.
  private withTab(id: string): string[] {
    const ids = this.state.openIds, sub = parseSubTab(id);
    if (!sub) return [...ids, id];
    let last = -1;
    ids.forEach((other, index) => { if (tabRootId(other) === sub.root) last = index; });
    return last >= 0 ? [...ids.slice(0, last + 1), id, ...ids.slice(last + 1)] : [...ids, sub.root, id];
  }

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
    if (route.kind === 'sub' && !this.live.has(route.root)) {
      return docAddressOrCalendar(this.visible().find(id => isNoteTab(id)));
    }
    const id = routeTab(route);
    if (register && id !== null && !this.state.openIds.includes(id)) this.change(this.withTab(id));
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
