import {
  $convertFromMarkdownString, $convertToMarkdownString, $convertSelectionToMarkdownString,
  CODE, CHECK_LIST, HEADING, QUOTE, UNORDERED_LIST, ORDERED_LIST, LINK,
  HIGHLIGHT, TEXT_FORMAT_TRANSFORMERS,
} from '@lexical/markdown';
import type { ElementTransformer, MultilineElementTransformer, TextMatchTransformer, Transformer } from '@lexical/markdown';
import { HeadingNode, QuoteNode } from '@lexical/rich-text';
import { ListNode, ListItemNode, $isListNode, $isListItemNode, $createListNode, $createListItemNode } from '@lexical/list';
import type { ListType } from '@lexical/list';
import { CodeNode, CodeHighlightNode, $createCodeNode, $isCodeNode } from '@lexical/code';
import { LinkNode } from '@lexical/link';
import { MarkNode, $wrapSelectionInMarkNode } from '@lexical/mark';
import {
  TableNode, TableRowNode, TableCellNode, $createTableNode, $createTableRowNode, $createTableCellNode,
  $isTableNode, $isTableCellNode, TableCellHeaderStates,
} from '@lexical/table';
import {
  $getRoot, $setSelection, $createRangeSelection, $createParagraphNode, $createLineBreakNode, $isParagraphNode,
  $isElementNode, $isTextNode, $getState, $setState, createState, $addUpdateTag, $createTextNode, TextNode,
} from 'lexical';
import type { TextFormatType } from 'lexical';
import type { ElementNode, LexicalNode, BaseSelection } from 'lexical';
import {
  AnnotationNode, CalloutNode, ReferenceNode, ImageNode, RelativeLinkNode,
  $createAnnotation, $isAnnotation, $createCallout, $isCallout, $createReference, $isReference, $createImage, $isImage, CHIP_FORMATS,
} from './nodes.tsx';
import { outermostInline, TAG_FORMATS, IMAGE_PATTERN, CHIP_PATTERN, LINK_PATTERN } from '../data/mdInline.ts';
import type { FormatMatch, LinkMatch } from '../data/mdInline.ts';
import { foldParagraphs, closeBlockEmphasis, doubleListIndent, fenceTracker, mapOutsideCode, tableCells, dividerWidths, widthHint, widthDashes, markerPlaceholders, stripMarkers } from './grammar.ts';

export const EDITOR_NODES = [HeadingNode, QuoteNode, ListNode, ListItemNode, CodeNode, CodeHighlightNode,
  TableNode, TableRowNode, TableCellNode, MarkNode, AnnotationNode, CalloutNode, ReferenceNode, ImageNode, RelativeLinkNode,
  { replace: LinkNode, with: (node: LinkNode) => new RelativeLinkNode(node.getURL(), { rel: node.getRel(), target: node.getTarget(), title: node.getTitle() }), withKlass: RelativeLinkNode }];
export const pendingState = createState('aisPending', { parse: value => value === '/' || value === '-' ? value : '' });
const dividerState = createState('aisDivider', { parse: value => value === true });

const fenceState = createState('aisFence', { parse: value => typeof value === 'string' && /^`{3,}$/.test(value) ? value : '```' });
const tailState = createState('aisFenceTail', { parse: value => typeof value === 'string' ? value : '' });
// C74: the language is the info string's first word when it has the reading view's
// language shape (1-30 of [A-Za-z0-9+#._-]), otherwise the [\w-] run as before. The
// rest stays on the fence line; spaces between the backticks and the language do not.
export function codeInfo(info: string): { language: string; tail: string } {
  const text = info.replace(/^[ \t]+/, '').replace(/[ \t]+$/, ''), word = /^\S*/.exec(text)![0];
  const language = /^[a-zA-Z0-9+#._-]{1,30}$/.test(word) ? word : /^[\w-]*/.exec(text)![0];
  return { language, tail: text.slice(language.length) };
}
function $appendCode(root: ElementNode, fence: string, info: { language: string; tail: string }, code: string) {
  const node = $createCodeNode(info.language || undefined);
  $setState(node, fenceState, fence); $setState(node, tailState, info.tail);
  root.append(node.append($createTextNode(code)));
}
// C73: the first code line keeps its spaces; the library dropped one on every open,
// so indented code shrank on each save. The fence judge is the one line joining uses (9.9).
const CODE_BLOCK: MultilineElementTransformer = {
  ...CODE,
  handleImportAfterStartMatch: ({ lines, rootNode, startLineIndex }) => {
    const start = /^[ \t]*(`{3,})(.*)$/.exec(lines[startLineIndex]);
    if (!start) return null;
    const fence = start[1], single = new RegExp(`\`{${fence.length},}$`).exec(start[2]);
    if (single && single.index > 0) { $appendCode(rootNode, fence, { language: '', tail: '' }, start[2].slice(0, single.index)); return [true, startLineIndex]; }
    const close = new RegExp(`^[ \\t]*\`{${fence.length},}[ \\t]*$`);
    let end = startLineIndex + 1;
    while (end < lines.length && !close.test(lines[end])) end++;
    const body = lines.slice(startLineIndex + 1, end);
    // 9.9 keeps dropping a blank first line and empty last lines of a multi-line block.
    if (body.length > 1) { if (!body[0].trim()) body.shift(); while (body.length && !body.at(-1)) body.pop(); }
    $appendCode(rootNode, fence, codeInfo(start[2]), body.join('\n'));
    return [true, Math.min(end, lines.length - 1)];
  },
  export: node => {
    if (!$isCodeNode(node)) return null;
    const raw = (item: LexicalNode): string => $isAnnotation(item) ? `<!--ais${item.getKind()}:${item.getIDs()[0]}-->${item.getChildren().map(raw).join('')}<!--/ais${item.getKind()}:${item.getIDs()[0]}-->`
      : $isElementNode(item) ? item.getChildren().map(raw).join('') : item.getTextContent();
    const body = node.getChildren().map(raw).join(''), runs = body.match(/`{3,}/g);
    let fence = $getState(node, fenceState);
    if (body.includes(fence) && runs) fence = '`'.repeat(Math.max(...runs.map(run => run.length)) + 1);
    return fence + (node.getLanguage() || '') + $getState(node, tailState) + (body ? '\n' + body : '') + '\n' + fence;
  },
};

const holder = (node: LexicalNode | null): node is ListItemNode => $isListItemNode(node) && node.getChildrenSize() === 1 && $isListNode(node.getFirstChild());
const listIndent = (space: string) => (space.match(/\t/g)?.length || 0) + Math.floor((space.match(/ /g)?.length || 0) / 4);
// C72, C196: an indented list line goes under the item above whatever its kind, and a
// nested numbered list keeps the number of its own first line. An outer list that
// begins with an indented line takes its start from the first outer line.
function $placeListLine(parent: ElementNode, children: LexicalNode[], kind: ListType, space: string, number: number, checked?: boolean): ListItemNode {
  const level = listIndent(space), previous = parent.getPreviousSibling(), item = $createListItemNode(kind === 'check' ? checked : undefined);
  const real = (list: ListNode) => list.getChildren().some(child => !holder(child));
  let list: ListNode;
  if ($isListNode(previous) && (level > 0 || previous.getListType() === kind || !real(previous))) {
    list = previous; parent.remove();
    if (!level && list.getListType() !== kind) list.setListType(kind);
  } else { list = $createListNode(kind); parent.replace(list); }
  for (let depth = 0; depth < level; depth++) {
    const last = list.getLastChild(), own = depth === level - 1;
    let nested = holder(last) ? last.getFirstChild() as ListNode : null;
    if (!nested || own && nested.getListType() !== kind) { nested = $createListNode(own ? kind : nested?.getListType() || kind); list.append($createListItemNode().append(nested)); }
    list = nested;
  }
  if (kind === 'number' && !real(list)) list.setStart(number);
  list.append(item.append(...children));
  return item;
}
const listRule = (base: ElementTransformer, kind: ListType): ElementTransformer => ({
  ...base,
  replace: (parent, children, match, importing) => {
    if (!importing) return base.replace(parent, children, match, importing);
    $placeListLine(parent, children, kind, match[1], kind === 'number' ? Number(match[2]) : 1);
  },
});

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
      const checked = match[2].toLowerCase() === 'x';
      if (!cell && importing) {
        const item = $placeListLine(parent, children, 'check', match[1], 1, checked);
        if (/[/\-]/.test(match[2])) $setState(item, pendingState, match[2] as '/' | '-');
        return;
      }
      CHECK_LIST.replace(parent, children, [match[0], match[1], `[${match[2]}]`, checked ? 'x' : ''], importing);
      const list = children[0]?.getParent();
      if ($isListItemNode(list) && /[/\-]/.test(match[2])) $setState(list, pendingState, match[2] as '/' | '-');
    },
    export: (node, children) => $isListNode(node) ? exportList(node, children, cell) : null,
  };
}
const IMAGE: TextMatchTransformer = {
  type: 'text-match', dependencies: [ImageNode], trigger: ')',
  importRegExp: IMAGE_PATTERN,
  regExp: /!\[([^\]\ue000]*)\]\(([^\s)]+)\)$/,
  replace: (node, match) => { node.replace($createImage(match[2], match[1])); },
  export: node => $isImage(node) ? `![${node.getAlt()}](${node.getSource()})` : null,
};
const CHIP: TextMatchTransformer = {
  type: 'text-match', dependencies: [ReferenceNode], trigger: ')',
  importRegExp: CHIP_PATTERN,
  regExp: /\[([^\]\ue000]+)\]\(#doc:([^\s)]+)\)$/,
  replace: (node, match) => { node.replace($createReference(match[2], match[1], false, CHIP_FORMATS.filter(format => node.hasFormat(format)))); },
  export: node => $isReference(node) ? `[${node.getLabel()}](#${node.isCard() ? 'sub' : 'doc'}:${node.getTarget()})` : null,
};
// C178, C197, C199, C200: one inline rule replaces the library's format and text-match
// import, so the editor pairs exactly like the title and the reading view do. The
// library still splits the node and recurses; this rule only picks what comes first.
const INLINE: TextMatchTransformer = {
  type: 'text-match', dependencies: [ImageNode, ReferenceNode, LinkNode], regExp: /$^/,
  importRegExp: { [Symbol.match]: (text: string) => {
    const found = outermostInline(text);
    if (!found) return null;
    return Object.assign([text.slice(found.start, found.end)], { index: found.start, input: text, found });
  } } as unknown as RegExp,
  replace: (node, match) => {
    const found = (match as unknown as { found: FormatMatch | LinkMatch }).found, text = node.getTextContent();
    if (found.kind === 'image') return IMAGE.replace!(node, text.match(IMAGE_PATTERN)!);
    if (found.kind === 'chip') return CHIP.replace!(node, text.match(CHIP_PATTERN)!);
    if (found.kind === 'link') return LINK.replace!(node, text.match(LINK_PATTERN)!);
    const mark = found as FormatMatch;
    node.setTextContent(mark.content);
    for (const format of TAG_FORMATS[mark.tag]) if (!node.hasFormat(format)) node.toggleFormat(format);
    return node;
  },
};
// C201: the library closes every format before a decorator, so `**앞 [칩](#doc:x) 뒤**`
// came back as `**앞** [칩](#doc:x) **뒤**`. Touching text and chips form one run when
// a chip in it carries a format. The run's first node writes the whole run through the
// library's format writer, using stand-ins (objects with the TextNode prototype) in which
// each chip is a placeholder letter with the chip's formats; the placeholders then become
// chip links. The other nodes of the run write nothing.
const inRun = (node: LexicalNode | null): node is TextNode | ReferenceNode => $isTextNode(node) || $isReference(node) && !node.isCard();
function chipRun(node: LexicalNode): (TextNode | ReferenceNode)[] | null {
  if (!inRun(node)) return null;
  let first: TextNode | ReferenceNode = node;
  for (let previous = first.getPreviousSibling(); inRun(previous); previous = previous.getPreviousSibling()) first = previous;
  const run = [first];
  for (let next = first.getNextSibling(); inRun(next); next = next.getNextSibling()) run.push(next);
  return run.some(member => $isReference(member) && member.getFormats().length) ? run : null;
}
const CHIP_RUN: TextMatchTransformer = {
  type: 'text-match', dependencies: [ReferenceNode], regExp: /$^/,
  export: (node, _children, exportFormat) => {
    const run = chipRun(node);
    if (!run) return null;
    if (!run[0].is(node)) return '';
    // Members with the same formats merge into one stand-in, so a space between two
    // chips does not close and reopen the format; inline code stays on its own.
    const links: string[] = [], groups: { text: string; member: TextNode | ReferenceNode; key: string | null }[] = [];
    for (const member of run) {
      const text = $isReference(member) ? String.fromCharCode(0xe100 + links.push(`[${member.getLabel()}](#doc:${member.getTarget()})`) - 1) : member.getTextContent();
      const key = member.hasFormat('code') ? null : CHIP_FORMATS.filter(format => member.hasFormat(format)).join();
      const last = groups.at(-1);
      if (key !== null && last && last.key === key) last.text += text; else groups.push({ text, member, key });
    }
    const standIns: TextNode[] = groups.map(({ member, text }, n) => Object.assign(Object.create(TextNode.prototype) as TextNode, {
      hasFormat: (format: TextFormatType) => member.hasFormat(format), getTextContent: () => text,
      getPreviousSibling: () => standIns[n - 1] ?? null, getNextSibling: () => standIns[n + 1] ?? null,
    }));
    return standIns.map((standIn, n) => exportFormat(standIn, groups[n].text)).join('').replace(/[\ue100-\ue8ff]/g, letter => links[letter.charCodeAt(0) - 0xe100]);
  },
};
const ANNOTATION: TextMatchTransformer = {
  type: 'text-match', dependencies: [AnnotationNode], regExp: /$^/,
  export: (node, children) => $isAnnotation(node) && node.getIDs()[0]
    ? `<!--ais${node.getKind()}:${node.getIDs()[0]}-->${children(node)}<!--/ais${node.getKind()}:${node.getIDs()[0]}-->` : null,
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
  const line = (cells: string[]) => '|' + cells.map(cell => cell ? ` ${cell} ` : '  ').join('|') + '|';
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
      table.append(row); row.append(cell); importInto(mapOutsideCode(text, part => part.replace(/\\n/g, '\n')), cell, true, false);
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
  return [CODE_BLOCK, ...(cell ? [] : [PARAGRAPH, { ...TABLE, export: (node: LexicalNode, _children: (node: ElementNode) => string, selected?: BaseSelection | null) => $isTableNode(node) ? $tableMarkdown(node, copy, selected) : null }]), CALLOUT, checklist(cell), ...(cell ? [] : [CARD]), HEADING,
    cell ? QUOTE : GUARDED_QUOTE, cell ? UNORDERED_LIST : listRule(UNORDERED_LIST, 'bullet'), cell ? ORDERED_LIST : listRule(ORDERED_LIST, 'number'),
    HIGHLIGHT, ...TEXT_FORMAT_TRANSFORMERS, ...(copy ? [] : [CHIP_RUN]), IMAGE, CHIP, LINK, ANNOTATION];
}
const INLINE_RULES: Transformer[] = [HIGHLIGHT, ...TEXT_FORMAT_TRANSFORMERS, IMAGE, CHIP, LINK];
// Opening a note uses INLINE in place of the library's inline rules; saving keeps them.
function importTransformers(cell: boolean): Transformer[] {
  return [...transformers(cell).filter(rule => !INLINE_RULES.includes(rule)), INLINE];
}
function extractCallouts(text: string) {
  const blocks: { kind: string; text: string }[] = [], lines = text.split('\n'), output: string[] = [], fenced = fenceTracker();
  for (let i = 0; i < lines.length; i++) {
    // C181: a callout written inside a code fence is code.
    if (fenced(lines[i])) { output.push(lines[i]); continue; }
    const match = /^> ?\[!([a-z\d_]+)\]\s*(.*)$/i.exec(lines[i]);
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
  const extracted = extractCallouts(fold ? closeBlockEmphasis(foldParagraphs(text)) : text);
  $convertFromMarkdownString(cell ? extracted.text : doubleListIndent(extracted.text), importTransformers(cell), root);
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
  $addUpdateTag('ais:import');
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
