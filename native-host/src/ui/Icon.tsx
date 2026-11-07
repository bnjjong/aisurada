import type { ReactNode } from "react";

/**
 * 노트 정보줄과 아젠다 화면의 아이콘 단추가 함께 쓰는 선 아이콘 모음입니다.
 * 같은 모양을 화면마다 따로 그리지 않으려고 한곳에 모았고, 색은 글자색(currentColor)을 따릅니다.
 */
const PATHS: Record<string, ReactNode> = {
  back: <path d="M10 3L5 8l5 5" />,
  issue: (
    <>
      <circle cx="8" cy="8" r="5.5" />
      <circle cx="8" cy="8" r="1" />
    </>
  ),
  plus: <path d="M8 3v10M3 8h10" />,
  trash: (
    <>
      <path d="M3 4.5h10M6 4.5V3a1 1 0 0 1 1-1h2a1 1 0 0 1 1 1v1.5M12.5 4.5l-.8 8.5a1.5 1.5 0 0 1-1.5 1.5h-4.4a1.5 1.5 0 0 1-1.5-1.5L3.5 4.5" />
      <path d="M6.5 7.5v4M9.5 7.5v4" />
    </>
  ),
  tree: (
    <>
      <rect x="2" y="2.5" width="4" height="3" rx=".8" />
      <rect x="9" y="6.5" width="5" height="3" rx=".8" />
      <rect x="9" y="11" width="5" height="3" rx=".8" />
      <path d="M4 5.5V12.5H9M4 8H9" />
    </>
  ),
  clock: (
    <>
      <circle cx="8" cy="8" r="6" />
      <path d="M8 5v3.5l2 1" />
    </>
  ),
  sort: <path d="M4.5 3v10M2 10.5L4.5 13 7 10.5M9.5 4h4M9.5 8h3M9.5 12h2" />,
  grid: (
    <>
      <rect x="2.5" y="2.5" width="4.5" height="4.5" rx="1" />
      <rect x="9" y="2.5" width="4.5" height="4.5" rx="1" />
      <rect x="2.5" y="9" width="4.5" height="4.5" rx="1" />
      <rect x="9" y="9" width="4.5" height="4.5" rx="1" />
    </>
  ),
  star: <path d="M8 2l1.9 4 4.4.5-3.3 3 .9 4.3L8 11.6 4.1 13.8 5 9.5 1.7 6.5 6.1 6z" />,
  evaluate: (
    <>
      <rect x="3" y="2.5" width="10" height="11" rx="1.5" />
      <path d="M5.5 6h5M5.5 9h3" />
    </>
  ),
  history: (
    <>
      <path d="M2.5 7.5A5.5 5.5 0 1 1 3.2 10.5" />
      <path d="M2.5 3.5v4h4" />
      <path d="M8 5v3.5l2 1" />
    </>
  ),
  tab: (
    <>
      <rect x="2" y="3" width="12" height="10" rx="1.5" />
      <path d="M2 6.5h12" />
      <path d="M6 3v3.5" />
    </>
  ),
  popout: (
    <>
      <path d="M7 3H3.5A1.5 1.5 0 0 0 2 4.5v8A1.5 1.5 0 0 0 3.5 14h8a1.5 1.5 0 0 0 1.5-1.5V9" />
      <path d="M10 2h4v4" />
      <path d="M7.5 8.5L14 2" />
    </>
  ),
  more: (
    <>
      <circle cx="3.5" cy="8" r="1" />
      <circle cx="8" cy="8" r="1" />
      <circle cx="12.5" cy="8" r="1" />
    </>
  ),
  close: <path d="M4 4l8 8M12 4l-8 8" />,
  copy: (
    <>
      <rect x="5.5" y="5.5" width="7.5" height="7.5" rx="1.5" />
      <path d="M3.5 10.5V4a1.5 1.5 0 0 1 1.5-1.5h6.5" />
    </>
  ),
  link: (
    <path d="M7 3.5H4a1.5 1.5 0 0 0-1.5 1.5v7A1.5 1.5 0 0 0 4 13.5h7a1.5 1.5 0 0 0 1.5-1.5v-3M9.5 2.5h4v4M6.5 9.5l7-7" />
  ),
};

export type IconName = keyof typeof PATHS;

/** [name] 아이콘을 [size]px 정사각형 선 그림으로 그립니다. 스크린 리더에는 숨기므로 뜻은 단추의 aria-label에 둡니다. */
export function Icon({ name, size = 14 }: { name: IconName; size?: number }) {
  return (
    <svg
      viewBox="0 0 16 16"
      width={size}
      height={size}
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      {PATHS[name]}
    </svg>
  );
}
