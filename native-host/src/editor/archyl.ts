import { invoke } from '@tauri-apps/api/core';

type Row = Record<string, unknown>;
type Level = 'context' | 'container' | 'component';
export type ArchylView = { projectId: string; level: Level; elementId?: string };
export type ArchylElement = {
  id: string;
  name: string;
  kind: 'system' | 'container' | 'component';
  parentId?: string;
  description: string;
  technology: string;
  type: string;
  isExternal?: boolean;
  position?: { x: number; y: number };
  tags?: string[];
};
export type ArchylModel = {
  elements: ArchylElement[];
  relationships: { source: string; target: string; label: string; technology: string }[];
};
export type ArchylProject = { id: string; name: string; repositories: string[] };

const ID = /^[a-zA-Z0-9_-]{1,128}$/;
const marker = '%% aisurada-archyl ';
export const archylHome = 'https://app.archyl.com/';
const byId = (value: unknown): value is string => typeof value === 'string' && ID.test(value);
const row = (value: unknown): Row => value && typeof value === 'object' && !Array.isArray(value) ? value as Row : {};
const text = (...values: unknown[]) => values.find((value): value is string => typeof value === 'string' && Boolean(value.trim()))?.trim() || '';
const array = (value: unknown): unknown[] => Array.isArray(value) ? value : [];
const unwrap = (value: unknown) => row(value).data ?? value;

function githubRepository(value: unknown): string {
  if (typeof value !== 'string') return '';
  const name = value.trim().replace(/^(?:https?:\/\/|ssh:\/\/git@)github\.com\/|^git@github\.com:/i, '').replace(/\/$|\.git$/gi, '');
  return /^[a-z0-9][a-z0-9-]{0,38}\/[a-z0-9._-]{1,100}$/i.test(name) ? name.toLowerCase() : '';
}

export const archylStatus = () => invoke<boolean>('archyl_status');
export const connectArchyl = () => invoke<void>('archyl_connect');
export const disconnectArchyl = () => invoke<void>('archyl_disconnect');

export type ArchylWriteTool =
  | 'create_system'
  | 'create_container'
  | 'create_component'
  | 'create_relationship'
  | 'update_positions'
  | 'update_system'
  | 'delete_system'
  | 'delete_relationship';

export async function archylWrite(tool: ArchylWriteTool, args: Record<string, unknown>): Promise<unknown> {
  const value = await invoke('archyl_write', { tool, args });
  return unwrap(value);
}

export async function createArchylSystem(params: {
  projectId: string;
  name: string;
  type: 'software_system' | 'person' | 'external_system';
  description?: string;
  isExternal?: boolean;
}) {
  return archylWrite('create_system', {
    projectId: params.projectId,
    name: params.name,
    type: params.type,
    ...(params.description ? { description: params.description } : {}),
    ...(params.isExternal !== undefined ? { isExternal: params.isExternal } : {}),
  });
}

export async function createArchylContainer(params: {
  systemId: string;
  name: string;
  containerType: string;
  description?: string;
}) {
  return archylWrite('create_container', {
    systemId: params.systemId,
    name: params.name,
    containerType: params.containerType,
    ...(params.description ? { description: params.description } : {}),
  });
}

export async function createArchylComponent(params: {
  containerId: string;
  name: string;
  componentType: string;
  description?: string;
}) {
  return archylWrite('create_component', {
    containerId: params.containerId,
    name: params.name,
    componentType: params.componentType,
    ...(params.description ? { description: params.description } : {}),
  });
}

export async function createArchylRelationship(params: {
  projectId: string;
  sourceId: string;
  sourceType: number; // 1: system, 2: container, 3: component
  targetId: string;
  targetType: number;
  type: string;
  description?: string;
}) {
  return archylWrite('create_relationship', {
    projectId: params.projectId,
    sourceId: params.sourceId,
    sourceType: params.sourceType,
    targetId: params.targetId,
    targetType: params.targetType,
    type: params.type,
    ...(params.description ? { description: params.description } : {}),
  });
}

export async function deleteArchylSystem(systemId: string) {
  return archylWrite('delete_system', { systemId });
}

export async function deleteArchylRelationship(relationshipId: string) {
  return archylWrite('delete_relationship', { relationshipId });
}

export async function listArchylProjects(): Promise<ArchylProject[]> {
  const value = unwrap(await invoke('archyl_read', { tool: 'list_projects' }));
  const items = Array.isArray(value) ? value : row(value).projects ?? row(value).items;
  if (!Array.isArray(items)) throw Error('Archyl 프로젝트 목록을 읽지 못했어요.');
  return items.flatMap((item) => {
    const project = row(item);
    const id = project.id ?? project.projectId;
    if (!byId(id)) return [];
    const direct = [project.repository, project.repositoryUrl, project.repoUrl, project.githubRepo, project.gitRepoUrl, project.repo];
    const repos = [...direct, ...array(project.repositories), ...array(project.repos)].flatMap((repo) => {
      const details = row(repo);
      return [repo, details.url, details.fullName, details.full_name, details.github, details.repositoryUrl, details.cloneUrl];
    });
    return [{ id, name: text(project.name, project.title) || id, repositories: Array.from(new Set(repos.map(githubRepository).filter(Boolean))) }];
  });
}

export async function getArchylModel(projectId: string): Promise<ArchylModel> {
  const value = unwrap(await invoke('archyl_read', {
    tool: 'get_project_c4_model',
    projectId,
  }));
  return parseArchylModel(value);
}

export async function getArchylProjectTeam(projectId: string): Promise<string> {
  const value = row(unwrap(await invoke('archyl_read', { tool: 'get_project', projectId })));
  const project = row(value.project ?? value);
  const teamId = text(project.teamId, project.team_id, row(project.team).id, value.teamId);
  if (!byId(teamId)) throw Error('Archyl 프로젝트가 속한 팀을 확인할 수 없어요.');
  return teamId;
}

export function parseArchylModel(value: unknown): ArchylModel {
  const root = row(value);
  const model = row(root.model ?? root.c4Model ?? root.architecture ?? value);
  const systems = model.systems ?? root.systems;
  if (!Array.isArray(systems)) throw Error('Archyl C4 모델을 읽지 못했어요.');
  const elements = new Map<string, ArchylElement>();
  const add = (items: unknown, kind: ArchylElement['kind'], parentId?: string) => {
    for (const item of array(items)) {
      const data = row(item);
      const { id } = data;
      if (!byId(id)) continue;
      const parent = text(data.parentId, kind === 'container' ? data.systemId : data.containerId, parentId);
      const posObj = row(data.position);
      const position = typeof posObj.x === 'number' && typeof posObj.y === 'number'
        ? { x: posObj.x as number, y: posObj.y as number }
        : undefined;
      elements.set(id, {
        id, kind, parentId: parent || elements.get(id)?.parentId,
        name: text(data.name, data.title) || id,
        description: text(data.description),
        technology: text(data.technology, ...array(data.technologies).map(item => row(item).name)),
        type: text(data.type, data.kind).toLowerCase(),
        isExternal: data.isExternal === true || text(data.type, data.kind).toLowerCase() === 'external_system',
        position,
        tags: array(data.tags).map(String),
      });
      if (kind === 'system') add(data.containers, 'container', id);
      if (kind === 'container') add(data.components, 'component', id);
    }
  };
  add(model.containers ?? root.containers, 'container');
  add(model.components ?? root.components, 'component');
  add(systems, 'system');
  const relations = model.relationships ?? root.relationships;
  if (relations !== undefined && !Array.isArray(relations)) throw Error('Archyl C4 모델의 연결 정보를 읽지 못했어요.');
  const relationships = array(relations).flatMap(item => {
    const relation = row(item);
    const source = text(relation.sourceId, relation.sourceElementId, relation.fromId, relation.source, row(relation.source).id);
    const target = text(relation.targetId, relation.targetElementId, relation.toId, relation.target, row(relation.target).id);
    return source && target ? [{ source, target, label: text(relation.label, relation.description), technology: text(relation.technology) }] : [];
  });
  return { elements: [...elements.values()], relationships };
}

export function archylView(source: string): ArchylView | undefined {
  const line = source.split('\n', 2).find(part => part.startsWith(marker));
  if (!line) return;
  try {
    const { projectId, level, elementId } = row(JSON.parse(line.slice(marker.length)));
    if (!byId(projectId)) return;
    if (level === 'context') return { projectId, level };
    if ((level === 'container' || level === 'component') && byId(elementId)) return { projectId, level, elementId };
  } catch {
    // 본문에서 메타데이터를 지우거나 고쳤으면 일반 Mermaid로 표시한다.
  }
}

export function archylSource(model: ArchylModel, view: ArchylView): string {
  const parent = model.elements.find(element => element.id === view.elementId);
  if ((view.level === 'container' && parent?.kind !== 'system') || (view.level === 'component' && parent?.kind !== 'container')) {
    throw Error('Archyl에서 이 뷰의 상위 요소를 찾지 못했어요.');
  }
  const scope = model.elements.filter(element => view.level === 'context'
    ? element.kind === 'system' : element.kind === view.level && element.parentId === parent?.id);
  if (!scope.length) throw Error('이 Archyl 뷰에 표시할 요소가 없어요.');
  const inside = new Set(scope.map(element => element.id));
  const relationships = model.relationships.filter(relation => inside.has(relation.source) || inside.has(relation.target));
  const neighbors = new Set(relationships.flatMap(relation => [relation.source, relation.target]));
  const external = model.elements.filter(element => !inside.has(element.id) && neighbors.has(element.id)
    && (view.level !== 'container' || element.kind !== 'component'));
  const visible = [...scope, ...external];
  if (visible.length > 150 || relationships.length > 300) throw Error('이 뷰가 너무 커요. 상위 시스템이나 하위 컨테이너 하나를 골라 주세요.');
  const aliases = new Map(visible.map((element, index) => [element.id, 'n' + (index + 1)]));
  const header = { context: 'C4Context', container: 'C4Container', component: 'C4Component' }[view.level];
  // 외부에서 받은 레이블이 Mermaid 문법이나 코드 울타리를 만들지 못하게 한다.
  const label = (value: string) => '"' + value.replace(/[\x00-\x1f\x7f"\x60<>&%\\]/g, ' ').slice(0, 600) + '"';
  const renderElement = (element: ArchylElement) => {
    const outside = !inside.has(element.id) || /external|third.party/.test(element.type);
    const name = label(element.name);
    const description = label(element.description);
    const technology = label(element.technology);
    const alias = aliases.get(element.id);
    if (element.kind === 'system') {
      const kind = /person|actor/.test(element.type) ? 'Person' : outside ? 'System_Ext' : 'System';
      return `${kind}(${alias}, ${name}, ${description})`;
    }
    const kind = element.kind === 'component' ? 'Component' : /database|datastore/.test(element.type) ? 'ContainerDb' : 'Container';
    return `${kind}${outside ? '_Ext' : ''}(${alias}, ${name}, ${technology}, ${description})`;
  };
  const boundary = parent && (view.level === 'container' ? 'System_Boundary' : 'Container_Boundary');
  const nodes = boundary && parent
    ? [`${boundary}(scope_boundary, ${label(parent.name)}) {`, ...scope.map(renderElement), '}', ...external.map(renderElement)]
    : visible.map(renderElement);
  const edges = relationships.flatMap(relation => {
    const source = aliases.get(relation.source);
    const target = aliases.get(relation.target);
    return source && target ? [`Rel(${source}, ${target}, ${label(relation.label)}, ${label(relation.technology)})`] : [];
  });
  return [header, marker + JSON.stringify(view), ...nodes, ...edges].join('\n');
}
