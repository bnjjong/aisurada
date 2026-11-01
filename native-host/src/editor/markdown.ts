import {
  $convertFromMarkdownString, $convertToMarkdownString, $convertSelectionToMarkdownString,
  CODE, CHECK_LIST, HEADING, QUOTE, UNORDERED_LIST, ORDERED_LIST, LINK,
  HIGHLIGHT, TEXT_FORMAT_TRANSFORMERS,
} from '@lexical/markdown';
import type { ElementTransformer, TextMatchTransformer, Transformer } from '@lexical/markdown';
import { HeadingNode, QuoteNode } from '@lexical/rich-text';
import { ListNode, ListItemNode, $isListNode, $isListItemNode } from '@lexical/list';
import { CodeNode, CodeHighlightNode } from '@lexical/code';
import { LinkNode } from '@lexical/link';
import { MarkNode, $wrapSelectionInMarkNode } from '@lexical/mark';
import {
  TableNode, TableRowNode, TableCellNode, $createTableNode, $createTableRowNode, $createTableCellNode,
  $isTableNode, $isTableCellNode, TableCellHeaderStates,
} from '@lexical/table';
import {
  $getRoot, $setSelection, $createRangeSelection, $createParagraphNode, $createLineBreakNode, $isParagraphNode,
  $isElementNode, $isTextNode, $getState, $setState, createState, $addUpdateTag,
} from 'lexical';
import type { ElementNode, LexicalNode, BaseSelection } from 'lexical';
import {
  AnnotationNode, CalloutNode, ReferenceNode, ImageNode, RelativeLinkNode,
  $createAnnotation, $isAnnotation, $createCallout, $isCallout, $createReference, $isReference, $createImage, $isImage,
} from './nodes.tsx';
import { foldParagraphs, doubleListIndent, halveListIndent, tableCells, dividerWidths, widthHint, widthDashes, markerPlaceholders, stripMarkers } from './grammar.ts';

export const EDITOR_NODES = [HeadingNode, QuoteNode, ListNode, ListItemNode, CodeNode, CodeHighlightNode,
  TableNode, TableRowNode, TableCellNode, MarkNode, AnnotationNode, CalloutNode, ReferenceNode, ImageNode, RelativeLinkNode,
  { replace: LinkNode, with: (node: LinkNode) => new RelativeLinkNode(node.getURL(), { rel: node.getRel(), target: node.getTarget(), title: node.getTitle() }), withKlass: RelativeLinkNode }];
export const pendingState = createState('wlPending', { parse: value => value === '/' || value === '-' ? value : '' });
const dividerState = createState('wlDivider', { parse: value => value === true });

const PARAGRAPH: ElementTransformer = {
  type: 'element', dependencies: [], regExp: /$^/,
  replace: () => false,
  export: (node, children) => {
    if (!$isParagraphNode(node) || $isCallout(node.getParent()) || $isTableCellNode(node.getParent())) return null;
    const text = children(node);
    return text.split('\n').map((line, index, lines) => index < lines.length - 1 ? line.replace(/(?: {2,}|\\)$/, '') + '  ' : line).join('\n');
  },
};
function exportList(node: ListNode, children: (node: ElementNode) => string, cell: boolean, depth = 0): string {
  let number = node.getStart();
  const output: string[] = [];
  for (const item of node.getChildren()) if ($isListItemNode(item)) {
    const only = item.getFirstChild();
    if (item.getChildrenSize() === 1 && $isListNode(only)) { output.push(exportList(only, children, cell, depth + 1)); continue; }
    const kind = node.getListType(), pending = item.getChecked() ? '' : $getState(item, pendingState);
    const marker = cell ? String((node.exportJSON().$ as Record<string, unknown> | undefined)?.mdListMarker || '-') : '-';
    const prefix = kind === 'number' ? `${number++}. ` : kind === 'check' ? `- [${item.getChecked() ? 'x' : pending || ' '}] ` : `${marker} `;
    let text = children(item);
    if (kind !== 'number') text = text.replace(/^(\s{0,3}\d+)\.\s/, '$1\\. ');
    output.push(' '.repeat(depth * (cell ? 4 : 2)) + prefix + text);
  }
  return output.join('\n');
}
function checklist(cell: boolean): ElementTransformer {
  return {
    ...CHECK_LIST, regExp: cell ? /^(\s*)[-*+]\s\[([ xX/\-])\]\s/ : /^(\s*)(?:[-*+]\s)?\[([ xX/\-])\]\s/,
    replace: (parent, children, match, importing) => {
      if (isInCell(parent) && !/^\s*[-*+]\s/.test(match[0])) return false;
      CHECK_LIST.replace(parent, children, [match[0], match[1], `[${match[2]}]`, match[2].toLowerCase() === 'x' ? 'x' : ''], importing);
      const list = children[0]?.getParent();
      if ($isListItemNode(list) && /[/\-]/.test(match[2])) $setState(list, pendingState, match[2] as '/' | '-');
    },
    export: (node, children) => $isListNode(node) ? exportList(node, children, cell) : null,
  };
}
const IMAGE: TextMatchTransformer = {
  type: 'text-match', dependencies: [ImageNode], trigger: ')',
  importRegExp: /!\[([^\]\ue000]*)\]\(([^\s)]+)\)/,
  regExp: /!\[([^\]\ue000]*)\]\(([^\s)]+)\)$/,
  replace: (node, match) => { node.replace($createImage(match[2], match[1])); },
  export: node => $isImage(node) ? `![${node.getAlt()}](${node.getSource()})` : null,
};
const CHIP: TextMatchTransformer = {
  type: 'text-match', dependencies: [ReferenceNode], trigger: ')',
  importRegExp: /\[([^\]\ue000]+)\]\(#doc:([^\s)]+)\)/,
  regExp: /\[([^\]\ue000]+)\]\(#doc:([^\s)]+)\)$/,
  replace: (node, match) => { node.replace($createReference(match[2], match[1])); },
  export: node => $isReference(node) ? `[${node.getLabel()}](#${node.isCard() ? 'sub' : 'doc'}:${node.getTarget()})` : null,
};
const ANNOTATION: TextMatchTransformer = {
  type: 'text-match', dependencies: [AnnotationNode], regExp: /$^/,
  export: (node, children) => $isAnnotation(node) && node.getIDs()[0]
    ? `<!--wl${node.getKind()}:${node.getIDs()[0]}-->${children(node)}<!--/wl${node.getKind()}:${node.getIDs()[0]}-->` : null,
};
const CARD: ElementTransformer = {
  type: 'element', dependencies: [ReferenceNode], regExp: /^\[([^\]\ue000]+)\]\(#sub:([^\s)]+)\)$/,
  replace: (parent, _children, match) => { if (isInCell(parent)) return false; parent.replace($createReference(match[2], match[1], true)); },
  export: node => $isReference(node) && node.isCard() ? `[${node.getLabel()}](#sub:${node.getTarget()})` : null,
};
const CALLOUT: ElementTransformer = {
  type: 'element', dependencies: [CalloutNode], regExp: /^> ?\[!([a-z\d_]+)\] ?/i,
  replace: (parent, children, match) => {
    const callout = $createCallout(match[1]), paragraph = $createParagraphNode().append(...children);
    parent.replace(callout); callout.append(paragraph); paragraph.selectStart();
  },
  export: node => {
    if (!$isCallout(node)) return null;
    const text = $convertToMarkdownString(transformers(isInCell(node)), node), lines = text.split('\n');
    let title = '';
    if ($isParagraphNode(node.getFirstChild())) { title = lines.shift() || ''; if (lines[0] === '') lines.shift(); }
    return [`> [!${node.getKind()}]${title ? ' ' + title : ''}`, ...lines.filter((line, index) => line || index < lines.length - 1).map(line => line ? '> ' + line : '>')].join('\n');
  },
};
function isInCell(node: LexicalNode): boolean { return node.getParents().some($isTableCellNode); }
export function $tableMarkdown(table: TableNode, copy = false, selection?: BaseSelection | null): string {
  const rows = table.getChildren<TableRowNode>().map(row => row.getChildren<TableCellNode>().filter(cell => !selection || cell.isSelected(selection) || cell.getAllTextNodes().some(text => text.isSelected(selection)))
    .map(cell => {
      const text = $convertToMarkdownString(transformers(true), cell).replace(/\n/g, '\\n').trim().replace(/\|/g, '\\|');
      return copy ? stripMarkers(text) : text;
    })).filter(row => row.length);
  if (!rows.length) return '';
  const count = Math.max(...rows.map(row => row.length)), dashes = copy ? Array(count).fill(3) : widthDashes(table.getColWidths(), count);
  const line = (cells: string[]) => '|' + cells.map(cell => cell ? ` ${cell} ` : ' ').join('|') + '|';
  return [line(rows[0]), line(dashes.map(n => '-'.repeat(n))), ...rows.slice(1).map(line)].join('\n');
}
export function $tableTSV(table: TableNode, selected?: Set<string>): string {
  return table.getChildren<TableRowNode>().map(row => row.getChildren<TableCellNode>().filter(cell => !selected || selected.has(cell.getKey()))
    .map(cell => cell.getTextContent().replace(/[\r\n\t]/g, ' ')).join('\t')).filter(Boolean).join('\n') + '\n';
}
const TABLE: ElementTransformer = {
  type: 'element', dependencies: [TableNode, TableRowNode, TableCellNode], regExp: /^\|.*\|\s?$/,
  replace: (parent, children, match, importing) => {
    if (isInCell(parent) || !importing && children.some(node => node.getTextContentSize() > 0 || !$isTextNode(node))) return false;
    const previous = parent.getPreviousSibling(), widths = dividerWidths(match[0]);
    if (widths && (!$isTableNode(previous) || !$getState(previous, dividerState))) {
      if ($isTableNode(previous)) {
        $setState(previous, dividerState, true); const hint = widthHint(widths); if (hint) previous.setColWidths(hint);
        previous.getFirstChild<TableRowNode>()?.getChildren<TableCellNode>().forEach(cell => cell.setHeaderStyles(TableCellHeaderStates.ROW));
      } else if ($isParagraphNode(previous) && previous.getTextContentSize()) previous.append($createLineBreakNode());
      parent.remove(); return;
    }
    const table = $isTableNode(previous) ? previous : $createTableNode(), row = $createTableRowNode();
    for (const text of tableCells(match[0])) {
      const cell = $createTableCellNode(TableCellHeaderStates.NO_STATUS);
      table.append(row); row.append(cell); importInto(text.replace(/\\n/g, '\n'), cell, true, false);
    }
    if ($isTableNode(previous)) parent.remove(); else parent.replace(table);
    if (!importing) table.getLastDescendant()?.selectEnd();
  },
  export: (node, _children, selection) => $isTableNode(node) ? $tableMarkdown(node, false, selection) : null,
};
const GUARDED_QUOTE: ElementTransformer = {
  ...QUOTE, regExp: /^>(.*)$/,
  replace: (parent, children, match, importing) => {
    const previous = parent.getPreviousSibling();
    if ($isTextNode(children[0])) children[0].setTextContent(match[1].replace(/^ /, ''));
    if (previous instanceof QuoteNode && previous.getTextContentSize() && importing) {
      return QUOTE.replace(parent, children, match, importing);
    }
    if (/^[^\s\\&]/.test(match[1])) { if ($isTextNode(children[0])) children[0].setTextContent('>' + match[1]); return false; }
    if (!/^>\s/.test(match[0])) return false;
    const original = match[0].slice(1).replace(/^ /, '');
    if ($isTextNode(children[0])) children[0].setTextContent(original);
    return QUOTE.replace(parent, children, match, importing);
  },
};
export function transformers(cell = false, copy = false): Transformer[] {
  const code = { ...CODE, export: (node: LexicalNode) => {
    let text = CODE.export?.(node, () => '') ?? null;
    if (text !== null && $isElementNode(node)) {
      const raw = (item: LexicalNode): string => $isAnnotation(item) ? `<!--wl${item.getKind()}:${item.getIDs()[0]}-->${item.getChildren().map(raw).join('')}<!--/wl${item.getKind()}:${item.getIDs()[0]}-->`
        : $isElementNode(item) ? item.getChildren().map(raw).join('') : item.getTextContent();
      const body = node.getChildren().map(raw).join('');
      if (body !== node.getTextContent()) text = text.slice(0, text.indexOf('\n') + 1) + body + text.slice(text.lastIndexOf('\n'));
    }
    return text === null || cell ? text : text.split('\n').map((line, index, lines) => index > 0 && index < lines.length - 1 ? halveListIndent(line) : line).join('\n');
  } };
  return [code, ...(cell ? [] : [PARAGRAPH, { ...TABLE, export: (node: LexicalNode, _children: (node: ElementNode) => string, selected?: BaseSelection | null) => $isTableNode(node) ? $tableMarkdown(node, copy, selected) : null }]), CALLOUT, checklist(cell), ...(cell ? [] : [CARD]), HEADING,
    cell ? QUOTE : GUARDED_QUOTE, UNORDERED_LIST, ORDERED_LIST,
    HIGHLIGHT, ...TEXT_FORMAT_TRANSFORMERS, IMAGE, CHIP, LINK, ANNOTATION];
}
function extractCallouts(text: string) {
  const blocks: { kind: string; text: string }[] = [], lines = text.split('\n'), output: string[] = [];
  let fenced = false;
  for (let i = 0; i < lines.length; i++) {
    if (/^\s*`{3}/.test(lines[i])) { fenced = !fenced; output.push(lines[i]); continue; }
    const match = !fenced && /^> ?\[!([a-z\d_]+)\]\s*(.*)$/i.exec(lines[i]);
    if (!match) { output.push(lines[i]); continue; }
    const body: string[] = [];
    while (i + 1 < lines.length && lines[i + 1].startsWith('>')) body.push(lines[++i].replace(/^> ?/, ''));
    const title = match[2].trim(), value = [title, body.join('\n')].filter(Boolean).join(title && body.length ? '\n\n' : '');
    const index = blocks.push({ kind: match[1], text: value }) - 1;
    output.push('', `\ue001${index}\ue001`, '');
  }
  return { text: output.join('\n'), blocks };
}
function importInto(text: string, root: ElementNode, cell: boolean, fold: boolean, depth = 0) {
  if (depth > 20) { root.append($createParagraphNode()); return; }
  const extracted = extractCallouts(fold ? foldParagraphs(text) : text);
  $convertFromMarkdownString(cell ? extracted.text : doubleListIndent(extracted.text), transformers(cell), root);
  for (const child of root.getChildren()) {
    if (!$isParagraphNode(child)) continue;
    const match = /^\ue001(\d+)\ue001$/.exec(child.getTextContent());
    const block = match && extracted.blocks[Number(match[1])];
    if (!block) continue;
    const callout = $createCallout(block.kind); child.replace(callout);
    importInto(block.text, callout, cell, false, depth + 1);
  }
}
function $wrapPlaceholders(root: ElementNode, pairs: ReturnType<typeof markerPlaceholders>['pairs']) {
  const find = (index: number) => {
    for (const node of root.getAllTextNodes()) {
      for (const match of node.getTextContent().matchAll(/\ue000[oc]:(\d+):[^\ue000]+\ue000/g)) if (Number(match[1]) === index) return { node, offset: match.index!, size: match[0].length };
    }
    return null;
  };
  for (const pair of pairs) {
    const open = find(pair.open), close = find(pair.close); if (!open || !close) continue;
    const selection = $createRangeSelection();
    selection.anchor.set(open.node.getKey(), open.offset + open.size, 'text'); selection.focus.set(close.node.getKey(), close.offset, 'text');
    if (!selection.isCollapsed()) $wrapSelectionInMarkNode(selection, false, pair.id, ids => $createAnnotation(ids, pair.kind));
  }
  for (const node of root.getAllTextNodes()) {
    const text = node.getTextContent().replace(/\ue000[^\ue000]*\ue000/g, '').replace(/\ue000/g, '');
    if (text !== node.getTextContent()) node.setTextContent(text);
  }
}
export function $importDocument(markdown: string) {
  $addUpdateTag('wl:import');
  const anchors = markerPlaceholders(markdown), root = $getRoot();
  importInto(anchors.text, root, false, true);
  if (anchors.marked) $wrapPlaceholders(root, anchors.pairs);
  if (!root.getChildrenSize()) root.append($createParagraphNode());
  $setSelection(null);
}
export function $exportDocument() { return $convertToMarkdownString(transformers()); }
export function $exportSelection(selection: BaseSelection) { return stripMarkers($convertSelectionToMarkdownString(transformers(false, true), selection)); }

// Run only in mounted editors. The headless lane intentionally omits these repairs.
export function $repairDocument() {
  function visit(node: ElementNode) {
    if ($isTableNode(node)) {
      const rows = node.getChildren<TableRowNode>(), count = Math.max(0, ...rows.map(row => row.getChildrenSize()));
      const widths = node.getColWidths();
      if (widths && widths.length !== count) node.setColWidths(Array.from({ length: count }, (_, i) => widths[Math.min(i, widths.length - 1)] || 1));
      for (const row of rows) while (row.getChildrenSize() < count) row.append($createTableCellNode(TableCellHeaderStates.NO_STATUS).append($createParagraphNode()));
    }
    if ($isListItemNode(node) && node.getChecked() && $getState(node, pendingState)) $setState(node, pendingState, '');
    for (const child of node.getChildren()) if ($isElementNode(child)) visit(child);
  }
  visit($getRoot());
}
