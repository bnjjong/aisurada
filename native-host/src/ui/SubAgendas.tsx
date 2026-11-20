import { useState } from "react";
import type { Agenda, Note, SubAgenda } from "../workspace/types.ts";
import type { WorkspaceController } from "../workspace/controller.ts";
import { Modal } from "./Modal.tsx";

export function SubAgendaChoices({
  items,
  selected,
  onChange,
  searchable = false,
}: {
  items: SubAgenda[];
  selected: string[];
  onChange(ids: string[]): void;
  searchable?: boolean;
}) {
  const [query, setQuery] = useState("");
  const visible = items.filter((item) =>
    item.title.toLocaleLowerCase().includes(query.trim().toLocaleLowerCase())
  );
  return (
    <fieldset className="sub-agenda-choices">
      <legend>서브 아젠다</legend>
      {searchable && (
        <input
          aria-label="서브 아젠다 찾기"
          placeholder="서브 아젠다 찾기"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
        />
      )}
      {visible.map((item) => (
        <label key={item.id}>
          <input
            type="checkbox"
            checked={selected.includes(item.id)}
            onChange={(event) =>
              onChange(event.target.checked
                ? [...selected, item.id]
                : selected.filter((id) => id !== item.id))
            }
          />
          {item.title}
        </label>
      ))}
      {!visible.length && <small>해당하는 서브 아젠다가 없어요.</small>}
    </fieldset>
  );
}

export function NoteSubAgendas({
  note,
  agenda,
  controller,
  readOnly,
}: {
  note: Note;
  agenda: Agenda;
  controller: WorkspaceController;
  readOnly: boolean;
}) {
  const items = agenda.subAgendas ?? [];
  const selected = items.filter((item) => note.subAgendaIds?.includes(item.id));
  const ids = selected.map((item) => item.id);
  const active = items.filter((item) => !item.archived);
  const write = (subAgendaIds: string[]) =>
    void controller.data.updateNote({ id: note.id }, { subAgendaIds });
  if (!selected.length && (readOnly || !active.length)) return null;
  return (
    <div className="sub-agenda-chips" aria-label="노트의 서브 아젠다">
      {selected.map((item) => (
        <span className="tag-chip" key={item.id}>
          {item.title}{item.archived && " · 보관됨"}
          {!readOnly && (
            <button type="button" aria-label={`${item.title} 분류 떼기`}
              onClick={() => write(ids.filter((id) => id !== item.id))}>×</button>
          )}
        </span>
      ))}
      {!readOnly && active.length > 0 && (
        <details className="sub-agenda-picker">
          <summary>서브 아젠다 지정</summary>
          <SubAgendaChoices items={active} selected={ids} onChange={write} searchable />
        </details>
      )}
    </div>
  );
}

export function ManageSubAgendas({
  agenda,
  controller,
  close,
}: {
  agenda: Agenda;
  controller: WorkspaceController;
  close(): void;
}) {
  const [editedId, setEditedId] = useState<string>();
  const [title, setTitle] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const items = agenda.subAgendas ?? [];
  const save = async (fields: { title?: string; archived?: boolean }, id?: string) => {
    setBusy(true);
    setError("");
    const result = await controller.data.saveSubAgenda(agenda.id, fields, id);
    setBusy(false);
    if (!result.ok) setError("서브 아젠다를 저장하지 못했어요. 겹치는 이름이 있는지 확인해 주세요.");
    return result.ok;
  };
  return (
    <Modal title={`${agenda.title} · 서브 아젠다`} close={close} className="manage-sub-agendas">
      <form onSubmit={async (event) => {
        event.preventDefault();
        if (!title.trim() || busy) return;
        if (await save({ title: title.trim() }, editedId)) {
          setTitle("");
          setEditedId(undefined);
        }
      }}>
        <div className="sub-agenda-management">
          {items.map((item) => (
            <div className="sub-agenda-line" key={item.id}>
              <span>{item.title}{item.archived && " · 보관됨"}</span>
              <button type="button" disabled={busy} onClick={() => {
                setEditedId(item.id);
                setTitle(item.title);
                setError("");
              }}>이름 바꾸기</button>
              <button type="button" disabled={busy} onClick={() => void save({ archived: !item.archived }, item.id)}>
                {item.archived ? "보관 해제" : "보관"}
              </button>
            </div>
          ))}
        </div>
        <label className="sub-agenda-line">
          <span>{editedId ? "새 이름" : "새 서브 아젠다"}</span>
          <input value={title} onChange={(event) => setTitle(event.target.value)} />
        </label>
        {error && <p role="alert">{error}</p>}
        <footer>
          {editedId && <button type="button" onClick={() => { setEditedId(undefined); setTitle(""); }}>취소</button>}
          <button type="submit" disabled={busy || !title.trim()} className="button-primary">
            {editedId ? "이름 저장" : "만들기"}
          </button>
        </footer>
      </form>
    </Modal>
  );
}
