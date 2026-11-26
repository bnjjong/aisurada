// C4 모델의 화면 쪽 타입과 순수 계산. 규칙(부모 종류, id 모양, 개수 상한)은 엔진이 지키고, 여기서는 폼을 열기 전에
// 알 수 있는 것(어느 종류를 고를 수 있는지, 뷰 이름, 새 id, 지우면 사라질 것)만 계산한다.

export type ElementKind = 'person' | 'system' | 'external' | 'container' | 'component';
export type C4Element = {
  id: string;
  kind: string;
  level: number;
  name: string;
  description: string;
  technology: string;
  parentId: string | null;
  codeRef: string | null;
};
/** `sourcePort`, `targetPort`는 선이 카드의 어느 연결 지점에 붙는지다. 비어 있으면 자동 배치가 정한다. */
export type C4Relationship = { id: string; source: string; target: string; label: string; technology: string; sourcePort?: string; targetPort?: string };
export type Point = { x: number; y: number };
export type C4Views = Record<string, Record<string, Point>>;
/** 프로젝트가 가진 아키텍처 하나의 이름표. `main`은 기존 아키텍처이고 파일이 없어도 늘 있다. 개수는 파일이 깨졌으면 없다. */
export type C4Diagram = { id: string; name: string; elements?: number; relationships?: number };
export const MAIN_DIAGRAM = 'main';
/** 이력(타임 트래블)의 점 하나. `at`은 유닉스 밀리초, `by`는 `app`이나 `ai`(시작 상태와 외부 변경은 빈 글). */
export type C4Snapshot = {
  seq: number;
  at: number;
  by: string;
  kind: 'change' | 'baseline' | 'external';
  summary: string;
  elements: number;
  relationships: number;
};
/** 이력의 점 목록. `latestIsCurrent`가 거짓이면 마지막 점 뒤에 이력에 없는 변경이 있다. */
export type C4History = { snapshots: C4Snapshot[]; latestIsCurrent: boolean };
/** 엔진이 주는 모델 한 벌. `precondition`은 다음 쓰기에 그대로 싣는 확인 값이다. */
export type C4Model = {
  project: string;
  diagram: C4Diagram;
  elements: C4Element[];
  relationships: C4Relationship[];
  views: C4Views;
  precondition: string;
  /** 과거 시점을 읽은 응답에만 있다. 이 모델로는 쓸 수 없다(`precondition`도 없다). */
  snapshot?: C4Snapshot;
};

export type C4Operation =
  | { op: 'put_element'; id: string; kind?: string; name?: string; description?: string; technology?: string; parent?: string; codeRef?: string }
  | { op: 'delete_element'; id: string }
  | { op: 'put_relationship'; id?: string; source?: string; target?: string; label?: string; technology?: string; sourcePort?: string; targetPort?: string }
  | { op: 'delete_relationship'; id: string }
  | { op: 'set_positions'; view: string; positions: Record<string, Point> };

export const LEVEL_LABELS = ['시스템 컨텍스트', '컨테이너', '컴포넌트'];
export const KIND_LABELS: Record<string, string> = {
  person: '사람', system: '시스템', external: '외부 시스템', container: '컨테이너', component: '컴포넌트',
};

/** 단계에서 새로 만들 수 있는 종류. 컨테이너와 컴포넌트는 부모 밑에만 둔다. */
export function kindsForLevel(level: number): ElementKind[] {
  if (level === 1) return ['system', 'person', 'external'];
  return level === 2 ? ['container'] : ['component'];
}

/** 단계의 요소가 놓일 부모 종류. 1단계는 부모가 없다. */
export function parentKindFor(level: number): ElementKind | null {
  return level === 2 ? 'system' : level === 3 ? 'container' : null;
}

/** 위치를 저장하는 뷰 이름. 부모가 필요한 단계에서 부모가 없으면 `null`이다. */
export function viewIdFor(level: number, parentId: string | null): string | null {
  if (level === 1) return 'context';
  if (!parentId) return null;
  return (level === 2 ? 'container:' : 'component:') + parentId;
}

const FALLBACK_SLUG: Record<string, string> = {
  person: 'person', system: 'system', external: 'external', container: 'container', component: 'component',
};

/** 이름에서 영문 id를 만든다. 영문이 없는 이름(한글 따위)은 종류 이름으로 시작해 겹치지 않는 번호를 붙인다. */
export function newElementId(name: string, kind: string, taken: Iterable<string>): string {
  const used = new Set(taken);
  const slug = name.normalize('NFKD').toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '').slice(0, 40).replace(/-+$/, '');
  const base = slug || FALLBACK_SLUG[kind] || 'item';
  if (!used.has(base) && slug) return base;
  for (let number = slug ? 2 : 1; ; number++) {
    const candidate = `${base}-${number}`;
    if (!used.has(candidate)) return candidate;
  }
}

export type ElementForm = { kind: string; name: string; description: string; technology: string; codeRef: string };
/** `sourcePort`, `targetPort`는 폼 칸이 아니다. 끌어서 만들 때 놓은 연결 지점을 새 관계에 실어 보내려고 들고 있다. */
export type RelationForm = { source: string; target: string; label: string; technology: string; sourcePort?: string; targetPort?: string };

export const emptyElementForm = (kind: string): ElementForm => ({ kind, name: '', description: '', technology: '', codeRef: '' });
export const elementFormOf = (element: C4Element): ElementForm => ({
  kind: element.kind, name: element.name, description: element.description, technology: element.technology, codeRef: element.codeRef ?? '',
});
export const relationFormOf = (relation: C4Relationship): RelationForm => ({
  source: relation.source, target: relation.target, label: relation.label, technology: relation.technology,
});

/** 새 요소를 만드는 변경. 부모는 지금 보고 있는 부모다. */
export function createElementOps(model: C4Model, form: ElementForm, parentId: string | null): C4Operation[] {
  const id = newElementId(form.name, form.kind, model.elements.map(item => item.id));
  const op: C4Operation = {
    op: 'put_element', id, kind: form.kind, name: form.name.trim(), description: form.description, technology: form.technology.trim(),
  };
  if (form.codeRef.trim()) op.codeRef = form.codeRef.trim();
  if (parentId) op.parent = parentId;
  return [op];
}

/** 요소를 고치는 변경. 바뀐 칸만 보낸다. 바뀐 것이 없으면 빈 목록이다. */
export function editElementOps(before: C4Element, form: ElementForm): C4Operation[] {
  const op: Extract<C4Operation, { op: 'put_element' }> = { op: 'put_element', id: before.id };
  if (form.kind !== before.kind) op.kind = form.kind;
  if (form.name.trim() !== before.name) op.name = form.name.trim();
  if (form.description.trim() !== before.description) op.description = form.description;
  if (form.technology.trim() !== before.technology) op.technology = form.technology.trim();
  if (form.codeRef.trim() !== (before.codeRef ?? '')) op.codeRef = form.codeRef.trim();
  return Object.keys(op).length > 2 ? [op] : [];
}

export function createRelationOps(form: RelationForm): C4Operation[] {
  const op: Extract<C4Operation, { op: 'put_relationship' }> = { op: 'put_relationship', source: form.source, target: form.target, label: form.label.trim(), technology: form.technology.trim() };
  if (form.sourcePort) op.sourcePort = form.sourcePort;
  if (form.targetPort) op.targetPort = form.targetPort;
  return [op];
}

export function editRelationOps(before: C4Relationship, form: RelationForm): C4Operation[] {
  const op: Extract<C4Operation, { op: 'put_relationship' }> = { op: 'put_relationship', id: before.id };
  if (form.source !== before.source) op.source = form.source;
  if (form.target !== before.target) op.target = form.target;
  if (form.label.trim() !== before.label) op.label = form.label.trim();
  if (form.technology.trim() !== before.technology) op.technology = form.technology.trim();
  return Object.keys(op).length > 2 ? [op] : [];
}

/** 요소를 지우면 함께 사라질 것: 자식 요소(손자까지)와 그 요소들에 닿은 관계 */
export function deletionImpact(model: C4Model, id: string): { elements: number; relationships: number } {
  const gone = new Set([id]);
  for (let grew = true; grew;) {
    grew = false;
    for (const item of model.elements) {
      if (!gone.has(item.id) && item.parentId && gone.has(item.parentId)) { gone.add(item.id); grew = true; }
    }
  }
  return {
    elements: gone.size - 1,
    relationships: model.relationships.filter(item => gone.has(item.source) || gone.has(item.target)).length,
  };
}

export function relationTitle(model: C4Model, relation: C4Relationship): string {
  const name = (id: string) => model.elements.find(item => item.id === id)?.name ?? id;
  return `${name(relation.source)} → ${name(relation.target)}`;
}

/** 카드 위에 보여 줄 변화: 앞 시점에 없던 요소는 `added`, 있었지만 내용이 달라진 요소는 `changed`. */
export type ElementChanges = Record<string, 'added' | 'changed'>;

/** 두 시점의 모델을 견줘 요소의 변화를 구한다. 위치는 보지 않는다. */
export function diffElements(previous: C4Model | undefined, current: C4Model): ElementChanges {
  const changes: ElementChanges = {};
  if (!previous) return changes;
  const before = new Map(previous.elements.map(item => [item.id, JSON.stringify(item)]));
  for (const item of current.elements) {
    const old = before.get(item.id);
    if (old === undefined) changes[item.id] = 'added';
    else if (old !== JSON.stringify(item)) changes[item.id] = 'changed';
  }
  return changes;
}

/** 타임라인에 그리는 점. `snapshot`이 없는 점은 이력에 아직 없는 지금 상태(`Now`)다. */
export type TravelPoint = { snapshot: C4Snapshot | null };

/** 이력에서 타임라인의 점을 만든다. 마지막 점은 늘 `Now`다(마지막 점이 지금과 같으면 그 점이 `Now`다). */
export function travelPoints(history: C4History): TravelPoint[] {
  const points: TravelPoint[] = history.snapshots.map(snapshot => ({ snapshot }));
  if (!history.latestIsCurrent || points.length === 0) points.push({ snapshot: null });
  return points;
}

/** 점 하나를 말로 풀어 쓴다. `Now`는 「현재 상태」, 나머지는 시각과 쓴 쪽과 바뀐 것이다. */
export function describePoint(point: TravelPoint, isNow: boolean, format: (at: number) => string = formatAt): string {
  const snapshot = point.snapshot;
  if (isNow) return snapshot?.summary ? `현재 상태 · ${snapshot.summary}` : '현재 상태';
  if (!snapshot) return '현재 상태';
  const by = snapshot.by === 'ai' ? 'AI' : snapshot.by === 'app' ? '앱' : '';
  return [format(snapshot.at), by, snapshot.summary || '변경'].filter(Boolean).join(' · ');
}

/** 시각을 `11월 26일 09:12` 모양으로 쓴다. */
export function formatAt(at: number): string {
  const date = new Date(at);
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${date.getMonth() + 1}월 ${date.getDate()}일 ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}
