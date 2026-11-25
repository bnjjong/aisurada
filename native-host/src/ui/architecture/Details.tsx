// 선택한 요소나 관계의 자세한 내용과 할 수 있는 일(편집, 삭제, 관계 추가, 안쪽 보기)
import { Icon } from '../Icon.tsx';
import { KIND_LABELS, relationTitle } from './model.ts';
import type { C4Element, C4Model, C4Relationship } from './model.ts';

export function Details({ model, element, relation, edit, remove, connect, open, pickRelation }: {
  model: C4Model;
  element?: C4Element;
  relation?: C4Relationship;
  edit(): void;
  remove(): void;
  connect(): void;
  open?(): void;
  pickRelation(id: string): void;
}) {
  if (!element && !relation) {
    return <p className="c4-details-hint">요소를 누르면 자세한 내용이 보여요. 끌어서 옮기고, 더블클릭하면 안쪽 단계로 들어가요.</p>;
  }
  if (relation) {
    return (
      <div className="c4-details-body">
        <div className="c4-details-head">
          <div><strong>{relationTitle(model, relation)}</strong><span>관계</span></div>
          <div className="c4-details-actions">
            <button type="button" onClick={edit}>편집</button>
            <button type="button" onClick={remove}>삭제</button>
          </div>
        </div>
        {relation.label && <p>{relation.label}</p>}
        {relation.technology && <p className="c4-tech">{relation.technology}</p>}
      </div>
    );
  }
  const item = element!;
  const touching = model.relationships.filter(row => row.source === item.id || row.target === item.id);
  const name = (id: string) => model.elements.find(other => other.id === id)?.name ?? id;
  return (
    <div className="c4-details-body">
      <div className="c4-details-head">
        <div><strong>{item.name}</strong><span>{KIND_LABELS[item.kind] ?? item.kind}</span></div>
        <div className="c4-details-actions">
          <button type="button" onClick={edit}>편집</button>
          <button type="button" onClick={connect}><Icon name="link" size={13} /> 관계 추가</button>
          {open && <button type="button" onClick={open}>안쪽 보기</button>}
          <button type="button" onClick={remove}>삭제</button>
        </div>
      </div>
      {item.description && <p>{item.description}</p>}
      {item.technology && <p className="c4-tech">{item.technology}</p>}
      {item.codeRef && <p className="c4-code-ref"><code>{item.codeRef}</code></p>}
      {touching.length > 0 && (
        <ul className="c4-relations" aria-label="이 요소의 관계">
          {touching.map(row => (
            <li key={row.id}>
              <button type="button" onClick={() => pickRelation(row.id)}>
                {row.source === item.id ? `→ ${name(row.target)}` : `← ${name(row.source)}`}
                {row.label && <span> · {row.label}</span>}
              </button>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}
