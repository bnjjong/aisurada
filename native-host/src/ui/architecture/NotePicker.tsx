// 요소에 붙일 노트를 고르는 창. 고른 노트 id 목록을 변경으로 바꿔 부모에게 넘기고, 저장과 검사는 엔진이 한다.
import { useMemo, useState } from 'react';
import { Icon } from '../Icon.tsx';
import { Modal } from '../Modal.tsx';
import { setNotesOps } from './model.ts';
import type { C4Element, C4Operation, NoteRef } from './model.ts';

/** 한 번에 보여 주는 줄 수. 노트가 아주 많아도 창이 느려지지 않게 검색으로 좁히게 한다. */
const MAX_ROWS = 200;

export function NotePickerModal({ element, candidates, submit, close }: {
  element: C4Element;
  /** vault의 노트들(보여 줄 차례대로) */
  candidates: NoteRef[];
  submit(operations: C4Operation[]): Promise<string | null>;
  close(): void;
}) {
  const [picked, setPicked] = useState<string[]>(element.notes);
  const [query, setQuery] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  const pickedSet = useMemo(() => new Set(picked), [picked]);

  const rows = useMemo(() => {
    const needle = query.trim().toLowerCase();
    const matches = needle ? candidates.filter(note => note.title.toLowerCase().includes(needle)) : candidates;
    // 이미 붙은 노트를 위로 올려 무엇이 붙어 있는지 먼저 보이게 한다.
    const attached = matches.filter(note => element.notes.includes(note.id));
    const others = matches.filter(note => !element.notes.includes(note.id));
    return attached.concat(others);
  }, [candidates, element.notes, query]);
  const shown = rows.slice(0, MAX_ROWS);

  const toggle = (id: string) => setPicked(current => current.includes(id) ? current.filter(other => other !== id) : [...current, id]);
  const changed = setNotesOps(element, picked).length > 0;

  async function save() {
    const operations = setNotesOps(element, picked);
    if (!operations.length) return close();
    setBusy(true); setError('');
    const failure = await submit(operations);
    if (failure === null) close();
    else { setError(failure); setBusy(false); }
  }

  return (
    <Modal title="노트 연결" close={close} className="ndf c4-form c4-pick" resizable={false}>
      <div className="c4-pick-body">
        <p className="c4-hint">‘{element.name}’에 붙일 노트를 골라 주세요. 여러 개를 고를 수 있어요.</p>
        <label className="c4-pick-search">
          <Icon name="search" size={14} />
          <input autoFocus type="search" aria-label="노트 검색" placeholder="노트 제목으로 찾기" value={query} onChange={event => setQuery(event.target.value)} />
        </label>
        {shown.length === 0 ? (
          <p className="c4-note-empty">{candidates.length === 0 ? '아직 노트가 없어요.' : '찾는 노트가 없어요.'}</p>
        ) : (
          <ul className="c4-pick-list" aria-label="노트 목록">
            {shown.map(note => (
              <li key={note.id}>
                <label className="c4-pick-row" data-checked={pickedSet.has(note.id) ? 1 : 0}>
                  <input type="checkbox" checked={pickedSet.has(note.id)} onChange={() => toggle(note.id)} />
                  <span className="c4-pick-box" aria-hidden="true">{pickedSet.has(note.id) && <Icon name="check" size={12} />}</span>
                  <span className="c4-note-text">
                    <span className="c4-note-title">{note.title}</span>
                    <span className="c4-note-meta">{note.meta}</span>
                  </span>
                </label>
              </li>
            ))}
          </ul>
        )}
        {rows.length > shown.length && <p className="c4-hint">{rows.length - shown.length}개가 더 있어요. 검색으로 좁혀 보세요.</p>}
        {error && <p role="alert" className="c4-error">{error}</p>}
      </div>
      <footer>
        <button type="button" onClick={close}>취소</button>
        <button type="button" className="button-primary" disabled={busy || !changed} onClick={() => void save()}>
          저장{picked.length ? ` · ${picked.length}개` : ''}
        </button>
      </footer>
    </Modal>
  );
}
