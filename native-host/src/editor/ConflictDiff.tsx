import { useEffect, useMemo, useRef, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { confirm } from '@tauri-apps/plugin-dialog';
import { compareBodies } from './diff.ts';

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
  const diff = useMemo(() => right === null ? null : compareBodies(snapshot, right), [snapshot, right]);
  return <div className="cdiff" role="dialog" aria-modal="true" aria-label="내 편집과 외부 파일 차이" tabIndex={-1}
    ref={element => element?.focus({ preventScroll: true })} onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(); } }}>
    <header><h2>내 편집과 외부 파일 차이</h2><span>왼쪽 번호: 내 편집 · 오른쪽 번호: 외부 파일</span></header>
    {failed ? <p className="cdiff-msg">외부 파일 본문을 못 읽어서 차이를 그릴 수 없습니다. 사이드카가 떠 있는지 보고 다시 열어 주세요.</p>
      : right === null ? <p className="cdiff-msg" role="status">외부 파일을 읽고 있습니다.</p> : <>
        {snapshot === right ? <p className="cdiff-msg">지금은 두 글이 글자까지 같습니다. 어느 쪽을 골라도 본문은 안 바뀝니다.</p>
          : <p className="cdiff-count">내 편집에만 {diff!.deleted}줄, 외부에만 {diff!.added}줄입니다.
            {!!diff!.skipped && ` 줄 ${diff!.skipped}개는 너무 길어서 줄 안까지 갈라 보지 않았습니다.`}</p>}
        {!right && !!snapshot && <p className="cdiff-msg">외부 파일 쪽이 비어 있습니다. 이 화면에서는 빈 까닭을 가릴 수 없습니다.</p>}
        <div className="cdiff-scroll"><table className="cdiff-table"><thead><tr><th>내 편집</th><th>외부 파일</th><th>상태</th><th>본문</th></tr></thead><tbody>
          {diff!.rows.map((row, index) => <tr key={index} className={`cdiff-row-${row.kind}`}><td>{row.left}</td><td>{row.right}</td><td className="cdiff-status-cell">{row.kind === 'del' ? '내 편집에만' : row.kind === 'add' ? '외부에만' : ''}</td>
            <td>{row.pieces ? row.pieces.map((piece, i) => <span key={i} className={piece.changed ? 'cdiff-word-change' : undefined}>{piece.text}</span>) : row.text || ' '}</td></tr>)}
        </tbody></table></div>
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
