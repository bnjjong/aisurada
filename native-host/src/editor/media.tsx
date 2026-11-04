import { useContext, useEffect, useRef, useState } from 'react';
import { convertFileSrc } from '@tauri-apps/api/core';
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
  const path = relativePath(url);
  if (!path || !features.noteDirectory) return;
  if (/\.(html?|svg)$/i.test(path)) {
    try { if (!await features.host?.confirm('첨부를 기본 앱에서 열까요? 파일 안의 스크립트가 실행될 수 있어요.')) return; } catch { return; }
  }
  await features.host?.openFile(`${features.noteDirectory}/${path}`);
}
export function ImageView({ source, alt }: { source: string; alt: string }) {
  const features = useContext(EditorContext), path = relativePath(source);
  if (path && features.noteDirectory && /\.html?$/i.test(path)) return <HTMLFrame path={path} label={alt} />;
  const url = path && features.noteDirectory && features.host?.native ? convertFileSrc(`${features.noteDirectory}/${path}`) : source;
  return <img src={url} alt={alt} loading="lazy" draggable="false" />;
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
