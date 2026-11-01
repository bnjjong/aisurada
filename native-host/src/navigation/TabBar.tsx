import { useLayoutEffect, useRef, useState } from 'react';
import type { KeyboardEvent, PointerEvent } from 'react';
import type { WorkspaceController } from '../workspace/controller.ts';
import type { WorkspaceState } from '../workspace/types.ts';
import { activeTab } from './routes.ts';
import { displayTitle } from '../ui/model.ts';

interface Drag {
  id: string;
  x: number;
  dragging: boolean;
  centers: number[];
}

export function tabTitle(id: string, state: WorkspaceState): string {
  if (id === 'term:main') return '터미널';
  if (id.startsWith('term:')) return `터미널 · ${id.slice(5)}`;
  if (id.startsWith('ext:')) return id.slice(4).split('/').pop() || id.slice(4);
  return state.workspace?.notes.find(note => note.id === id)?.title || id;
}

export function TabBar({ controller, state }: { controller: WorkspaceController; state: WorkspaceState }) {
  const active = activeTab(state.route);
  const buttons = useRef(new Map<string | null, HTMLButtonElement>());
  const list = useRef<HTMLDivElement>(null);
  const drag = useRef<Drag | null>(null);
  const suppressClick = useRef<string | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const ids: (string | null)[] = [null, ...state.tabs.openIds];

  useLayoutEffect(() => {
    const item = buttons.current.get(active)?.closest('.tab-item');
    if (!item || !list.current) return;
    const rect = item.getBoundingClientRect(), view = list.current.getBoundingClientRect();
    if (rect.left < view.left) list.current.scrollLeft += rect.left - view.left;
    else if (rect.right > view.right) list.current.scrollLeft += rect.right - view.right;
  }, [active, state.tabs.openIds]);

  const keydown = (event: KeyboardEvent<HTMLButtonElement>, id: string | null) => {
    const index = ids.indexOf(id);
    let target: number | undefined;
    if (event.key === 'ArrowLeft') target = (index - 1 + ids.length) % ids.length;
    if (event.key === 'ArrowRight') target = (index + 1) % ids.length;
    if (event.key === 'Home') target = 0;
    if (event.key === 'End') target = ids.length - 1;
    if (target !== undefined) {
      event.preventDefault(); controller.selectTab(ids[target]); buttons.current.get(ids[target])?.focus();
    } else if ((event.key === 'Delete' || event.key === 'Backspace') && id !== null) {
      event.preventDefault(); controller.closeTab(id);
      requestAnimationFrame(() => list.current?.querySelector<HTMLButtonElement>('[aria-selected="true"]')?.focus());
    }
  };
  const pointerDown = (event: PointerEvent<HTMLDivElement>, id: string) => {
    if (event.button !== 0 || (event.target as Element).closest('.tab-close')) return;
    const centers = [...list.current!.querySelectorAll<HTMLElement>('.tab-item[data-tab-id]')]
      .filter(element => element.dataset.tabId !== id)
      .map(element => { const box = element.getBoundingClientRect(); return box.left + box.width / 2; });
    drag.current = { id, x: event.clientX, dragging: false, centers };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const pointerMove = (event: PointerEvent<HTMLDivElement>) => {
    if (drag.current && !drag.current.dragging && Math.abs(event.clientX - drag.current.x) > 5) {
      drag.current.dragging = true; setDragging(drag.current.id);
    }
  };
  const pointerUp = (event: PointerEvent<HTMLDivElement>) => {
    const current = drag.current;
    if (!current) return;
    if (current.dragging) {
      controller.reorderTab(current.id, current.centers.filter(center => center < event.clientX).length);
      suppressClick.current = current.id;
    }
    drag.current = null; setDragging(null);
    if (event.currentTarget.hasPointerCapture(event.pointerId)) event.currentTarget.releasePointerCapture(event.pointerId);
  };
  return <div className="tab-bar" role="tablist" aria-label="열린 화면" ref={list}>
    {ids.map(id => {
      const title = id === null ? '내 기록' : tabTitle(id, state);
      const note = state.workspace?.notes.find(n => n.id === id);
      return <div className={`tab-item ${id === null ? 'tab-main' : ''} ${active === id ? 'tab-selected' : ''} ${dragging === id && id !== null ? 'tab-dragging' : ''}`}
        key={id ?? '__main__'} data-tab-id={id ?? undefined}
        onClick={event => {
          if ((event.target as Element).closest('.tab-close')) return;
          if (id !== null && suppressClick.current === id) { suppressClick.current = null; return; }
          controller.selectTab(id);
        }}
        onPointerDown={id === null ? undefined : event => pointerDown(event, id)} onPointerMove={pointerMove} onPointerUp={pointerUp}
        onPointerCancel={() => { drag.current = null; setDragging(null); }}>
        <button role="tab" aria-selected={active === id} aria-controls="screen-content" tabIndex={active === id ? 0 : -1}
          className="tab-button" title={title} ref={element => { if (element) buttons.current.set(id, element); else buttons.current.delete(id); }}
          onKeyDown={event => keydown(event, id)}>{id === null && <span aria-hidden="true">w.</span>}<span className="tab-label">{note && state.prefs.titleDate ? <><small>{note.date}</small>{displayTitle(note,true).replace(/^\d{8}-?/,'')}</> : title}</span></button>
        {id !== null && <button className="tab-close" tabIndex={-1} aria-label={`${title} 탭 닫기`} onClick={() => controller.closeTab(id)}>×</button>}
      </div>;
    })}
  </div>;
}
