import { DecoratorNode, ElementNode, $applyNodeReplacement, $getNodeByKey, $createTextNode } from 'lexical';
import type { NodeKey, SerializedLexicalNode, SerializedElementNode, LexicalUpdateJSON, EditorConfig, LexicalEditor } from 'lexical';
import { LinkNode } from '@lexical/link';
import type { SerializedLinkNode, LinkAttributes } from '@lexical/link';
import { MarkNode } from '@lexical/mark';
import type { SerializedMarkNode } from '@lexical/mark';
import { useContext } from 'react';
import type { ReactNode } from 'react';
import { EditorContext } from './context.ts';
import { ImageView } from './media.tsx';
import { calloutStyle, relativePath } from './grammar.ts';

export class RelativeLinkNode extends LinkNode {
  static getType() { return 'ais-link'; }
  static clone(node: RelativeLinkNode) { return new RelativeLinkNode(node.__url, { rel: node.__rel, target: node.__target, title: node.__title }, node.__key); }
  static importJSON(value: SerializedLinkNode) { return new RelativeLinkNode(value.url).updateFromJSON(value); }
  sanitizeUrl(url: string) {
    return relativePath(url) || url.startsWith('#') || url.startsWith('/') || url.startsWith('~/') || /^file:\/\//i.test(url)
      ? url
      : super.sanitizeUrl(url);
  }
  constructor(url = '', attributes: LinkAttributes = {}, key?: NodeKey) { super(url, attributes, key); }
  createDOM(config: EditorConfig) {
    const element = super.createDOM(config);
    this.applyLinkTooltip(element);
    return element;
  }
  updateDOM(prevNode: this, element: HTMLAnchorElement, config: EditorConfig) {
    const changed = super.updateDOM(prevNode, element, config);
    this.applyLinkTooltip(element);
    return changed;
  }
  private applyLinkTooltip(element: HTMLElement) {
    const url = this.getURL();
    const isMac = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/i.test(navigator.platform || navigator.userAgent);
    const mod = isMac ? '⌘' : 'Ctrl';
    let action = '링크 열기';
    if (url.startsWith('/') || url.startsWith('~/') || /^file:\/\//i.test(url)) action = '기본 앱에서 파일 열기';
    else if (url.startsWith('#doc:')) action = '노트 열기';
    else if (url.startsWith('#sub:')) action = '하위 페이지 열기';
    else if (url.startsWith('#')) action = '소제목으로 이동';
    element.title = `${url}\n(${mod} + 클릭으로 ${action})`;
  }
}
export const $createRelativeLink = (url: string, text: string) => {
  const link = new RelativeLinkNode(url);
  link.append($createTextNode(text));
  return $applyNodeReplacement(link);
};

type CalloutJSON = SerializedElementNode & { kind: string };
export class CalloutNode extends ElementNode {
  __kind: string;
  constructor(kind = 'info', key?: NodeKey) { super(key); this.__kind = kind; }
  static getType() { return 'ais-callout'; }
  static clone(node: CalloutNode) { return new CalloutNode(node.__kind, node.__key); }
  static importJSON(value: CalloutJSON) { return new CalloutNode(value.kind).updateFromJSON(value); }
  exportJSON(): CalloutJSON { return { ...super.exportJSON(), kind: this.getKind() }; }
  getKind() { return this.getLatest().__kind; }
  setKind(kind: string) { this.getWritable().__kind = kind; return this; }
  isShadowRoot() { return true; }
  createDOM() { const element = document.createElement('aside'); element.className = `lex-callout lex-callout-${calloutStyle(this.__kind)}`; element.dataset.callout = this.__kind; return element; }
  updateDOM(previous: this, element: HTMLElement) {
    if (previous.__kind !== this.__kind) { element.className = `lex-callout lex-callout-${calloutStyle(this.__kind)}`; element.dataset.callout = this.__kind; }
    return false;
  }
  collapseAtStart() { const children = this.getChildren(); children.forEach(child => this.insertBefore(child)); this.remove(); return true; }
}
export const $createCallout = (kind = 'info') => $applyNodeReplacement(new CalloutNode(kind));
export const $isCallout = (node: unknown): node is CalloutNode => node instanceof CalloutNode;

type AnnotationJSON = SerializedMarkNode & { kind: 'c' | 'a' };
export class AnnotationNode extends MarkNode {
  __kind: 'c' | 'a';
  constructor(ids: readonly string[] = [], kind: 'c' | 'a' = 'c', key?: NodeKey) { super(ids, key); this.__kind = kind; }
  static getType() { return 'ais-annotation'; }
  static clone(node: AnnotationNode) { return new AnnotationNode(node.__ids, node.__kind, node.__key); }
  static importJSON(value: AnnotationJSON) { return new AnnotationNode(value.ids, value.kind).updateFromJSON(value); }
  updateFromJSON(value: LexicalUpdateJSON<AnnotationJSON>) { super.updateFromJSON(value); this.getWritable().__kind = value.kind; return this; }
  exportJSON(): AnnotationJSON { return { ...super.exportJSON(), kind: this.getKind() }; }
  getKind() { return this.getLatest().__kind; }
  createDOM() { const element = document.createElement('mark'); this.attributes(element); return element; }
  private attributes(element: HTMLElement) {
    element.className = `lex-annotation lex-${this.__kind === 'c' ? 'comment' : 'ask'}`;
    element.setAttribute(this.__kind === 'c' ? 'data-comment-id' : 'data-ask-id', this.__ids[0] || '');
  }
  updateDOM(_previous: this, element: HTMLElement) { this.attributes(element); return false; }
}
export const $createAnnotation = (ids: readonly string[], kind: 'c' | 'a') => $applyNodeReplacement(new AnnotationNode(ids, kind));
export const $isAnnotation = (node: unknown): node is AnnotationNode => node instanceof AnnotationNode;

// C201: a note chip carries the bold, italic, strikethrough and highlight around it.
// Inline code and underline never go on a chip.
export const CHIP_FORMATS = ['bold', 'italic', 'strikethrough', 'highlight'] as const;
export type ChipFormat = typeof CHIP_FORMATS[number];
const chipFormats = (value: unknown): ChipFormat[] => CHIP_FORMATS.filter(format => Array.isArray(value) && value.includes(format));
type ReferenceJSON = SerializedLexicalNode & { target: string; label: string; card: boolean; formats?: ChipFormat[] };
const FORMAT_TAGS: Record<ChipFormat, 'strong' | 'em' | 's' | 'mark'> = { bold: 'strong', italic: 'em', strikethrough: 's', highlight: 'mark' };
function ReferenceView({ target, label, card, formats = [] }: { target: string; label: string; card: boolean; formats?: ChipFormat[] }) {
  const features = useContext(EditorContext);
  const title = (card ? features.children : features.notes)?.find(note => note.id === target)?.title || label || '제목 없음';
  return <a className={card ? 'child-page-card' : 'doc-link-chip'} href={`#${card ? 'sub' : 'doc'}:${target}`}
    {...(card ? { 'data-child-page': target } : { 'data-doc-link': target })} onClick={event => {
      event.preventDefault(); event.stopPropagation();
      if (card) features.openChild?.(target); else if (event.metaKey || event.ctrlKey) {
        if (features.notes?.some(note => note.id === target)) features.openNote?.(target);
        else void features.host?.warn('노트를 찾을 수 없어요');
      }
    }}>{card && <span aria-hidden="true">↳ </span>}{formats.reduce<ReactNode>((inner, format) => { const Tag = FORMAT_TAGS[format]; return <Tag>{inner}</Tag>; }, title)}</a>;
}
export class ReferenceNode extends DecoratorNode<ReactNode> {
  __target: string; __label: string; __card: boolean; __formats: ChipFormat[];
  constructor(target = '', label = '', card = false, formats: ChipFormat[] = [], key?: NodeKey) {
    super(key); this.__target = target; this.__label = label; this.__card = card; this.__formats = card ? [] : chipFormats(formats);
  }
  static getType() { return 'ais-reference'; }
  static clone(node: ReferenceNode) { return new ReferenceNode(node.__target, node.__label, node.__card, node.__formats, node.__key); }
  static importJSON(value: ReferenceJSON) { return new ReferenceNode(value.target, value.label, value.card, chipFormats(value.formats)); }
  exportJSON(): ReferenceJSON {
    const formats = this.getFormats();
    return { ...super.exportJSON(), target: this.getTarget(), label: this.getLabel(), card: this.isCard(), ...(formats.length ? { formats } : {}) };
  }
  getTarget() { return this.getLatest().__target; } getLabel() { return this.getLatest().__label; } isCard() { return this.getLatest().__card; }
  getFormats() { return this.getLatest().__formats; }
  hasFormat(format: string) { return (this.getFormats() as string[]).includes(format); }
  setFormats(formats: readonly string[]) { if (!this.__card) this.getWritable().__formats = chipFormats(formats); }
  getTextContent() { return this.getLabel(); }
  isInline() { return !this.__card; }
  createDOM() { const element = document.createElement(this.__card ? 'div' : 'span'); element.contentEditable = 'false'; return element; }
  updateDOM() { return false; }
  exportDOM() { const element = document.createElement('a'); element.href = `#${this.isCard() ? 'sub' : 'doc'}:${this.getTarget()}`; element.textContent = this.getLabel(); return { element }; }
  decorate() { return <ReferenceView target={this.getTarget()} label={this.getLabel()} card={this.isCard()} formats={this.getFormats()} />; }
}
export const $createReference = (target: string, label: string, card = false, formats: readonly string[] = []) =>
  $applyNodeReplacement(new ReferenceNode(target, label, card, chipFormats(formats)));
export const $isReference = (node: unknown): node is ReferenceNode => node instanceof ReferenceNode;

type ImageJSON = SerializedLexicalNode & { source: string; alt: string; width?: number };
/**
 * 노트에 넣은 이미지입니다. 마크다운에서는 `![alt](주소)`, 크기를 정하면 `![alt|너비](주소)` 입니다.
 * `__width`는 화면에 그리는 가로(px)이고, 0이면 크기를 정하지 않아 본문 폭을 넘지 않는 원래 크기로 보입니다.
 */
export class ImageNode extends DecoratorNode<ReactNode> {
  __source: string; __alt: string; __width: number;
  constructor(source = '', alt = '', width = 0, key?: NodeKey) { super(key); this.__source = source; this.__alt = alt; this.__width = width; }
  static getType() { return 'ais-image'; }
  static clone(node: ImageNode) { return new ImageNode(node.__source, node.__alt, node.__width, node.__key); }
  static importJSON(value: ImageJSON) { return new ImageNode(value.source, value.alt, value.width ?? 0); }
  exportJSON(): ImageJSON { return { ...super.exportJSON(), source: this.getSource(), alt: this.getAlt(), width: this.getWidth() }; }
  getSource() { return this.getLatest().__source; } getAlt() { return this.getLatest().__alt; } getWidth() { return this.getLatest().__width; }
  setWidth(width: number) { this.getWritable().__width = width > 0 ? Math.round(width) : 0; return this; }
  isInline() { return true; }
  createDOM() { const element = document.createElement('span'); element.className = 'lex-image'; element.contentEditable = 'false'; return element; }
  updateDOM() { return false; }
  exportDOM() { const element = document.createElement('img'); element.setAttribute('src', this.getSource()); element.alt = this.getAlt(); if (this.getWidth() > 0) element.setAttribute('width', String(this.getWidth())); return { element }; }
  decorate(editor: LexicalEditor) {
    const key = this.getKey();
    return <ImageView source={this.getSource()} alt={this.getAlt()} width={this.getWidth()} nodeKey={key}
      onWidth={width => editor.update(() => { const node = $getNodeByKey(key); if ($isImage(node)) node.setWidth(width); })} />;
  }
}
export const $createImage = (source: string, alt = '', width = 0) => $applyNodeReplacement(new ImageNode(source, alt, width));
export const $isImage = (node: unknown): node is ImageNode => node instanceof ImageNode;
