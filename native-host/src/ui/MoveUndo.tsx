import { useSyncExternalStore } from "react";
import { Icon } from "./Icon.tsx";

/** 막 옮긴 노트 하나의 되돌리기 정보. [undo]가 원래 아젠다로 다시 옮긴다. */
export interface MoveUndo {
  noteId: string;
  /** 옮겨 간 아젠다 이름(알림 글에 쓴다) */
  title: string;
  undo(): Promise<unknown>;
}

/** 되돌리기 알림을 띄워 두는 시간. 알림 센터의 토스트(8초)와 맞춘다. */
export const MOVE_UNDO_MS = 8000;

let current: MoveUndo | null = null;
let timer: ReturnType<typeof setTimeout> | undefined;
const listeners = new Set<() => void>();
const publish = (next: MoveUndo | null) => {
  current = next;
  listeners.forEach((listener) => listener());
};

/**
 * 노트를 다른 아젠다로 옮긴 직후 「「…」로 옮겼어요 · 되돌리기」 알림을 띄운다(TSK-2 결정 5-가: 확인 창 없이 바로 옮긴다).
 * 아젠다 화면에서 옮기면 그 노트가 목록에서 빠지며 노트 머리도 사라지므로, 알림은 노트 머리가 아니라 앱 맨 위에서 그린다.
 * 새로 띄우면 앞의 알림을 바꿔 끼운다.
 */
export function showMoveUndo(next: MoveUndo): void {
  clearTimeout(timer);
  publish(next);
  timer = setTimeout(() => publish(null), MOVE_UNDO_MS);
}

/** 되돌리기 알림을 닫는다. */
export function dismissMoveUndo(): void {
  clearTimeout(timer);
  publish(null);
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

/** 앱 아래쪽 가운데에 뜨는 되돌리기 알림. 「되돌리기」는 노트만 원래 아젠다로 옮기고, 옮기며 새로 만든 아젠다는 남긴다. */
export function MoveUndoToast() {
  const move = useSyncExternalStore(subscribe, () => current, () => null);
  if (!move) return null;
  return (
    <div className="move-undo" role="status" aria-live="polite" data-note-id={move.noteId}>
      <span className="move-undo-text">「{move.title}」로 옮겼어요</span>
      <button
        type="button"
        className="move-undo-action"
        onClick={() => {
          dismissMoveUndo();
          void move.undo();
        }}
      >
        되돌리기
      </button>
      <button type="button" className="move-undo-close" aria-label="알림 닫기" onClick={dismissMoveUndo}>
        <Icon name="close" size={12} />
      </button>
    </div>
  );
}
