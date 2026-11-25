import { useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { CSSProperties, PointerEvent } from 'react';
import { Icon } from '../Icon.tsx';
import type { C4Element, C4Relationship, Point } from './model.ts';
import { bounds, CARD_HEIGHT, CARD_WIDTH, KIND_LABELS, MAX_SCALE, MAX_ZOOM, MIN_ZOOM, measureEdge, placeLabels, positionNodes, routeEdges, viewOrigin, viewScale, wheelFactor, zoomCameraAt } from './layout.ts';
import type { CameraFrame } from './layout.ts';
import type { DiagramLabel } from './layout.ts';

const KIND_ICON: Record<string, string> = {
  person: 'person', system: 'grid', external: 'globe', container: 'tab', component: 'tree', code: 'note',
};
const FLOW_PERIOD_MS = 1450;

type DiagramNodeLike = Point & { id: string };

function iconFor(item: C4Element) {
  const hint = (item.name + ' ' + item.technology).toLowerCase();
  if (/\bgithub\b/.test(hint)) return 'gitBranch';
  if (/database|db\b|sqlite|postgres|mysql|데이터베이스/.test(hint)) return 'database';
  return KIND_ICON[item.kind] ?? 'tree';
}

const DRAG_THRESHOLD = 4;

export function Diagram({
  elements, relationships, positions, selectedElement, selectedRelationship, selectElement, selectRelationship, drill,
  onMove, resetKey, connecting = false, emptyText,
}: {
  elements: C4Element[];
  relationships: C4Relationship[];
  /** 이 뷰에 저장된 좌표. 없는 요소는 격자로 놓는다 */
  positions: Record<string, Point>;
  selectedElement: string | null;
  selectedRelationship: string | null;
  selectElement(id: string | null): void;
  selectRelationship(id: string): void;
  drill(id: string): void;
  /** 끌기를 마치면 보이는 모든 요소의 좌표를 알린다. 없으면 끌 수 없다 */
  onMove?(positions: Record<string, Point>): void;
  /** 바뀌면 끄는 중에 임시로 둔 자리를 버리고 저장된 자리로 돌아간다 */
  resetKey: number;
  connecting?: boolean;
  emptyText?: string;
}) {
  const viewport = useRef<HTMLDivElement>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  const flowCanvas = useRef<HTMLCanvasElement>(null);
  const flowStart = useRef<number | null>(null);
  const [playing, setPlaying] = useState(true);
  const [area, setArea] = useState({ width: 1, height: 1 });
  const [labels, setLabels] = useState<DiagramLabel[]>([]);
  const [camera, setCamera] = useState({ zoom: 1, x: 0, y: 0 });
  const drag = useRef<{ id: number; x: number; y: number; moved: boolean } | null>(null);
  const panned = useRef(false);
  const grab = useRef<{ id: string; pointer: number; x: number; y: number; origin: Point; last: Point; active: boolean } | null>(null);
  const suppressClick = useRef(false);
  const [moved, setMoved] = useState<Record<string, Point>>({});
  const placed = useMemo(() => positionNodes(elements, positions), [elements, positions]);
  const nodes = useMemo(() => placed.map(item => moved[item.id] ? { ...item, ...moved[item.id] } : item), [placed, moved]);
  const edges = useMemo(() => routeEdges(nodes, relationships), [nodes, relationships]);
  const flowPaths = useMemo(() => edges.map(measureEdge), [edges]);
  const natural = useMemo(() => bounds(nodes), [nodes]);
  // 끌기를 시작하면 화면 틀(맞춤 배율과 가운데 정렬)을 붙잡는다. 안 그러면 카드를 바깥으로 끌 때마다 배율이 다시 계산되어
  // 카드가 포인터를 따라오지 못한다. 요소가 늘거나 줄거나 「화면에 맞추기」를 누르면 풀린다.
  const [locked, setLocked] = useState<typeof natural | null>(null);
  const box = locked ?? natural;
  const idSignature = elements.map(item => item.id).join('|');
  useEffect(() => { setLocked(null); }, [idSignature]);
  const fit = Math.min(1.12, Math.max(.025, (area.width - 46) / box.width), Math.max(.025, (area.height - 78) / box.height));
  const frame: CameraFrame = { fit, width: area.width, height: area.height, box };
  const scale = viewScale(camera, frame);
  const typeScale = Math.max(1, Math.min(1.6, 1 / scale));
  const { x, y } = viewOrigin(camera, frame);
  const resize = (amount: number) => setCamera(current => ({
    ...current, zoom: Math.max(MIN_ZOOM, Math.min(Math.min(MAX_ZOOM, MAX_SCALE / fit), current.zoom * amount)),
  }));
  // 스크롤(휠, 트랙패드)로 포인터 아래 지점을 기준 삼아 확대·축소한다. 페이지가 같이 스크롤되지 않도록
  // 취소할 수 있는(passive가 아닌) 리스너로 받는다. 이벤트가 빨리 이어져도 직전 카메라에서 이어 계산한다.
  const wheelFrame = useRef(frame);
  wheelFrame.current = frame;
  useEffect(() => {
    const node = viewport.current;
    if (!node) return;
    const onWheel = (event: WheelEvent) => {
      if (!event.deltaY) return;
      event.preventDefault();
      const rect = node.getBoundingClientRect();
      const anchor = { x: event.clientX - rect.left - node.clientLeft, y: event.clientY - rect.top - node.clientTop };
      const factor = wheelFactor(event.deltaY, event.deltaMode, event.ctrlKey);
      setCamera(current => zoomCameraAt(current, factor, anchor, wheelFrame.current));
    };
    node.addEventListener('wheel', onWheel, { passive: false });
    return () => node.removeEventListener('wheel', onWheel);
  }, []);
  const reset = () => { setLocked(null); setCamera({ zoom: 1, x: 0, y: 0 }); };

  useEffect(() => { setMoved({}); }, [resetKey]);

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
      const active = edge.id === selectedRelationship;
      context.strokeStyle = context.fillStyle = active ? accent : stroke;
      context.globalAlpha = active ? 1 : .85;
      context.lineWidth = (active ? 2.2 : 1.3) / scale;
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
    const nextLabels = placeLabels(edges, nodes, { ...area, x, y, scale }, selectedRelationship, title => context.measureText(title).width);
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
  }, [area, edges, nodes, scale, selectedRelationship, x, y]);

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
    drag.current = { id: event.pointerId, x: event.clientX - camera.x, y: event.clientY - camera.y, moved: false };
    panned.current = false;
    event.currentTarget.setPointerCapture(event.pointerId);
  }

  function startGrab(event: PointerEvent<HTMLButtonElement>, node: DiagramNodeLike) {
    if (!onMove || event.button !== 0) return;
    event.stopPropagation();
    grab.current = { id: node.id, pointer: event.pointerId, x: event.clientX, y: event.clientY, origin: { x: node.x, y: node.y }, last: { x: node.x, y: node.y }, active: false };
    try { event.currentTarget.setPointerCapture(event.pointerId); } catch { /* 이미 끝난 포인터 */ }
  }

  function moveGrab(event: PointerEvent<HTMLButtonElement>) {
    const current = grab.current;
    if (!current || current.pointer !== event.pointerId) return;
    const dx = event.clientX - current.x;
    const dy = event.clientY - current.y;
    if (!current.active && Math.hypot(dx, dy) < DRAG_THRESHOLD) return;
    if (!current.active) setLocked(frame => frame ?? natural);
    current.active = true;
    current.last = { x: Math.round(current.origin.x + dx / scale), y: Math.round(current.origin.y + dy / scale) };
    setMoved(previous => ({ ...previous, [current.id]: current.last }));
  }

  function endGrab(event: PointerEvent<HTMLButtonElement>) {
    const current = grab.current;
    if (!current || current.pointer !== event.pointerId) return;
    grab.current = null;
    if (!current.active || !onMove) return;
    suppressClick.current = true;
    setTimeout(() => { suppressClick.current = false; }, 0);
    // 끌던 카드는 마지막 좌표를 쓴다. 이벤트가 빨리 이어지면 아직 다시 그리기 전이라 `nodes`가 낡을 수 있다.
    onMove({ ...Object.fromEntries(nodes.map(item => [item.id, { x: Math.round(item.x), y: Math.round(item.y) }])), [current.id]: current.last });
  }

  return (
    <div
      className="c4-diagram"
      ref={viewport}
      data-connecting={connecting}
      aria-label="C4 다이어그램"
      onPointerDown={startPan}
      onPointerMove={event => {
        const start = drag.current;
        if (start?.id === event.pointerId) {
          if (!start.moved && Math.hypot(event.clientX - start.x - camera.x, event.clientY - start.y - camera.y) < DRAG_THRESHOLD) return;
          start.moved = true;
          panned.current = true;
          setCamera(current => ({ ...current, x: event.clientX - start.x, y: event.clientY - start.y }));
        }
      }}
      onClick={event => {
        if (panned.current) { panned.current = false; return; }
        if (!(event.target as HTMLElement).closest('button')) selectElement(null);
      }}
      onLostPointerCapture={() => { drag.current = null; }}
    >
      <canvas ref={canvas} aria-hidden="true" />
      {playing && edges.length > 0 && <canvas className="c4-flow" ref={flowCanvas} aria-hidden="true" />}
      {!nodes.length && <p className="c4-diagram-empty">{emptyText ?? '이 레벨에 표시할 요소가 없어요.'}</p>}
      <div
        className="c4-diagram-world"
        data-density={scale < .46 ? 'minimal' : scale < .8 ? 'compact' : 'normal'}
        style={{ '--c4-type-scale': typeScale, transform: 'translate(' + x + 'px, ' + y + 'px) scale(' + scale + ')' } as CSSProperties}
      >
        {nodes.map(item => (
          <button
            type="button"
            key={item.id}
            className="c4-card"
            data-kind={item.kind}
            aria-pressed={selectedElement === item.id}
            data-movable={Boolean(onMove)}
            data-dragging={Boolean(moved[item.id])}
            style={{ left: item.x, top: item.y, width: CARD_WIDTH, height: CARD_HEIGHT }}
            title={[item.name, item.description, item.technology].filter(Boolean).join('\n')}
            onPointerDown={event => startGrab(event, item)}
            onPointerMove={moveGrab}
            onPointerUp={endGrab}
            onPointerCancel={endGrab}
            onClick={() => { if (!suppressClick.current) selectElement(item.id); }}
            onDoubleClick={() => drill(item.id)}
          >
            <span className="c4-card-head">
              <span className="c4-card-icon"><Icon name={iconFor(item)} size={18} /></span>
              <span className="c4-card-kind">{KIND_LABELS[item.kind] ?? '요소'}</span>
            </span>
            <strong>{item.name}</strong>
            {item.description && <span className="c4-card-description">{item.description}</span>}
            {item.technology && <span className="c4-card-tech">{item.technology}</span>}
          </button>
        ))}
      </div>
      {labels.map(({ edge, title, center, width, height, compact }) => {
        const style = { left: center.x, top: center.y, width, height };
        const caption = compact ? <Icon name="link" size={12} /> : <span>{title}</span>;
        return (
          <button
            key={edge.id}
            className="c4-edge"
            type="button"
            style={style}
            title={title}
            aria-label={title}
            aria-pressed={edge.id === selectedRelationship}
            onClick={() => selectRelationship(edge.id)}
          >{caption}</button>
        );
      })}
      <div className="c4-zoom" aria-label="다이어그램 보기 도구">
        <button type="button" aria-label="축소" onClick={() => resize(.8)}>−</button>
        <output>{Math.round(scale * 100)}%</output>
        <button type="button" aria-label="확대" onClick={() => resize(1.25)}>+</button>
        <button type="button" aria-label="화면에 맞추기" onClick={reset}><Icon name="maximize" size={14} /></button>
        {edges.length > 0 && (
          <button
            type="button"
            className="c4-flow-toggle"
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
        <div className="c4-minimap" aria-hidden="true">
          {nodes.map(item => (
            <i key={item.id} data-selected={item.id === selectedElement} style={{
              left: 100 * (item.x - natural.minX) / natural.width + '%',
              top: 100 * (item.y - natural.minY) / natural.height + '%',
              width: 100 * CARD_WIDTH / natural.width + '%',
              height: 100 * CARD_HEIGHT / natural.height + '%',
            }} />
          ))}
        </div>
      )}
    </div>
  );
}
