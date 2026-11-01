import { useEffect, useMemo, useRef, useState } from 'react';
import { compareBodies } from './diff.ts';

export function ConflictDiff({ left, readRight, close, discard, keep }: {
  left: string; readRight(): Promise<string>; close(): void; discard(): void; keep(): void;
}) {
  const [snapshot] = useState(left), read = useRef(readRight);
  const [right, setRight] = useState<string | null>(null), [failed, setFailed] = useState(false);
  useEffect(() => { let live = true; void read.current().then(body => { if (live) setRight(body); }, () => { if (live) setFailed(true); }); return () => { live = false; }; }, []);
  const diff = useMemo(() => right === null ? null : compareBodies(snapshot, right), [snapshot, right]);
  return <div className="cdiff" role="dialog" aria-modal="true" aria-label="내 편집과 디스크 차이" tabIndex={-1}
    ref={element => element?.focus({ preventScroll: true })} onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); close(); } }}>
    <header><h2>내 편집과 디스크 차이</h2><span>왼쪽 번호: 내 편집 · 오른쪽 번호: 디스크</span></header>
    {failed ? <p className="cdiff-msg">디스크 본문을 못 읽어서 차이를 그릴 수 없습니다. 사이드카가 떠 있는지 보고 다시 열어 주세요.</p>
      : right === null ? <p className="cdiff-msg" role="status">디스크를 읽고 있습니다.</p> : <>
        {snapshot === right ? <p className="cdiff-msg">지금은 두 글이 글자까지 같습니다. 어느 쪽을 골라도 본문은 안 바뀝니다.</p>
          : <p className="cdiff-count">내 편집에만 {diff!.deleted}줄, 디스크에만 {diff!.added}줄입니다.
            {!!diff!.skipped && ` 줄 ${diff!.skipped}개는 너무 길어서 줄 안까지 갈라 보지 않았습니다.`}</p>}
        {!right && !!snapshot && <p className="cdiff-msg">디스크 쪽이 비어 있습니다. 이 화면에서는 빈 까닭을 가릴 수 없습니다.</p>}
        <div className="cdiff-scroll"><table className="cdiff-table"><thead><tr><th>내 편집</th><th>디스크</th><th>표시</th><th>본문</th></tr></thead><tbody>
          {diff!.rows.map((row, index) => <tr key={index} className={`cdiff-row-${row.kind}`}><td>{row.left}</td><td>{row.right}</td><td>{row.kind === 'del' ? '−' : row.kind === 'add' ? '+' : ''}</td>
            <td>{row.pieces ? row.pieces.map((piece, i) => <span key={i} className={piece.changed ? 'cdiff-word-change' : undefined}>{piece.text}</span>) : row.text || ' '}</td></tr>)}
        </tbody></table></div>
      </>}
    <footer><button onClick={close}>닫기</button><button onClick={() => { close(); discard(); }}>디스크 버전 불러오기</button><button onClick={() => { close(); keep(); }}>내 편집 유지</button></footer>
  </div>;
}
