import { useEffect, useEffectEvent, useRef, useState } from 'react';
import { createCommand } from 'lexical';
import type { EditorHost } from './context.ts';
import { relativePath } from './grammar.ts';
import { Modal } from '../ui/Modal.tsx';

const ORIGIN = 'https://embed.diagrams.net';
const EDITOR_URL = ORIGIN + '/?embed=1&proto=json&spin=1&noExitBtn=1&noSaveBtn=1&saveAndExit=0&suppressNewWindows=1';
const PNG_PREFIX = 'data:image/png;base64,';
const MAX_BYTES = 50_000_000;
const MAX_BASE64 = Math.ceil(MAX_BYTES / 3) * 4;
const TIMEOUT = 25_000;

export const OPEN_DRAWIO_COMMAND = createCommand<string>('aisurada:open-drawio');
export type DrawioAttachment = { directory: string; name: string };

export function drawioAttachmentName(source: string): string | undefined {
  return /^attach\/([^/]+\.drawio\.png)$/i.exec(relativePath(source) ?? '')?.[1];
}

type DiagramSource = { data: string; label: string };
type Phase = 'choose' | 'opening' | 'loading' | 'ready' | 'exporting' | 'writing' | 'unavailable';
type EditorMessage = {
  event?: string;
  error?: unknown;
  data?: unknown;
  format?: string;
  message?: { requestId?: string };
};

export function DrawioDialog({ existing, host, onSave, onClose }: {
  existing?: DrawioAttachment;
  host: EditorHost;
  onSave(bytes: Uint8Array, label: string): Promise<void>;
  onClose(): void;
}) {
  const [source, setSource] = useState<DiagramSource | null>(null);
  const [phase, setPhase] = useState<Phase>(existing ? 'opening' : 'choose');
  const currentPhase = useRef(phase);
  const [error, setError] = useState('');
  const frame = useRef<HTMLIFrameElement>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const timer = useRef<number | undefined>(undefined);
  const exportId = useRef('');
  const asking = useRef(false);
  const report = (value: unknown) => setError(value instanceof Error ? value.message : String(value));
  const show = (value: Phase) => { currentPhase.current = value; setPhase(value); };
  const save = useEffectEvent(onSave);
  const closeAfterSave = useEffectEvent(onClose);

  useEffect(() => {
    if (!existing) return;
    let active = true;
    void host.readDrawio(existing.directory, existing.name).then(data => {
      if (active) setSource({ data, label: 'draw.io 다이어그램' });
    }, error => {
      if (active) { report(error); show('unavailable'); }
    });
    return () => { active = false; };
  }, [existing, host]);

  useEffect(() => {
    const editorFrame = frame.current;
    if (!source || !editorFrame) return;
    let active = true;
    async function receive(event: MessageEvent) {
      if (event.origin !== ORIGIN || event.source !== editorFrame?.contentWindow
        || typeof event.data !== 'string' || event.data.length > MAX_BASE64 * 4) return;
      let message: EditorMessage;
      try {
        const data = JSON.parse(event.data);
        if (!data || typeof data !== 'object') return;
        message = data;
      } catch { return; }

      if (message.event === 'init' && currentPhase.current === 'opening') {
        show('loading');
        editorFrame.contentWindow?.postMessage(JSON.stringify({ action: 'load', xml: source?.data }), ORIGIN);
      } else if (message.event === 'load' && currentPhase.current === 'loading') {
        window.clearTimeout(timer.current);
        if (message.error) {
          setError('draw.io에서 다이어그램을 열지 못했어요.');
          show('unavailable');
        } else {
          show('ready');
          editorFrame.focus();
        }
      } else if (message.event === 'export' && currentPhase.current === 'exporting'
        && message.message?.requestId === exportId.current) {
        window.clearTimeout(timer.current);
        show('writing');
        try {
          if (message.format !== 'xmlpng' || typeof message.data !== 'string' || !message.data.startsWith(PNG_PREFIX)) {
            throw new Error('draw.io에서 그림을 만들지 못했어요.');
          }
          const encoded = message.data.slice(PNG_PREFIX.length);
          if (encoded.length > MAX_BASE64) throw new Error('다이어그램 첨부는 50MB까지 저장할 수 있어요.');
          const bytes = Uint8Array.from(atob(encoded), byte => byte.charCodeAt(0));
          await save(bytes, source?.label || 'draw.io 다이어그램');
          if (active) closeAfterSave();
        } catch (error) {
          if (active) { report(error); show('ready'); }
        }
      }
    }

    window.addEventListener('message', receive);
    editorFrame.src = EDITOR_URL;
    timer.current = window.setTimeout(() => {
      setError('draw.io 편집기를 열지 못했어요. 인터넷 연결을 확인하고 다시 열어 주세요.');
      show('unavailable');
    }, TIMEOUT);
    return () => {
      active = false;
      window.clearTimeout(timer.current);
      window.removeEventListener('message', receive);
    };
  }, [source]);

  function openEditor(next: DiagramSource) {
    setError('');
    show('opening');
    setSource(next);
  }

  async function importFile(file: File | undefined) {
    if (!file || currentPhase.current !== 'choose') return;
    if (!/\.drawio$/i.test(file.name) || file.size > MAX_BYTES) {
      setError('50MB 이하의 .drawio 파일을 골라 주세요.');
      return;
    }
    show('opening');
    try {
      const data = await file.text();
      const doc = new DOMParser().parseFromString(data, 'application/xml');
      if (doc.doctype || doc.getElementsByTagName('parsererror').length
        || !['mxfile', 'mxGraphModel'].includes(doc.documentElement.localName)) {
        throw new Error('이 파일에서 draw.io 다이어그램을 읽을 수 없어요.');
      }
      const label = file.name.replace(/\.drawio$/i, '').replace(/[\[\]|\r\n]/g, ' ').trim();
      openEditor({ data, label: label || 'draw.io 다이어그램' });
    } catch (error) {
      report(error);
      show('choose');
    }
  }

  function exportDiagram() {
    if (currentPhase.current !== 'ready' || asking.current || !frame.current?.contentWindow) return;
    setError('');
    show('exporting');
    exportId.current = crypto.randomUUID();
    frame.current.contentWindow.postMessage(JSON.stringify({
      action: 'export',
      format: 'xmlpng',
      requestId: exportId.current,
      scale: 2,
      border: 8,
    }), ORIGIN);
    timer.current = window.setTimeout(() => {
      setError('draw.io에서 그림을 받지 못했어요. 다시 저장해 주세요.');
      show('ready');
    }, TIMEOUT);
  }

  async function close() {
    if (asking.current || currentPhase.current === 'exporting' || currentPhase.current === 'writing') return;
    if (currentPhase.current === 'ready') {
      asking.current = true;
      try {
        if (!await host.confirm('이번 편집 내용을 저장하지 않고 닫을까요?', {
          title: 'draw.io 닫기', okLabel: '닫기', cancelLabel: '편집 계속',
        })) return;
      } finally { asking.current = false; }
    }
    onClose();
  }

  const busy = phase === 'exporting' || phase === 'writing';
  return <Modal key={source ? 'editor' : 'picker'} title="draw.io" className={source ? 'drawio-modal drawio-modal-editor' : 'drawio-modal'}
    close={() => void close()} resizable={Boolean(source)} defaultWidth={1100} defaultHeight={750}>
    {source && <iframe ref={frame} title="draw.io 다이어그램 편집기" tabIndex={0}
      sandbox="allow-scripts allow-same-origin allow-forms" referrerPolicy="no-referrer"
      hidden={phase === 'unavailable'} />}
    {(phase === 'opening' || phase === 'loading') && <p role="status">draw.io 편집기를 여는 중이에요…</p>}
    {error && <p role="alert" className="drawio-error">{error}</p>}
    <footer>
      {phase === 'choose' ? <>
        <input ref={fileInput} type="file" accept=".drawio" hidden onChange={event => {
          void importFile(event.currentTarget.files?.[0]);
          event.currentTarget.value = '';
        }} />
        <button className="button" type="button" onClick={() => fileInput.current?.click()}>파일 가져오기</button>
        <button className="button button-primary" type="button" onClick={() => openEditor({ data: '', label: 'draw.io 다이어그램' })}>
          새 다이어그램
        </button>
      </> : <>
        <button className="button" type="button" disabled={busy} onClick={() => void close()}>취소</button>
        {source && phase !== 'unavailable' && <button className="button button-primary" type="button"
          disabled={phase !== 'ready'} onClick={exportDiagram}>{busy ? '저장 중…' : '노트에 저장'}</button>}
      </>}
    </footer>
  </Modal>;
}
