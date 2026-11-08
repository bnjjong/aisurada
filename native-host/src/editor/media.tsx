import { useContext, useEffect, useRef, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext';
import { useLexicalNodeSelection } from '@lexical/react/useLexicalNodeSelection';
import { $getNodeByKey, $getSelection, $isNodeSelection, CLICK_COMMAND, COMMAND_PRIORITY_LOW, KEY_BACKSPACE_COMMAND, KEY_DELETE_COMMAND } from 'lexical';
import { EditorContext } from './context.ts';
import type { EditorFeatures } from './context.ts';
import { relativePath } from './grammar.ts';

const heights = new Map<string, number>();
export function frameHeight(url: string, value?: number): number {
  if (value !== undefined && Number.isFinite(value)) {
    heights.delete(url); heights.set(url, Math.max(160, Math.min(1600, Math.ceil(value))));
    if (heights.size > 2000) heights.delete(heights.keys().next().value!);
  }
  return heights.get(url) || 480;
}
export async function openEditorLink(url: string, features: EditorFeatures, heading?: (slug: string) => boolean) {
  if (url.startsWith('#sub:')) { features.openChild?.(url.slice(5)); return; }
  if (url.startsWith('#doc:')) {
    const id = url.slice(5);
    if (features.notes?.some(note => note.id === id)) features.openNote?.(id);
    else await features.host?.warn('노트를 찾을 수 없어요');
    return;
  }
  if (url.startsWith('#')) { if (!heading?.(url.slice(1))) await features.host?.warn('절을 찾을 수 없어요'); return; }
  if (/^(https?:|mailto:|tel:)/i.test(url)) { await features.host?.openURL(url); return; }
  if (url.startsWith('/') || url.startsWith('~/') || /^file:\/\//i.test(url)) {
    let path = url;
    if (/^file:\/\//i.test(url)) {
      try { path = decodeURIComponent(new URL(url).pathname); }
      catch { path = decodeURIComponent(url.replace(/^file:\/\/(localhost)?/i, '')); }
    } else {
      try { path = decodeURIComponent(url); } catch { path = url; }
    }
    try {
      if (features.host?.openPath) await features.host.openPath(path);
      else await features.host?.openFile(path);
    } catch (e) {
      const msg = typeof e === 'string' ? e : (e instanceof Error ? e.message : '');
      await features.host?.warn(msg ? `파일을 열 수 없어요: ${path}\n(${msg})` : `파일을 열 수 없어요: ${path}`);
    }
    return;
  }
  const path = relativePath(url);
  if (!path || !features.noteDirectory) return;
  if (/\.(html?|svg)$/i.test(path)) {
    try { if (!await features.host?.confirm('첨부를 기본 앱에서 열까요? 파일 안의 스크립트가 실행될 수 있어요.')) return; } catch { return; }
  }
  await features.host?.openFile(`${features.noteDirectory}/${path}`);
}
export function ImageView({ source, alt, width, nodeKey, onWidth }: { source: string; alt: string; width: number; nodeKey: string; onWidth(width: number): void }) {
  const features = useContext(EditorContext), path = relativePath(source);
  if (path && features.noteDirectory && /\.html?$/i.test(path)) return <HTMLFrame path={path} label={alt} />;
  const url = path && features.noteDirectory && features.host?.native ? convertFileSrc(`${features.noteDirectory}/${path}`) : source;
  return <ImageBox url={url} source={source} alt={alt} width={width} nodeKey={nodeKey} onWidth={onWidth} />;
}

const IMAGE_MIN = 48;
// 도구줄의 크기 고르기. width < 0 은 본문 폭에 맞춤, 0 은 크기를 정하지 않은 원래 크기.
const IMAGE_PRESETS: { label: string; width: number }[] = [
  { label: '작게', width: 240 }, { label: '중간', width: 480 }, { label: '본문 폭', width: -1 }, { label: '원래 크기', width: 0 },
];
/**
 * 노트 이미지 한 장. 누르면 선택되어 모서리 손잡이(끌어서 가로 조절, 세로는 비율대로)와
 * 크기 도구줄(작게·중간·본문 폭·원래 크기)이 나타난다. 더블클릭하면 원래 크기로 돌아간다.
 * 크기는 [onWidth]로 노트에 저장되고, 본문 폭보다 크게는 키울 수 없다.
 */
function ImageBox({ url, source, alt, width, nodeKey, onWidth }: { url: string; source: string; alt: string; width: number; nodeKey: string; onWidth(width: number): void }) {
  const [editor] = useLexicalComposerContext();
  const [selected, setSelected, clearSelected] = useLexicalNodeSelection(nodeKey);
  const [editable, setEditable] = useState(() => editor.isEditable());
  const [live, setLive] = useState<number | null>(null);
  const frame = useRef<HTMLSpanElement>(null), image = useRef<HTMLImageElement>(null);
  useEffect(() => editor.registerEditableListener(setEditable), [editor]);
  // 에디터의 기본 클릭 처리(리치텍스트)는 클릭 때 노드 선택을 지운다. 그보다 먼저 이 이미지를 눌렀는지 보고 선택한다.
  useEffect(() => editor.registerCommand(CLICK_COMMAND, (event: MouseEvent) => {
    const target = event.target as Element | null;
    if (!editable || !target || !frame.current?.contains(target)) return false;
    // 도구줄과 손잡이에서 난 클릭(끌기를 놓을 때 포함)도 여기서 끝낸다. 넘기면 기본 처리가 이미지 선택을 지운다.
    if (target.closest('.lex-image-bar, .lex-image-grip')) return true;
    if (event.shiftKey) setSelected(!selected); else { clearSelected(); setSelected(true); }
    return true;
  }, COMMAND_PRIORITY_LOW), [editor, editable, selected, setSelected, clearSelected]);
  useEffect(() => {
    if (!selected) return;
    const remove = (event: KeyboardEvent) => {
      if (!$isNodeSelection($getSelection())) return false;
      event.preventDefault(); $getNodeByKey(nodeKey)?.remove(); return true;
    };
    const stop = [KEY_DELETE_COMMAND, KEY_BACKSPACE_COMMAND].map(command => editor.registerCommand(command, remove, COMMAND_PRIORITY_LOW));
    return () => stop.forEach(off => off());
  }, [editor, nodeKey, selected]);
  const available = () => Math.max(IMAGE_MIN, Math.floor((frame.current?.closest('p, td, th, li, .lex-content') as HTMLElement | null)?.clientWidth ?? 600));
  const clamp = (value: number, max: number) => Math.max(IMAGE_MIN, Math.min(max, Math.round(value)));
  const grab = (event: React.PointerEvent<HTMLSpanElement>) => {
    const target = image.current;
    if (!target) return;
    event.preventDefault(); event.stopPropagation();
    const grip = event.currentTarget, start = event.clientX, from = target.getBoundingClientRect().width, max = available();
    grip.setPointerCapture(event.pointerId);
    const move = (next: PointerEvent) => setLive(clamp(from + next.clientX - start, max));
    const release = (next: PointerEvent) => {
      grip.removeEventListener('pointermove', move); grip.removeEventListener('pointerup', release); grip.removeEventListener('pointercancel', release);
      setLive(null); onWidth(clamp(from + next.clientX - start, max));
    };
    grip.addEventListener('pointermove', move); grip.addEventListener('pointerup', release); grip.addEventListener('pointercancel', release);
  };
  const isSvg = /\.svg(?:[?#]|$)/i.test(source) || /\.svg(?:[?#]|$)/i.test(url);
  const shown = live ?? (width > 0 ? width : 0);
  const hasFixedWidth = shown > 0;
  return <span ref={frame} className={`lex-image-frame${isSvg ? ' is-svg' : ''}${hasFixedWidth ? ' has-fixed-width' : ''}${selected && editable ? ' is-selected' : ''}`}>
    <img ref={image} src={url} alt={alt} loading="lazy" draggable="false" style={shown ? { width: shown } : (isSvg ? { width: '100%' } : undefined)}
      onDoubleClick={() => { if (editable && width > 0) onWidth(0); }} />
    {selected && editable && <>
      <span className="lex-image-bar" role="toolbar" aria-label="이미지 크기">
        {IMAGE_PRESETS.map(preset => <button key={preset.label} type="button" onMouseDown={event => event.preventDefault()}
          onClick={event => { event.stopPropagation(); onWidth(preset.width < 0 ? available() : preset.width); }}
          aria-pressed={preset.width > 0 ? width === preset.width : preset.width === 0 && width === 0}>{preset.label}</button>)}
        {shown > 0 && <span className="lex-image-size">{shown}px</span>}
      </span>
      <span className="lex-image-grip" title="끌어서 크기 조절" aria-label="끌어서 이미지 크기 조절" onPointerDown={grab}>
        <svg viewBox="0 0 12 12" width="10" height="10" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" aria-hidden="true"><path d="M10.5 3.5L3.5 10.5M10.5 7.5L7.5 10.5" /></svg>
      </span>
    </>}
  </span>;
}
export const FRAME_LINK = /^(?:https?|mailto|tel):/i;
function HTMLFrame({ path, label }: { path: string; label: string }) {
  const features = useContext(EditorContext), frame = useRef<HTMLIFrameElement>(null);
  const url = 'aisframe://localhost' + `${features.noteDirectory}/${path}`.split('/').map(encodeURIComponent).join('/');
  const [height, setHeight] = useState(() => frameHeight(url));
  const lastInput = useRef(0), lastOpen = useRef(0);
  useEffect(() => {
    const input = (event: Event) => { if (event.isTrusted) lastInput.current = Date.now(); };
    const message = (event: MessageEvent) => {
      if (event.source !== frame.current?.contentWindow || !event.data || typeof event.data !== 'object') return;
      if (typeof event.data.aisframeHeight === 'number' && Number.isFinite(event.data.aisframeHeight)) setHeight(frameHeight(url, event.data.aisframeHeight));
      // 8.7: only http:, https:, mailto: and tel: open, after trimming; anything else is dropped.
      const link = typeof event.data.aisframeOpen === 'string' ? event.data.aisframeOpen.trim() : null, now = Date.now();
      if (typeof link === 'string' && link.length <= 8192 && FRAME_LINK.test(link) && document.activeElement === frame.current
        && (navigator.userActivation?.isActive || now - lastInput.current < 1500) && now - lastOpen.current >= 1000) {
        lastOpen.current = now; void openEditorLink(link, features).catch(() => {});
      }
    };
    addEventListener('pointerdown', input, true); addEventListener('keydown', input, true); addEventListener('message', message);
    return () => { removeEventListener('pointerdown', input, true); removeEventListener('keydown', input, true); removeEventListener('message', message); };
  }, [url, features]);
  return <span className="ais-html-frame"><span className="ais-html-frame-header"><strong>{label || path}</strong>
    <button className="ais-html-frame-ask" data-rel={path} data-label={label} hidden>AI에게 묻기</button>
    <button className="ais-html-frame-open" onClick={() => void openEditorLink(path, features)}>브라우저로 열기</button></span>
    <iframe ref={frame} src={url} title={label || path} sandbox="allow-scripts" referrerPolicy="no-referrer" loading="lazy" className="ais-html-frame-view" style={{ height }} /></span>;
}
