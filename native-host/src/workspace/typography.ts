// 설정의 「글꼴과 크기」. 화면 배율(웹뷰 확대)과 앱·터미널 글꼴을 다룬다.
// 값은 skin처럼 prefs에 함께 저장되는 확장 값이다. 기본 설정(16장의 21개)에는 나타나지 않고, 사용자가 고른 뒤에만 저장된다.
import { KEYS, readJSON } from './storage.ts';
import { asRow } from './types.ts';
import type { StoragePort } from './types.ts';

/** 화면 배율로 고를 수 있는 값(%). 순서가 곧 설정과 단축키가 오르내리는 차례다. */
export const UI_SCALES: readonly number[] = [90, 100, 110, 125, 150, 175];
export const DEFAULT_UI_SCALE = 100;
/** 글꼴 이름 입력의 최대 길이. 글꼴 목록(쉼표로 이은 것)까지 넉넉히 받는다. */
export const MAX_FONT_LENGTH = 200;
/** 글꼴을 바꿀 때 열려 있는 터미널이 새 글꼴을 읽도록 알리는 window 이벤트 이름. */
export const TYPOGRAPHY_EVENT = 'ais:typography';

export interface Typography {
  /** 화면 배율(%). UI_SCALES 중 하나 */
  scale: number;
  /** 앱 글꼴. 빈 글은 기본 글꼴 */
  uiFont: string;
  /** 터미널 글꼴. 빈 글은 기본 글꼴(D2Coding) */
  terminalFont: string;
}
export const DEFAULT_TYPOGRAPHY: Typography = { scale: DEFAULT_UI_SCALE, uiFont: '', terminalFont: '' };

/** 알 수 없는 값은 기본 배율로 읽는다. 목록에 없는 숫자도 기본으로 본다. */
export function normalizeScale(value: unknown): number {
  return typeof value === 'number' && UI_SCALES.includes(value) ? value : DEFAULT_UI_SCALE;
}

/**
 * 글꼴 입력에서 쓸 수 없는 이유를 돌려준다. 쓸 수 있으면 null.
 * CSS를 깨뜨리는 글자(`;` `{` `}` `<` `>` `\` 와 주석 표시)와 너무 긴 글을 막는다.
 */
export function fontProblem(text: string): string | null {
  if (text.length > MAX_FONT_LENGTH) return `글꼴은 ${MAX_FONT_LENGTH}자까지만 적을 수 있어요.`;
  if (/[;{}<>\\]|\/\*|\*\//.test(text)) return '글꼴 이름에 쓸 수 없는 글자(; { } < > \\)가 있어요.';
  return null;
}

/** 저장된 글꼴 글을 읽는다. 글이 아니거나 쓸 수 없으면 빈 글(기본 글꼴), 앞뒤 공백과 줄바꿈은 걷어 낸다. */
export function normalizeFont(value: unknown): string {
  if (typeof value !== 'string') return '';
  const text = value.replace(/[\u0000-\u001f\u007f]+/g, ' ').trim();
  return fontProblem(text) ? '' : text;
}

/**
 * 글꼴 [font]를 CSS font-family 값으로 만든다. 끝에 [fallback]을 붙여서, 고른 글꼴에 없는 글자(한글 등)는 기본 글꼴이 맡는다.
 * 쉼표나 따옴표가 없는 이름은 한 글꼴 이름으로 보고 따옴표로 감싼다(`Noto Sans KR`, `Source Han 2`처럼 공백과 숫자가 있어도 되도록).
 * 쉼표나 따옴표가 있으면 사용자가 CSS 글꼴 목록을 직접 쓴 것이라 그대로 둔다.
 */
export function cssFontFamily(font: string, fallback: string): string {
  const name = normalizeFont(font);
  if (!name) return fallback;
  return `${/[,"']/.test(name) ? name : `"${name}"`}, ${fallback}`;
}

/** 앱 글꼴과 터미널 글꼴이 CSS에서 이어받는 기본 글꼴 변수. styles.css의 :root에 정의돼 있다. */
const UI_FALLBACK = 'var(--font-ui-base)';
const TERMINAL_FALLBACK = 'var(--font-mono)';

/**
 * 저장소(prefs)에서 글꼴과 크기를 읽는다. 값이 없거나 이상하면 기본값이다.
 * 예전 저장본에는 이 값이 없으므로 기본값으로 읽혀 화면이 달라지지 않는다.
 */
export function readTypography(storage: StoragePort): Typography {
  const state = asRow(asRow(readJSON(storage, KEYS.prefs))?.state);
  return {
    scale: normalizeScale(state?.uiScale),
    uiFont: normalizeFont(state?.uiFont),
    terminalFont: normalizeFont(state?.terminalFont),
  };
}

/** 현재 배율에서 [direction]만큼 오르내린 배율. 0이면 기본 배율. 맨 끝에서는 그대로다. */
export function stepScale(current: number, direction: 1 | -1 | 0): number {
  if (direction === 0) return DEFAULT_UI_SCALE;
  const at = UI_SCALES.indexOf(normalizeScale(current));
  return UI_SCALES[Math.min(UI_SCALES.length - 1, Math.max(0, at + direction))];
}

/** 글꼴을 `<html>`의 CSS 변수로 건다. 빈 글(기본)이면 변수를 지워 styles.css의 기본값으로 돌아간다. */
export function applyTypographyVars(
  root: { style: { setProperty(name: string, value: string): void; removeProperty(name: string): unknown } },
  typography: Typography,
) {
  const set = (name: string, font: string, fallback: string) => {
    if (normalizeFont(font)) root.style.setProperty(name, cssFontFamily(font, fallback));
    else root.style.removeProperty(name);
  };
  set('--font-ui', typography.uiFont, UI_FALLBACK);
  set('--font-terminal', typography.terminalFont, TERMINAL_FALLBACK);
}

/** 설정의 미리보기에 쓰는 font-family 값. 기본 글꼴이면 변수 그대로다. */
export const previewFamily = {
  ui: (font: string) => cssFontFamily(font, UI_FALLBACK),
  terminal: (font: string) => cssFontFamily(font, TERMINAL_FALLBACK),
};
