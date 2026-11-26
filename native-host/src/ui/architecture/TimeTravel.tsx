// 타임 트래블 막대: 아키텍처의 과거 시점을 점으로 늘어놓고, 이전·다음·점 선택·타임랩스로 훑는다.
// 마지막 점은 늘 「Now」(지금)다. 어느 시점을 보여 줄지와 모델 읽기는 부모가 한다. 여기서는 그리기만 한다.
import { useEffect, useRef } from 'react';
import { Icon } from '../Icon.tsx';
import { describePoint, formatAt } from './model.ts';
import type { TravelPoint } from './model.ts';

export function TimeTravel({ points, index, playing, loading, select, togglePlay, close }: {
  points: TravelPoint[];
  index: number;
  playing: boolean;
  loading: boolean;
  select(index: number): void;
  togglePlay(): void;
  close(): void;
}) {
  const last = points.length - 1;
  const isNow = index === last;
  const selected = useRef<HTMLButtonElement>(null);
  // 점이 많으면 가로로 밀리므로 고른 점이 늘 보이게 한다.
  useEffect(() => { selected.current?.scrollIntoView?.({ block: 'nearest', inline: 'center' }); }, [index]);
  return (
    <section className="c4-travel" aria-label="타임 트래블">
      <div className="c4-travel-bar">
        <span className="c4-travel-title"><Icon name="clock" size={16} /> 타임 트래블</span>
        <button type="button" className="c4-travel-lapse" aria-pressed={playing} disabled={last < 1} onClick={togglePlay}>
          <Icon name={playing ? 'pause' : 'play'} size={13} /> 타임랩스
        </button>
        <button type="button" className="c4-travel-step" aria-label="이전 시점" disabled={index <= 0} onClick={() => select(index - 1)}>
          <span className="c4-travel-chevron" data-dir="prev"><Icon name="chevron" size={14} /></span>
        </button>
        <ol className="c4-travel-line">
          {points.map((point, i) => {
            const now = i === last;
            const label = describePoint(point, now, formatAt);
            return (
              <li key={point.snapshot?.seq ?? 'now'} data-now={now}>
                <button
                  type="button"
                  ref={i === index ? selected : undefined}
                  className="c4-travel-dot"
                  aria-label={now ? `지금 · ${label}` : label}
                  aria-current={i === index ? 'step' : undefined}
                  data-seq={point.snapshot?.seq}
                  title={label}
                  onClick={() => select(i)}
                />
                {now && <span className="c4-travel-now" data-active={i === index}>Now</span>}
              </li>
            );
          })}
        </ol>
        <button type="button" className="c4-travel-step" aria-label="다음 시점" disabled={isNow} onClick={() => select(index + 1)}>
          <Icon name="chevron" size={14} />
        </button>
        <button type="button" className="c4-travel-close" aria-label="타임 트래블 닫기" onClick={close}><Icon name="close" size={14} /></button>
      </div>
      <p className="c4-travel-state" role="status" aria-live="polite">
        {describePoint(points[index] ?? { snapshot: null }, isNow)}{loading ? ' · 불러오는 중' : ''}
      </p>
    </section>
  );
}
