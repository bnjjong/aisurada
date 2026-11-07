import { useAskOutcome, useAskState } from '@claude-plug/state';

/**
 * 헤드가 몸짓으로 보여 주는 상태다(시안: 로고 22). 모양은 `styles.css`의 `.bm` 규칙이 만든다.
 * - still: 움직이지 않는 정면. 앱 아이콘과 같은 모습이다. 작은 탭 아이콘에 쓴다.
 * - idle: 평소. 목 LED가 초록으로 숨 쉬고, 렌즈가 가끔 깜빡이고, 약 9초마다 좌우를 둘러본다.
 * - thinking: Claude의 응답을 기다리는 중. 목 LED가 깜빡이고, 렌즈가 차례로 켜지고, 헤드가 돈다.
 * - ok: 방금 응답이 도착했다. 목 LED가 초록이고 고개를 끄덕인다.
 * - error: 방금 질문이 실패했다. 렌즈와 링이 붉어지고 고개를 가로젓는다.
 */
export type BrandState = 'still' | 'idle' | 'thinking' | 'ok' | 'error';

/** Claude에게 묻는 중이면 thinking, 방금 끝났으면 ok 또는 error, 아니면 idle이다. */
export function useBrandState(): BrandState {
  const waiting = useAskState((state) => Object.keys(state.pending).length > 0);
  const outcome = useAskOutcome();
  if (waiting) return 'thinking';
  return outcome ?? 'idle';
}

/** 질문 상태를 따라 움직이는 로고다. 사이드바처럼 오래 떠 있는 자리에 쓴다. */
export function LiveBrandMark({ className }: { className?: string }) {
  const state = useBrandState();
  return <BrandMark className={className} state={state} />;
}

const LENSES = [
  { x: 222, y: 228, delay: '0s' },
  { x: 290, y: 228, delay: '-1.2s' },
  { x: 222, y: 296, delay: '-.4s' },
  { x: 290, y: 296, delay: '-.8s' },
];

const PANEL_LEFT = 'M74 206L152 226V298L74 318Z';
const PANEL_RIGHT = 'M438 206L360 226V298L438 318Z';

/** 헤드 옆의 붉은 패널이다. 돌 때 앞뒤가 바뀌므로 같은 모양을 머리 뒤와 앞에 한 번씩 그린다. */
function Panel({ className, d }: { className: string; d: string }) {
  return (
    <g className={className}>
      <path d={d} fill="#ff5148" stroke="#ff5148" strokeWidth="34" strokeLinejoin="round" opacity=".3" />
      <path d={d} fill="#f0503f" stroke="#ee4a3a" strokeWidth="14" strokeLinejoin="round" />
    </g>
  );
}

/**
 * 사이드바와 시작 화면에 쓰는 앱 로고(야간 헤드)다.
 * `src-tauri/icons/source.svg`와 같은 그림이고, 36px에서 보이지 않는 별과 그라데이션은 뺀 평면 버전이다.
 * 같은 화면에 여러 번 그려도 id가 겹치지 않도록 그라데이션을 쓰지 않는다.
 */
export function BrandMark({ className = 'brand-mark', state = 'idle' }: { className?: string; state?: BrandState }) {
  return (
    <svg className={`${className} bm is-${state}`} viewBox="0 0 512 512" aria-hidden="true" focusable="false">
      <rect width="512" height="512" rx="112" fill="#10205f" />
      <g className="bm-neck">
        <rect x="230" y="372" width="52" height="60" rx="14" fill="#121a38" stroke="#070b1c" strokeWidth="4" />
        <circle className="bm-ledh" cx="256" cy="414" r="34" fillOpacity=".28" />
        <circle className="bm-led" cx="256" cy="414" r="20" />
      </g>
      <g transform="translate(0 -6)">
        <g className="bm-hd">
          <Panel className="bm-pbl" d={PANEL_LEFT} />
          <Panel className="bm-pbr" d={PANEL_RIGHT} />
          <circle cx="256" cy="262" r="116" fill="#26336a" stroke="#4a5a9c" strokeWidth="4" />
          <g className="bm-cap">
            <ellipse cx="256" cy="132" rx="64" ry="26" fill="#222e60" stroke="#4a5a9c" strokeWidth="3" />
            <circle cx="256" cy="126" r="13" fill="#05081a" stroke="#6a7ac0" strokeWidth="3" />
          </g>
          <g className="bm-face">
            <circle className="bm-halo" cx="256" cy="262" r="132" fill="none" stroke="#5be38f" strokeWidth="16" opacity=".18" />
            <circle className="bm-ring" cx="256" cy="262" r="118" fill="#070b1c" stroke="#5be38f" strokeWidth="7" />
            <circle cx="256" cy="262" r="112" fill="#1b2340" />
            {LENSES.map(({ x, y, delay }) => (
              <g key={`${x}-${y}`}>
                <circle cx={x} cy={y} r="31" fill="#070b1c" />
                <g className="bm-ln" style={{ animationDelay: delay }}>
                  <circle className="bm-lens" cx={x} cy={y} r="26" fill="#5be38f" />
                  <circle cx={x - 8} cy={y - 8} r="7" fill="#fff" opacity=".9" />
                </g>
              </g>
            ))}
          </g>
          <Panel className="bm-pfl" d={PANEL_LEFT} />
          <Panel className="bm-pfr" d={PANEL_RIGHT} />
        </g>
      </g>
    </svg>
  );
}
