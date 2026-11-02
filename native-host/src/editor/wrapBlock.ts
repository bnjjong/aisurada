import { $createListItemNode, $createListNode, $isListItemNode, $isListNode } from '@lexical/list';
import type { ListItemNode, ListNode } from '@lexical/list';
import { $createCodeNode } from '@lexical/code';
import { $createTableNodeWithDimensions } from '@lexical/table';
import { $setBlocksType } from '@lexical/selection';
import { $createParagraphNode, $createTextNode, $getSelection, $insertNodes, $isElementNode, $isRangeSelection } from 'lexical';
import type { ElementNode, LexicalNode } from 'lexical';
import { $createCallout, $isCallout } from './nodes.tsx';

// A list item whose only child is a list is the holder of a nested list, not an item of its own.
const isHolder = (item: ListItemNode) => item.getChildrenSize() === 1 && $isListNode(item.getFirstChild());

export function $currentListItem(node: LexicalNode): ListItemNode | null {
  for (let current: LexicalNode | null = node; current; current = current.getParent()) {
    if ($isListItemNode(current) && !isHolder(current)) return current;
  }
  return null;
}

// The number the exporter gives `child`: holders of nested lists take no number.
const numberOf = (list: ListNode, child: LexicalNode) =>
  list.getStart() + child.getPreviousSiblings().filter(node => $isListItemNode(node) && !isHolder(node)).length;

/**
 * C190: 「코드 블록」 and 「콜아웃」 inside a list wrap only the text of the item
 * the cursor is in. Every enclosing list is split up to the outermost one and the
 * block takes that list's place. The item's own nested list stays at its depth at
 * the front of the list after the block and is not merged into the items after it.
 */
export function $wrapListItem(item: ListItemNode, kind: 'code' | 'callout'): ElementNode {
  let child: LexicalNode = item, list = item.getParent() as ListNode, carried: ListItemNode | null = null, after: ListNode;
  for (;;) {
    const following = child.getNextSiblings(), first = following.find(node => $isListItemNode(node) && !isHolder(node));
    after = $createListNode(list.getListType(), first ? numberOf(list, first) : list.getStart());
    if (carried) after.append(carried);
    after.append(...following);
    const holder = list.getParent();
    if (!$isListItemNode(holder) || !$isListNode(holder.getParent())) break;
    carried = after.getChildrenSize() ? $createListItemNode().append(after) : null;
    child = holder; list = holder.getParent() as ListNode;
  }
  const content = item.getChildren().filter(node => !$isListNode(node));
  const paragraph = $createParagraphNode();
  const block = kind === 'code'
    ? $createCodeNode('plaintext').append($createTextNode(content.map(node => node.getTextContent()).join('')))
    : $createCallout().append(paragraph.append(...content));
  list.insertAfter(block);
  if (after.getChildrenSize()) block.insertAfter(after);
  for (let gone: LexicalNode | null = item; gone;) {
    const parent: ElementNode | null = gone.getParent();
    gone.remove();
    gone = parent && ($isListNode(parent) || $isListItemNode(parent)) && $isElementNode(parent) && parent.getChildrenSize() === 0 ? parent : null;
  }
  if (kind === 'code') block.selectEnd(); else paragraph.selectEnd();
  return block;
}

// The toolbar and slash menu's 「표」, 「콜아웃」 and 「코드 블록」 at the current selection.
export function $insertBlock(kind: string) {
  const selected = $getSelection(); if (!$isRangeSelection(selected)) return;
  if (kind === 'table') { $insertNodes([$createTableNodeWithDimensions(3, 3, { rows: true, columns: false })]); return; }
  const node = selected.anchor.getNode(), item = $currentListItem(node);
  if (kind === 'code') { if (item) $wrapListItem(item, 'code'); else $setBlocksType(selected, () => $createCodeNode('plaintext')); return; }
  if (node.getParents().some($isCallout) || $isCallout(node)) return;
  if (item) { $wrapListItem(item, 'callout'); return; }
  const top = node.getTopLevelElement(); if (!top) return;
  const wrapper = $createCallout(); top.insertBefore(wrapper); wrapper.append(top); top.selectStart();
}
