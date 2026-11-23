import { useEffect, useRef, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { confirm } from '@tauri-apps/plugin-dialog';
import { DiffView } from '../ui/diffView/DiffView.tsx';

async function askConfirm(msg: string): Promise<boolean> {
  if (isTauri()) {
    return await confirm(msg, { title: '편집 버리기', kind: 'warning', okLabel: '버리기', cancelLabel: '취소' });
  }
  return window.confirm(msg);
}

export function ConflictDiff({ left, readRight, close, discard, keep }: {
  left: string; readRight(): Promise<string>; close(): void; discard(): void; keep(): void;
}) {
  const [snapshot] = useState(left), read = useRef(readRight);
  const [right, setRight] = useState<string | null>(null), [failed, setFailed] = useState(false);
  useEffect(() => { let live = true; void read.current().then(body => { if (live) setRight(body); }, () => { if (live) setFailed(true); }); return () => { live = false; }; }, []);
  return <div className="cdiff" role="dialog" aria-modal="true" aria-label="내 편집과 외부 파일 차이" tabIndex={-1}
    ref={element => element?.focus({ preventScroll: true })} onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(); } }}>
    <header><h2>내 편집과 외부 파일 차이</h2><span>열 때 읽은 외부 파일과 견줍니다. 이후 변경은 다시 열면 볼 수 있습니다.</span></header>
    {failed ? <p className="cdiff-msg">외부 파일 본문을 못 읽어서 차이를 그릴 수 없습니다. 파일이 아직 있는지 보고 다시 열어 주세요.</p>
      : right === null ? <p className="cdiff-msg" role="status">외부 파일을 읽고 있습니다.</p> : <>
        {!right && !!snapshot && <p className="cdiff-msg">외부 파일 쪽이 비어 있습니다. 이 화면에서는 빈 까닭을 가릴 수 없습니다.</p>}
        <div className="cdiff-scroll">
          <DiffView before={snapshot} after={right} beforeLabel="내 편집" afterLabel="외부 파일"
            label="내 편집과 외부 파일 비교" tone="neutral" rememberAs="conflict" formal
            sameMessage="지금은 두 글이 글자까지 같습니다. 어느 쪽을 골라도 본문은 안 바뀝니다." />
        </div>
      </>}
    <footer>
      <button onClick={close}>닫기</button>
      <button data-conflict-action="load" onClick={async () => {
        const ok = await askConfirm("지금까지 작성한 편집 내용을 버리고 외부 파일을 불러올까요?");
        if (ok) {
          close();
          discard();
        }
      }}>내 편집 버리고 외부 파일 불러오기</button>
      <button data-conflict-action="keep" onClick={() => { close(); keep(); }}>외부 변경 덮어쓰고 내 편집 유지</button>
    </footer>
  </div>;
}
