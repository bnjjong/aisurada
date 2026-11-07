/**
 * 사이드바와 시작 화면에 쓰는 앱 로고(야간 헤드)다.
 * `src-tauri/icons/source.svg`와 같은 그림이고, 36px에서 보이지 않는 별과 그라데이션은 뺀 평면 버전이다.
 * 같은 화면에 여러 번 그려도 id 충돌이 없도록 그라데이션을 쓰지 않는다.
 */
export function BrandMark({ className = 'brand-mark' }: { className?: string }) {
  return (
    <svg className={className} viewBox="0 0 512 512" aria-hidden="true" focusable="false">
      <rect width="512" height="512" rx="112" fill="#10205f" />
      <g transform="translate(0 -6)">
        <path d="M74 206L152 226V298L74 318Z" fill="#ff5148" stroke="#ff5148" strokeWidth="34" strokeLinejoin="round" opacity=".3" />
        <path d="M438 206L360 226V298L438 318Z" fill="#ff5148" stroke="#ff5148" strokeWidth="34" strokeLinejoin="round" opacity=".3" />
        <path d="M74 206L152 226V298L74 318Z" fill="#f0503f" stroke="#ee4a3a" strokeWidth="14" strokeLinejoin="round" />
        <path d="M438 206L360 226V298L438 318Z" fill="#f0503f" stroke="#ee4a3a" strokeWidth="14" strokeLinejoin="round" />
        <circle cx="256" cy="262" r="132" fill="none" stroke="#5be38f" strokeWidth="16" opacity=".18" />
        <circle cx="256" cy="262" r="118" fill="#070b1c" stroke="#5be38f" strokeWidth="7" />
        <circle cx="256" cy="262" r="112" fill="#1b2340" />
        <circle cx="222" cy="228" r="31" fill="#070b1c" />
        <circle cx="290" cy="228" r="31" fill="#070b1c" />
        <circle cx="222" cy="296" r="31" fill="#070b1c" />
        <circle cx="290" cy="296" r="31" fill="#070b1c" />
        <circle cx="222" cy="228" r="26" fill="#5be38f" />
        <circle cx="290" cy="228" r="26" fill="#5be38f" />
        <circle cx="222" cy="296" r="26" fill="#5be38f" />
        <circle cx="290" cy="296" r="26" fill="#5be38f" />
        <circle cx="214" cy="220" r="7" fill="#fff" opacity=".9" />
        <circle cx="282" cy="220" r="7" fill="#fff" opacity=".9" />
        <circle cx="214" cy="288" r="7" fill="#fff" opacity=".9" />
        <circle cx="282" cy="288" r="7" fill="#fff" opacity=".9" />
      </g>
    </svg>
  );
}
