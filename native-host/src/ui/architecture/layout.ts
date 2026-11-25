import type { C4Element, C4Model, C4Relationship, Point } from './model.ts';
import { KIND_LABELS, LEVEL_LABELS } from './model.ts';

export { KIND_LABELS, LEVEL_LABELS };

export const CARD_WIDTH = 226;
export const CARD_HEIGHT = 144;
const GAP_X = 96;
const GAP_Y = 50;
export type DiagramNode = C4Element & Point;
export type DiagramEdge = C4Relationship & { start: Point; end: Point; a: Point; b: Point };

export function projectSlice(model: C4Model, level: number, parentId: string | null) {
  const nodes = model.elements.filter(item => item.level === level && (level === 1 || item.parentId === parentId));
  const ids = new Set(nodes.map(item => item.id));
  const related = new Set<string>();
  if (level > 1) {
    for (const relation of model.relationships) {
      if (ids.has(relation.source)) related.add(relation.target);
      if (ids.has(relation.target)) related.add(relation.source);
    }
  }
  return nodes.concat(model.elements.filter(item => !ids.has(item.id) && related.has(item.id)));
}

/** 저장된 좌표가 있는 요소는 그 자리에 두고, 없는 요소는 저장된 요소들 아래에 격자로 늘어놓는다. */
export function positionNodes(elements: C4Element[], positions: Record<string, Point> = {}): DiagramNode[] {
  const fixed: DiagramNode[] = [];
  const missing: C4Element[] = [];
  for (const item of elements) {
    const point = positions[item.id];
    if (point) fixed.push({ ...item, x: point.x, y: point.y });
    else missing.push(item);
  }
  const baselineX = fixed.length ? Math.min(...fixed.map(item => item.x)) : 0;
  const baselineY = fixed.length ? Math.max(...fixed.map(item => item.y)) + CARD_HEIGHT + GAP_Y : 0;
  const columns = Math.min(4, Math.ceil(Math.sqrt(missing.length)));
  return fixed.concat(missing.map((item, index) => ({
    ...item,
    x: baselineX + (index % columns) * (CARD_WIDTH + GAP_X),
    y: baselineY + Math.floor(index / columns) * (CARD_HEIGHT + GAP_Y),
  })));
}

export function bounds(nodes: DiagramNode[]) {
  if (!nodes.length) return { minX: 0, minY: 0, width: 1, height: 1 };
  const minX = Math.min(...nodes.map(item => item.x));
  const minY = Math.min(...nodes.map(item => item.y));
  return {
    minX, minY,
    width: Math.max(...nodes.map(item => item.x)) + CARD_WIDTH - minX,
    height: Math.max(...nodes.map(item => item.y)) + CARD_HEIGHT - minY,
  };
}

export const MIN_ZOOM = .45;
export const MAX_ZOOM = 8;
export const MAX_SCALE = 2.5;
/** 화면 아래쪽에 두는 확대 도구 줄의 높이. 가운데 정렬에서 뺀다. */
const TOOLBAR_SPACE = 32;

export type Camera = { zoom: number; x: number; y: number };
/** 카메라가 보는 틀: 맞춤 배율, 보이는 칸의 크기, 요소 전체의 경계. */
export type CameraFrame = { fit: number; width: number; height: number; box: ReturnType<typeof bounds> };

/** 맞춤 배율에 사용자가 고른 확대를 곱한 실제 배율. 너무 커지지 않게 막는다. */
export const viewScale = (camera: Camera, frame: CameraFrame) => Math.min(MAX_SCALE, camera.zoom * frame.fit);

/** 요소 전체를 칸 가운데에 두고 카메라가 민 만큼 옮긴 월드의 왼쪽 위 화면 좌표. */
export function viewOrigin(camera: Camera, frame: CameraFrame): Point {
  const scale = viewScale(camera, frame);
  return {
    x: (frame.width - frame.box.width * scale) / 2 - frame.box.minX * scale + camera.x,
    y: (frame.height - TOOLBAR_SPACE - frame.box.height * scale) / 2 - frame.box.minY * scale + camera.y,
  };
}

/**
 * `anchor`(칸 안의 화면 좌표) 아래의 그림 위치가 그대로 있도록 `factor`배 확대·축소한 새 카메라.
 * 배율 상한에 닿은 뒤에는 확대 값을 더 올리지 않아서, 돌아설 때 바로 반응한다.
 */
export function zoomCameraAt(camera: Camera, factor: number, anchor: Point, frame: CameraFrame): Camera {
  const maxZoom = Math.min(MAX_ZOOM, MAX_SCALE / frame.fit);
  const zoom = Math.max(MIN_ZOOM, Math.min(maxZoom, camera.zoom * factor));
  if (zoom === camera.zoom) return camera;
  const from = viewScale(camera, frame);
  const to = viewScale({ ...camera, zoom }, frame);
  const origin = viewOrigin(camera, frame);
  const world = { x: (anchor.x - origin.x) / from, y: (anchor.y - origin.y) / from };
  const base = viewOrigin({ zoom, x: 0, y: 0 }, frame);
  return { zoom, x: anchor.x - world.x * to - base.x, y: anchor.y - world.y * to - base.y };
}

/** 휠 한 번의 움직임을 확대 배수로 바꾼다. 트랙패드 모아 쥐기(ctrl 키가 붙어 온다)는 더 민감하게 받는다. */
export function wheelFactor(deltaY: number, deltaMode: number, pinch: boolean): number {
  const lines = deltaMode === 1 ? 16 : deltaMode === 2 ? 400 : 1;
  const clamped = Math.max(-240, Math.min(240, deltaY * lines));
  return Math.exp(-clamped * (pinch ? .01 : .0015));
}

/** 카드 가장자리에서 선을 시작하거나 놓을 수 있는 연결 지점. `at`은 그 변을 따라 0~1로 잰 자리다. */
export type PortSide = 'top' | 'bottom' | 'left' | 'right';
export type Port = { id: string; side: PortSide; at: number };

const CENTER_PORTS: Port[] = [
  { id: 'top-2', side: 'top', at: .5 }, { id: 'bottom-2', side: 'bottom', at: .5 },
  { id: 'left-2', side: 'left', at: .5 }, { id: 'right-2', side: 'right', at: .5 },
];
const BOX_PORTS: Port[] = [
  { id: 'top-1', side: 'top', at: .25 }, { id: 'top-2', side: 'top', at: .5 }, { id: 'top-3', side: 'top', at: .75 },
  { id: 'bottom-1', side: 'bottom', at: .25 }, { id: 'bottom-2', side: 'bottom', at: .5 }, { id: 'bottom-3', side: 'bottom', at: .75 },
  { id: 'left-1', side: 'left', at: .25 }, { id: 'left-2', side: 'left', at: .5 }, { id: 'left-3', side: 'left', at: .75 },
  { id: 'right-1', side: 'right', at: .25 }, { id: 'right-2', side: 'right', at: .5 }, { id: 'right-3', side: 'right', at: .75 },
];

/** 엔진이 저장하는 연결 지점 이름(`PORT_IDS`)으로 지점을 찾는다. 모르는 이름이면 `undefined`다. */
export const portById = (id: string | undefined): Port | undefined => {
  if (id === 'left') return BOX_PORTS.find(port => port.id === 'left-2');
  if (id === 'right') return BOX_PORTS.find(port => port.id === 'right-2');
  return BOX_PORTS.find(port => port.id === id);
};

/** 변이 바깥을 향하는 방향. 선은 이 방향으로 카드에서 뻗어 나온다. */
export function portNormal(port: Port): Point {
  switch (port.side) {
    case 'top': return { x: 0, y: -1 };
    case 'bottom': return { x: 0, y: 1 };
    case 'left': return { x: -1, y: 0 };
    default: return { x: 1, y: 0 };
  }
}

/** 상자 모양 카드는 네 변마다 3개씩 12개, 알약 모양인 사람은 모서리가 둥글어서 네 변의 가운데만 둔다. */
export const portsFor = (kind: string): Port[] => kind === 'person' ? CENTER_PORTS : BOX_PORTS;

/** 카드 상자 안에서의 연결 지점 비율(0~1). */
export function portFraction(port: Port): Point {
  switch (port.side) {
    case 'top': return { x: port.at, y: 0 };
    case 'bottom': return { x: port.at, y: 1 };
    case 'left': return { x: 0, y: port.at };
    default: return { x: 1, y: port.at };
  }
}

/** 카드 왼쪽 위가 `node`일 때 연결 지점의 월드 좌표. */
export function portPoint(node: Point, port: Port): Point {
  const fraction = portFraction(port);
  return { x: node.x + fraction.x * CARD_WIDTH, y: node.y + fraction.y * CARD_HEIGHT };
}

/** `point`(월드 좌표)에서 가장 가까운 연결 지점. */
export function nearestPort(node: Point, point: Point, kind: string): Port {
  const ports = portsFor(kind);
  let best = ports[0];
  let distance = Infinity;
  for (const port of ports) {
    const at = portPoint(node, port);
    const length = Math.hypot(at.x - point.x, at.y - point.y);
    if (length < distance) { best = port; distance = length; }
  }
  return best;
}

/** 칸 안의 화면 좌표를 월드 좌표로 바꾼다. `origin`은 월드 왼쪽 위의 화면 좌표다. */
export const toWorld = (screen: Point, origin: Point, scale: number): Point => ({
  x: (screen.x - origin.x) / scale, y: (screen.y - origin.y) / scale,
});

/** `point`(월드 좌표) 아래에 있는 카드의 id. 겹치면 나중에 그린(위에 있는) 카드가 이긴다. `margin`만큼 바깥도 친다. */
export function nodeAt(nodes: (Point & { id: string })[], point: Point, margin = 0): string | null {
  for (let index = nodes.length - 1; index >= 0; index--) {
    const node = nodes[index];
    if (point.x >= node.x - margin && point.x <= node.x + CARD_WIDTH + margin
      && point.y >= node.y - margin && point.y <= node.y + CARD_HEIGHT + margin) return node.id;
  }
  return null;
}

/** 끄는 동안 보이는 점선 곡선의 제어점. 가로로 먼저 뻗었다가 도착점으로 들어온다. */
export function linkCurve(from: Point, to: Point): { a: Point; b: Point } {
  const half = (to.x - from.x) / 2;
  return { a: { x: from.x + half, y: from.y }, b: { x: to.x - half, y: to.y } };
}

export function routeEdges(nodes: DiagramNode[], relationships: C4Relationship[]): DiagramEdge[] {
  const byId = new Map(nodes.map(item => [item.id, item]));
  const count = new Map<string, number>();
  const edges: DiagramEdge[] = [];
  const ports = new Map<string, { point: Point; handle: Point; axis: keyof Point; node: DiagramNode; order: number }[]>();
  const addPort = (node: DiagramNode, horizontal: boolean, sign: number, point: Point, handle: Point, order: number) => {
    const key = JSON.stringify([node.id, horizontal, sign]);
    const group = ports.get(key) ?? [];
    group.push({ point, handle, axis: horizontal ? 'y' : 'x', node, order });
    ports.set(key, group);
  };

  for (const relation of relationships) {
    const source = byId.get(relation.source);
    const target = byId.get(relation.target);
    if (!source || !target) continue;
    const dx = target.x - source.x;
    const dy = target.y - source.y;
    const key = [source.id, target.id].sort().join('/');
    const index = count.get(key) ?? 0;
    count.set(key, index + 1);
    const shift = index === 0 ? 0 : (index % 2 ? 1 : -1) * Math.ceil(index / 2) * 23;
    const sx = source.x + CARD_WIDTH / 2;
    const sy = source.y + CARD_HEIGHT / 2;
    const tx = target.x + CARD_WIDTH / 2;
    const ty = target.y + CARD_HEIGHT / 2;
    const horizontal = Math.abs(dx) / CARD_WIDTH >= Math.abs(dy) / CARD_HEIGHT;
    const sign = Math.sign(horizontal ? dx : dy) || 1;
    let start: Point, end: Point, a: Point, b: Point;
    if (source.id === target.id) {
      start = { x: source.x + CARD_WIDTH, y: sy + 14 };
      end = { x: source.x + CARD_WIDTH, y: sy - 14 };
      a = { x: start.x + 75 + Math.abs(shift), y: start.y + 23 };
      b = { x: end.x + 75 + Math.abs(shift), y: end.y - 23 };
    } else if (horizontal) {
      const edgeShift = Math.max(-32, Math.min(32, shift));
      start = { x: sx + sign * CARD_WIDTH / 2, y: sy + edgeShift };
      end = { x: tx - sign * CARD_WIDTH / 2, y: ty + edgeShift };
      const middle = (start.x + end.x) / 2;
      a = { x: middle, y: start.y + shift / 3 };
      b = { x: middle, y: end.y + shift / 3 };
    } else {
      const edgeShift = Math.max(-75, Math.min(75, shift));
      start = { x: sx + edgeShift, y: sy + sign * CARD_HEIGHT / 2 };
      end = { x: tx + edgeShift, y: ty - sign * CARD_HEIGHT / 2 };
      const middle = (start.y + end.y) / 2;
      a = { x: start.x + shift / 3, y: middle };
      b = { x: end.x + shift / 3, y: middle };
    }
    // 저장된 연결 지점이 있으면 그 지점에서 시작·도착하고, 그 변이 향하는 쪽으로 곡선이 뻗는다.
    // 지점이 정해진 끝은 아래의 자동 간격 나눔에 끼지 않는다.
    const fromPort = source.id === target.id ? undefined : portById(relation.sourcePort);
    const toPort = source.id === target.id ? undefined : portById(relation.targetPort);
    if (fromPort) start = portPoint(source, fromPort);
    if (toPort) end = portPoint(target, toPort);
    if (fromPort || toPort) {
      const reach = Math.max(44, Math.min(150, Math.hypot(end.x - start.x, end.y - start.y) / 2));
      if (fromPort) { const normal = portNormal(fromPort); a = { x: start.x + normal.x * reach, y: start.y + normal.y * reach }; }
      if (toPort) { const normal = portNormal(toPort); b = { x: end.x + normal.x * reach, y: end.y + normal.y * reach }; }
    }
    edges.push({ ...relation, start, end, a, b });
    if (source.id !== target.id) {
      if (!fromPort) addPort(source, horizontal, sign, start, a, horizontal ? ty : tx);
      if (!toPort) addPort(target, horizontal, -sign, end, b, horizontal ? sy : sx);
    }
  }

  for (const group of ports.values()) {
    if (group.length < 2) continue;
    group.sort((a, b) => a.order - b.order || a.point[a.axis] - b.point[b.axis]);
    const spacing = Math.min(20, 64 / (group.length - 1));
    group.forEach(({ point, handle, node, axis }, index) => {
      const center = node[axis] + (axis === 'x' ? CARD_WIDTH : CARD_HEIGHT) / 2;
      const next = center + (index - (group.length - 1) / 2) * spacing;
      handle[axis] += next - point[axis];
      point[axis] = next;
    });
  }
  return edges;
}

type Rectangle = { left: number; right: number; top: number; bottom: number };
type Viewport = Point & { width: number; height: number; scale: number };

export type DiagramLabel = {
  edge: DiagramEdge;
  index: number;
  title: string;
  center: Point;
  anchor: Point;
  width: number;
  height: number;
  compact: boolean;
};

function overlaps(a: Rectangle, b: Rectangle, margin = 0) {
  return a.left < b.right + margin && b.left < a.right + margin
    && a.top < b.bottom + margin && b.top < a.bottom + margin;
}

function pointOnCurve(edge: DiagramEdge, t: number): Point {
  const u = 1 - t;
  const value = (key: 'x' | 'y') =>
    u ** 3 * edge.start[key] + 3 * u ** 2 * t * edge.a[key]
    + 3 * u * t ** 2 * edge.b[key] + t ** 3 * edge.end[key];
  return { x: value('x'), y: value('y') };
}

export function measureEdge(edge: DiagramEdge): (progress: number) => Point {
  const segments = 24;
  const lengths = [0];
  let previous = edge.start;
  for (let step = 1; step <= segments; step++) {
    const next = pointOnCurve(edge, step / segments);
    lengths.push(lengths[step - 1] + Math.hypot(next.x - previous.x, next.y - previous.y));
    previous = next;
  }
  const length = lengths[segments];
  return progress => {
    if (progress <= 0 || length === 0) return edge.start;
    if (progress >= 1) return edge.end;
    const distance = progress * length;
    const step = lengths.findIndex(value => value >= distance);
    const fraction = (distance - lengths[step - 1]) / (lengths[step] - lengths[step - 1]);
    return pointOnCurve(edge, (step - 1 + fraction) / segments);
  };
}

export function nearestAnchorOnCurve(edge: DiagramEdge, point: Point, project: (p: Point) => Point): Point {
  let closest = project(pointOnCurve(edge, 0.5));
  let minDistance = Math.hypot(point.x - closest.x, point.y - closest.y);
  for (let step = 0; step <= 40; step++) {
    const t = step / 40;
    const pt = project(pointOnCurve(edge, t));
    const dist = Math.hypot(point.x - pt.x, point.y - pt.y);
    if (dist < minDistance) {
      minDistance = dist;
      closest = pt;
    }
  }
  return closest;
}

export function placeLabels(
  edges: DiagramEdge[],
  nodes: DiagramNode[],
  view: Viewport,
  selected: string | null,
  measure: (text: string) => number,
  /** 라벨을 카드와 같은 비율로 키우는 배율. 글자, 여백, 높이, 후보 너비를 함께 곱한다. `measure`도 같은 배율의 글꼴로 재야 한다. */
  unit = 1,
  /** 사용자가 직접 옮겨 둔 라벨의 월드 좌표. 있으면 이 위치를 우선 쓴다. */
  positions?: Record<string, Point>,
): DiagramLabel[] {
  const project = (point: Point) => ({
    x: view.x + point.x * view.scale,
    y: view.y + point.y * view.scale,
  });
  const obstacles: Rectangle[] = nodes.map(node => {
    const origin = project(node);
    return {
      left: origin.x, right: origin.x + CARD_WIDTH * view.scale,
      top: origin.y, bottom: origin.y + CARD_HEIGHT * view.scale,
    };
  });
  // 글이 한 줄로 들어갈 수 있으면 줄바꿈 없이 한 줄로 놓는다. 칸이 좁을 때만 두 줄이나 아이콘으로 줄인다.
  const maxWidth = Math.max(104 * unit, Math.min(520 * unit, Math.floor(view.width * .46)));
  const offsets = [0, -18, 18, -34, 34, -50, 50].map(offset => offset * unit);
  const compactWidth = 26 * unit;
  const placements: DiagramLabel[] = [];
  const candidates = edges.map((edge, index) => ({ edge, index }));
  if (selected) candidates.sort((a, b) => Number(b.edge.id === selected) - Number(a.edge.id === selected));

  for (const { edge, index } of candidates) {
    const title = [edge.label || '관계', edge.technology].filter(Boolean).join(' · ');
    if (view.scale < .3 && edge.id !== selected) continue;
    // 좌우 여백 14와 테두리 2를 더한다(`architecture.css`의 `.c4-edge`와 같은 값이다)
    const textWidth = Math.ceil(measure(title)) + 14 * unit + 2;

    // 사용자가 직접 위치를 지정한 경우 우선 배치한다
    const custom = positions?.[edge.id];
    if (custom) {
      const center = project(custom);
      const width = Math.min(maxWidth, textWidth);
      const height = textWidth > width ? Math.round(36 * unit + 2) : Math.round(20 * unit + 2);
      const anchor = nearestAnchorOnCurve(edge, center, project);
      const rect = {
        left: center.x - width / 2, right: center.x + width / 2,
        top: center.y - height / 2, bottom: center.y + height / 2,
      };
      placements.push({ edge, index, title, center, anchor, width, height, compact: false });
      obstacles.push(rect);
      continue;
    }

    const widths = [...new Set([Math.min(maxWidth, textWidth), Math.min(168 * unit, textWidth), Math.min(120 * unit, textWidth), Math.min(92 * unit, textWidth), compactWidth].map(Math.round))];
    const anchors = [.5, .36, .64, .23, .77, .12, .88].map(t => {
      const anchor = project(pointOnCurve(edge, t));
      const next = project(pointOnCurve(edge, t + .01));
      const dx = next.x - anchor.x;
      const dy = next.y - anchor.y;
      const magnitude = Math.hypot(dx, dy) || 1;
      return { anchor, normal: { x: -dy / magnitude, y: dx / magnitude } };
    });
    let placed = false;
    for (const width of widths) {
      const compact = width === Math.round(compactWidth) && width < textWidth;
      const height = !compact && textWidth > width ? Math.round(36 * unit + 2) : Math.round(20 * unit + 2);
      for (const offset of offsets) {
        for (const { anchor, normal } of anchors) {
          const center = { x: anchor.x + normal.x * offset, y: anchor.y + normal.y * offset };
          const rect = {
            left: center.x - width / 2, right: center.x + width / 2,
            top: center.y - height / 2, bottom: center.y + height / 2,
          };
          if (rect.left < 6 || rect.right > view.width - 6 || rect.top < 6 || rect.bottom > view.height - 49
            || obstacles.some(other => overlaps(rect, other, 5))) continue;
          placements.push({ edge, index, title, center, anchor, width, height, compact });
          obstacles.push(rect);
          placed = true;
          break;
        }
        if (placed) break;
      }
      if (placed) break;
    }
  }
  return placements;
}
