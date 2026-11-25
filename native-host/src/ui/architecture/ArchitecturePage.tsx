import { useEffect, useMemo, useRef, useState } from 'react';
import { CLAUDE_ENABLED } from '@claude-plug/state';
import type { WorkspaceController } from '../../workspace/controller.ts';
import type { WorkspaceState } from '../../workspace/types.ts';
import { asText } from '../../workspace/types.ts';
import { Icon } from '../Icon.tsx';
import { Page } from '../OtherPages.tsx';
import { Diagram } from './Diagram.tsx';
import { LEVEL_LABELS, projectSlice } from './layout.ts';
import { errorText, getArchylModel, readProjectLink, sendArchylRequest, useArchylAccount, writeProjectLink } from './client.ts';
import type { ArchylMessage, C4Model, ArchylProject } from './client.ts';
import './architecture.css';

export function ArchitecturePage({ slug, state, controller }: {
  slug: string;
  state: WorkspaceState;
  controller: WorkspaceController;
}) {
  const workspace = state.workspace!;
  const project = workspace.projects.find(item => item.slug === slug || item.id === slug);
  if (!project) return <Page title="이 프로젝트를 찾을 수 없어요" />;
  const linkKey = 'aisurada.archyl.project.' + JSON.stringify([workspace.path, project.id ?? slug]);
  return <ProjectArchitecture key={linkKey} linkKey={linkKey} name={asText(project.name) || slug} controller={controller} />;
}

function ProjectArchitecture({ linkKey, name, controller }: {
  linkKey: string;
  name: string;
  controller: WorkspaceController;
}) {
  const [id, setId] = useState(() => readProjectLink(linkKey));
  const [error, setError] = useState('');
  const account = useArchylAccount(controller);
  const linked = account.connection?.projects.find(item => item.id === id);

  function changeLink(value: string) {
    if (writeProjectLink(linkKey, value)) { setId(value); setError(''); }
    else setError('이 기기에 프로젝트 연결을 저장하지 못했어요.');
  }

  return (
    <section className="c4-page" aria-label="C4 아키텍처">
      <header className="c4-page-header">
        <div><h1>C4 아키텍처</h1><p>{name}</p></div>
        {account.connection && account.connection.projects.length > 0 && (
          <label>Archyl 프로젝트
            <select value={linked?.id ?? ''} onChange={event => changeLink(event.target.value)}>
              <option value="">프로젝트 선택</option>
              {account.connection.projects.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
            </select>
          </label>
        )}
      </header>
      {error && <p className="c4-error" role="alert">{error}</p>}
      {!controller.isNative() ? <p>데스크톱 앱에서 Archyl을 연결할 수 있어요.</p> : !linked ? (
        <div className="c4-empty">
          {!account.checked && account.phase === 'checking' ? <p role="status">Archyl 연결을 확인하고 있어요.</p> : account.connection ? (
            <p>{account.connection.projects.length === 0
              ? '이 Archyl 계정으로 볼 수 있는 프로젝트가 없어요.'
              : id ? '이전에 연결한 프로젝트에 접근할 수 없어요. 현재 계정의 프로젝트를 선택해 주세요.'
                : '이 프로젝트에서 열 Archyl 프로젝트를 선택해 주세요.'}</p>
          ) : (
            <>
              <p>Archyl에 로그인하면 이 프로젝트의 C4 다이어그램을 볼 수 있어요.</p>
              {account.error && <p className="c4-error" role="alert">{account.error}</p>}
              <button className="button-primary" type="button" onClick={() => controller.settings(true, 'connections')}>연결 설정</button>
            </>
          )}
        </div>
      ) : (
        <LinkedArchitecture key={linked.id + ':' + account.revision} project={linked} writable={account.connection!.writable} controller={controller} />
      )}
    </section>
  );
}

function LinkedArchitecture({ project, writable, controller }: {
  project: ArchylProject;
  writable: boolean;
  controller: WorkspaceController;
}) {
  const [model, setModel] = useState<C4Model | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState('');
  const [level, setLevel] = useState(1);
  const [parentId, setParentId] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [chatOpen, setChatOpen] = useState(true);
  const [draft, setDraft] = useState('');
  const [history, setHistory] = useState<ArchylMessage[]>([]);
  const [sending, setSending] = useState(false);
  const request = useRef<HTMLTextAreaElement>(null);
  const toggle = useRef<HTMLButtonElement>(null);
  const log = useRef<HTMLDivElement>(null);
  const busy = useRef(false);
  const generation = useRef(0);
  const host = controller.callHost;
  const projectUrl = `https://app.archyl.com/project/${encodeURIComponent(project.id)}`;

  useEffect(() => {
    const sequence = ++generation.current;
    getArchylModel(host, project.id).then(
      value => { if (generation.current === sequence) setModel(value); },
      failure => { if (generation.current === sequence) setError(errorText(failure)); },
    ).finally(() => { if (generation.current === sequence) setLoading(false); });
    return () => { ++generation.current; };
  }, [host, project.id]);

  useEffect(() => {
    if (log.current) log.current.scrollTop = log.current.scrollHeight;
  }, [history, sending, chatOpen]);

  const selection = model?.elements.find(item => item.id === selectedId);
  const relation = model?.relationships.find(item => item.id && item.id === selectedId);
  const targetName = selection?.name ?? (relation ? relation.label || '선택한 관계' : '');
  const parents = model?.elements.filter(item => item.level === level - 1) ?? [];
  const parent = parents.find(item => item.id === parentId) ?? parents.find(item =>
    model?.elements.some(child => child.parentId === item.id)) ?? parents[0];
  const parentKey = parent?.id ?? null;
  const visible = useMemo(() => model ? projectSlice(model, level, parentKey) : [], [level, model, parentKey]);
  const canSend = writable && CLAUDE_ENABLED && Boolean(model) && !loading && !sending;
  const showContext = (next: number) => {
    setLevel(next);
    if (selection?.level === next - 1 && model?.elements.some(item => item.parentId === selection.id)) setParentId(selection.id);
    setSelectedId(null);
  };
  const drill = (id: string) => {
    const item = model?.elements.find(element => element.id === id);
    if (!item || item.level >= LEVEL_LABELS.length || !model?.elements.some(element => element.parentId === id)) return;
    setLevel(item.level + 1);
    setParentId(item.id);
    setSelectedId(null);
  };

  async function reload() {
    if (busy.current) return;
    busy.current = true;
    const sequence = ++generation.current;
    setLoading(true); setError('');
    try {
      const value = await getArchylModel(host, project.id);
      if (generation.current === sequence) setModel(value);
    } catch (failure) {
      if (generation.current === sequence) setError(errorText(failure));
    } finally {
      busy.current = false;
      if (generation.current === sequence) setLoading(false);
    }
  }

  async function send() {
    const text = draft.trim();
    if (!text || !canSend || busy.current) return;
    const sequence = generation.current;
    const prior = history;
    const selected = selection?.id ?? relation?.id ?? null;
    busy.current = true;
    setSending(true);
    setDraft('');
    setHistory(items => [...items, { role: 'user', text }]);
    try {
      const reply = await sendArchylRequest(host, project.id, selected, text, prior);
      if (generation.current !== sequence) return;
      if (reply.model) {
        const updated = reply.model;
        setModel(updated);
        if (selected && !updated.elements.some(item => item.id === selected) && !updated.relationships.some(item => item.id === selected)) {
          setSelectedId(null);
        }
      }
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
    }
  }

  return (
    <div className="c4-workspace">
      <div className="c4-toolbar">
        <nav aria-label="C4 레벨">
          {LEVEL_LABELS.map((label, index) => (
            <button key={label} type="button" aria-pressed={level === index + 1} onClick={() => showContext(index + 1)}>{label}</button>
          ))}
        </nav>
        <div className="c4-toolbar-actions">
          <a className="c4-project-link" href={projectUrl} target="_blank" rel="noreferrer">
            <Icon name="popout" size={14} /> Archyl에서 열기
          </a>
          <button type="button" onClick={() => void reload()} disabled={loading || sending} aria-label="다이어그램 새로고침"><Icon name="history" size={15} /></button>
          <button
            ref={toggle}
            type="button"
            aria-controls="c4-chat-panel"
            aria-expanded={chatOpen}
            onClick={() => { setChatOpen(true); requestAnimationFrame(() => request.current?.focus()); }}
          ><Icon name="tab" size={14} />{chatOpen ? '챗으로 이동' : '챗 열기'}</button>
        </div>
      </div>
      {error && <p className="c4-error" role="alert">{error}</p>}
      <div className="c4-workbench" data-chat-open={chatOpen}>
        <div className="c4-map-column">
          {level > 1 && parents.length > 0 && (
            <label className="c4-parent">{LEVEL_LABELS[level - 2]}
              <select value={parentKey ?? ''} onChange={event => { setParentId(event.target.value); setSelectedId(null); }}>
                {parents.map(item => <option key={item.id} value={item.id}>{item.name}</option>)}
              </select>
            </label>
          )}
          {model ? (
            <Diagram
              key={level + ':' + (parentKey ?? '')}
              elements={visible}
              relationships={model.relationships}
              selected={selectedId}
              select={setSelectedId}
              drill={drill}
            />
          ) : <div className="c4-empty"><p role={loading ? 'status' : undefined}>{loading ? '다이어그램을 불러오고 있어요.' : '다이어그램을 새로고침해 주세요.'}</p></div>}
          {targetName && <p className="c4-details"><strong>{targetName}</strong>{selection?.description && <span>{selection.description}</span>}</p>}
        </div>
        <aside id="c4-chat-panel" className="c4-chat" aria-label="아키텍처 챗" hidden={!chatOpen}>
          <header>
            <div><h2>아키텍처 챗</h2><span>Archyl MCP</span></div>
            <button type="button" aria-label="챗 접기" onClick={() => { setChatOpen(false); toggle.current?.focus(); }}><Icon name="close" size={15} /></button>
          </header>
          <div className="c4-chat-log" role="log" aria-label="챗 대화" ref={log}>
            {history.map((message, index) => (
              <div className="c4-chat-message" data-role={message.role} key={index}>
                {message.updated && <span className="c4-chat-applied">Archyl 반영</span>}
                <p>{message.text}</p>
                {message.warning && <p className="c4-error">{message.warning}</p>}
              </div>
            ))}
            {sending && <p role="status">요청을 해석하고 Archyl에 적용하고 있어요.</p>}
          </div>
          <form className="c4-compose" onSubmit={event => { event.preventDefault(); void send(); }}>
            <p className="c4-chat-target" aria-live="polite">
              <Icon name="issue" size={14} />
              {targetName ? <><span>선택한 요소</span><strong>{targetName}</strong></> : <span>다이어그램 전체</span>}
            </p>
            {!writable && <p>이 Archyl 연결은 조회 전용이에요. <button type="button" onClick={() => controller.settings(true, 'connections')}>연결 설정</button></p>}
            {writable && !CLAUDE_ENABLED && <p>이 빌드에서는 Claude 챗을 사용할 수 없어요.</p>}
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
  );
}
