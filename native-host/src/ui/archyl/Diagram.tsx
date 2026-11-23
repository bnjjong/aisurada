import { useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { CSSProperties, PointerEvent } from 'react';
import { Icon } from '../Icon.tsx';
import type { ArchylElement, ArchylRelationship } from './client.ts';
import { bounds, CARD_HEIGHT, CARD_WIDTH, KIND_LABELS, measureEdge, placeLabels, positionNodes, routeEdges } from './layout.ts';
import type { DiagramLabel } from './layout.ts';

const KIND_ICON: Record<string, string> = {
  person: 'person', system: 'grid', external: 'globe', container: 'tab', component: 'tree', code: 'note',
};
const FLOW_PERIOD_MS = 1450;

function iconFor(item: ArchylElement) {
  const icon = item.icon?.toLowerCase() ?? '';
  const name = item.name.toLowerCase();
  if (icon.includes('github') || /^github\b/.test(name)) return 'gitBranch';
  if (icon.includes('archyl') || name === 'archyl') return 'tree';
  if (icon.includes('database')) return 'database';
  return KIND_ICON[item.kind] ?? 'tree';
}

export function Diagram({ elements, relationships, selected, select, drill }: {
  elements: ArchylElement[];
  relationships: ArchylRelationship[];
  selected: string | null;
  select(id: string | null): void;
  drill(id: string): void;
}) {
  const viewport = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const flowCanvas = useRef<HTMLCanvasElement>(null);
  const flowStart = useRef<number | null>(null);
  const [playing, setPlaying] = useState(true);
  const [area, setArea] = useState({ width: 1, height: 1 });
  const [labels, setLabels] = useState<DiagramLabel[]>([]);
  const [camera, setCamera] = useState({ zoom: 1, x: 0, y: 0 });
  const drag = useRef<{ id: number; x: number; y: number } | null>(null);
  const nodes = useMemo(() => positionNodes(elements), [elements]);
  const edges = useMemo(() => routeEdges(nodes, relationships), [nodes, relationships]);
  const flowPaths = useMemo(() => edges.map(measureEdge), [edges]);
  const box = useMemo(() => bounds(nodes), [nodes]);
  const fit = Math.min(1.12, Math.max(.025, (area.width - 46) / box.width), Math.max(.025, (area.height - 78) / box.height));
  const scale = Math.min(2.5, camera.zoom * fit);
  const typeScale = Math.max(1, Math.min(1.6, 1 / scale));
  const x = (area.width - box.width * scale) / 2 - box.minX * scale + camera.x;
  const y = (area.height - 32 - box.height * scale) / 2 - box.minY * scale + camera.y;
  const resize = (amount: number) => setCamera(current => ({
    ...current, zoom: Math.max(.45, Math.min(8, current.zoom * amount)),
  }));
  const reset = () => setCamera({ zoom: 1, x: 0, y: 0 });

  useLayoutEffect(() => {
    const node = viewport.current;
    if (!node) return;
    const measure = () => setArea({ width: node.clientWidth, height: node.clientHeight });
    const observer = new ResizeObserver(measure);
    observer.observe(node);
    const appearance = new MutationObserver(measure);
    appearance.observe(document.documentElement, { attributes: true, attributeFilter: ['data-skin', 'data-theme', 'style'] });
    const systemTheme = window.matchMedia('(prefers-color-scheme: dark)');
    systemTheme.addEventListener('change', measure);
    measure();
    return () => {
      observer.disconnect();
      appearance.disconnect();
      systemTheme.removeEventListener('change', measure);
    };
  }, []);

  useLayoutEffect(() => {
    const target = canvas.current;
    if (!target || !area.width || !area.height) return;
    const ratio = Math.min(window.devicePixelRatio || 1, 2);
    const width = Math.ceil(area.width * ratio);
    const height = Math.ceil(area.height * ratio);
    if (target.width !== width) target.width = width;
    if (target.height !== height) target.height = height;
    const context = target.getContext('2d');
    if (!context) return;
    context.setTransform(1, 0, 0, 1, 0, 0);
    context.clearRect(0, 0, width, height);
    context.setTransform(ratio * scale, 0, 0, ratio * scale, ratio * x, ratio * y);
    const style = getComputedStyle(target);
    const stroke = style.color;
    const accent = style.getPropertyValue('--accent').trim() || stroke;
    context.strokeStyle = context.fillStyle = stroke;
    context.font = '500 11px ' + style.fontFamily;
    for (const edge of edges) {
      const active = Boolean(edge.id && edge.id === selected);
      context.strokeStyle = context.fillStyle = active ? accent : stroke;
      context.globalAlpha = active ? 1 : .9;
      context.lineWidth = (active ? 2.3 : 1.55) / scale;
      context.beginPath();
      context.moveTo(edge.start.x, edge.start.y);
      context.bezierCurveTo(edge.a.x, edge.a.y, edge.b.x, edge.b.y, edge.end.x, edge.end.y);
      context.stroke();
      const angle = Math.atan2(edge.end.y - edge.b.y, edge.end.x - edge.b.x);
      const length = 7 / scale;
      context.beginPath();
      context.moveTo(edge.end.x, edge.end.y);
      context.lineTo(edge.end.x - length * Math.cos(angle - .44), edge.end.y - length * Math.sin(angle - .44));
      context.lineTo(edge.end.x - length * Math.cos(angle + .44), edge.end.y - length * Math.sin(angle + .44));
      context.closePath();
      context.fill();
    }
    const nextLabels = placeLabels(edges, nodes, { ...area, x, y, scale }, selected, title => context.measureText(title).width);
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.strokeStyle = stroke;
    context.globalAlpha = .8;
    context.lineWidth = 1;
    for (const label of nextLabels) {
      if (label.center.x === label.anchor.x && label.center.y === label.anchor.y) continue;
      context.beginPath();
      context.moveTo(label.anchor.x, label.anchor.y);
      context.lineTo(label.center.x, label.center.y);
      context.stroke();
    }
    setLabels(nextLabels);
  }, [area, edges, nodes, scale, selected, x, y]);

  useLayoutEffect(() => {
    const target = flowCanvas.current;
    if (!target || !playing || !flowPaths.length || !area.width || !area.height) return;
    const ratio = Math.min(window.devicePixelRatio || 1, 2);
    const width = Math.ceil(area.width * ratio);
    const height = Math.ceil(area.height * ratio);
    if (target.width !== width) target.width = width;
    if (target.height !== height) target.height = height;
    const context = target.getContext('2d');
    if (!context) return;
    context.setTransform(ratio, 0, 0, ratio, 0, 0);
    context.fillStyle = getComputedStyle(target).color;
    const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)');
    let frame = 0;
    const clear = () => context.clearRect(0, 0, width / ratio, height / ratio);
    const draw = (now: number) => {
      flowStart.current ??= now;
      const progress = ((now - flowStart.current) % FLOW_PERIOD_MS) / FLOW_PERIOD_MS;
      clear();
      context.globalAlpha = Math.min(1, progress * 14, (1 - progress) * 14);
      for (const locate of flowPaths) {
        const point = locate(progress);
        const px = x + point.x * scale;
        const py = y + point.y * scale;
        context.beginPath();
        context.arc(px, py, 2.8, 0, 2 * Math.PI);
        context.fill();
      }
      frame = requestAnimationFrame(draw);
    };
    const stop = () => {
      cancelAnimationFrame(frame);
      clear();
    };
    const sync = () => {
      stop();
      if (!document.hidden && !reducedMotion.matches) frame = requestAnimationFrame(draw);
    };
    reducedMotion.addEventListener('change', sync);
    document.addEventListener('visibilitychange', sync);
    sync();
    return () => {
      stop();
      reducedMotion.removeEventListener('change', sync);
      document.removeEventListener('visibilitychange', sync);
    };
  }, [area, flowPaths, playing, scale, x, y]);

  function startPan(event: PointerEvent<HTMLDivElement>) {
    if (event.button !== 0 || (event.target as HTMLElement).closest('button')) return;
    drag.current = { id: event.pointerId, x: event.clientX - camera.x, y: event.clientY - camera.y };
    event.currentTarget.setPointerCapture(event.pointerId);
  }

  return (
    <div
      className="archyl-diagram"
      ref={viewport}
      aria-label="Archyl C4 다이어그램"
      onPointerDown={startPan}
      onPointerMove={event => {
        const start = drag.current;
        if (start?.id === event.pointerId) {
          setCamera(current => ({ ...current, x: event.clientX - start.x, y: event.clientY - start.y }));
        }
      }}
      onLostPointerCapture={() => { drag.current = null; }}
    >
      <canvas ref={canvas} aria-hidden="true" />
      {playing && edges.length > 0 && <canvas className="archyl-flow" ref={flowCanvas} aria-hidden="true" />}
      {!nodes.length && <p className="archyl-diagram-empty">이 레벨에 표시할 요소가 없어요.</p>}
      <div
        className="archyl-diagram-world"
        data-density={scale < .46 ? 'minimal' : scale < .8 ? 'compact' : 'normal'}
        style={{ '--archyl-type-scale': typeScale, transform: 'translate(' + x + 'px, ' + y + 'px) scale(' + scale + ')' } as CSSProperties}
      >
        {nodes.map(item => (
          <button
            type="button"
            key={item.id}
            className="archyl-card"
            data-kind={item.kind}
            aria-pressed={selected === item.id}
            style={{ left: item.x, top: item.y, width: CARD_WIDTH, height: CARD_HEIGHT, borderTopColor: item.color && /^#[\da-f]{3,8}$/i.test(item.color) ? item.color : undefined }}
            title={[item.name, item.description, item.technology].filter(Boolean).join('\n')}
            onClick={() => select(item.id)}
            onDoubleClick={() => drill(item.id)}
          >
            <span className="archyl-card-kind"><span className="archyl-card-icon"><Icon name={iconFor(item)} size={15} /></span>{KIND_LABELS[item.kind] ?? '요소'}</span>
            <strong>{item.name}</strong>
            {item.description && <span className="archyl-card-description">{item.description}</span>}
            {item.technology && <span className="archyl-card-tech">{item.technology}</span>}
          </button>
        ))}
      </div>
      {labels.map(({ edge, index, title, center, width, height, compact }) => {
        const style = { left: center.x, top: center.y, width, height };
        const caption = compact ? <Icon name="link" size={12} /> : <span>{title}</span>;
        return edge.id ? (
          <button
            key={edge.id}
            className="archyl-edge"
            type="button"
            style={style}
            title={title}
            aria-label={title}
            aria-pressed={edge.id === selected}
            onClick={() => select(edge.id)}
          >{caption}</button>
        ) : <span className="archyl-edge" key={index} style={style} title={title}>{caption}</span>;
      })}
      <div className="archyl-zoom" aria-label="다이어그램 보기 도구">
        <button type="button" aria-label="축소" onClick={() => resize(.8)}>−</button>
        <output>{Math.round(scale * 100)}%</output>
        <button type="button" aria-label="확대" onClick={() => resize(1.25)}>+</button>
        <button type="button" aria-label="화면에 맞추기" onClick={reset}><Icon name="maximize" size={14} /></button>
        {edges.length > 0 && (
          <button
            type="button"
            className="archyl-flow-toggle"
            aria-label="관계 흐름 애니메이션"
            aria-pressed={playing}
            title={playing ? '관계 흐름 일시정지' : '관계 흐름 재생'}
            onClick={() => {
              flowStart.current = null;
              setPlaying(current => !current);
            }}
          >
            <Icon name={playing ? 'pause' : 'play'} size={14} />
          </button>
        )}
      </div>
      {nodes.length > 0 && (
        <div className="archyl-minimap" aria-hidden="true">
          {nodes.map(item => (
            <i key={item.id} data-selected={item.id === selected} style={{
              left: 100 * (item.x - box.minX) / box.width + '%',
              top: 100 * (item.y - box.minY) / box.height + '%',
              width: 100 * CARD_WIDTH / box.width + '%',
              height: 100 * CARD_HEIGHT / box.height + '%',
            }} />
          ))}
        </div>
      )}
    </div>
  );
}
