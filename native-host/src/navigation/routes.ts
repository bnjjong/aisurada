import type { Agenda, Workspace } from '../workspace/types.ts';

export type CalendarMode = 'day' | 'week' | 'month';
export interface CalendarView { mode: CalendarMode; date: string; week: string; }
type PlainPage = 'favorites' | 'connections' | 'review' | 'trash' | 'guide' | 'resources' | 'tasks';
export type ScreenRoute =
  | { kind: 'root' | 'unknown' }
  | { kind: 'launcher'; create: boolean }
  | { kind: 'doc'; id: string }
  | { kind: 'sub'; root: string; ids: string[] }
  | { kind: 'file'; path: string }
  | { kind: 'calendar'; query: URLSearchParams }
  | { kind: 'agenda'; id: string; open: string[] }
  | { kind: 'notes'; open: string[] }
  | { kind: 'templates'; open: string }
  | { kind: 'session'; id: string }
  | { kind: 'project'; slug: string }
  | { kind: 'task'; key: string }
  | { kind: 'terminal'; id: string | null }
  | { kind: PlainPage };

export function parseRoute(address: string): ScreenRoute {
  try {
    if (!address.startsWith('/') || address.startsWith('//')) return { kind: 'unknown' };
    const url = new URL(address, 'http://aisurada.local');
    const path = url.pathname, query = url.searchParams;
    if (path === '/') return { kind: 'root' };
    if (path === '/launcher') return { kind: 'launcher', create: query.get('mode') === 'create' };
    if (path === '/file') return { kind: 'file', path: query.get('path') || '' };
    if (path === '/calendar') return { kind: 'calendar', query };
    if (path === '/notes') {
      const open = (query.get('open') || '').split(',').filter(Boolean);
      return { kind: 'notes', open };
    }
    if (path === '/templates') return { kind: 'templates', open: query.get('open') || '' };
    if (path === '/terminal') return { kind: 'terminal', id: null };
    if (path.startsWith('/sub/')) {
      // 후속 노트: /sub/<최상위 노트 id>/<후속 id>/<그 아래 후속 id>… 사슬 전체가 주소다.
      const parts = path.slice('/sub/'.length).split('/').filter(Boolean).map(decodeURIComponent);
      return parts.length >= 2 ? { kind: 'sub', root: parts[0], ids: parts.slice(1) } : { kind: 'unknown' };
    }
    const plain = ['favorites', 'connections', 'review', 'trash', 'guide', 'resources', 'tasks'] as const;
    const page = plain.find(kind => path === `/${kind}`);
    if (page) return { kind: page };
    const segment = /^\/(doc|agenda|session|terminal|project|task)\/([^/]+)$/.exec(path);
    if (!segment) return { kind: 'unknown' };
    const id = decodeURIComponent(segment[2]);
    switch (segment[1]) {
      case 'doc': return { kind: 'doc', id };
      case 'agenda': return { kind: 'agenda', id, open: (query.get('open') || '').split(',').filter(Boolean) };
      case 'session': return { kind: 'session', id };
      case 'project': return { kind: 'project', slug: id };
      case 'task': return { kind: 'task', key: id };
      default: return { kind: 'terminal', id };
    }
  } catch { return { kind: 'unknown' }; }
}

export const docAddress = (id: string) => `/doc/${encodeURIComponent(id)}`;
export const fileAddress = (path: string) => `/file?path=${encodeURIComponent(path)}`;
export const agendaAddress = (id: string) => `/agenda/${encodeURIComponent(id)}`;
export const projectAddress = (slug: string) => `/project/${encodeURIComponent(slug)}`;
export const taskAddress = (key: string) => `/task/${encodeURIComponent(key)}`;
export const notesAddress = (open: string[] = []) =>
  '/notes' + (open.length ? '?open=' + open.map(encodeURIComponent).join(',') : '');
export const subAddress = (root: string, ids: string[]) => '/sub/' + [root, ...ids].map(encodeURIComponent).join('/');

/** 후속 노트 탭의 식별자는 `sub:<최상위 노트 id>/<후속 id>/…`로, 최상위 노트부터 이 노트까지의 사슬이다. */
export const subTabId = (root: string, ids: string[]) => 'sub:' + [root, ...ids].join('/');
export const isSubTab = (id: string) => id.startsWith('sub:');

/** 후속 노트 탭의 식별자를 최상위 노트 id와 후속 id 사슬로 푼다. 후속 노트 탭이 아니거나 사슬이 비면 null이다. */
export function parseSubTab(id: string): { root: string; ids: string[] } | null {
  if (!isSubTab(id)) return null;
  const parts = id.slice('sub:'.length).split('/').filter(Boolean);
  return parts.length >= 2 ? { root: parts[0], ids: parts.slice(1) } : null;
}

/** 최상위 볼트 노트를 가리키는 탭인가. 터미널·외부 파일·세션·후속 노트 탭은 아니다. */
export const isNoteTab = (id: string) => !/^(term:|ext:|sess:|sub:)/.test(id);

/** 앱을 껐다 켜도 남기는 탭인가. 최상위 노트 탭과 후속 노트 탭이다. */
export const isStoredTab = (id: string) => isNoteTab(id) || parseSubTab(id) !== null;

/** 탭이 기대는 최상위 노트 id. 노트 탭은 자기 id, 후속 노트 탭은 최상위 노트 id, 그 밖의 탭은 null이다. */
export function tabRootId(id: string): string | null {
  if (isNoteTab(id)) return id;
  return parseSubTab(id)?.root ?? null;
}

export function routeTab(route: ScreenRoute): string | null {
  if (route.kind === 'doc') return route.id;
  if (route.kind === 'sub') return subTabId(route.root, route.ids);
  if (route.kind === 'file' && route.path) return `ext:${route.path}`;
  if (route.kind === 'terminal') return `term:${route.id ?? 'main'}`;
  return null;
}

export function activeTab(address: string): string | null {
  const route = parseRoute(address);
  // Frozen 3.8.4: session terminals open a tab but do not activate it.
  return route.kind === 'terminal' && route.id !== null ? null : routeTab(route);
}

export function tabAddress(id: string): string {
  const sub = parseSubTab(id);
  if (sub) return subAddress(sub.root, sub.ids);
  if (id.startsWith('ext:')) return fileAddress(id.slice(4));
  if (id === 'term:main') return '/terminal';
  if (id.startsWith('term:')) return `/terminal/${encodeURIComponent(id.slice(5))}`;
  return docAddress(id);
}

export function findAgenda(workspace: Workspace, id: string): Agenda | undefined {
  return workspace.agendas.find(agenda => agenda.id === id) || workspace.agendas.find(agenda => agenda.slug === id);
}

export function validDate(value: string | null): value is string {
  if (!value || !/^\d{4}-\d{2}-\d{2}$/.test(value)) return false;
  const date = new Date(`${value}T00:00:00Z`);
  return Number.isFinite(date.getTime()) && date.toISOString().slice(0, 10) === value;
}

export function weekMonday(value: string): string {
  const date = new Date(`${value}T00:00:00Z`);
  date.setUTCDate(date.getUTCDate() - (date.getUTCDay() + 6) % 7);
  return date.toISOString().slice(0, 10);
}

export function calendarView(query: URLSearchParams, previous: CalendarView | null, today: string): CalendarView {
  const mode = query.get('mode');
  const date = query.get('date'), week = query.get('week');
  return {
    mode: mode === 'day' || mode === 'week' || mode === 'month' ? mode : previous?.mode || 'day',
    date: validDate(date) ? date : previous?.date || today,
    week: validDate(week) ? weekMonday(week) : previous?.week || weekMonday(today),
  };
}
