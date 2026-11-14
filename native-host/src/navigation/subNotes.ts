import type { WorkspaceController } from '../workspace/controller.ts';
import type { Note, WorkspaceState } from '../workspace/types.ts';

/**
 * 최상위 노트부터 [ids]까지 후속 노트 목록을 한 단계씩 읽어 오고, 읽는 동안 그 목록들을 화면에 쓰는 중으로 붙들어 둔다.
 *
 * 후속 노트는 주소만으로는 어디 있는지 모른다. 사슬을 이루는 부모 목록을 위에서부터 읽어야 엔진에 보낼 경로가 정해지기 때문이다.
 * 후속 노트 탭과 탭 이름 표시, 노트 창의 후속 노트 창이 같은 규칙으로 읽도록 한 곳에 둔다.
 * 사슬 어딘가에 노트가 없으면 [done]이 오류로 끝나고, [release]는 붙들어 둔 것을 놓는다.
 */
export function holdSubChain(
  controller: WorkspaceController,
  root: string,
  ids: string[],
): { release: () => void; done: Promise<void> } {
  const stops: (() => void)[] = [];
  let cancelled = false;
  const done = (async () => {
    if (!controller.getSnapshot().workspace?.notes.some((note) => note.id === root)) {
      throw new Error('상위 노트를 찾을 수 없어요.');
    }
    let parent = root;
    for (const id of ids) {
      if (cancelled) return;
      stops.push(controller.data.retainSubNotes(parent));
      let notes = await controller.data.loadSubNotes(parent);
      // 같은 목록을 더 늦게 요청한 읽기가 있으면 먼저 요청한 읽기는 낡은 목록을 돌려준다. 한 번 더 읽어 확인한다.
      if (!cancelled && !notes.some((note) => note.id === id)) notes = await controller.data.loadSubNotes(parent);
      if (!notes.some((note) => note.id === id)) throw new Error('후속 노트를 찾을 수 없어요.');
      parent = id;
    }
  })();
  return {
    release: () => {
      cancelled = true;
      stops.forEach((stop) => stop());
    },
    done,
  };
}

/** 사슬의 마지막 후속 노트. 아직 읽지 못했거나 없으면 undefined다. */
export function subNoteAt(state: WorkspaceState, root: string, ids: string[]): Note | undefined {
  const parent = ids.length > 1 ? ids[ids.length - 2] : root;
  return state.vault.subNotes[parent]?.find((note) => note.id === ids[ids.length - 1]);
}
