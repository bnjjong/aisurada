import { useEffect, useLayoutEffect, useRef, useState } from 'react';
import type { KeyboardEvent, PointerEvent } from 'react';
import type { WorkspaceController } from '../workspace/controller.ts';
import type { WorkspaceState } from '../workspace/types.ts';
import { activeTab, parseSubTab, tabRootId } from './routes.ts';
import { holdSubChain, subNoteAt } from './subNotes.ts';
import { displayTitle } from '../ui/model.ts';
import { BrandMark } from '../ui/BrandMark.tsx';
import { useAskState } from '@claude-plug/state';

interface Drag {
  id: string;
  x: number;
  dragging: boolean;
  centers: number[];
}

export function tabTitle(id: string, state: WorkspaceState): string {
  if (id === 'term:main') return '터미널';
  if (id.startsWith('term:')) {
    const termId = id.slice(5);
    const session = (state as any).sessions?.find?.((s: any) => s.id === termId);
    const label = session?.title || (termId.length > 8 ? termId.slice(0, 8) : termId);
    return `터미널 · ${label}`;
  }
  if (id.startsWith('ext:')) return id.slice(4).split('/').pop() || id.slice(4);
  const sub = parseSubTab(id);
  // 후속 노트 탭: 제목만 돌려준다. ↳ 표시는 탭 바가 따로 붙인다. 목록을 아직 읽지 못했으면 이름 대신 「후속 노트」다.
  if (sub) return subNoteAt(state, sub.root, sub.ids)?.title || '후속 노트';
  return state.workspace?.notes.find(note => note.id === id)?.title || id;
}

/**
 * 탭의 「AI 답이 왔어요」 점의 설명 글. 점이 없으면 null이다.
 * 안 본 답의 열쇠는 물음을 던진 노트의 id이고 값은 그 사슬의 최상위 노트 id다(4.10).
 * 후속 노트의 답은 그 후속 노트 탭에 「AI 답이 왔어요」로 알리고, 최상위 노트 탭에도 「하위 노트에 AI 답이 왔어요」로 함께 알린다.
 */
export function tabAskDot(id: string, unseen: Record<string, string>): string | null {
  const sub = parseSubTab(id);
  if (sub) return unseen[sub.ids[sub.ids.length - 1]] === sub.root ? 'AI 답이 왔어요' : null;
  if (!Object.values(unseen).includes(id)) return null;
  return unseen[id] === id ? 'AI 답이 왔어요' : '하위 노트에 AI 답이 왔어요';
}

/** 후속 노트 탭에 마우스를 올리면 보이는 설명. 최상위 노트부터 이 노트까지 제목을 › 로 잇는다. */
export function tabTooltip(id: string, state: WorkspaceState): string {
  const sub = parseSubTab(id);
  if (!sub) return tabTitle(id, state);
  const titles = [state.workspace?.notes.find(note => note.id === sub.root)?.title || sub.root];
  sub.ids.forEach((_, n) => titles.push(subNoteAt(state, sub.root, sub.ids.slice(0, n + 1))?.title || '후속 노트'));
  return titles.join(' › ');
}

export function TabBar({ controller, state }: { controller: WorkspaceController; state: WorkspaceState }) {
  const active = activeTab(state.route);
  // 4.10: the key of an unseen answer maps to the root note id; the root note's own tab shows the dot.
  const unseen = useAskState((ask) => ask.unseen);
  const buttons = useRef(new Map<string | null, HTMLButtonElement>());
  const list = useRef<HTMLDivElement>(null);
  const drag = useRef<Drag | null>(null);
  const suppressClick = useRef<string | null>(null);
  const [dragging, setDragging] = useState<string | null>(null);
  const ids: (string | null)[] = [
    null,
    ...new Set(
      state.tabs.openIds.filter((id) => {
        const root = tabRootId(id);
        return (
          root === null ||
          id === active ||
          (state.workspace?.notes.some((n) => n.id === root) ?? true)
        );
      }),
    ),
  ];

  // 후속 노트 탭의 이름은 목록을 읽어야 알 수 있다. 앱을 다시 켠 직후에도 이름이 보이도록 탭이 열려 있는 동안 그 사슬을 읽어 둔다.
  const subTabs = state.tabs.openIds.filter((id) => parseSubTab(id) !== null).join('\n');
  const vaultPath = state.workspace?.path;
  useEffect(() => {
    if (!subTabs || !vaultPath) return;
    const held = subTabs.split('\n').map((id) => {
      const sub = parseSubTab(id)!;
      const chain = holdSubChain(controller, sub.root, sub.ids);
      // 읽지 못한 탭의 이유는 탭 화면이 알린다. 여기서는 이름을 못 얻을 뿐이다.
      chain.done.catch(() => {});
      return chain;
    });
    return () => held.forEach((chain) => chain.release());
  }, [controller, subTabs, vaultPath]);

  useLayoutEffect(() => {
    const item = buttons.current.get(active)?.closest('.tab-item, .tabx-item');
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
    if (event.button !== 0 || (event.target as Element).closest('.tab-close, .tabx-close')) return;
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
  return <div className="tab-bar" role="tablist" aria-label="열린 화면" ref={list} onWheel={(e) => {
    if (e.deltaY) {
      e.currentTarget.scrollLeft += e.deltaY;
    }
  }}>
    {ids.map(id => {
      const title = id === null ? '내 기록' : tabTitle(id, state);
      const isSub = id !== null && parseSubTab(id) !== null;
      const askDot = id === null ? null : tabAskDot(id, unseen);
      const note = state.workspace?.notes.find(n => n.id === id);
      return <div className={`tab-item tabx-item ${id === null ? 'tab-main tabx-main' : ''} ${active === id ? 'tab-selected tabx-selected' : ''} ${dragging === id && id !== null ? 'tab-dragging' : ''}`}
        key={id ?? '__main__'} data-tab-id={id ?? undefined} aria-selected={active === id}
        onClick={event => {
          if ((event.target as Element).closest('.tab-close, .tabx-close')) return;
          if (id !== null && suppressClick.current === id) { suppressClick.current = null; return; }
          controller.selectTab(id);
        }}
        onPointerDown={id === null ? undefined : event => pointerDown(event, id)} onPointerMove={pointerMove} onPointerUp={pointerUp}
        onPointerCancel={() => { drag.current = null; setDragging(null); }}>
        <button role="tab" aria-selected={active === id} aria-controls="screen-content" tabIndex={active === id ? 0 : -1}
          className="tab-button" title={id === null ? title : tabTooltip(id, state)} data-tab-id={id ?? undefined} ref={element => { if (element) buttons.current.set(id, element); else buttons.current.delete(id); }}
          onKeyDown={event => keydown(event, id)}>{id === null && <BrandMark className="tab-mark" state="still" />}{isSub && <span className="tab-sub-mark" aria-hidden="true">↳</span>}<span className="tab-label">{note && state.prefs.titleDate ? <><small>{note.date}</small>{displayTitle(note,true).replace(/^\d{8}-?/,'')}</> : title}</span>{askDot && <span className="tab-ask-dot" role="img" aria-label={askDot} />}</button>
        {id !== null && <button className="tab-close tabx-close" tabIndex={-1} aria-label={`${title} 탭 닫기`} onClick={() => controller.closeTab(id)}>×</button>}
      </div>;
    })}
  </div>;
}
