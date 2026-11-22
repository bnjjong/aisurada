import { useLayoutEffect, useRef, useState } from 'react';
import type { KeyboardEvent, PointerEvent, ReactNode, RefObject } from 'react';

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

export function DrawioView({ src, alt, width, imageRef, onError, onResetWidth, children }: {
  src: string;
  alt: string;
  width: number;
  imageRef: RefObject<HTMLImageElement | null>;
  onError(): void;
  onResetWidth(): void;
  children: ReactNode;
}) {
  const canvasRef = useRef<HTMLSpanElement>(null);
  const drag = useRef<(Point & { id: number }) | null>(null);
  const [canvas, setCanvas] = useState<Size>({ width: 0, height: 0 });
  const [image, setImage] = useState<Size | null>(null);
  const [view, setView] = useState(FIT);

  useLayoutEffect(() => {
    const element = canvasRef.current;
    if (!element) return;
    const measure = () => setCanvas({ width: element.clientWidth, height: element.clientHeight });
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
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

  function handleKey(event: KeyboardEvent<HTMLSpanElement>) {
    if (event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) return;
    if (event.key === 'Escape' && view.scale !== null) {
      setView(FIT);
    } else {
      if (!canPan) return;
      switch (event.key) {
        case 'ArrowLeft': pan(ARROW_STEP, 0); break;
        case 'ArrowRight': pan(-ARROW_STEP, 0); break;
        case 'ArrowUp': pan(0, ARROW_STEP); break;
        case 'ArrowDown': pan(0, -ARROW_STEP); break;
        default: return;
      }
    }
    event.preventDefault();
    event.stopPropagation();
  }

  return <span className="drawio-view" role="group" aria-label={alt || 'draw.io 다이어그램'} onKeyDownCapture={handleKey}>
    <span className="drawio-view-bar" role="group" aria-label="다이어그램 보기 도구">
      <button type="button" aria-label="축소" disabled={!fit || scale <= fit} onClick={() => zoom(1 / ZOOM_STEP)}>−</button>
      <output className="drawio-view-scale" aria-label="배율">{scale > 0 ? `${Math.round(scale * 100)}%` : '–'}</output>
      <button type="button" aria-label="확대" disabled={!fit || scale >= MAX_SCALE} onClick={() => zoom(ZOOM_STEP)}>+</button>
      <button type="button" disabled={!fit || view.scale === null} onClick={() => setView(FIT)}>전체 맞춤</button>
    </span>
    <span className="drawio-view-media">
      <span ref={canvasRef} className={`drawio-view-canvas${canPan ? ' is-pannable' : ''}`}
        tabIndex={0} role="region" aria-label={alt || '다이어그램'}
        onDoubleClick={() => { if (view.scale === null) onResetWidth(); }}
        onPointerDown={startDrag} onPointerMove={moveDrag} onLostPointerCapture={event => {
          if (drag.current?.id === event.pointerId) drag.current = null;
        }}>
        <img ref={imageRef} src={src} alt={alt} loading="lazy" draggable={false}
          style={{
            width: width || undefined,
            transform: fit && view.scale !== null ? `translate(${offset.x}px, ${offset.y}px) scale(${scale / fit})` : undefined,
          }}
          onLoad={event => {
            const { naturalWidth, naturalHeight } = event.currentTarget;
            if (naturalWidth > 0 && naturalHeight > 0) setImage({ width: naturalWidth, height: naturalHeight });
          }}
          onError={onError} />
      </span>
      {children}
    </span>
  </span>;
}
