import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { CLAUDE_ENABLED } from '@claude-plug/state';
import type { WorkspaceController } from '../../workspace/controller.ts';
import type { WorkspaceState } from '../../workspace/types.ts';
import { asText } from '../../workspace/types.ts';
import { Icon } from '../Icon.tsx';
import { Confirm } from '../Modal.tsx';
import { Page } from '../OtherPages.tsx';
import { Details } from './Details.tsx';
import { Diagram } from './Diagram.tsx';
import { ElementFormModal, RelationFormModal } from './EditForms.tsx';
import { LEVEL_LABELS, projectSlice } from './layout.ts';
import { applyC4Changes, errorText, getC4Model, isPreconditionError, sendC4Request } from './client.ts';
import type { C4Message } from './client.ts';
import { KIND_LABELS, deletionImpact, parentKindFor, relationTitle, viewIdFor } from './model.ts';
import type { C4Element, C4Model, C4Operation, C4Relationship, Point } from './model.ts';
import './architecture.css';

const NO_POSITIONS: Record<string, Point> = {};

type Dialog =
  | { type: 'element'; existing?: C4Element }
  | { type: 'relation'; existing?: C4Relationship; source?: string; target?: string }
  | { type: 'delete'; target: { kind: 'element' | 'relationship'; id: string } };

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
  const [chatOpen, setChatOpen] = useState(true);
  const [draft, setDraft] = useState('');
  const [history, setHistory] = useState<C4Message[]>([]);
  const [sending, setSending] = useState(false);
  const request = useRef<HTMLTextAreaElement>(null);
  const toggle = useRef<HTMLButtonElement>(null);
  const log = useRef<HTMLDivElement>(null);
  const modelRef = useRef<C4Model | null>(null);
  const queue = useRef<Promise<unknown>>(Promise.resolve());
  const busy = useRef(false);
  const dialogOpen = useRef(false);
  const generation = useRef(0);
  const call = controller.callEngine;
  const host = controller.callHost;
  dialogOpen.current = dialog !== null;

  const accept = useCallback((next: C4Model) => { modelRef.current = next; setModel(next); }, []);

  const load = useCallback(async (quiet: boolean) => {
    const sequence = ++generation.current;
    if (!quiet) { setLoading(true); setError(''); }
    try {
      const next = await getC4Model(call, project);
      if (generation.current !== sequence) return;
      if (!quiet || next.precondition !== modelRef.current?.precondition) accept(next);
    } catch (failure) {
      if (generation.current === sequence && !quiet) setError(errorText(failure));
    } finally {
      if (generation.current === sequence && !quiet) setLoading(false);
    }
  }, [accept, call, project]);

  useEffect(() => { void load(false); return () => { ++generation.current; }; }, [load]);

  // 창이 다시 눈에 띄면 AI나 다른 창이 고친 것을 따라간다. 편집 창이 열려 있거나 쓰는 중이면 건드리지 않는다.
  useEffect(() => {
    const refresh = () => { if (!dialogOpen.current && !busy.current) void load(true); };
    window.addEventListener('focus', refresh);
    return () => window.removeEventListener('focus', refresh);
  }, [load]);

  useEffect(() => {
    if (log.current) log.current.scrollTop = log.current.scrollHeight;
  }, [history, sending, chatOpen]);

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

  const parentKind = parentKindFor(level);
  const parents = useMemo(() => model?.elements.filter(item => item.kind === parentKind) ?? [], [model, parentKind]);
  const parent = parents.find(item => item.id === parentId) ?? parents[0];
  const parentKey = parent?.id ?? null;
  const visible = useMemo(() => model ? projectSlice(model, level, parentKey) : [], [level, model, parentKey]);
  const viewId = viewIdFor(level, parentKey);
  const positions = (viewId && model?.views[viewId]) || NO_POSITIONS;
  const element = model?.elements.find(item => item.id === selectedElement);
  const relation = model?.relationships.find(item => item.id === selectedRelationship);
  const targetName = element?.name ?? (relation && model ? relationTitle(model, relation) : '');
  const canAdd = Boolean(model) && !loading && (level === 1 || Boolean(parent));
  const canSend = CLAUDE_ENABLED && controller.isNative() && Boolean(model) && !loading && !sending;
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

  async function send() {
    const text = draft.trim();
    if (!text || !canSend || busy.current) return;
    const sequence = generation.current;
    const prior = history;
    const selected = element?.id ?? relation?.id ?? null;
    busy.current = true;
    setSending(true); setDraft('');
    setHistory(items => [...items, { role: 'user', text }]);
    try {
      await queue.current;
      const reply = await sendC4Request(host, project, selected, text, prior);
      if (generation.current !== sequence) return;
      if (reply.model) accept(reply.model);
      setHistory(items => [...items, {
        role: 'assistant', text: reply.message, warning: reply.warning ?? undefined, updated: reply.applied > 0,
      }]);
    } catch (failure) {
      if (generation.current !== sequence) return;
      setHistory(items => [...items, { role: 'assistant', text: errorText(failure) }]);
      setDraft(text);
    } finally {
      busy.current = false;
      if (generation.current === sequence) setSending(false);
      setResetKey(key => key + 1);
    }
  }

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
            <button type="button" onClick={() => void load(false)} disabled={loading || sending} aria-label="다시 불러오기"><Icon name="history" size={15} /></button>
            <button
              ref={toggle}
              type="button"
              aria-controls="c4-chat-panel"
              aria-expanded={chatOpen}
              onClick={() => { setChatOpen(true); requestAnimationFrame(() => request.current?.focus()); }}
            ><Icon name="tab" size={14} />{chatOpen ? '챗으로 이동' : '챗 열기'}</button>
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
        <div className="c4-workbench" data-chat-open={chatOpen}>
          <div className="c4-map-column">
            {level > 1 && parents.length > 0 && (
              <label className="c4-parent">{KIND_LABELS[parentKind!]}
                <select value={parentKey ?? ''} onChange={event => { setParentId(event.target.value); setSelectedElement(null); setSelectedRelationship(null); setConnect(null); }}>
                  {parents.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
                </select>
              </label>
            )}
            {model ? (
              <Diagram
                key={level + ':' + (parentKey ?? '')}
                elements={visible}
                relationships={model.relationships}
                positions={positions}
                selectedElement={selectedElement}
                selectedRelationship={selectedRelationship}
                selectElement={pickElement}
                selectRelationship={pickRelationship}
                drill={drill}
                onMove={moveNodes}
                resetKey={resetKey}
                connecting={connect !== null}
                emptyText={emptyText}
              />
            ) : <div className="c4-empty"><p role={loading ? 'status' : undefined}>{loading ? '다이어그램을 불러오고 있어요.' : '다시 불러오기를 눌러 주세요.'}</p></div>}
            {model && (
              <section className="c4-details" aria-label="선택한 항목" aria-live="polite">
                <Details
                  model={model}
                  element={element}
                  relation={relation}
                  edit={() => setDialog(element ? { type: 'element', existing: element } : { type: 'relation', existing: relation })}
                  remove={() => setDialog({ type: 'delete', target: element ? { kind: 'element', id: element.id } : { kind: 'relationship', id: relation!.id } })}
                  connect={() => setDialog({ type: 'relation', source: element?.id })}
                  open={element && (element.kind === 'system' || element.kind === 'container') ? () => drill(element.id) : undefined}
                  pickRelation={pickRelationship}
                />
              </section>
            )}
          </div>
          <aside id="c4-chat-panel" className="c4-chat" aria-label="아키텍처 챗" hidden={!chatOpen}>
            <header>
              <div><h2>아키텍처 챗</h2><span>Claude</span></div>
              <button type="button" aria-label="챗 접기" onClick={() => { setChatOpen(false); toggle.current?.focus(); }}><Icon name="close" size={15} /></button>
            </header>
            <div className="c4-chat-log" role="log" aria-label="챗 대화" ref={log}>
              {history.map((message, index) => (
                <div className="c4-chat-message" data-role={message.role} key={index}>
                  {message.updated && <span className="c4-chat-applied">모델 반영</span>}
                  <p>{message.text}</p>
                  {message.warning && <p className="c4-error">{message.warning}</p>}
                </div>
              ))}
              {sending && <p role="status">요청을 해석하고 모델에 적용하고 있어요.</p>}
            </div>
            <form className="c4-compose" onSubmit={event => { event.preventDefault(); void send(); }}>
              <p className="c4-chat-target" aria-live="polite">
                <Icon name="issue" size={14} />
                {targetName ? <><span>선택한 항목</span><strong>{targetName}</strong></> : <span>다이어그램 전체</span>}
              </p>
              {!controller.isNative() && <p>데스크톱 앱에서 챗을 쓸 수 있어요.</p>}
              {controller.isNative() && !CLAUDE_ENABLED && <p>이 빌드에서는 Claude 챗을 사용할 수 없어요.</p>}
              <label htmlFor="c4-request">변경 요청</label>
              <textarea
                id="c4-request"
                ref={request}
                value={draft}
                rows={3}
                maxLength={4000}
                placeholder="예: 이 요소의 설명을 바꿔줘."
                disabled={!canSend}
                onChange={event => setDraft(event.target.value)}
              />
              <button className="button-primary" type="submit" disabled={!canSend || !draft.trim()}><Icon name="arrowUp" size={14} />보내기</button>
            </form>
          </aside>
        </div>
      </div>
      {model && dialog?.type === 'element' && (
        <ElementFormModal model={model} level={level} parentId={parentKey} existing={dialog.existing} submit={mutate} close={() => setDialog(null)} />
      )}
      {model && dialog?.type === 'relation' && (
        <RelationFormModal model={model} existing={dialog.existing} source={dialog.source} target={dialog.target} submit={mutate} close={() => setDialog(null)} />
      )}
      {deleting && (
        <Confirm title="지울까요?" description={deleteText} close={() => setDialog(null)} accept={() => confirmDelete(deleting)} />
      )}
    </section>
  );
}
