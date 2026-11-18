// 승인 대기함 창을 어느 화면에서든 열 수 있게 작업 공간 껍데기에 한 번만 세운다. 열림 상태는 approvalsDialogStore가 든다.
import { useEffect, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import { ApprovalsDialog } from "./ApprovalPanel.tsx";
import { closeApprovalsDialog, useApprovalsDialogOpen } from "./approvalsDialogStore.ts";
import { syncGlobalResources } from "./globalResources/globalResourcesStore.ts";
import { listResources } from "./resourcesClient.ts";

export function ApprovalsHost({ controller }: { controller: WorkspaceController }) {
  const opened = useApprovalsDialogOpen();
  const [names, setNames] = useState<Record<string, string>>({});

  // 참조 변경에서 자원 id를 이름으로 보이려고, 창이 열릴 때 글로벌 자원 이름을 읽어 둔다. 못 읽으면 id가 그대로 보인다
  useEffect(() => {
    if (!opened) return;
    let alive = true;
    listResources(controller.callEngine, { global: true }).then(
      ({ resources }) => {
        if (alive) setNames(Object.fromEntries(resources.map((r) => [r.id, r.name])));
      },
      () => {},
    );
    return () => {
      alive = false;
    };
  }, [opened, controller.callEngine]);

  if (!opened) return null;
  return (
    <ApprovalsDialog
      call={controller.callEngine}
      names={names}
      close={closeApprovalsDialog}
      // 승인하거나 되돌리면 글로벌 리소스 화면의 목록이 바로 새 내용을 보이게 다시 읽는다
      onChanged={() => void syncGlobalResources(controller.callEngine)}
    />
  );
}
