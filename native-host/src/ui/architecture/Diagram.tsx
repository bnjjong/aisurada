import { Fragment, useEffect, useLayoutEffect, useMemo, useRef, useState } from 'react';
import type { CSSProperties, PointerEvent } from 'react';
import { Icon } from '../Icon.tsx';
import type { C4Element, C4Relationship, Point } from './model.ts';
import { bounds, CARD_HEIGHT, CARD_WIDTH, KIND_LABELS, MAX_SCALE, MAX_ZOOM, MIN_ZOOM, linkCurve, measureEdge, nearestPort, nodeAt, placeLabels, portFraction, portPoint, portsFor, positionNodes, routeEdges, toWorld, viewOrigin, viewScale, wheelFactor, zoomCameraAt } from './layout.ts';
import type { CameraFrame, Port } from './layout.ts';
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
/** 카드 가장자리에서 이만큼(화면 픽셀) 바깥에 놓아도 그 카드로 친다. 연결 지점이 카드 가장자리에 걸쳐 있어서다. */
const DROP_MARGIN = 12;

/**
 * 선을 끄는 중의 상태. `create`는 카드의 연결 지점에서 새 관계를 시작하고, `reconnect`는 고른 관계의 한쪽 끝을 옮긴다.
 * `from`은 움직이지 않는 쪽의 월드 좌표이고, `pointer`는 칸 안의 화면 좌표다.
 */
type Link = {
  kind: 'create' | 'reconnect';
  fixedId: string;
  from: Point;
  edge?: string;
  end?: 'source' | 'target';
  pointer: Point;
  over: string | null;
};

function portStyle(port: Port): CSSProperties {
  const fraction = portFraction(port);
  return { left: fraction.x * 100 + '%', top: fraction.y * 100 + '%' };
}

export function Diagram({
  elements, relationships, positions, selectedElement, selectedRelationship, selectElement, selectRelationship, drill,
  onMove, onConnect, onReconnect, resetKey, connecting = false, emptyText,
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
  /** 카드의 연결 지점에서 다른 카드로 끌어다 놓으면 알린다. 없으면 연결 지점을 보이지 않는다 */
  onConnect?(source: string, target: string): void;
  /** 고른 관계의 한쪽 끝을 다른 카드로 끌어다 놓으면 새 출발·도착 요소를 알린다. 없으면 끝을 옮길 수 없다 */
  onReconnect?(relationshipId: string, source: string, target: string): void;
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
  // 카드와 글자는 `scale()` 변환 대신 `zoom`으로 키운다. 변환은 작게 그린 층을 늘려 쓰는 길이 있어 글자가 번질 수 있고,
  // `zoom`은 처음부터 최종 크기로 배치한다. 이동도 소수점 위치에 그리면 번지므로 화면 픽셀에 맞춰 반올림한다.
  const density = typeof window === 'undefined' ? 1 : window.devicePixelRatio || 1;
  const pixelX = Math.round(x * density) / density;
  const pixelY = Math.round(y * density) / density;
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

  // 선 끌기(연결 지점에서 새 관계, 또는 고른 관계의 끝 옮기기). 포인터가 카드 위인지는 DOM이 아니라 월드 좌표로 판정한다.
  const [link, setLink] = useState<Link | null>(null);
  const latest = useRef({ nodes, relationships, pixelX, pixelY, scale, link, onConnect, onReconnect });
  latest.current = { nodes, relationships, pixelX, pixelY, scale, link, onConnect, onReconnect };
  const localPoint = (event: { clientX: number; clientY: number }): Point => {
    const node = viewport.current!;
    const rect = node.getBoundingClientRect();
    return { x: event.clientX - rect.left - node.clientLeft, y: event.clientY - rect.top - node.clientTop };
  };
  const hitCard = (point: Point, fixedId: string): string | null => {
    const current = latest.current;
    const id = nodeAt(current.nodes, toWorld(point, { x: current.pixelX, y: current.pixelY }, current.scale), DROP_MARGIN / current.scale);
    return id === fixedId ? null : id;
  };
  const linking = link !== null;
  useEffect(() => {
    if (!linking) return;
    const move = (event: globalThis.PointerEvent) => {
      const point = localPoint(event);
      setLink(current => current && { ...current, pointer: point, over: hitCard(point, current.fixedId) });
    };
    const finish = (event: globalThis.PointerEvent) => {
      const current = latest.current;
      const active = current.link;
      setLink(null);
      // 끝난 뒤에 이어 오는 click이 바탕을 눌러 선택을 풀지 않게 한 번 삼킨다
      panned.current = true;
      setTimeout(() => { panned.current = false; }, 0);
      if (!active) return;
      const over = hitCard(localPoint(event), active.fixedId);
      if (!over) return;
      if (active.kind === 'create') { current.onConnect?.(active.fixedId, over); return; }
      const edge = current.relationships.find(item => item.id === active.edge);
      if (!edge) return;
      const source = active.end === 'source' ? over : edge.source;
      const target = active.end === 'target' ? over : edge.target;
      if (source !== edge.source || target !== edge.target) current.onReconnect?.(edge.id, source, target);
    };
    const cancel = () => setLink(null);
    const key = (event: KeyboardEvent) => { if (event.key === 'Escape') { event.stopPropagation(); setLink(null); } };
    window.addEventListener('pointermove', move);
    window.addEventListener('pointerup', finish);
    window.addEventListener('pointercancel', cancel);
    window.addEventListener('keydown', key, true);
    return () => {
      window.removeEventListener('pointermove', move);
      window.removeEventListener('pointerup', finish);
      window.removeEventListener('pointercancel', cancel);
      window.removeEventListener('keydown', key, true);
    };
  }, [linking]);

  function startCreate(event: PointerEvent<HTMLElement>, node: DiagramNodeLike, port: Port) {
    if (event.button !== 0) return;
    event.stopPropagation();
    event.preventDefault();
    setLink({ kind: 'create', fixedId: node.id, from: portPoint(node, port), pointer: localPoint(event), over: null });
  }

  function startReconnect(event: PointerEvent<HTMLElement>, edge: (typeof edges)[number], end: 'source' | 'target') {
    if (event.button !== 0) return;
    event.stopPropagation();
    event.preventDefault();
    const fixed = end === 'target' ? edge.start : edge.end;
    setLink({ kind: 'reconnect', edge: edge.id, end, fixedId: end === 'target' ? edge.source : edge.target, from: fixed, pointer: localPoint(event), over: null });
  }

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

  const toScreen = (point: Point): Point => ({ x: pixelX + point.x * scale, y: pixelY + point.y * scale });
  const overNode = link?.over ? nodes.find(item => item.id === link.over) : undefined;
  const overPortValue = link && overNode ? nearestPort(overNode, toWorld(link.pointer, { x: pixelX, y: pixelY }, scale), overNode.kind) : undefined;
  const overPort = overNode && overPortValue ? { node: overNode.id, port: overPortValue.id } : undefined;
  const selectedEdge = onReconnect && selectedRelationship ? edges.find(item => item.id === selectedRelationship) : undefined;
  let preview: { path: string; end: Point } | null = null;
  if (link) {
    const from = toScreen(link.from);
    const end = overNode && overPortValue ? toScreen(portPoint(overNode, overPortValue)) : link.pointer;
    const { a, b } = linkCurve(from, end);
    preview = { path: 'M ' + from.x + ' ' + from.y + ' C ' + a.x + ' ' + a.y + ' ' + b.x + ' ' + b.y + ' ' + end.x + ' ' + end.y, end };
  }

  return (
    <div
      className="c4-diagram"
      ref={viewport}
      data-connecting={connecting}
      data-linking={linking}
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
      <div className="c4-diagram-pan" style={{ transform: 'translate(' + pixelX + 'px, ' + pixelY + 'px)' }}>
      <div
        className="c4-diagram-world"
        data-density={scale < .46 ? 'minimal' : scale < .8 ? 'compact' : 'normal'}
        style={{ '--c4-type-scale': typeScale, zoom: scale } as CSSProperties}
      >
        {nodes.map(item => (
          <Fragment key={item.id}>
          <button
            type="button"
            className="c4-card"
            data-kind={item.kind}
            data-link-target={link?.over === item.id}
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
          {onConnect && (
            <span
              className="c4-ports"
              data-for={item.id}
              data-show={link?.over === item.id ? 'target' : selectedElement === item.id ? 'selected' : undefined}
              style={{ left: item.x, top: item.y, width: CARD_WIDTH, height: CARD_HEIGHT }}
              aria-hidden="true"
            >
              {portsFor(item.kind).map(port => (
                <i
                  key={port.id}
                  className="c4-port"
                  data-port={port.id}
                  data-near={overPort?.node === item.id && overPort.port === port.id}
                  style={portStyle(port)}
                  onPointerDown={event => startCreate(event, item, port)}
                />
              ))}
            </span>
          )}
          </Fragment>
        ))}
      </div>
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
      {selectedEdge && (['source', 'target'] as const).map(end => {
        const at = toScreen(end === 'source' ? selectedEdge.start : selectedEdge.end);
        return (
          <button
            key={end}
            type="button"
            className="c4-edge-end"
            data-end={end}
            aria-label={end === 'source' ? '관계의 시작 끝을 끌어서 옮기기' : '관계의 도착 끝을 끌어서 옮기기'}
            title={end === 'source' ? '끌어서 시작 요소 바꾸기' : '끌어서 도착 요소 바꾸기'}
            style={{ left: at.x, top: at.y }}
            onPointerDown={event => startReconnect(event, selectedEdge, end)}
          />
        );
      })}
      {preview && (
        <svg className="c4-link" aria-hidden="true">
          <path d={preview.path} />
          <circle cx={preview.end.x} cy={preview.end.y} r={link?.over ? 6 : 4.5} />
        </svg>
      )}
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
