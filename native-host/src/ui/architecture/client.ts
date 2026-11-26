// C4 모델을 엔진(`get_c4_model`, `apply_c4_changes`와 아키텍처 관리 명령 4개)으로 읽고 쓰는 얇은 층.
// 규칙과 검사는 엔진이 한다. 여기서는 응답 모양을 확인하고 오류를 사람이 읽는 글로 바꾼다.
import { isRecord, explainEngineError } from '../engineClient.ts';
import type { EngineCall } from '../engineClient.ts';
import { MAIN_DIAGRAM } from './model.ts';
import type { C4Diagram, C4History, C4Model, C4Operation } from './model.ts';

export function errorText(value: unknown): string {
  return explainEngineError(value, (_, kind) => kind === 'precondition_failed'
    ? '다른 곳(AI나 다른 창)에서 먼저 고쳤어요. 최신 모델을 다시 불러왔어요. 한 번 더 시도해 주세요.' : undefined);
}

export const isPreconditionError = (value: unknown) =>
  isRecord(value) && (value.kind === 'precondition_failed' || /AIS-412/.test(String(value.message ?? value.stderr ?? '')));

/** 엔진 응답을 모델로 읽는다. 모양이 틀리면 오류를 던진다. */
export function readModel(value: unknown, past = false): C4Model {
  if (!isRecord(value) || !Array.isArray(value.elements) || !Array.isArray(value.relationships)
    || !isRecord(value.views) || (!past && typeof value.precondition !== 'string') || typeof value.project !== 'string'
    || !isRecord(value.diagram) || typeof value.diagram.id !== 'string') {
    throw new Error('C4 모델 응답의 모양을 확인하지 못했어요.');
  }
  return value as unknown as C4Model;
}

/** `at`을 주면 이력의 그 시점을 읽는다(읽기 전용, `precondition` 없음). */
export async function getC4Model(call: EngineCall, project: string, diagram: string = MAIN_DIAGRAM, at?: number): Promise<C4Model> {
  const args = at === undefined ? { project, diagram } : { project, diagram, at };
  return readModel(await call({ command: 'get_c4_model', args }), at !== undefined);
}

/** 변경 묶음을 한 번에 적용한다. 읽은 모델의 확인 값을 실어 보내서, 그 사이 바뀌었으면 엔진이 아무것도 쓰지 않고 거절한다. */
export async function applyC4Changes(call: EngineCall, model: C4Model, operations: C4Operation[]): Promise<C4Model> {
  return readModel(await call({
    command: 'apply_c4_changes',
    args: { project: model.project, diagram: model.diagram.id, f1: model.precondition, operations },
  }));
}

function readDiagram(value: unknown): C4Diagram {
  if (!isRecord(value) || typeof value.id !== 'string' || typeof value.name !== 'string') throw new Error('아키텍처 응답의 모양을 확인하지 못했어요.');
  return value as unknown as C4Diagram;
}

/** 프로젝트의 아키텍처 목록. 첫 항목은 늘 `main`이다. */
export async function listC4Diagrams(call: EngineCall, project: string): Promise<C4Diagram[]> {
  const answer = await call({ command: 'list_c4_diagrams', args: { project } });
  if (!isRecord(answer) || !Array.isArray(answer.diagrams)) throw new Error('아키텍처 목록 응답의 모양을 확인하지 못했어요.');
  return answer.diagrams.map(readDiagram);
}

async function diagramAnswer(call: EngineCall, command: string, args: Record<string, string>): Promise<C4Diagram> {
  const answer = await call({ command, args });
  return readDiagram(isRecord(answer) ? answer.diagram : undefined);
}

export const createC4Diagram = (call: EngineCall, project: string, name: string) =>
  diagramAnswer(call, 'create_c4_diagram', { project, name });
export const renameC4Diagram = (call: EngineCall, project: string, diagram: string, name: string) =>
  diagramAnswer(call, 'rename_c4_diagram', { project, diagram, name });
export async function deleteC4Diagram(call: EngineCall, project: string, diagram: string): Promise<void> {
  await call({ command: 'delete_c4_diagram', args: { project, diagram } });
}

/** 아키텍처의 변경 이력(오래된 것부터). */
export async function listC4History(call: EngineCall, project: string, diagram: string = MAIN_DIAGRAM): Promise<C4History> {
  const answer = await call({ command: 'list_c4_history', args: { project, diagram } });
  if (!isRecord(answer) || !Array.isArray(answer.snapshots) || typeof answer.latestIsCurrent !== 'boolean') {
    throw new Error('이력 응답의 모양을 확인하지 못했어요.');
  }
  return { snapshots: answer.snapshots as C4History['snapshots'], latestIsCurrent: answer.latestIsCurrent };
}
