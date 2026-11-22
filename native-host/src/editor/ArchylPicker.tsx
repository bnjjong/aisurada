import { useEffect, useState } from 'react';
import { Modal } from '../ui/Modal.tsx';
import { archylSource, getArchylModel } from './archyl.ts';
import type { ArchylModel, ArchylView } from './archyl.ts';
import { renderDiagram } from './mermaid.tsx';

export function ArchylPicker({ projectId, close, insert }: {
  projectId: string;
  close(): void;
  insert(source: string): void;
}) {
  const [model, setModel] = useState<ArchylModel>();
  const [level, setLevel] = useState<ArchylView['level']>('context');
  const [system, setSystem] = useState('');
  const [container, setContainer] = useState('');
  const [error, setError] = useState('');
  const [busy, setBusy] = useState(false);

  useEffect(() => {
    let current = true;
    setModel(undefined);
    setError('');
    void getArchylModel(projectId).then(value => {
      if (current) setModel(value);
    }).catch(reason => {
      if (current) setError(String(reason));
    });
    return () => { current = false; };
  }, [projectId]);

  async function submit() {
    if (!model) return;
    setBusy(true);
    setError('');
    try {
      const elementId = level === 'component' ? container : level === 'container' ? system : undefined;
      const source = archylSource(model, { projectId, level, elementId });
      await renderDiagram(source, document.documentElement.dataset.theme || 'system');
      insert(source);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  const systems = model?.elements.filter(element => element.kind === 'system') ?? [];
  const containers = model?.elements.filter(element => element.kind === 'container' && element.parentId === system) ?? [];
  const canInsert = Boolean(model && (level === 'context' || (system && (level !== 'component' || container))));
  return (
    <Modal title="C4 다이어그램 삽입" close={close} className="ndf">
      <form onSubmit={event => { event.preventDefault(); void submit(); }}>
        {!model && !error && <p role="status">Archyl에서 C4 모델을 불러오는 중이에요…</p>}
        {model && (
          <>
            <label>
              C4 뷰
              <select value={level} onChange={event => setLevel(event.target.value as ArchylView['level'])}>
                <option value="context">시스템 컨텍스트</option>
                <option value="container">컨테이너</option>
                <option value="component">컴포넌트</option>
              </select>
            </label>
            {level !== 'context' && (
              <label>
                시스템
                <select value={system} onChange={event => { setSystem(event.target.value); setContainer(''); }}>
                  <option value="">시스템 선택</option>
                  {systems.map(element => <option key={element.id} value={element.id}>{element.name}</option>)}
                </select>
              </label>
            )}
            {level === 'component' && (
              <label>
                컨테이너
                <select value={container} disabled={!system} onChange={event => setContainer(event.target.value)}>
                  <option value="">컨테이너 선택</option>
                  {containers.map(element => <option key={element.id} value={element.id}>{element.name}</option>)}
                </select>
              </label>
            )}
          </>
        )}
        {error && <p role="alert">{error}</p>}
        <footer className="ndf-foot">
          <button type="button" onClick={close}>닫기</button>
          <button type="submit" disabled={!canInsert || busy}>{busy ? '그리는 중…' : '삽입'}</button>
        </footer>
      </form>
    </Modal>
  );
}
