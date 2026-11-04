import type { Agenda, Workspace } from '../workspace/types.ts';

export type CalendarMode = 'day' | 'week' | 'month';
export interface CalendarView { mode: CalendarMode; date: string; week: string; }
type PlainPage = 'favorites' | 'connections' | 'review' | 'trash' | 'guide';
export type ScreenRoute =
  | { kind: 'root' | 'unknown' }
  | { kind: 'launcher'; create: boolean }
  | { kind: 'doc'; id: string }
  | { kind: 'file'; path: string }
  | { kind: 'calendar'; query: URLSearchParams }
  | { kind: 'agenda'; id: string; open: string[] }
  | { kind: 'templates'; open: string }
  | { kind: 'session'; id: string }
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
    if (path === '/templates') return { kind: 'templates', open: query.get('open') || '' };
    if (path === '/terminal') return { kind: 'terminal', id: null };
    const plain = ['favorites', 'connections', 'review', 'trash', 'guide'] as const;
    const page = plain.find(kind => path === `/${kind}`);
    if (page) return { kind: page };
    const segment = /^\/(doc|agenda|session|terminal)\/([^/]+)$/.exec(path);
    if (!segment) return { kind: 'unknown' };
    const id = decodeURIComponent(segment[2]);
    switch (segment[1]) {
      case 'doc': return { kind: 'doc', id };
      case 'agenda': return { kind: 'agenda', id, open: (query.get('open') || '').split(',').filter(Boolean) };
      case 'session': return { kind: 'session', id };
      default: return { kind: 'terminal', id };
    }
  } catch { return { kind: 'unknown' }; }
}

export const docAddress = (id: string) => `/doc/${encodeURIComponent(id)}`;
export const fileAddress = (path: string) => `/file?path=${encodeURIComponent(path)}`;
export const agendaAddress = (id: string) => `/agenda/${encodeURIComponent(id)}`;
export const isNoteTab = (id: string) => !/^(term:|ext:|sess:)/.test(id);

export function routeTab(route: ScreenRoute): string | null {
  if (route.kind === 'doc') return route.id;
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
