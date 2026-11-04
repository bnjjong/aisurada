import { useEffect, useRef, useState } from 'react';
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext';
import { $getClipboardDataFromSelection } from '@lexical/clipboard';
import { $isTableNode, $isTableCellNode, $isTableSelection, $createTableSelectionFrom, $insertTableRowAtNode, $insertTableColumnAtNode,
  $deleteTableColumnAtSelection, $deleteTableRowAtSelection, TableNode } from '@lexical/table';
import type { TableCellNode, TableRowNode } from '@lexical/table';
import { $getRoot, $getSelection, $setSelection, $getNodeByKey, $getNearestNodeFromDOMNode, $isElementNode, $createParagraphNode,
  $isRangeSelection, COPY_COMMAND, CUT_COMMAND, KEY_DOWN_COMMAND, COMMAND_PRIORITY_HIGH, $addUpdateTag } from 'lexical';
import type { LexicalNode } from 'lexical';
import type { EditorProps } from './Editor.tsx';
import { $exportSelection, $tableMarkdown, $tableTSV } from './markdown.ts';
import { OPEN_COMMENT_COMMAND, OPEN_ASK_COMMAND } from './tools.tsx';
import { AskHoverCard } from '@claude-plug/ask';

type Cell = { key: string; x: number; y: number; right: number; tableRight: number; tableTop: number; tableBottom: number };
export function tablesOnlyJSON(raw?: string): string | undefined {
  if (!raw) return undefined;
  try {
    const value = JSON.parse(raw);
    const collect = (nodes: { type?: string; children?: never[] }[]): unknown[] => nodes.flatMap(node => node.type === 'table' ? [node] : collect(node.children || []));
    return JSON.stringify({ ...value, nodes: collect(value.nodes || []) });
  } catch { return undefined; }
}
export function TableTools(props: EditorProps) {
  const [editor] = useLexicalComposerContext();
  const [cell, setCell] = useState<Cell | null>(null), [open, setOpen] = useState(false), copyMode = useRef(false);
  const [dots, setDots] = useState<{ key: string; x: number; y: number; ids: string[]; asks: string[] }[]>([]), [rowCard, setRowCard] = useState<string | null>(null);
  const fixedWidths = useRef(new Map<string, number>());
  const scaled = useRef(new Set<string>());
  const stopResize = useRef<(() => void) | null>(null);
  const outer = () => editor.getRootElement()?.closest<HTMLElement>('.lex-editor') ?? null;
  function tablesFromSelection() {
    const selection = $getSelection(), found = new Map<string, TableNode>();
    for (const node of selection?.getNodes() || []) {
      const table = $isTableNode(node) ? node : node.getParents().find($isTableNode);
      if (table) found.set(table.getKey(), table);
    }
    return [...found.values()];
  }
  const copyTable = (tsv: boolean) => {
    if (!cell) return;
    editor.update(() => { const target = $getNodeByKey<TableCellNode>(cell.key), table = target?.getParents().find($isTableNode); table?.select(0, table.getChildrenSize()); }, { discrete: true });
    copyMode.current = tsv; editor.focus(() => document.execCommand('copy'));
  };
  function operation(name: string) {
    if (name === 'markdown' || name === 'tsv') { copyTable(name === 'tsv'); setOpen(false); return; }
    editor.update(() => {
      const target = cell && $getNodeByKey<TableCellNode>(cell.key); if (!$isTableCellNode(target)) return;
      const table = target.getParents().find($isTableNode)!;
      target.selectStart();
      if (name === 'before-row' || name === 'after-row') $insertTableRowAtNode(target, name === 'after-row');
      if (name === 'remove-row') $deleteTableRowAtSelection();
      if (name === 'before-col' || name === 'after-col') {
        const widths = [...(table.getColWidths() || target.getParent()!.getChildren().map(node => editor.getElementByKey(node.getKey())!.getBoundingClientRect().width))];
        const index = target.getIndexWithinParent(), insertion = index + (name === 'after-col' ? 1 : 0);
        $insertTableColumnAtNode(target, name === 'after-col'); widths.splice(insertion, 0, widths[index]); table.setColWidths(widths);
      }
      if (name === 'remove-col') $deleteTableColumnAtSelection();
      if (name === 'fit') {
        const root = editor.getRootElement();
        if (!root) return;
        const width = root.clientWidth, widths = table.getColWidths() || Array(table.getFirstChild<TableRowNode>()!.getChildrenSize()).fill(1);
        const total = widths.reduce((a, b) => a + b, 0); table.setColWidths(widths.map(n => n / total * width)); fixedWidths.current.delete(table.getKey());
        const element = editor.getElementByKey(table.getKey()); if (element) element.style.width = '100%';
      }
      if (name === 'remove-table') { const next = $createParagraphNode(); table.insertAfter(next); table.remove(); next.selectStart(); }
    }); setOpen(false);
  }
  function resize(event: React.PointerEvent, whole: boolean) {
    if (!cell) return; event.preventDefault();
    const key = cell.key, start = event.clientX;
    let tableKey = '', widths: number[] = [], index = 0;
    editor.read(() => {
      const target = $getNodeByKey<TableCellNode>(key), table = target?.getParents().find($isTableNode); if (!target || !table) return;
      tableKey = table.getKey(); index = target.getIndexWithinParent();
      widths = table.getFirstChild<TableRowNode>()!.getChildren().map(node => editor.getElementByKey(node.getKey())!.getBoundingClientRect().width);
    });
    const total = widths.reduce((a, b) => a + b, 0); if (!total || !whole && index >= widths.length - 1) return;
    const move = (event: PointerEvent) => {
      const next = [...widths], delta = event.clientX - start;
      if (whole) { const scaled = Math.max(total + delta, 48 / Math.min(...widths.map(n => n / total))); for (let i = 0; i < next.length; i++) next[i] = widths[i] / total * scaled; }
      else { const change = Math.max(48 - next[index], Math.min(delta, next[index + 1] - 48)); next[index] += change; next[index + 1] -= change; }
      editor.update(() => { const table = $getNodeByKey(tableKey); if ($isTableNode(table)) table.setColWidths(next); });
      const value = next.reduce((a, b) => a + b, 0), element = editor.getElementByKey(tableKey);
      fixedWidths.current.set(tableKey, value); if (element) element.style.width = value + 'px';
    };
    stopResize.current?.();
    const stop = () => { document.removeEventListener('pointermove', move); document.removeEventListener('pointerup', stop); stopResize.current = null; };
    stopResize.current = stop;
    document.addEventListener('pointermove', move); document.addEventListener('pointerup', stop, { once: true });
  }
  useEffect(() => {
    let active = true;
    let rafId = 0;
    const root = editor.getRootElement();
    if (!root) return;
    const collect = () => {
      const container = outer();
      if (!container || !root) return;
      const base = container.getBoundingClientRect(), next: typeof dots = [];
      root.querySelectorAll<HTMLTableRowElement>('tr').forEach(row => {
        const ids = [...new Set([...row.querySelectorAll<HTMLElement>('[data-comment-id]')].map(mark => mark.dataset.commentId!))];
        // 17.4: with askDocId the row also draws a dot for its question marks and its card carries them.
        const asks = props.askDocId ? [...new Set([...row.querySelectorAll<HTMLElement>('[data-ask-id]')].map(mark => mark.dataset.askId!))] : [];
        if (!ids.length && !asks.length) return;
        const rect = row.getBoundingClientRect(); let key = '';
        editor.read(() => { key = $getNearestNodeFromDOMNode(row)?.getKey() || ''; });
        if (key) next.push({ key, ids, asks, x: rect.left - base.left - 14, y: rect.top - base.top + 8 });
      });
      setDots(next);
    };
    const scale = () => {
      if (!root) return;
      const keys: string[] = [];
      editor.read(() => { const find = (node: LexicalNode) => { if ($isTableNode(node) && !scaled.current.has(node.getKey())) keys.push(node.getKey()); if ($isElementNode(node)) node.getChildren().forEach(find); }; find($getRoot()); });
      if (!keys.length) return;
      editor.update(() => { $addUpdateTag('wl:repair');
      const walk = (node: LexicalNode) => {
        if ($isTableNode(node) && !scaled.current.has(node.getKey())) {
          scaled.current.add(node.getKey()); const widths = node.getColWidths();
          if (widths?.length) { const sum = widths.reduce((a, b) => a + b, 0); node.setColWidths(widths.map(n => n / sum * root.clientWidth)); }
        }
        if ($isElementNode(node)) node.getChildren().forEach(walk);
      }; walk($getRoot()); });
    };
    scale();
    const update = editor.registerUpdateListener(() => {
      cancelAnimationFrame(rafId);
      rafId = requestAnimationFrame(() => {
        if (active && editor.getRootElement()) { collect(); scale(); }
      });
    });
    const over = (event: MouseEvent) => {
      const container = outer();
      if (!root || !container) return;
      const element = (event.target as Element).closest<HTMLTableCellElement>('td,th'); if (!element || !root.contains(element)) return;
      const tableElem = element.closest('table');
      if (!tableElem) return;
      const base = container.getBoundingClientRect(), rect = element.getBoundingClientRect(), table = tableElem.getBoundingClientRect();
      editor.read(() => {
        const node = $getNearestNodeFromDOMNode(element);
        if ($isTableCellNode(node)) setCell({ key: node.getKey(), x: rect.right - base.left - 28, y: rect.top - base.top + 2, right: rect.right - base.left,
          tableRight: table.right - base.left, tableTop: table.top - base.top, tableBottom: table.bottom - base.top });
      });
    };
    const outside = (event: PointerEvent) => { if (!(event.target as Element).closest('.lex-table-menu,.lex-cell-menu-btn')) setOpen(false); };
    const escape = (event: KeyboardEvent) => { if (event.key === 'Escape' && !event.defaultPrevented && open) { event.preventDefault(); event.stopPropagation(); setOpen(false); } };
    root.addEventListener('mouseover', over); document.addEventListener('pointerdown', outside); document.addEventListener('keydown', escape, true);
    const copy = (event: ClipboardEvent | null, cut = false) => {
      const selected = $getSelection(), tables = tablesFromSelection();
      if (!selected || !tables.length || !event?.clipboardData) return false;
      event.preventDefault(); const selectedCells = $isTableSelection(selected) ? new Set(selected.getNodes().filter($isTableCellNode).map(node => node.getKey())) : undefined;
      const data = $getClipboardDataFromSelection(selected);
      data['text/plain'] = copyMode.current ? tables.map(table => $tableTSV(table, selectedCells)).join('\n')
        : $isTableSelection(selected) ? tables.map(table => $tableMarkdown(table, true, selected)).join('\n\n') : $exportSelection(selected);
      if (copyMode.current && !$isTableSelection(selected)) {
        const html = new DOMParser().parseFromString(data['text/html'] || '', 'text/html');
        data['text/html'] = [...html.querySelectorAll('table')].map(table => table.outerHTML).join('');
        data['application/x-lexical-editor'] = tablesOnlyJSON(data['application/x-lexical-editor']);
      }
      for (const [type, value] of Object.entries(data)) if (value) event.clipboardData.setData(type, value);
      copyMode.current = false;
      if (cut) {
        if ($isTableSelection(selected)) selected.getNodes().filter($isTableCellNode).forEach(cell => cell.clear().append($createParagraphNode()));
        else if ($isRangeSelection(selected)) selected.removeText();
      }
      return true;
    };
    const copyOff = editor.registerCommand(COPY_COMMAND, event => copy(event as ClipboardEvent), COMMAND_PRIORITY_HIGH);
    const cutOff = editor.registerCommand(CUT_COMMAND, event => copy(event as ClipboardEvent, true), COMMAND_PRIORITY_HIGH);
    const keyOff = editor.registerCommand(KEY_DOWN_COMMAND, event => {
      if (!event || event.altKey || !event.shiftKey || !(/Mac/.test(navigator.platform) ? event.metaKey : event.ctrlKey) || event.code !== 'KeyC' || !tablesFromSelection().length) return false;
      event.preventDefault(); copyMode.current = true; document.execCommand('copy'); return true;
    }, COMMAND_PRIORITY_HIGH);
    return () => { active = false; cancelAnimationFrame(rafId); stopResize.current?.(); update(); copyOff(); cutOff(); keyOff(); root.removeEventListener('mouseover', over); document.removeEventListener('pointerdown', outside); document.removeEventListener('keydown', escape, true); };
  }, [editor, open]);
  const operations = [['before-row', '위에 행 추가'], ['after-row', '아래에 행 추가'], ['remove-row', '행 삭제'], ['before-col', '왼쪽에 열 추가'], ['after-col', '오른쪽에 열 추가'], ['remove-col', '열 삭제'], ['fit', '표 너비 화면에 맞춤'], ['markdown', '표 복사 — 마크다운'], ['tsv', '표 복사 — Slack과 시트'], ['remove-table', '표 삭제']];
  return <>
    {cell && <><button className="lex-cell-menu-btn" title="표 편집" aria-label="표 편집" style={{ left: cell.x, top: cell.y }} onMouseDown={event => event.preventDefault()} onClick={() => setOpen(!open)}>⋯</button>
      <div className="lex-col-grip" style={{ left: cell.right - 3, top: cell.y - 2, height: 28 }} onPointerDown={event => resize(event, false)} />
      <div className="lex-table-grip" style={{ left: cell.tableRight - 3, top: cell.tableTop, height: cell.tableBottom - cell.tableTop }} onPointerDown={event => resize(event, true)} onDoubleClick={() => operation('fit')} /></>}
    {open && cell && <div className="lex-table-menu" style={{ left: cell.x, top: cell.y + 26 }}>{operations.map(([id, title]) => <button key={id} onClick={() => operation(id)}>{title}</button>)}</div>}
    {dots.map(dot => <div className="lex-row-dot" key={dot.key} style={{ left: dot.x, top: dot.y }} onMouseEnter={() => setRowCard(dot.key)} onMouseLeave={() => setRowCard(null)}>
      <button aria-label={dot.ids.length ? '이 줄의 코멘트' : '이 줄의 문답'} onClick={() => { if (dot.ids.length) props.features?.openComment?.(dot.ids[0]); }}>●</button>
      {rowCard === dot.key && <div className="comment-card">{dot.ids.map(id => <p key={id}>{props.features?.comments?.[id] || '이 코멘트를 못 찾았어요. 표시만 남아 있어요.'}</p>)}
        {dot.asks.map(id => <AskHoverCard key={id} docId={props.askDocId || ''} turnId={id} onFollowUp={props.onAskFollowUp} onClose={() => setRowCard(null)} pinned={false} />)}</div>}
    </div>)}
    {cell && props.features?.comment && <button className="lex-row-comment" style={{ left: cell.x - 35, top: cell.y }} title="이 줄에 코멘트 달기" aria-label="이 줄에 코멘트 달기" onMouseDown={event => event.preventDefault()} onClick={() => editor.update(() => {
      const target = $getNodeByKey<TableCellNode>(cell.key), table = target?.getParents().find($isTableNode), row = target?.getParent<TableRowNode>();
      if (table && row) { $setSelection($createTableSelectionFrom(table, row.getFirstChild<TableCellNode>()!, row.getLastChild<TableCellNode>()!)); editor.dispatchCommand(OPEN_COMMENT_COMMAND, undefined); }
    })}>＋</button>}
    {cell && props.onAskAi && <button className="lex-row-ask lex-row-comment ask-add-btn" style={{ left: cell.x - (props.features?.comment ? 65 : 35), top: cell.y }} title="이 줄을 AI에게 묻기" aria-label="이 줄을 AI에게 묻기" onMouseDown={event => event.preventDefault()} onClick={() => editor.update(() => {
      const target = $getNodeByKey<TableCellNode>(cell.key), table = target?.getParents().find($isTableNode), row = target?.getParent<TableRowNode>();
      if (table && row) { $setSelection($createTableSelectionFrom(table, row.getFirstChild<TableCellNode>()!, row.getLastChild<TableCellNode>()!)); editor.dispatchCommand(OPEN_ASK_COMMAND, undefined); }
    })}>AI</button>}
  </>;
}
