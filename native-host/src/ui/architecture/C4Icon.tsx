// 요소 아이콘 하나를 그린다. 선 아이콘은 `Icon`, 서비스 로고는 채운 SVG다.
import type { CSSProperties } from 'react';
import { Icon } from '../Icon.tsx';
import { LOGOS } from './logos.ts';
import { lineIconName, logoTone } from './icons.ts';

export function C4Icon({ name, size = 18 }: { name: string; size?: number }) {
  const logo = LOGOS[name];
  if (!logo) return <Icon name={lineIconName(name)} size={size} />;
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true">
      <path d={logo.path} />
    </svg>
  );
}

/** 카드·상세 머리의 아이콘 타일에 줄 색. 선 아이콘은 종류 색을 그대로 쓰니 없다. */
export function tileStyle(name: string): CSSProperties | undefined {
  const logo = LOGOS[name];
  if (!logo) return undefined;
  const tone = logoTone(logo.hex);
  return { '--kind': tone.from, '--kind-end': tone.to, '--tile-ink': tone.ink } as CSSProperties;
}
