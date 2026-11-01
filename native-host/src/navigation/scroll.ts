// Kept per controller (and therefore per window), never in browser storage.
export class NoteScrollMemory {
  private positions = new Map<string, number>();
  get(id: string): number | undefined { return this.positions.get(id); }
  set(id: string, top: number) { this.positions.set(id, top); }
}

export interface ScrollSurface {
  scrollTop: number;
  addEventListener(type: string, listener: EventListener, options?: AddEventListenerOptions | boolean): void;
  removeEventListener(type: string, listener: EventListener, options?: EventListenerOptions | boolean): void;
}
export interface ScrollClock {
  now(): number;
  frame(callback: FrameRequestCallback): number;
  cancel(id: number): void;
}

export function rememberNoteScroll(surface: ScrollSurface, id: string, memory: NoteScrollMemory, clock: ScrollClock): () => void {
  const position = memory.get(id);
  const until = clock.now() + 1500;
  let holding = position !== undefined, frame: number | undefined;
  const restore = () => {
    if (!holding) return;
    surface.scrollTop = position!;
    if (clock.now() < until) frame = clock.frame(restore);
    else holding = false;
  };
  const release: EventListener = () => { holding = false; if (frame !== undefined) clock.cancel(frame); };
  const scrolled: EventListener = () => { if (!holding) memory.set(id, surface.scrollTop); };
  surface.addEventListener('scroll', scrolled, { passive: true });
  for (const type of ['wheel', 'pointerdown', 'keydown']) surface.addEventListener(type, release, { passive: true });
  restore();
  return () => {
    if (frame !== undefined) clock.cancel(frame);
    surface.removeEventListener('scroll', scrolled);
    for (const type of ['wheel', 'pointerdown', 'keydown']) surface.removeEventListener(type, release);
  };
}
