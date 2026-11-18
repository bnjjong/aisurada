// 승인 대기함의 열림 상태를 한 곳에 둔다.
// 레일 말풍선, 글로벌 리소스 화면의 배너·카드, 에이전트·스킬 편집창이 어느 화면에서든 같은 창(`ApprovalsHost`)을 열 수 있게 하려는 것이다.
import { useSyncExternalStore } from "react";

let opened = false;
const listeners = new Set<() => void>();

const emit = () => {
  for (const listener of [...listeners]) listener();
};

export const subscribeApprovalsDialog = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};

export const isApprovalsDialogOpen = () => opened;

/** 승인 대기함을 연다. 이미 열려 있으면 아무것도 하지 않는다. */
export function openApprovalsDialog() {
  if (opened) return;
  opened = true;
  emit();
}

/** 승인 대기함을 닫는다. */
export function closeApprovalsDialog() {
  if (!opened) return;
  opened = false;
  emit();
}

export const useApprovalsDialogOpen = (): boolean => useSyncExternalStore(subscribeApprovalsDialog, isApprovalsDialogOpen, isApprovalsDialogOpen);
