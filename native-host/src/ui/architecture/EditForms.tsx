// 요소와 관계를 만들고 고치는 창. 값을 모아 변경 목록으로 바꿔 부모에게 넘기고, 저장과 검사는 엔진이 한다.
import { useState } from 'react';
import { Modal } from '../Modal.tsx';
import { KIND_LABELS, createElementOps, createRelationOps, editElementOps, editRelationOps, elementFormOf, emptyElementForm, kindsForLevel, relationFormOf } from './model.ts';
import type { C4Element, C4Model, C4Operation, C4Relationship, ElementForm, RelationForm } from './model.ts';

type Submit = (operations: C4Operation[]) => Promise<string | null>;

function useSubmit(submit: Submit, close: () => void) {
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);
  async function run(operations: C4Operation[]) {
    if (!operations.length) return close();
    setBusy(true); setError('');
    const failure = await submit(operations);
    if (failure === null) close();
    else { setError(failure); setBusy(false); }
  }
  return { error, busy, run };
}

export function ElementFormModal({ model, level, parentId, existing, submit, close }: {
  model: C4Model;
  level: number;
  parentId: string | null;
  existing?: C4Element;
  submit: Submit;
  close(): void;
}) {
  const kinds = existing ? kindsForLevel(existing.level) : kindsForLevel(level);
  const [form, setForm] = useState<ElementForm>(() => existing ? elementFormOf(existing) : emptyElementForm(kinds[0]));
  const { error, busy, run } = useSubmit(submit, close);
  const set = <K extends keyof ElementForm>(key: K, value: ElementForm[K]) => setForm(current => ({ ...current, [key]: value }));
  const onSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    void run(existing ? editElementOps(existing, form) : createElementOps(model, form, parentId));
  };
  return (
    <Modal title={existing ? '요소 편집' : '요소 추가'} close={close} className="ndf c4-form" resizable={false}>
      <form onSubmit={onSubmit}>
        {kinds.length > 1 && (
          <label>종류
            <select value={form.kind} onChange={event => set('kind', event.target.value)}>
              {kinds.map(kind => <option key={kind} value={kind}>{KIND_LABELS[kind]}</option>)}
            </select>
          </label>
        )}
        <label>이름
          <input required autoFocus maxLength={120} value={form.name} onChange={event => set('name', event.target.value)} />
        </label>
        <label>설명
          <textarea rows={3} maxLength={2000} value={form.description} onChange={event => set('description', event.target.value)} />
        </label>
        <label>기술
          <input maxLength={120} value={form.technology} placeholder="예: Rust, React" onChange={event => set('technology', event.target.value)} />
        </label>
        <label>코드 위치
          <input maxLength={300} value={form.codeRef} placeholder="예: engine-rs/crates/ais-core" onChange={event => set('codeRef', event.target.value)} />
          <small className="c4-hint">이 요소의 코드가 있는 파일이나 폴더 경로예요(선택).</small>
        </label>
        {error && <p role="alert" className="c4-error">{error}</p>}
        <footer>
          <button type="button" onClick={close}>취소</button>
          <button type="submit" className="button-primary" disabled={busy || !form.name.trim()}>{existing ? '저장' : '추가'}</button>
        </footer>
      </form>
    </Modal>
  );
}

export function RelationFormModal({ model, existing, source, target, submit, close }: {
  model: C4Model;
  existing?: C4Relationship;
  source?: string;
  target?: string;
  submit: Submit;
  close(): void;
}) {
  const first = model.elements[0]?.id ?? '';
  const [form, setForm] = useState<RelationForm>(() => existing ? relationFormOf(existing) : {
    source: source ?? first, target: target ?? model.elements.find(item => item.id !== (source ?? first))?.id ?? first, label: '', technology: '',
  });
  const { error, busy, run } = useSubmit(submit, close);
  const set = <K extends keyof RelationForm>(key: K, value: RelationForm[K]) => setForm(current => ({ ...current, [key]: value }));
  const onSubmit = (event: React.FormEvent) => {
    event.preventDefault();
    void run(existing ? editRelationOps(existing, form) : createRelationOps(form));
  };
  const options = model.elements.map(item => <option key={item.id} value={item.id}>{item.name} · {KIND_LABELS[item.kind] ?? item.kind}</option>);
  return (
    <Modal title={existing ? '관계 편집' : '관계 추가'} close={close} className="ndf c4-form" resizable={false}>
      <form onSubmit={onSubmit}>
        <label>출발
          <select value={form.source} onChange={event => set('source', event.target.value)}>{options}</select>
        </label>
        <label>도착
          <select value={form.target} onChange={event => set('target', event.target.value)}>{options}</select>
        </label>
        <label>설명
          <input autoFocus maxLength={200} value={form.label} placeholder="예: 작업 지시 및 승인" onChange={event => set('label', event.target.value)} />
        </label>
        <label>기술
          <input maxLength={120} value={form.technology} placeholder="예: HTTPS/JSON" onChange={event => set('technology', event.target.value)} />
        </label>
        {error && <p role="alert" className="c4-error">{error}</p>}
        <footer>
          <button type="button" onClick={close}>취소</button>
          <button type="submit" className="button-primary" disabled={busy || !form.source || !form.target}>{existing ? '저장' : '추가'}</button>
        </footer>
      </form>
    </Modal>
  );
}
