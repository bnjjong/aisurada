import type { ArchylElement, ArchylModel, ArchylRelationship } from './client.ts';

export const CARD_WIDTH = 226;
export const CARD_HEIGHT = 106;
const GAP_X = 96;
const GAP_Y = 50;
export const LEVEL_LABELS = ['시스템 컨텍스트', '컨테이너', '컴포넌트', '코드'];
export const KIND_LABELS: Record<string, string> = {
  person: '사람', system: '시스템', external: '외부 시스템', container: '컨테이너', component: '컴포넌트', code: '코드',
};
export type Point = { x: number; y: number };
export type DiagramNode = ArchylElement & Point;
export type DiagramEdge = ArchylRelationship & { start: Point; end: Point; a: Point; b: Point; labelPosition: Point };

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
    let start: Point, end: Point, a: Point, b: Point;
    if (source.id === target.id) {
      start = { x: source.x + CARD_WIDTH, y: sy + 14 };
      end = { x: source.x + CARD_WIDTH, y: sy - 14 };
      a = { x: start.x + 75 + Math.abs(shift), y: start.y + 23 };
      b = { x: end.x + 75 + Math.abs(shift), y: end.y - 23 };
    } else if (Math.abs(dx) / CARD_WIDTH >= Math.abs(dy) / CARD_HEIGHT) {
      const sign = Math.sign(dx) || 1;
      const edgeShift = Math.max(-32, Math.min(32, shift));
      start = { x: sx + sign * CARD_WIDTH / 2, y: sy + edgeShift };
      end = { x: tx - sign * CARD_WIDTH / 2, y: ty + edgeShift };
      const middle = (start.x + end.x) / 2;
      a = { x: middle, y: start.y + shift / 3 };
      b = { x: middle, y: end.y + shift / 3 };
    } else {
      const sign = Math.sign(dy) || 1;
      const edgeShift = Math.max(-75, Math.min(75, shift));
      start = { x: sx + edgeShift, y: sy + sign * CARD_HEIGHT / 2 };
      end = { x: tx + edgeShift, y: ty - sign * CARD_HEIGHT / 2 };
      const middle = (start.y + end.y) / 2;
      a = { x: start.x + shift / 3, y: middle };
      b = { x: end.x + shift / 3, y: middle };
    }
    edges.push({
      ...relation, start, end, a, b,
      labelPosition: { x: (start.x + end.x + 3 * (a.x + b.x)) / 8, y: (start.y + end.y + 3 * (a.y + b.y)) / 8 },
    });
  }
  return edges;
}
