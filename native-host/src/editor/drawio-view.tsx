import { useLayoutEffect, useRef, useState } from 'react';
import type { KeyboardEvent, PointerEvent } from 'react';

type Size = { width: number; height: number };
type Point = { x: number; y: number };
type View = { scale: number | null; offset: Point };

const FIT: View = { scale: null, offset: { x: 0, y: 0 } };
const ZOOM_STEP = 1.25;
const MAX_SCALE = 2;
const ARROW_STEP = 48;

function limitOffset(offset: Point, image: Size, canvas: Size, scale: number): Point {
  const x = Math.max(0, (image.width * scale - canvas.width) / 2);
  const y = Math.max(0, (image.height * scale - canvas.height) / 2);
  return { x: Math.max(-x, Math.min(x, offset.x)), y: Math.max(-y, Math.min(y, offset.y)) };
}

export function DrawioView({ src, alt, onClose }: { src: string; alt: string; onClose(): void }) {
  const canvasRef = useRef<HTMLSpanElement>(null);
  const drag = useRef<(Point & { id: number }) | null>(null);
  const [canvas, setCanvas] = useState<Size>({ width: 0, height: 0 });
  const [image, setImage] = useState<Size | null>(null);
  const [failed, setFailed] = useState(false);
  const [view, setView] = useState(FIT);

  useLayoutEffect(() => {
    const element = canvasRef.current;
    if (!element) return;
    const measure = () => setCanvas({ width: element.clientWidth, height: element.clientHeight });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    element.focus({ preventScroll: true });
    return () => observer.disconnect();
  }, []);

  const fit = image && Math.min(1, canvas.width / image.width, canvas.height / image.height);
  const scale = Math.max(fit || 0, view.scale ?? 0);
  const offset = image ? limitOffset(view.offset, image, canvas, scale) : FIT.offset;
  const canPan = Boolean(image && fit &&
    (image.width * scale - canvas.width > 0.5 || image.height * scale - canvas.height > 0.5));

  function zoom(factor: number) {
    if (!image || !fit) return;
    setView(current => {
      const previous = Math.max(fit, current.scale ?? fit);
      const next = Math.min(MAX_SCALE, previous * factor);
      if (next <= fit) return FIT;
      const center = limitOffset(current.offset, image, canvas, previous);
      const ratio = next / previous;
      return { scale: next, offset: limitOffset({ x: center.x * ratio, y: center.y * ratio }, image, canvas, next) };
    });
  }

  function pan(x: number, y: number) {
    if (!image || !fit) return;
    setView(current => {
      const scale = Math.max(fit, current.scale ?? fit);
      const from = limitOffset(current.offset, image, canvas, scale);
      return { ...current, offset: limitOffset({ x: from.x + x, y: from.y + y }, image, canvas, scale) };
    });
  }

  function startDrag(event: PointerEvent<HTMLSpanElement>) {
    if (!canPan || event.button !== 0 || drag.current) return;
    event.preventDefault();
    event.currentTarget.focus({ preventScroll: true });
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = { id: event.pointerId, x: event.clientX, y: event.clientY };
  }

  function moveDrag(event: PointerEvent<HTMLSpanElement>) {
    const from = drag.current;
    if (!from || from.id !== event.pointerId) return;
    drag.current = { id: from.id, x: event.clientX, y: event.clientY };
    pan(event.clientX - from.x, event.clientY - from.y);
  }

  function panWithArrow(event: KeyboardEvent<HTMLSpanElement>) {
    if (!canPan || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    switch (event.key) {
      case 'ArrowLeft': pan(ARROW_STEP, 0); break;
      case 'ArrowRight': pan(-ARROW_STEP, 0); break;
      case 'ArrowUp': pan(0, ARROW_STEP); break;
      case 'ArrowDown': pan(0, -ARROW_STEP); break;
      default: return;
    }
    event.preventDefault();
    event.stopPropagation();
  }

  return <span className="drawio-view" role="group" aria-label="draw.io 자세히 보기" onKeyDownCapture={event => {
    if (event.key !== 'Escape') return;
    event.preventDefault();
    event.stopPropagation();
    onClose();
  }}>
    <span className="drawio-view-bar" role="group" aria-label="다이어그램 보기 도구">
      <button type="button" aria-label="축소" disabled={!fit || scale <= fit} onClick={() => zoom(1 / ZOOM_STEP)}>−</button>
      <output className="drawio-view-scale" aria-label="배율">{scale > 0 ? `${Math.round(scale * 100)}%` : '–'}</output>
      <button type="button" aria-label="확대" disabled={!fit || scale >= MAX_SCALE} onClick={() => zoom(ZOOM_STEP)}>+</button>
      <button type="button" disabled={!fit || view.scale === null} onClick={() => setView(FIT)}>전체 맞춤</button>
      <button type="button" className="drawio-view-close" onClick={onClose}>닫기</button>
    </span>
    <span ref={canvasRef} className={`drawio-view-canvas${canPan ? ' is-pannable' : ''}`}
      tabIndex={0} role="region" aria-label={alt || '다이어그램'} onKeyDownCapture={panWithArrow}
      onPointerDown={startDrag} onPointerMove={moveDrag} onLostPointerCapture={event => {
        if (drag.current?.id === event.pointerId) drag.current = null;
      }}>
      {failed ? <span className="lex-image-missing" role="alert">그림 파일을 불러올 수 없어요.</span> :
        <img src={src} alt={alt} draggable={false}
          style={image && scale > 0 ? {
            width: image.width * scale,
            transform: `translate(-50%, -50%) translate(${offset.x}px, ${offset.y}px)`,
          } : { visibility: 'hidden' }}
          onLoad={event => {
            const { naturalWidth, naturalHeight } = event.currentTarget;
            if (naturalWidth > 0 && naturalHeight > 0) setImage({ width: naturalWidth, height: naturalHeight });
          }}
          onError={() => setFailed(true)} />}
    </span>
  </span>;
}
