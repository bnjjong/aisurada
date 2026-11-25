import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { WorkspaceController } from '../../workspace/controller.ts';
import type { WorkspaceState } from '../../workspace/types.ts';
import { asText } from '../../workspace/types.ts';
import { Icon } from '../Icon.tsx';
import { Confirm } from '../Modal.tsx';
import { Page } from '../OtherPages.tsx';
import { isRecord } from '../engineClient.ts';
import { Details } from './Details.tsx';
import { Diagram } from './Diagram.tsx';
import { DiagramNameModal } from './DiagramForms.tsx';
import { ElementFormModal, RelationFormModal } from './EditForms.tsx';
import { LEVEL_LABELS, projectSlice } from './layout.ts';
import { applyC4Changes, createC4Diagram, deleteC4Diagram, errorText, getC4Model, isPreconditionError, listC4Diagrams, renameC4Diagram } from './client.ts';
import { KIND_LABELS, MAIN_DIAGRAM, deletionImpact, parentKindFor, relationTitle, viewIdFor } from './model.ts';
import type { C4Diagram, C4Element, C4Model, C4Operation, C4Relationship, Point } from './model.ts';
import './architecture.css';

const NO_POSITIONS: Record<string, Point> = {};

type Dialog =
  | { type: 'element'; existing?: C4Element }
  | { type: 'relation'; existing?: C4Relationship; source?: string; target?: string; sourcePort?: string; targetPort?: string }
  | { type: 'delete'; target: { kind: 'element' | 'relationship'; id: string } }
  | { type: 'diagram-create' }
  | { type: 'diagram-rename' }
  | { type: 'diagram-delete' };

/** 아키텍처를 지울 때 보여 주는 글. 몇 개가 사라지는지와 되돌리는 길을 알린다. */
function diagramDeleteText(item: C4Diagram, model: C4Model | null): string {
  const elements = model?.diagram.id === item.id ? model.elements.length : item.elements ?? 0;
  const relations = model?.diagram.id === item.id ? model.relationships.length : item.relationships ?? 0;
  const gone = elements || relations ? ` 요소 ${elements}개와 관계 ${relations}개가 함께 사라져요.` : '';
  const tail = item.id === MAIN_DIAGRAM ? ' 기본 아키텍처는 비워지고 이름은 ‘기본’으로 돌아가요.' : '';
  return `‘${item.name}’ 아키텍처를 지워요.${gone}${tail} 이력에서 되돌릴 수 있어요.`;
}

export function ArchitecturePage({ slug, state, controller }: {
  slug: string;
  state: WorkspaceState;
  controller: WorkspaceController;
}) {
  const workspace = state.workspace!;
  const project = workspace.projects.find(item => item.slug === slug || item.id === slug);
  if (!project) return <Page title="이 프로젝트를 찾을 수 없어요" />;
  const projectSlug = asText(project.slug) || slug;
  return <ProjectArchitecture key={projectSlug} project={projectSlug} name={asText(project.name) || slug} controller={controller} />;
}

function ProjectArchitecture({ project, name, controller }: {
  project: string;
  name: string;
  controller: WorkspaceController;
}) {
  const [diagrams, setDiagrams] = useState<C4Diagram[]>([]);
  const [diagramId, setDiagramId] = useState(MAIN_DIAGRAM);
  const [model, setModel] = useState<C4Model | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [notice, setNotice] = useState('');
  const [level, setLevel] = useState(1);
  const [parentId, setParentId] = useState<string | null>(null);
  const [selectedElement, setSelectedElement] = useState<string | null>(null);
  const [selectedRelationship, setSelectedRelationship] = useState<string | null>(null);
  const [dialog, setDialog] = useState<Dialog | null>(null);
  const [connect, setConnect] = useState<{ source: string | null } | null>(null);
  const [resetKey, setResetKey] = useState(0);
  const modelRef = useRef<C4Model | null>(null);
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const dialogOpen = useRef(false);
  const generation = useRef(0);
  const call = controller.callEngine;
  dialogOpen.current = dialog !== null;

  const accept = useCallback((next: C4Model) => { modelRef.current = next; setModel(next); }, []);

  /** 아키텍처 목록을 다시 읽는다. 실패하면 있던 목록을 그대로 둔다. */
  const refreshDiagrams = useCallback(async () => {
    try { setDiagrams(await listC4Diagrams(call, project)); } catch { /* 목록은 보조라서 모델 읽기를 막지 않는다 */ }
  }, [call, project]);

  /** 다른 아키텍처를 연다. 이전 것의 선택과 단계는 버리고, 모델은 새로 읽는다. */
  const showDiagram = useCallback((id: string) => {
    modelRef.current = null; setModel(null);
    setDiagramId(id);
    setLevel(1); setParentId(null);
    setSelectedElement(null); setSelectedRelationship(null); setConnect(null); setNotice('');
  }, []);

  const load = useCallback(async (quiet: boolean) => {
    const sequence = ++generation.current;
    if (!quiet) { setLoading(true); setError(''); }
    try {
      const next = await getC4Model(call, project, diagramId);
      if (generation.current !== sequence) return;
      if (!quiet || next.precondition !== modelRef.current?.precondition) accept(next);
    } catch (failure) {
      if (generation.current !== sequence) return;
      // 보던 아키텍처가 다른 곳에서 지워졌으면 기본 아키텍처로 돌아간다.
      if (diagramId !== MAIN_DIAGRAM && isRecord(failure) && failure.kind === 'not_found') {
        void refreshDiagrams().then(() => showDiagram(MAIN_DIAGRAM));
        setNotice('보던 아키텍처가 다른 곳에서 지워졌어요. 기본 아키텍처로 돌아왔어요.');
        return;
      }
      if (!quiet) setError(errorText(failure));
    } finally {
      if (generation.current === sequence && !quiet) setLoading(false);
    }
  }, [accept, call, diagramId, project, refreshDiagrams, showDiagram]);

  useEffect(() => { void load(false); return () => { ++generation.current; }; }, [load]);

  // 창이 다시 눈에 띄면 AI나 다른 창이 고친 것을 따라간다. 편집 창이 열려 있으면 건드리지 않는다.
  useEffect(() => { void refreshDiagrams(); }, [refreshDiagrams]);

  useEffect(() => {
    const refresh = () => { if (!dialogOpen.current) { void load(true); void refreshDiagrams(); } };
    window.addEventListener('focus', refresh);
    return () => window.removeEventListener('focus', refresh);
  }, [load, refreshDiagrams]);

  /** 변경 묶음을 차례대로 한 번씩 적용한다. 성공하면 `null`, 실패하면 사람이 읽는 이유를 돌려준다. */
  const mutate = useCallback((operations: C4Operation[]): Promise<string | null> => {
    const job = queue.current.then(async () => {
      const current = modelRef.current;
      if (!current) return '모델을 아직 불러오지 못했어요.';
      try {
        accept(await applyC4Changes(call, current, operations));
        return null;
      } catch (failure) {
        if (isPreconditionError(failure)) void load(true);
        return errorText(failure);
      } finally {
        setResetKey(key => key + 1);
      }
    });
    queue.current = job.catch(() => null);
    return job;
  }, [accept, call, load]);

  /** 아키텍처 관리 명령 하나를 쓰기 줄에 세워 실행한다(진행 중인 위치 저장과 섞이지 않게). 실패하면 사람이 읽는 이유를 돌려준다. */
  const manage = useCallback((run: () => Promise<void>): Promise<string | null> => {
    const job = queue.current.then(async () => {
      try { await run(); return null; } catch (failure) { return errorText(failure); }
    });
    queue.current = job.catch(() => null);
    return job;
  }, []);

  const createDiagram = (name: string) => manage(async () => {
    const made = await createC4Diagram(call, project, name);
    await refreshDiagrams();
    showDiagram(made.id);
  });
  const renameDiagram = (name: string) => manage(async () => {
    await renameC4Diagram(call, project, diagramId, name);
    await refreshDiagrams();
  });
  const removeDiagram = () => manage(async () => {
    await deleteC4Diagram(call, project, diagramId);
    await refreshDiagrams();
    showDiagram(MAIN_DIAGRAM);
    if (diagramId === MAIN_DIAGRAM) void load(false);
  }).then(failure => setNotice(failure ?? ''));

  const shown = diagrams.find(item => item.id === diagramId) ?? model?.diagram ?? { id: diagramId, name: diagramId === MAIN_DIAGRAM ? '기본' : diagramId };

  const parentKind = parentKindFor(level);
  const parents = useMemo(() => model?.elements.filter(item => item.kind === parentKind) ?? [], [model, parentKind]);
  const parent = parents.find(item => item.id === parentId) ?? parents[0];
  const parentKey = parent?.id ?? null;
  const visible = useMemo(() => model ? projectSlice(model, level, parentKey) : [], [level, model, parentKey]);
  const viewId = viewIdFor(level, parentKey);
  const positions = (viewId && model?.views[viewId]) || NO_POSITIONS;
  const element = model?.elements.find(item => item.id === selectedElement);
  const relation = model?.relationships.find(item => item.id === selectedRelationship);
  const canAdd = Boolean(model) && !loading && (level === 1 || Boolean(parent));
  const emptyText = !model ? undefined
    : level === 1 ? '아직 요소가 없어요. ‘요소 추가’로 시스템을 먼저 만들어 보세요.'
      : !parent ? `${LEVEL_LABELS[level - 1]} 단계는 ${KIND_LABELS[parentKind!]} 안에 만들어요. 먼저 ${KIND_LABELS[parentKind!]}을(를) 추가해 주세요.`
        : `이 ${KIND_LABELS[parent.kind]}에는 아직 ${LEVEL_LABELS[level - 1]}가 없어요. ‘요소 추가’로 만들어 보세요.`;

  const showLevel = (next: number) => {
    setLevel(next);
    if (element && parentKindFor(next) === element.kind) setParentId(element.id);
    setSelectedElement(null); setSelectedRelationship(null); setConnect(null);
  };
  const drill = (id: string) => {
    const item = model?.elements.find(other => other.id === id);
    if (!item || (item.kind !== 'system' && item.kind !== 'container')) return;
    setLevel(item.level + 1); setParentId(item.id);
    setSelectedElement(null); setSelectedRelationship(null); setConnect(null);
  };
  const pickElement = (id: string | null) => {
    setNotice('');
    if (id && connect) {
      if (!connect.source) { setConnect({ source: id }); setSelectedElement(id); setSelectedRelationship(null); return; }
      setDialog({ type: 'relation', source: connect.source, target: id });
      setConnect(null);
      return;
    }
    setSelectedElement(id); setSelectedRelationship(null);
  };
  const pickRelationship = (id: string) => { setNotice(''); setSelectedRelationship(id); setSelectedElement(null); setConnect(null); };
  const moveNodes = viewId ? (moved: Record<string, Point>) => {
    void mutate([{ op: 'set_positions', view: viewId, positions: moved }]).then(failure => setNotice(failure ?? ''));
  } : undefined;

  const confirmDelete = (target: { kind: 'element' | 'relationship'; id: string }) => {
    const operation: C4Operation = target.kind === 'element'
      ? { op: 'delete_element', id: target.id } : { op: 'delete_relationship', id: target.id };
    return mutate([operation]).then(failure => setNotice(failure ?? ''));
  };

  const deleting = dialog?.type === 'delete' && model ? dialog.target : null;
  const deleteText = (() => {
    if (!deleting || !model) return '';
    if (deleting.kind === 'relationship') {
      const row = model.relationships.find(item => item.id === deleting.id);
      return row ? `‘${relationTitle(model, row)}’ 관계를 지워요.` : '이 관계를 지워요.';
    }
    const item = model.elements.find(other => other.id === deleting.id);
    const impact = deletionImpact(model, deleting.id);
    const also = [impact.elements ? `안쪽 요소 ${impact.elements}개` : '', impact.relationships ? `관계 ${impact.relationships}개` : ''].filter(Boolean);
    return `‘${item?.name ?? deleting.id}’을(를) 지워요.${also.length ? ` ${also.join('와 ')}도 함께 사라져요.` : ''} 이력에서 되돌릴 수 있어요.`;
  })();

  return (
    <section className="c4-page" aria-label="C4 아키텍처">
      <header className="c4-page-header">
        <div><h1>C4 아키텍처</h1><p>{name}</p></div>
      </header>
      <div className="c4-workspace">
        <div className="c4-diagram-bar">
          <nav aria-label="아키텍처 목록" className="c4-diagram-tabs">
            {(diagrams.length ? diagrams : [shown]).map(item => (
              <button key={item.id} type="button" aria-pressed={item.id === diagramId} data-diagram={item.id} title={item.name} onClick={() => showDiagram(item.id)}>{item.name}</button>
            ))}
          </nav>
          <div className="c4-diagram-actions">
            <button type="button" onClick={() => setDialog({ type: 'diagram-create' })} disabled={loading && !model}><Icon name="plus" size={14} /> 새 아키텍처</button>
            <button type="button" onClick={() => setDialog({ type: 'diagram-rename' })} disabled={!model} aria-label="아키텍처 이름 바꾸기" title="이름 바꾸기"><Icon name="edit" size={14} /></button>
            <button type="button" onClick={() => setDialog({ type: 'diagram-delete' })} disabled={!model} aria-label="아키텍처 지우기" title="지우기"><Icon name="trash" size={14} /></button>
          </div>
        </div>
        <div className="c4-toolbar">
          <nav aria-label="C4 레벨">
            {LEVEL_LABELS.map((label, index) => (
              <button key={label} type="button" aria-pressed={level === index + 1} onClick={() => showLevel(index + 1)}>{label}</button>
            ))}
          </nav>
          <div className="c4-toolbar-actions">
            <button type="button" onClick={() => setDialog({ type: 'element' })} disabled={!canAdd}><Icon name="plus" size={14} /> 요소 추가</button>
            <button
              type="button"
              aria-pressed={connect !== null}
              disabled={!model || model.elements.length < 2}
              onClick={() => { setNotice(''); setConnect(current => current ? null : { source: element?.id ?? null }); }}
            ><Icon name="link" size={14} /> 관계 추가</button>
            <button type="button" onClick={() => void load(false)} disabled={loading} aria-label="다시 불러오기"><Icon name="history" size={15} /></button>
          </div>
        </div>
        {connect && (
          <p className="c4-connect-hint" role="status">
            {connect.source
              ? `‘${model?.elements.find(item => item.id === connect.source)?.name ?? ''}’에서 이을 요소를 눌러 주세요.`
              : '관계를 시작할 요소를 눌러 주세요.'}
            <button type="button" onClick={() => setConnect(null)}>취소</button>
          </p>
        )}
        {error && <p className="c4-error" role="alert">{error}</p>}
        {notice && <p className="c4-error" role="alert">{notice}</p>}
        <div className="c4-workbench">
          <div className="c4-map-column">
            {level > 1 && parents.length > 0 && (
              <label className="c4-parent">{KIND_LABELS[parentKind!]}
                <select value={parentKey ?? ''} onChange={event => { setParentId(event.target.value); setSelectedElement(null); setSelectedRelationship(null); setConnect(null); }}>
                  {parents.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
                </select>
              </label>
            )}
            <div className="c4-diagram-wrap">
              {model ? (
                <Diagram
                  key={diagramId + ':' + level + ':' + (parentKey ?? '')}
                  elements={visible}
                  relationships={model.relationships}
                  positions={positions}
                  selectedElement={selectedElement}
                  selectedRelationship={selectedRelationship}
                  selectElement={pickElement}
                  selectRelationship={pickRelationship}
                  drill={drill}
                  onMove={moveNodes}
                  onConnect={canAdd ? (source, target, sourcePort, targetPort) => { setNotice(''); setConnect(null); setDialog({ type: 'relation', source, target, sourcePort, targetPort }); } : undefined}
                  onReconnect={(id, change) => {
                    setNotice('');
                    void mutate([{ op: 'put_relationship', id, ...change }]).then(failure => setNotice(failure ?? ''));
                  }}
                  resetKey={resetKey}
                  connecting={connect !== null}
                  emptyText={emptyText}
                />
              ) : <div className="c4-empty"><p role={loading ? 'status' : undefined}>{loading ? '다이어그램을 불러오고 있어요.' : '다시 불러오기를 눌러 주세요.'}</p></div>}
              {model && (element || relation) && (
                <aside className="c4-details" aria-label="선택한 항목" aria-live="polite">
                  <Details
                    model={model}
                    element={element}
                    relation={relation}
                    edit={() => setDialog(element ? { type: 'element', existing: element } : { type: 'relation', existing: relation })}
                    remove={() => setDialog({ type: 'delete', target: element ? { kind: 'element', id: element.id } : { kind: 'relationship', id: relation!.id } })}
                    connect={() => setDialog({ type: 'relation', source: element?.id })}
                    open={element && (element.kind === 'system' || element.kind === 'container') ? () => drill(element.id) : undefined}
                    pickRelation={pickRelationship}
                    onClose={() => { setSelectedElement(null); setSelectedRelationship(null); }}
                  />
                </aside>
              )}
            </div>
          </div>
        </div>
      </div>
      {model && dialog?.type === 'element' && (
        <ElementFormModal model={model} level={level} parentId={parentKey} existing={dialog.existing} submit={mutate} close={() => setDialog(null)} />
      )}
      {model && dialog?.type === 'relation' && (
        <RelationFormModal model={model} existing={dialog.existing} source={dialog.source} target={dialog.target} sourcePort={dialog.sourcePort} targetPort={dialog.targetPort} submit={mutate} close={() => setDialog(null)} />
      )}
      {dialog?.type === 'diagram-create' && (
        <DiagramNameModal title="새 아키텍처" submit={createDiagram} close={() => setDialog(null)} />
      )}
      {dialog?.type === 'diagram-rename' && (
        <DiagramNameModal title="아키텍처 이름 바꾸기" initial={shown.name} submit={renameDiagram} close={() => setDialog(null)} />
      )}
      {dialog?.type === 'diagram-delete' && (
        <Confirm title="아키텍처를 지울까요?" description={diagramDeleteText(shown, model)} close={() => setDialog(null)} accept={removeDiagram} />
      )}
      {deleting && (
        <Confirm title="지울까요?" description={deleteText} close={() => setDialog(null)} accept={() => confirmDelete(deleting)} />
      )}
    </section>
  );
}
