// 선택한 요소나 관계의 자세한 내용과 할 수 있는 일(편집, 삭제, 관계 추가, 안쪽 보기)
import { Icon } from '../Icon.tsx';
import { KIND_LABELS, relationTitle } from './model.ts';
import type { C4Element, C4Model, C4Relationship } from './model.ts';
import { iconFor } from './Diagram.tsx';

export function Details({
  model,
  element,
  relation,
  edit,
  remove,
  connect,
  open,
  pickRelation,
  onClose,
}: {
  model: C4Model;
  element?: C4Element;
  relation?: C4Relationship;
  edit(): void;
  remove(): void;
  connect(): void;
  open?(): void;
  pickRelation(id: string): void;
  onClose?(): void;
}) {
  if (!element && !relation) {
    return null;
  }

  if (relation) {
    const source = model.elements.find(e => e.id === relation.source)?.name ?? relation.source;
    const target = model.elements.find(e => e.id === relation.target)?.name ?? relation.target;
    return (
      <div className="c4-details-panel">
        <div className="c4-details-head">
          <div className="c4-details-title-group">
            <span className="c4-details-icon" data-kind="relation">
              <Icon name="link" size={16} />
            </span>
            <div className="c4-details-title-text">
              <strong>{relationTitle(model, relation)}</strong>
              <span className="c4-details-kind-badge">관계</span>
            </div>
          </div>
          {onClose && (
            <button type="button" className="c4-details-close-btn" onClick={onClose} aria-label="닫기">
              <Icon name="close" size={14} />
            </button>
          )}
        </div>
        <div className="c4-details-body">
          <div className="c4-details-section">
            <span className="c4-details-section-label">연결 경로</span>
            <p className="c4-rel-path">
              <span>{source}</span>
              <span className="c4-rel-arrow">→</span>
              <span>{target}</span>
            </p>
          </div>
          {relation.label && (
            <div className="c4-details-section">
              <span className="c4-details-section-label">설명</span>
              <p className="c4-details-desc">{relation.label}</p>
            </div>
          )}
          {relation.technology && (
            <div className="c4-details-section">
              <span className="c4-details-section-label">기술 스택</span>
              <p className="c4-tech">{relation.technology}</p>
            </div>
          )}
        </div>
        <div className="c4-details-actions">
          <button type="button" className="c4-action-btn primary" onClick={edit}>
            <Icon name="edit" size={13} /> 편집
          </button>
          <button type="button" className="c4-action-btn danger" onClick={remove}>
            <Icon name="trash" size={13} /> 삭제
          </button>
        </div>
      </div>
    );
  }

  const item = element!;
  const touching = model.relationships.filter(row => row.source === item.id || row.target === item.id);
  const name = (id: string) => model.elements.find(other => other.id === id)?.name ?? id;

  return (
    <div className="c4-details-panel">
      <div className="c4-details-head">
        <div className="c4-details-title-group">
          <span className="c4-details-icon" data-kind={item.kind}>
            <Icon name={iconFor(item)} size={18} />
          </span>
          <div className="c4-details-title-text">
            <strong>{item.name}</strong>
            <span className="c4-details-kind-badge">{KIND_LABELS[item.kind] ?? item.kind}</span>
          </div>
        </div>
        {onClose && (
          <button type="button" className="c4-details-close-btn" onClick={onClose} aria-label="닫기">
            <Icon name="close" size={14} />
          </button>
        )}
      </div>
      <div className="c4-details-body">
        <div className="c4-details-section">
          <span className="c4-details-section-label">설명</span>
          <p className="c4-details-desc">
            {item.description ? item.description : <span className="c4-empty-hint">설명이 없습니다.</span>}
          </p>
        </div>
        {item.technology && (
          <div className="c4-details-section">
            <span className="c4-details-section-label">기술 스택</span>
            <p className="c4-tech">{item.technology}</p>
          </div>
        )}
        {item.codeRef && (
          <div className="c4-details-section">
            <span className="c4-details-section-label">코드 참조</span>
            <p className="c4-code-ref"><code>{item.codeRef}</code></p>
          </div>
        )}
        {touching.length > 0 && (
          <div className="c4-details-section">
            <span className="c4-details-section-label">연결된 관계 ({touching.length})</span>
            <ul className="c4-relations" aria-label="이 요소의 관계">
              {touching.map(row => (
                <li key={row.id}>
                  <button type="button" onClick={() => pickRelation(row.id)}>
                    <span className="c4-rel-arrow">{row.source === item.id ? '→' : '←'}</span>
                    <span className="c4-rel-target">{name(row.source === item.id ? row.target : row.source)}</span>
                    {row.label && <span className="c4-rel-label"> · {row.label}</span>}
                  </button>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
      <div className="c4-details-actions">
        <button type="button" className="c4-action-btn primary" onClick={edit}>
          <Icon name="edit" size={13} /> 편집
        </button>
        <button type="button" className="c4-action-btn" onClick={connect}>
          <Icon name="link" size={13} /> 관계 추가
        </button>
        {open && (
          <button type="button" className="c4-action-btn" onClick={open}>
            <Icon name="popout" size={13} /> 안쪽 보기
          </button>
        )}
        <button type="button" className="c4-action-btn danger" onClick={remove}>
          <Icon name="trash" size={13} /> 삭제
        </button>
      </div>
    </div>
  );
}
