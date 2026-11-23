import type { ArchylElement, ArchylModel, ArchylRelationship } from './client.ts';

export const CARD_WIDTH = 226;
export const CARD_HEIGHT = 144;
const GAP_X = 96;
const GAP_Y = 50;
export const LEVEL_LABELS = ['시스템 컨텍스트', '컨테이너', '컴포넌트', '코드'];
export const KIND_LABELS: Record<string, string> = {
  person: '사람', system: '시스템', external: '외부 시스템', container: '컨테이너', component: '컴포넌트', code: '코드',
};
export type Point = { x: number; y: number };
export type DiagramNode = ArchylElement & Point;
export type DiagramEdge = ArchylRelationship & { start: Point; end: Point; a: Point; b: Point };

export function projectSlice(model: ArchylModel, level: number, parentId: string | null) {
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

export function positionNodes(elements: ArchylElement[]): DiagramNode[] {
  const occupied = new Set<string>();
  const fixed: DiagramNode[] = [];
  const missing: ArchylElement[] = [];
  for (const item of elements) {
    const point = item.position;
    const key = point && JSON.stringify([point.x, point.y]);
    if (point && key && !occupied.has(key)) {
      occupied.add(key);
      fixed.push({ ...item, ...point });
    } else {
      missing.push(item);
    }
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

export function routeEdges(nodes: DiagramNode[], relationships: ArchylRelationship[]): DiagramEdge[] {
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
    edges.push({ ...relation, start, end, a, b });
    if (source.id !== target.id) {
      addPort(source, horizontal, sign, start, a, horizontal ? ty : tx);
      addPort(target, horizontal, -sign, end, b, horizontal ? sy : sx);
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

export function placeLabels(
  edges: DiagramEdge[],
  nodes: DiagramNode[],
  view: Viewport,
  selected: string | null,
  measure: (text: string) => number,
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
  const maxWidth = Math.max(96, Math.min(152, Math.floor(view.width / 5)));
  const placements: DiagramLabel[] = [];
  const candidates = edges.map((edge, index) => ({ edge, index }));
  if (selected) candidates.sort((a, b) => Number(b.edge.id === selected) - Number(a.edge.id === selected));

  for (const { edge, index } of candidates) {
    const title = [edge.label || '관계', edge.technology].filter(Boolean).join(' · ');
    if (view.scale < .3 && edge.id !== selected) continue;
    const textWidth = Math.ceil(measure(title)) + 12;
    const widths = [...new Set([Math.min(maxWidth, textWidth), Math.min(118, textWidth), Math.min(85, textWidth), 24])];
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
      const compact = width === 24 && width < textWidth;
      const height = !compact && textWidth > width ? 34 : 20;
      for (const offset of [0, -16, 16, -30, 30, -46, 46]) {
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
