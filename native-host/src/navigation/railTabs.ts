// 레일(왼쪽 메뉴)의 두 탭: 노트와 에이전트. 어느 탭이 켜질지는 화면 주소가 정하고,
// 주소가 어느 쪽에도 속하지 않는 화면(연동 가이드, 세션, 터미널)에서는 직전에 켜 둔 탭을 그대로 둔다.
// 탭마다 마지막으로 본 화면을 기억해 두었다가 탭을 누르면 그 화면으로 돌아간다. 기억은 앱을 켜 둔 동안만 간다.
import { parseRoute } from './routes.ts';
import type { ScreenRoute } from './routes.ts';

export type RailTab = 'notes' | 'agents';

export const RAIL_TABS: readonly RailTab[] = ['notes', 'agents'];

/** 탭을 처음 누를 때 여는 화면. */
export const RAIL_TAB_HOME: Readonly<Record<RailTab, string>> = { notes: '/notes', agents: '/tasks' };

/** 이 화면이 속한 탭. 어느 쪽에도 속하지 않으면(문서·파일 창은 레일이 없다) null이다. */
export function railTabOf(route: ScreenRoute): RailTab | null {
  switch (route.kind) {
    case 'notes':
    case 'calendar':
    case 'agenda':
    case 'favorites':
    case 'connections':
    case 'templates':
    case 'trash':
    case 'review':
      return 'notes';
    case 'tasks':
    case 'task':
    case 'resources':
    case 'project':
      return 'agents';
    default:
      return null;
  }
}

interface RailTabMemory {
  tab: RailTab;
  last: Record<RailTab, string>;
}

const memory: RailTabMemory = { tab: 'notes', last: { ...RAIL_TAB_HOME } };

/** 지금 주소에서 켜질 탭. 탭이 없는 화면이면 직전에 켜 둔 탭이다. */
export function activeRailTab(address: string): RailTab {
  return railTabOf(parseRoute(address)) ?? memory.tab;
}

/** 탭이 있는 화면을 보고 있으면 그 탭과 주소를 기억한다. 같은 주소를 여러 번 불러도 같다. */
export function rememberRailTab(address: string): void {
  const tab = railTabOf(parseRoute(address));
  if (!tab) return;
  memory.tab = tab;
  memory.last[tab] = address;
}

/** 탭을 누르면 갈 주소. 지금 이미 그 탭의 화면이면 null이다(그대로 둔다). */
export function railTabTarget(tab: RailTab, address: string): string | null {
  return railTabOf(parseRoute(address)) === tab ? null : memory.last[tab];
}

/** 시험이 서로 새지 않게 처음 상태로 돌린다. */
export function resetRailTabs(): void {
  memory.tab = 'notes';
  memory.last = { ...RAIL_TAB_HOME };
}
