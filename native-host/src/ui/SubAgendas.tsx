import { useCallback, useId, useRef, useState } from "react";
import type { Agenda, Note, SubAgenda } from "../workspace/types.ts";
import type { WorkspaceController } from "../workspace/controller.ts";
import { Icon } from "./Icon.tsx";
import { Modal } from "./Modal.tsx";
import { PickerPopover, PickerSearch } from "./PickerPopover.tsx";

interface SubAgendaSelection {
  items: SubAgenda[];
  selected: string[];
  onChange(ids: string[]): void;
}

function toggleSelection(selected: string[], id: string): string[] {
  return selected.includes(id) ? selected.filter((value) => value !== id) : [...selected, id];
}

export function SubAgendaChoices({ items, selected, onChange }: SubAgendaSelection) {
  return (
    <fieldset className="sub-agenda-choices">
      <legend>서브 아젠다</legend>
      {items.map((item) => (
        <label key={item.id}>
          <input
            type="checkbox"
            checked={selected.includes(item.id)}
            onChange={() => onChange(toggleSelection(selected, item.id))}
          />
          {item.title}
        </label>
      ))}
    </fieldset>
  );
}

export function SubAgendaOptions({ items, selected, onChange }: SubAgendaSelection) {
  return (
    <div className="agenda-picker-list" role="menu" aria-label="서브 아젠다 목록">
      {items.map((item) => {
        const checked = selected.includes(item.id);
        return (
          <button key={item.id} type="button" className="act-item agenda-picker-item"
            role="menuitemcheckbox" aria-checked={checked}
            onClick={() => onChange(toggleSelection(selected, item.id))}>
            <span className="agenda-picker-name" title={item.title}>{item.title}</span>
            {(item.archived || checked) && (
              <small>
                {item.archived && "보관됨"}
                {checked && <Icon name="check" size={12} />}
              </small>
            )}
          </button>
        );
      })}
      {!items.length && <p className="agenda-picker-empty">맞는 서브 아젠다가 없어요.</p>}
    </div>
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
  const anchor = useRef<HTMLButtonElement>(null);
  const menuId = useId();
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const close = useCallback(() => setOpen(false), []);
  const items = agenda.subAgendas ?? [];
  const selected = items.filter((item) => note.subAgendaIds?.includes(item.id));
  const ids = selected.map((item) => item.id);
  const choices = items.filter((item) => !item.archived || ids.includes(item.id));
  const searchable = choices.length > 5 || query.length > 0;
  const search = query.trim().toLocaleLowerCase();
  const shown = choices.filter((item) => item.title.toLocaleLowerCase().includes(search));
  const save = (subAgendaIds: string[]) =>
    void controller.data.updateNote({ id: note.id }, { subAgendaIds });

  if (readOnly ? !selected.length : !choices.length) return null;
  const labels = selected.map((item) => item.title + (item.archived ? " · 보관됨" : ""));
  const summary = (
    <>
      <span className="doc-agenda-prefix">서브 아젠다</span>
      {labels[0] && <span className="doc-agenda-title">{labels[0]}</span>}
      {labels.length > 1 && <span className="sub-agenda-count">+{labels.length - 1}</span>}
    </>
  );
  if (readOnly) {
    return <span className="doc-agenda-chip sub-agenda-chip" title={labels.join(", ")}>{summary}</span>;
  }

  return (
    <>
      <button
        type="button"
        className="doc-agenda-chip sub-agenda-chip is-clickable"
        ref={anchor}
        aria-label={["서브 아젠다 지정", ...labels].join(" · ")}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        title={labels.join(", ") || "서브 아젠다 지정"}
        onClick={() => {
          if (open) close();
          else {
            setQuery("");
            setOpen(true);
          }
        }}
      >
        {summary}
        <span className="doc-agenda-caret" aria-hidden="true"><Icon name="chevron" size={10} /></span>
      </button>
      {open && (
        <PickerPopover id={menuId} anchor={anchor} label="서브 아젠다 지정" onClose={close}>
          {searchable && (
            <PickerSearch label="서브 아젠다 찾기" value={query} onChange={setQuery}
              onSubmit={() => { if (shown[0]) save(toggleSelection(ids, shown[0].id)); }} />
          )}
          <SubAgendaOptions items={shown} selected={ids} onChange={save} />
        </PickerPopover>
      )}
    </>
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
