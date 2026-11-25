// C4 모델을 엔진(`get_c4_model`, `apply_c4_changes`)으로 읽고 쓰는 얇은 층.
// 규칙과 검사는 엔진이 한다. 여기서는 응답 모양을 확인하고 오류를 사람이 읽는 글로 바꾼다.
import { isRecord, explainEngineError } from '../engineClient.ts';
import type { EngineCall } from '../engineClient.ts';
import type { C4Model, C4Operation } from './model.ts';

export function errorText(value: unknown): string {
  return explainEngineError(value, (_, kind) => kind === 'precondition_failed'
    ? '다른 곳(AI나 다른 창)에서 먼저 고쳤어요. 최신 모델을 다시 불러왔어요. 한 번 더 시도해 주세요.' : undefined);
}

export const isPreconditionError = (value: unknown) =>
  isRecord(value) && (value.kind === 'precondition_failed' || /AIS-412/.test(String(value.message ?? value.stderr ?? '')));

/** 엔진 응답을 모델로 읽는다. 모양이 틀리면 오류를 던진다. */
export function readModel(value: unknown): C4Model {
  if (!isRecord(value) || !Array.isArray(value.elements) || !Array.isArray(value.relationships)
    || !isRecord(value.views) || typeof value.precondition !== 'string' || typeof value.project !== 'string') {
    throw new Error('C4 모델 응답의 모양을 확인하지 못했어요.');
  }
  return value as unknown as C4Model;
}

export async function getC4Model(call: EngineCall, project: string): Promise<C4Model> {
  return readModel(await call({ command: 'get_c4_model', args: { project } }));
}

/** 변경 묶음을 한 번에 적용한다. 읽은 모델의 확인 값을 실어 보내서, 그 사이 바뀌었으면 엔진이 아무것도 쓰지 않고 거절한다. */
export async function applyC4Changes(call: EngineCall, model: C4Model, operations: C4Operation[]): Promise<C4Model> {
  return readModel(await call({
    command: 'apply_c4_changes',
    args: { project: model.project, f1: model.precondition, operations },
  }));
}
