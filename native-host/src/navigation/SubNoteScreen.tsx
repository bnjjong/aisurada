import { useEffect, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { WorkspaceState } from "../workspace/types.ts";
import { NoteScreen } from "./NoteReading.tsx";
import { holdSubChain, subNoteAt } from "./subNotes.ts";
import { subTabId } from "./routes.ts";
import { Page } from "../ui/OtherPages.tsx";

/**
 * 후속 노트 탭의 화면. 최상위 노트부터 [ids]까지 목록을 읽어 온 뒤 그 후속 노트를 일반 노트 화면과 같은 모양으로 보여 준다.
 * 사슬 어딘가의 노트가 지워졌으면 찾을 수 없다고 알리고, 탭은 사용자가 직접 닫게 둔다.
 */
export function SubNoteScreen({
  root,
  ids,
  controller,
  state,
}: {
  root: string;
  ids: string[];
  controller: WorkspaceController;
  state: WorkspaceState;
}) {
  const [failure, setFailure] = useState("");
  const chain = ids.join("/");
  useEffect(() => {
    let current = true;
    setFailure("");
    const held = holdSubChain(controller, root, ids);
    held.done.catch((error) => {
      if (current) setFailure(error instanceof Error ? error.message : String(error));
    });
    return () => {
      current = false;
      held.release();
    };
  }, [controller, root, chain]);
  const note = subNoteAt(state, root, ids);
  if (failure) {
    return (
      <Page title="이 후속 노트를 찾을 수 없어요">
        <p role="alert">{failure}</p>
        <div className="page-actions">
          <button onClick={() => controller.closeTab(subTabId(root, ids))}>탭 닫기</button>
        </div>
      </Page>
    );
  }
  if (!note) return <p role="status" className="subnote-loading">후속 노트를 여는 중…</p>;
  return <NoteScreen note={note} parent={ids.length > 1 ? ids[ids.length - 2] : root} root={root} controller={controller} />;
}
