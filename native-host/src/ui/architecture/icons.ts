// C4 요소에 고를 수 있는 아이콘의 이름표. 이름의 원본은 엔진의 `ais-domain/src/c4/icons.rs`이고, 두 목록이 같은지는
// `scripts/architecture-icons.test.mjs`가 맞춘다. 선 아이콘의 그림은 `ui/Icon.tsx`, 서비스 로고의 그림은 `logos.ts`에 있다.
import type { IconName } from '../Icon.tsx';
import { LOGOS } from './logos.ts';

export type IconChoice = { name: string; label: string };
export type IconGroup = { title: string; items: IconChoice[] };

/** 앱이 직접 그린 선 아이콘을 묶음별로 늘어놓은 것(고르는 창의 차례다) */
export const LINE_GROUPS: IconGroup[] = [
  { title: '사람·주체', items: [['person', '사람'], ['bot', '에이전트'], ['sparkle', 'AI'], ['chat', '대화·메신저'], ['mail', '메일']].map(choice) },
  { title: '시스템·실행', items: [['grid', '시스템'], ['server', '서버'], ['cpu', '엔진·처리'], ['box', '컨테이너'], ['terminal', '명령줄'], ['code', '코드'], ['plug', '연동·MCP']].map(choice) },
  { title: '화면', items: [['window', '화면·앱'], ['tab', '탭·페이지'], ['tree', '컴포넌트']].map(choice) },
  { title: '데이터', items: [['database', '데이터베이스'], ['folder', '폴더'], ['note', '노트·문서'], ['book', '가이드'], ['queue', '큐·목록'], ['image', '이미지']].map(choice) },
  { title: '연결·외부', items: [['globe', '외부 서비스'], ['cloud', '클라우드'], ['git-branch', 'Git 분기'], ['link', '연결'], ['bell', '알림'], ['zap', '이벤트']].map(choice) },
  { title: '보안', items: [['shield', '보호'], ['lock', '잠금'], ['key', '키·토큰']].map(choice) },
  { title: '그 밖', items: [['bug', '버그·제보'], ['calendar', '일정'], ['chart', '통계'], ['settings', '설정']].map(choice) },
];

function choice([name, label]: string[]): IconChoice { return { name, label }; }

export const LOGOS_GROUP_TITLE = '서비스 로고';

/** 서비스 로고(이름순) */
export const LOGO_CHOICES: IconChoice[] = Object.entries(LOGOS)
  .map(([name, logo]) => ({ name, label: logo.title }))
  .sort((a, b) => a.label.localeCompare(b.label, 'en'));

export const LINE_NAMES: string[] = LINE_GROUPS.flatMap(group => group.items.map(item => item.name));
export const LOGO_NAMES: string[] = Object.keys(LOGOS);

const KNOWN = new Set([...LINE_NAMES, ...LOGO_NAMES]);
const LABELS = new Map<string, string>([...LINE_GROUPS.flatMap(group => group.items), ...LOGO_CHOICES].map(item => [item.name, item.label]));

/** 고를 수 있는 이름인가. 파일에 모르는 이름이 있어도(새 앱이 쓴 것) 화면은 기본 아이콘으로 그린다. */
export const isKnownIcon = (name: string | null | undefined): name is string => Boolean(name) && KNOWN.has(name as string);
export const iconLabel = (name: string): string => LABELS.get(name) ?? name;

/** `Icon.tsx`의 이름. 파일에 적는 이름은 `git-branch`처럼 줄표로 잇고, `Icon.tsx`는 `gitBranch`처럼 붙여 쓴다. */
export const lineIconName = (name: string): IconName =>
  name.replace(/-([a-z])/g, (_, letter: string) => letter.toUpperCase()) as IconName;

/** 이름이나 라벨에 [query]가 들어 있는 것만 남긴다(대소문자 구분 없음). 비면 그대로다. */
export function filterChoices(items: IconChoice[], query: string): IconChoice[] {
  const needle = query.trim().toLowerCase();
  return needle ? items.filter(item => item.name.includes(needle) || item.label.toLowerCase().includes(needle)) : items;
}

const lumaOf = (hex: string) => {
  const [r, g, b] = [0, 2, 4].map(at => parseInt(hex.slice(at, at + 2), 16));
  return (0.2126 * r + 0.7152 * g + 0.0722 * b) / 255;
};

/** 아이콘 목록에서 로고를 그릴 글자색. 브랜드 색이되, 검정에 가까운 색은 어두운 화면에서 안 보이니 기본 글자색을 쓴다. */
export const logoInk = (hex: string): string => (lumaOf(hex) < .25 ? 'var(--ink)' : '#' + hex);

/** 로고 타일의 색. 브랜드 색을 바탕으로 쓰고, 너무 밝은 색(노랑 따위)에는 흰 로고가 안 보이니 어두운 글자색을 쓴다. */
export function logoTone(hex: string): { from: string; to: string; ink: string } {
  const [r, g, b] = [0, 2, 4].map(at => parseInt(hex.slice(at, at + 2), 16));
  const luma = lumaOf(hex);
  const shade = (k: number) => '#' + [r, g, b].map(value => Math.round(value * k).toString(16).padStart(2, '0')).join('');
  return { from: '#' + hex, to: shade(.8), ink: luma > .72 ? '#1f2328' : '#ffffff' };
}
