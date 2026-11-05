import { useEffect, useRef, useState } from 'react';
import { useLexicalComposerContext } from '@lexical/react/LexicalComposerContext';
import { $wrapSelectionInMarkNode } from '@lexical/mark';
import { $isCodeNode } from '@lexical/code';
import { $isTableSelection, $isTableCellNode, $isTableNode } from '@lexical/table';
import {
  $getRoot, $getSelection, $setSelection, $isRangeSelection, $isTextNode, $getNodeByKey, $createRangeSelection,
  $createParagraphNode, $isParagraphNode, $isRootNode,
  FORMAT_TEXT_COMMAND, PASTE_COMMAND, KEY_TAB_COMMAND, COMMAND_PRIORITY_HIGH, COMMAND_PRIORITY_LOW, createCommand,
} from 'lexical';
import type { BaseSelection, ElementNode, TextNode } from 'lexical';
import type { EditorProps } from './Editor.tsx';
import { $createAnnotation, $isCallout, $createReference, $createImage } from './nodes.tsx';
import { calloutStyle } from './grammar.ts';
import { $insertBlock, $formatSelection, $selectionHasFormat } from './wrapBlock.ts';

type Trigger = { key: string; start: number; size: number };
type Menu = { kind: 'slash' | 'note'; trigger: Trigger; query: string; selected: number; x: number; y: number };
export const OPEN_COMMENT_COMMAND = createCommand<void>('aisurada:comment-compose');
export const OPEN_ASK_COMMAND = createCommand<void>('aisurada:ask-compose');
const formats = ['bold', 'italic', 'strikethrough', 'code'] as const;
const imageTypes: Record<string, string> = { 'image/png': 'png', 'image/jpeg': 'jpg', 'image/gif': 'gif', 'image/webp': 'webp', 'image/avif': 'avif', 'image/bmp': 'bmp', 'image/svg+xml': 'svg' };
const languages = ['plaintext', 'kotlin', 'java', 'typescript', 'javascript', 'python', 'sql', 'bash', 'yaml', 'json', 'xml', 'groovy', 'dockerfile', 'properties', 'diff', 'mermaid'];

// 슬래시 메뉴의 아이콘. 글자(한자, 기호)가 아니라 선으로 그려서 한글 화면에 낯선 글자가 섞이지 않고, 글꼴과 상관없이 같게 보인다.
const slashIcons: Record<string, string> = {
  page: 'M4 2.5h5l3 3v8H4v-11zM9 2.5v3h3',
  link: 'M6.8 9.2l2.4-2.4M5.6 7.6l-.9.9a2.3 2.3 0 003.3 3.3l.9-.9M10.4 8.4l.9-.9A2.3 2.3 0 008 4.2l-.9.9',
  table: 'M3.5 3.5h9a1 1 0 011 1v7a1 1 0 01-1 1h-9a1 1 0 01-1-1v-7a1 1 0 011-1zM2.5 7h11M6.5 7v5.5',
  callout: 'M3.5 3.5h9a1 1 0 011 1v7a1 1 0 01-1 1h-9a1 1 0 01-1-1v-7a1 1 0 011-1zM5 6.5h6M5 9.5h4',
  code: 'M6 5L3 8l3 3M10 5l3 3-3 3',
};
function SlashIcon({ name }: { name: string }) {
  return <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><path d={slashIcons[name]} /></svg>;
}
function tableRowCells(selection: BaseSelection): ElementNode[] {
  const rows = new Map<string, ElementNode>();
  for (const node of selection.getNodes()) {
    const cell = $isTableCellNode(node) ? node : node.getParents().find($isTableCellNode);
    const row = cell?.getParent(); if (row) rows.set(row.getKey(), row);
  }
  return [...rows.values()].flatMap(row => row.getChildren<ElementNode>());
}
export function EditorTools(props: EditorProps) {
  const [editor] = useLexicalComposerContext(), latest = useRef(props); latest.current = props;
  const [selectionBar, setBar] = useState<{ x: number; y: number; quote: string; special: boolean; formats: boolean[] } | null>(null);
  const [compose, setCompose] = useState<{ quote: string; x: number; y: number } | null>(null), [comment, setComment] = useState('');
  const selection = useRef<BaseSelection | null>(null), composeRef = useRef(compose); composeRef.current = compose;
  const [menu, setMenu] = useState<Menu | null>(null), menuRef = useRef(menu); menuRef.current = menu;
  const [code, setCode] = useState<{ key: string; language: string } | null>(null), [callout, setCallout] = useState<{ key: string; kind: string } | null>(null);
  // 코드·콜아웃 안에 있을 때만 그 블록 오른쪽 위에 뜨는 선택 줄의 자리, 그리고 커서가 빈 줄에 있을 때 보이는 「+」와 안내의 자리.
  const [anchor, setAnchor] = useState<{ top: number; right: number } | null>(null), [line, setLine] = useState<{ x: number; y: number } | null>(null);
  const [card, setCard] = useState<{ ids: string[]; x: number; y: number; pinned: boolean } | null>(null);
  const closeCard = useRef<ReturnType<typeof setTimeout> | undefined>(undefined), closeCompose = () => { setCompose(null); setComment(''); };
  const outer = () => editor.getRootElement()?.closest<HTMLElement>('.lex-editor') ?? null;
  const coords = (rect: DOMRect) => {
    const box = outer()?.getBoundingClientRect();
    if (!box) return { x: 0, y: 0 };
    return { x: Math.max(8, Math.min(rect.left - box.left, box.width - 260)), y: Math.max(0, rect.top - box.top - 42) };
  };
  const slashItems = [
    ...(props.features?.createChild ? [{ id: 'page', icon: 'page', label: '페이지', desc: '하위 노트를 만들어 카드로 넣어요', aliases: 'page 후속 하위 노트 새 페이지' }] : []),
    ...(props.features?.notes ? [{ id: 'note', icon: 'link', label: '노트 링크', desc: '다른 노트로 가는 링크', aliases: 'note link 링크 기록' }] : []),
    { id: 'table', icon: 'table', label: '표', desc: '3×3 표를 넣어요', aliases: 'table 테이블 시트' },
    { id: 'callout', icon: 'callout', label: '콜아웃', desc: '눈에 띄는 안내 상자', aliases: 'callout 정보 강조 안내' },
    { id: 'code', icon: 'code', label: '코드 블록', desc: '코드를 그대로 보여 줘요', aliases: 'code 코드 소스' },
  ].filter(item => (item.label + ' ' + item.aliases).toLowerCase().includes(menu?.query.toLowerCase() || ''));
  const notes = (props.features?.notes || []).filter(note => note.id !== props.features?.currentId && note.title.trim()
    && (props.features?.showRecords || note.tier !== 'record') && note.title.toLowerCase().includes(menu?.query.toLowerCase() || ''))
    .sort((a, b) => b.created.localeCompare(a.created)).slice(0, 20);
  function restore() { if (selection.current) $setSelection(selection.current.clone()); }
  function format(value: typeof formats[number]) { editor.update(() => { restore(); editor.dispatchCommand(FORMAT_TEXT_COMMAND, value); }); }
  const block = $insertBlock;
  // 빈 줄의 「+」: 슬래시를 대신 쳐 준다. 그러면 직접 `/`를 칠 때와 같은 길로 메뉴가 열린다.
  function insertSlash() { editor.update(() => { const selected = $getSelection(); if ($isRangeSelection(selected)) selected.insertText('/'); }); editor.focus(); }
  function removeTrigger(trigger: Trigger) {
    const node = $getNodeByKey<TextNode>(trigger.key); if (!$isTextNode(node)) return null;
    const range = $createRangeSelection(); range.anchor.set(trigger.key, trigger.start, 'text'); range.focus.set(trigger.key, trigger.start + trigger.size, 'text');
    $setSelection(range); range.removeText(); return range;
  }
  async function choose(id: string) {
    const active = menuRef.current; if (!active) return;
    if (active.kind === 'note') {
      const note = (latest.current.features?.notes || []).find(item => item.id === id); if (!note) return;
      editor.update(() => { const range = removeTrigger(active.trigger); range?.insertNodes([$createReference(id, note.title.replace(/\[/g, '［').replace(/\]/g, '］'))]); });
      setMenu(null); editor.focus(); return;
    }
    if (id === 'note') { setMenu({ ...active, kind: 'note', query: '', selected: 0 }); return; }
    setMenu(null);
    if (id !== 'page') { editor.update(() => { removeTrigger(active.trigger); block(id); }); editor.focus(); return; }
    let target = '';
    editor.update(() => {
      const range = removeTrigger(active.trigger), anchor = range?.anchor.getNode();
      target = (anchor?.getParents().find($isTableNode) || anchor?.getTopLevelElement())?.getKey() || '';
    });
    const child = await latest.current.features?.createChild?.(); if (!child) return;
    editor.update(() => {
      const node = $getNodeByKey(target), card = $createReference(child.id, child.title, true), paragraph = $createParagraphNode();
      if ($isParagraphNode(node) && !node.getTextContent()) node.replace(card);
      else if (node?.isAttached()) node.insertAfter(card); else $getRoot().append(card);
      card.insertAfter(paragraph); paragraph.selectStart();
    }); editor.focus();
  }
  function beginComment() {
    if (!selectionBar) return;
    setCompose({ quote: selectionBar.quote, x: selectionBar.x, y: selectionBar.y + 42 }); setComment('');
  }
  function wrapAskSelection(turnId: string, targetSelection: BaseSelection) {
    editor.update(() => {
      $setSelection(targetSelection.clone());
      const selected = $getSelection();
      if ($isTableSelection(selected)) {
        for (const cell of tableRowCells(selected)) {
          const range = $createRangeSelection(); range.anchor.set(cell.getKey(), 0, 'element'); range.focus.set(cell.getKey(), cell.getChildrenSize(), 'element');
          $wrapSelectionInMarkNode(range, false, turnId, ids => $createAnnotation(ids, 'a'));
        }
      } else if ($isRangeSelection(selected)) $wrapSelectionInMarkNode(selected, selected.isBackward(), turnId, ids => $createAnnotation(ids, 'a'));
    });
    editor.focus();
  }
  function beginAsk() {
    if (!selectionBar || !selection.current) return;
    const target = selection.current.clone();
    props.onOpenAskCompose?.(selectionBar.quote, { left: selectionBar.x, top: selectionBar.y + 42 }, (turnId: string) => wrapAskSelection(turnId, target));
  }
  function submitComment() {
    if (!comment.trim() || !composeRef.current || !selection.current) return;
    const body = comment.trim(), quote = composeRef.current.quote, id = `cm${Date.now().toString(36)}-${crypto.randomUUID().slice(0, 8)}`;
    editor.update(() => {
      restore(); const selected = $getSelection();
      if ($isTableSelection(selected)) {
        for (const cell of tableRowCells(selected)) {
          const range = $createRangeSelection(); range.anchor.set(cell.getKey(), 0, 'element'); range.focus.set(cell.getKey(), cell.getChildrenSize(), 'element');
          $wrapSelectionInMarkNode(range, false, id, ids => $createAnnotation(ids, 'c'));
        }
      } else if ($isRangeSelection(selected)) $wrapSelectionInMarkNode(selected, selected.isBackward(), id, ids => $createAnnotation(ids, 'c'));
    });
    void latest.current.features?.comment?.(id, body, quote); closeCompose(); editor.focus();
  }
  useEffect(() => {
    const root = editor.getRootElement();
    if (!root) return;
    const update = editor.registerUpdateListener(({ editorState, dirtyElements, dirtyLeaves, tags }) => {
      if (!editor.getRootElement()) return;
      editorState.read(() => {
        const selected = $getSelection(), node = $isRangeSelection(selected) ? selected.anchor.getNode() : null;
        const inCode = node && [node, ...node.getParents()].find($isCodeNode);
        const inCallout = node && [node, ...node.getParents()].find($isCallout);
        setCode(inCode ? { key: inCode.getKey(), language: inCode.getLanguage() || '' } : null);
        setCallout(inCallout ? { key: inCallout.getKey(), kind: inCallout.getKind() } : null);
        const box = outer()?.getBoundingClientRect(), activeBlock = inCode || inCallout;
        const blockRect = activeBlock && box ? editor.getElementByKey(activeBlock.getKey())?.getBoundingClientRect() : null;
        const place = blockRect && box ? { top: Math.max(0, Math.round(blockRect.top - box.top - 34)), right: Math.max(0, Math.round(box.right - blockRect.right)) } : null;
        setAnchor(previous => previous?.top === place?.top && previous?.right === place?.right ? previous : place);
        const emptyLine = $isRangeSelection(selected) && selected.isCollapsed() && $isParagraphNode(node) && node.isEmpty() && $isRootNode(node.getParent()) ? node : null;
        const lineRect = emptyLine && box && root.contains(document.activeElement) ? editor.getElementByKey(emptyLine.getKey())?.getBoundingClientRect() : null;
        const spot = lineRect && box ? { x: Math.round(lineRect.left - box.left), y: Math.round(lineRect.top - box.top) } : null;
        setLine(previous => previous?.x === spot?.x && previous?.y === spot?.y ? previous : spot);
        const text = selected?.getTextContent() || '';
        if (selected && text.trim() && ($isRangeSelection(selected) && !selected.isCollapsed() || $isTableSelection(selected))) {
          if (!composeRef.current) selection.current = selected.clone();
          const dom = window.getSelection(), rect = dom?.rangeCount ? dom.getRangeAt(0).getBoundingClientRect() : root.getBoundingClientRect();
          const quote = $isTableSelection(selected) ? tableRowCells(selected).reduce((lines, cell, index, cells) => lines + cell.getTextContent()
            + (index === cells.length - 1 || cell.getParent()?.getKey() !== cells[index + 1]?.getParent()?.getKey() ? '\n' : '\t'), '') : text;
          setBar({ ...coords(rect), quote, special: !!inCode || $isTableSelection(selected), formats: formats.map(format => $isRangeSelection(selected) && $selectionHasFormat(format)) });
        } else if (!composeRef.current) setBar(null);
        if ((!dirtyElements.size && !dirtyLeaves.size) || tags.has('ais:import') || tags.has('ais:repair') || tags.has('ais:mirror') || tags.has('aisurada-mirror') || menuRef.current) return;
        if ($isRangeSelection(selected) && selected.isCollapsed() && $isTextNode(node)) {
          const before = node.getTextContent().slice(0, selected.anchor.offset);
          const pair = /(?:^|[^\[])\[\[$/.test(before);
          const slash = /(?:^|\s)\/$/.test(before);
          if (pair && latest.current.features?.notes || slash) {
            const dom = window.getSelection(), rect = dom?.rangeCount ? dom.getRangeAt(0).getBoundingClientRect() : root.getBoundingClientRect(), position = coords(rect);
            setMenu({ kind: pair ? 'note' : 'slash', trigger: { key: node.getKey(), start: selected.anchor.offset - (pair ? 2 : 1), size: pair ? 2 : 1 }, query: '', selected: 0, x: position.x, y: position.y + 68 });
          }
        }
      });
    });
    const paste = editor.registerCommand(PASTE_COMMAND, event => {
      const features = latest.current.features;
      if (!event || !('clipboardData' in event) || !features?.host?.native) return false;
      const files = [...(event.clipboardData?.files || [])].filter(file => file.type.startsWith('image/'));
      if (!files.length) return false;
      event.preventDefault();
      if (!features.noteDirectory) { void features.host.warn('이 노트에는 첨부를 둘 수 없어요. 앱에서 한 번 열면 폴더로 바뀝니다.'); return true; }
      void (async () => {
        const failed: string[] = [];
        for (const file of files) {
          try {
            const extension = imageTypes[file.type]; if (!extension) throw Error(`지원하지 않는 그림 형식: ${file.type}`);
            if (file.size > 5_000_000) throw Error('5,000,000바이트를 넘는 그림이에요.');
            const name = `img-${Date.now().toString(36)}-${crypto.randomUUID().slice(0, 8)}.${extension}`;
            await features.host!.writeImage(features.noteDirectory!, name, new Uint8Array(await file.arrayBuffer()));
            editor.update(() => {
              const image = $createImage('attach/' + name), selected = $getSelection();
              if ($isRangeSelection(selected)) selected.insertNodes([image]); else $getRoot().append($createParagraphNode().append(image));
            });
          } catch (error) { failed.push(`${file.name || file.type}: ${String(error)}`); }
        }
        if (failed.length) await features.host!.warn('그림을 못 붙였어요.\n' + failed.join('\n'));
      })(); return true;
    }, COMMAND_PRIORITY_HIGH);
    const tab = editor.registerCommand(KEY_TAB_COMMAND, event => {
      const selected = $getSelection();
      if (!$isRangeSelection(selected)) return false;
      const parents = selected.anchor.getNode().getParents();
      if (!parents.some($isCodeNode) || parents.some($isTableCellNode)) return false;
      event.preventDefault(); selected.insertText('\t'); return true;
    }, COMMAND_PRIORITY_HIGH);
    const formatCommand = editor.registerCommand(FORMAT_TEXT_COMMAND, $formatSelection, COMMAND_PRIORITY_LOW);
    const commentCommand = editor.registerCommand(OPEN_COMMENT_COMMAND, () => {
      const currentRoot = editor.getRootElement();
      if (!currentRoot) return false;
      const selected = $getSelection(); if (!selected) return false;
      selection.current = selected.clone();
      const quote = $isTableSelection(selected) ? tableRowCells(selected).map(cell => cell.getTextContent()).join('\t') + '\n' : selected.getTextContent();
      const rect = currentRoot.getBoundingClientRect(); setCompose({ quote, ...coords(rect), y: 70 }); return true;
    }, COMMAND_PRIORITY_HIGH);
    const askCommand = editor.registerCommand(OPEN_ASK_COMMAND, () => {
      const currentRoot = editor.getRootElement();
      if (!currentRoot) return false;
      const selected = $getSelection(); if (!selected) return false;
      selection.current = selected.clone();
      const target = selected.clone();
      const quote = $isTableSelection(selected)
        ? tableRowCells(selected).reduce((lines, cell, index, cells) => lines + cell.getTextContent()
            + (index === cells.length - 1 || cell.getParent()?.getKey() !== cells[index + 1]?.getParent()?.getKey() ? '\n' : '\t'), '')
        : selected.getTextContent();
      const dom = window.getSelection();
      const rect = dom?.rangeCount ? dom.getRangeAt(0).getBoundingClientRect() : currentRoot.getBoundingClientRect();
      const position = coords(rect);
      const top = rect.height > 0 ? position.y + 42 : 70;
      latest.current.onOpenAskCompose?.(quote, { left: position.x, top }, (turnId: string) => wrapAskSelection(turnId, target));
      return true;
    }, COMMAND_PRIORITY_HIGH);
    const over = (event: MouseEvent) => {
      const currentRoot = editor.getRootElement();
      const container = outer();
      if (!currentRoot || !container) return;
      const mark = (event.target as Element).closest<HTMLElement>('[data-comment-id]');
      if (!mark || !currentRoot.contains(mark) || mark.closest('td,th')) return;
      clearTimeout(closeCard.current);
      const ids: string[] = []; let node: HTMLElement | null = mark;
      while (node && node !== currentRoot) { if (node.dataset.commentId) ids.unshift(node.dataset.commentId); node = node.parentElement; }
      const rect = mark.getBoundingClientRect(), base = container.getBoundingClientRect();
      setCard(previous => previous?.pinned ? previous : { ids, x: Math.max(0, rect.left - base.left), y: rect.bottom - base.top + 8, pinned: false });
    };
    const leave = () => { closeCard.current = setTimeout(() => setCard(previous => previous?.pinned ? previous : null), 180); };
    const click = (event: MouseEvent) => {
      const mark = (event.target as Element).closest<HTMLElement>('[data-comment-id]'); if (!mark || mark.closest('td,th')) return;
      setCard(previous => previous ? { ...previous, pinned: true } : previous);
      const rect = [...mark.getClientRects()].at(-1);
      if (mark.classList.contains('lex-comment-end') && rect && event.clientX >= rect.right - 20 && event.clientY >= rect.top - 6) latest.current.features?.openComment?.(mark.dataset.commentId!);
    };
    const drop = (event: DragEvent) => { if (event.dataTransfer?.files.length) event.preventDefault(); };
    const outside = (event: PointerEvent) => { if (!(event.target as Element).closest('.comment-compose-popover,.sel-bar,.slash-menu,.note-picker,.comment-card')) { closeCompose(); setMenu(null); setCard(null); } };
    const blur = () => setLine(null);
    root.addEventListener('mouseover', over); root.addEventListener('mouseleave', leave); root.addEventListener('click', click); root.addEventListener('drop', drop); root.addEventListener('blur', blur);
    document.addEventListener('pointerdown', outside);
    return () => { update(); paste(); tab(); formatCommand(); commentCommand(); askCommand(); clearTimeout(closeCard.current); root.removeEventListener('mouseover', over); root.removeEventListener('mouseleave', leave); root.removeEventListener('click', click); root.removeEventListener('drop', drop); root.removeEventListener('blur', blur); document.removeEventListener('pointerdown', outside); };
  }, [editor]);
  const candidates = menu?.kind === 'note' ? notes.map(note => ({ id: note.id, label: note.title, hint: note.agenda })) : slashItems;
  return <>
    {(code || callout) && anchor && <div className="lex-toolbar lex-block-bar" style={{ top: anchor.top, right: anchor.right }}>
      {code && <select aria-label="코드 언어" value={code.language} onChange={event => editor.update(() => { const node = $getNodeByKey(code.key); if ($isCodeNode(node)) node.setLanguage(event.target.value); })}>
        {!languages.includes(code.language) && <option value={code.language}>{code.language || '언어 없음'}</option>}{languages.map(language => <option key={language}>{language}</option>)}</select>}
      {callout && ['info', 'success', 'warn', 'danger'].map((kind, index) => <button key={kind} aria-pressed={calloutStyle(callout.kind) === kind}
        onMouseDown={event => event.preventDefault()} onClick={() => editor.update(() => { const node = $getNodeByKey(callout.key); if ($isCallout(node)) node.setKind(kind); })}>{['정보', '성공', '주의', '위험'][index]}</button>)}
    </div>}
    {line && !menu && <div className="lex-line-hint" style={{ left: line.x, top: line.y }}>
      <button type="button" className="lex-line-plus" title="블록 넣기" aria-label="블록 넣기" onMouseDown={event => event.preventDefault()} onClick={insertSlash}>+</button>
      <span aria-hidden="true">/ 를 눌러 표, 콜아웃, 코드 블록을 넣어요</span>
    </div>}
    {selectionBar && !compose && !menu && <div className="sel-bar" style={{ left: selectionBar.x, top: selectionBar.y }} onMouseDown={event => event.preventDefault()}>
      {!selectionBar.special && formats.map((value, index) => <button className={`sel-format-btn sel-format-${value}`} aria-pressed={selectionBar.formats[index]} key={value} onClick={() => format(value)}>{['B', 'I', 'S', '</>'][index]}</button>)}
      {props.features?.comment && <button className="comment-add-btn" onClick={beginComment}>코멘트</button>}
      {props.onAskAi && <button className="ask-add-btn comment-add-btn" onClick={beginAsk}>AI에게 묻기</button>}
    </div>}
    {compose && <div className="comment-compose-popover comment-compose" style={{ left: compose.x, top: compose.y }} onKeyDown={event => {
      if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); closeCompose(); }
      if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) { event.preventDefault(); event.stopPropagation(); submitComment(); }
    }}><blockquote>{compose.quote}</blockquote><textarea className="comment-input" aria-label="코멘트 내용" autoFocus value={comment} onChange={event => setComment(event.target.value)} />
      <div><button className="link-popover-btn" onClick={closeCompose}>취소</button><button className="link-popover-btn primary" disabled={!comment.trim()} onClick={submitComment}>추가</button></div></div>}
    {menu && <div className={menu.kind === 'note' ? 'note-picker' : 'slash-menu'} style={{ left: menu.x, top: menu.y }}>
      <input aria-label={menu.kind === 'note' ? '노트 링크 검색' : '블록 검색'} ref={input => input?.focus({ preventScroll: true })} value={menu.query} onChange={event => setMenu({ ...menu, query: event.target.value, selected: 0 })}
        onKeyDown={event => {
          if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); setMenu(null); editor.focus(); }
          if (event.key === 'ArrowUp' || event.key === 'ArrowDown') { event.preventDefault(); event.stopPropagation(); setMenu({ ...menu, selected: Math.max(0, Math.min(candidates.length - 1, menu.selected + (event.key === 'ArrowDown' ? 1 : -1))) }); }
          if (event.key === 'Enter') { event.preventDefault(); event.stopPropagation(); if (candidates[menu.selected]) void choose(candidates[menu.selected].id); }
        }} />
      {candidates.length ? candidates.map((item, index) => <button className={menu.kind === 'note' ? 'note-picker-item' : 'slash-item'} key={item.id} data-slash-id={item.id}
        aria-selected={menu.selected === index} onMouseDown={event => event.preventDefault()} onMouseEnter={() => setMenu({ ...menu, selected: index })} onClick={() => void choose(item.id)}>
        {'icon' in item && <span className="slash-item-icon" aria-hidden="true"><SlashIcon name={item.icon} /></span>}
        <span className="slash-item-label">{item.label}{'desc' in item && <small>{item.desc}</small>}</span>{'hint' in item && <small>{item.hint}</small>}</button>) : <p>일치하는 {menu.kind === 'note' ? '노트가' : '블록이'} 없어요.</p>}
    </div>}
    {card && <div className="comment-card" style={{ left: card.x, top: card.y }} onMouseEnter={() => clearTimeout(closeCard.current)}
      onMouseLeave={() => { if (!card.pinned) setCard(null); }} tabIndex={-1} onKeyDown={event => { if (event.key === 'Escape') { event.preventDefault(); event.stopPropagation(); setCard(null); } }}>
      {card.ids.map(id => <section key={id}><p>{props.features?.comments?.[id] || '이 코멘트를 못 찾았어요. 표시만 남아 있어요.'}</p><button onClick={() => { setCard(null); props.features?.openComment?.(id); }}>스레드에서 보기</button></section>)}</div>}
  </>;
}
