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

// x, y는 마우스가 있는 「줄」의 오른쪽 끝 자리(AI, 코멘트, ⋯ 버튼이 줄마다 한 벌 뜬다). right는 마우스가 있는 칸의 오른쪽 가장자리(열 너비 손잡이용)다.
type Cell = { key: string; x: number; y: number; right: number; tableRight: number; tableTop: number; tableBottom: number };
// 줄 단위 버튼은 마우스가 어느 열에 있는지 알 수 없으므로, 열 작업은 커서가 놓인 칸의 열에 한다. 그 표 안에 커서가 없으면 null이다.
function $caretCellIn(table: TableNode): TableCellNode | null {
  const selected = $getSelection(), at = $isRangeSelection(selected) || $isTableSelection(selected) ? selected.anchor.getNode() : null;
  const found = at ? ($isTableCellNode(at) ? at : at.getParents().find($isTableCellNode)) : undefined;
  return found && found.getParents().find($isTableNode)?.is(table) ? found : null;
}
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
  const rowPinned = useRef(false);
  const rowCloseTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const cancelRowClose = () => { if (rowCloseTimer.current) clearTimeout(rowCloseTimer.current); rowCloseTimer.current = null; };
  const closeRowSoon = () => { cancelRowClose(); if (!rowPinned.current) rowCloseTimer.current = setTimeout(() => { setRowCard(null); rowCloseTimer.current = null; }, 180); };
  useEffect(() => () => cancelRowClose(), []);
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
      // 줄 작업은 버튼이 뜬 줄(target의 줄)에, 열 작업은 커서가 놓인 칸의 열에 한다.
      const column = $caretCellIn(table);
      if (name.endsWith('-col') && !column) return;
      (name.endsWith('-col') ? column! : target).selectStart();
      if (name === 'before-row' || name === 'after-row') $insertTableRowAtNode(target, name === 'after-row');
      if (name === 'remove-row') $deleteTableRowAtSelection();
      if (column && (name === 'before-col' || name === 'after-col')) {
        const widths = [...(table.getColWidths() || column.getParent()!.getChildren().map(node => editor.getElementByKey(node.getKey())!.getBoundingClientRect().width))];
        const index = column.getIndexWithinParent(), insertion = index + (name === 'after-col' ? 1 : 0);
        $insertTableColumnAtNode(column, name === 'after-col'); widths.splice(insertion, 0, widths[index]); table.setColWidths(widths);
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
      editor.update(() => { $addUpdateTag('ais:repair');
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
    // The document can already be in the DOM before this listener is installed.
    rafId = requestAnimationFrame(() => { if (active && editor.getRootElement()) collect(); });
    const over = (event: MouseEvent) => {
      const container = outer();
      if (!root || !container) return;
      const element = (event.target as Element).closest<HTMLTableCellElement>('td,th'); if (!element || !root.contains(element)) return;
      const tableElem = element.closest('table');
      if (!tableElem) return;
      const base = container.getBoundingClientRect(), rect = element.getBoundingClientRect(), table = tableElem.getBoundingClientRect();
      const row = (element.closest('tr') ?? element).getBoundingClientRect(); // 버튼은 칸이 아니라 줄에 붙인다. 마우스가 같은 줄의 다른 열로 가도 제자리에 있다.
      editor.read(() => {
        const node = $getNearestNodeFromDOMNode(element);
        if ($isTableCellNode(node)) setCell({ key: node.getKey(), x: row.right - base.left - 28, y: row.top - base.top + 2, right: rect.right - base.left,
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
  }, [editor, open, props.askDocId]);
  const operations = [['before-row', '위에 행 추가'], ['after-row', '아래에 행 추가'], ['remove-row', '행 삭제'], ['before-col', '커서 열 왼쪽에 추가'], ['after-col', '커서 열 오른쪽에 추가'], ['remove-col', '커서 열 삭제'], ['fit', '표 너비 화면에 맞춤'], ['markdown', '표 복사 — 마크다운'], ['tsv', '표 복사 — Slack과 시트'], ['remove-table', '표 삭제']];
  // 메뉴가 열려 있을 때만 읽는다. 커서가 이 표 안에 없으면 열 작업 항목은 눌러도 할 일이 없으므로 꺼 둔다.
  const hasCaret = open && cell ? editor.getEditorState().read(() => { const table = $getNodeByKey(cell.key)?.getParents().find($isTableNode); return !!table && !!$caretCellIn(table); }) : false;
  return <>
    {cell && <><button className="lex-cell-menu-btn" title="표 편집" aria-label="표 편집" style={{ left: cell.x, top: cell.y }} onMouseDown={event => event.preventDefault()} onClick={() => setOpen(!open)}>⋯</button>
      <div className="lex-col-grip" style={{ left: cell.right - 3, top: cell.y - 2, height: 28 }} onPointerDown={event => resize(event, false)} />
      <div className="lex-table-grip" style={{ left: cell.tableRight - 3, top: cell.tableTop, height: cell.tableBottom - cell.tableTop }} onPointerDown={event => resize(event, true)} onDoubleClick={() => operation('fit')} /></>}
    {open && cell && <div className="lex-table-menu" style={{ left: cell.x, top: cell.y + 26 }}>{operations.map(([id, title]) => {
      const needsCaret = id.endsWith('-col') && !hasCaret;
      return <button key={id} disabled={needsCaret} title={needsCaret ? '열 작업은 표 안의 칸을 눌러 커서를 둔 뒤에 쓸 수 있어요' : undefined} onClick={() => operation(id)}>{title}</button>;
    })}</div>}
    {dots.map(dot => <div className={`lex-row-dot lex-rownote-dot${dot.asks.length ? ' is-ask' : ''}${dot.ids.length ? ' is-comment' : ''}`} key={dot.key} style={{ left: dot.x, top: dot.y }} onMouseEnter={() => { cancelRowClose(); if (!rowPinned.current) setRowCard(dot.key); }} onMouseLeave={closeRowSoon}>
      <button aria-label={dot.ids.length ? '이 줄의 코멘트' : '이 줄의 문답'} onClick={() => { if (dot.ids.length) props.features?.openComment?.(dot.ids[0]); }}>●</button>
      {rowCard === dot.key && <div className="comment-card">{dot.ids.map(id => <p key={id}>{props.features?.comments?.[id] || '이 코멘트를 못 찾았어요. 표시만 남아 있어요.'}</p>)}
        {dot.asks.map(id => <AskHoverCard key={id} docId={props.askDocId || ''} turnId={id} onFollowUp={props.onAskFollowUp} onClose={() => { cancelRowClose(); rowPinned.current = false; setRowCard(null); }} onPin={() => { cancelRowClose(); rowPinned.current = true; }} pinned={false} />)}</div>}
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
