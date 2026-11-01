import { DecoratorNode, ElementNode, $applyNodeReplacement } from 'lexical';
import type { NodeKey, SerializedLexicalNode, SerializedElementNode, LexicalUpdateJSON } from 'lexical';
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
  static getType() { return 'wl-link'; }
  static clone(node: RelativeLinkNode) { return new RelativeLinkNode(node.__url, { rel: node.__rel, target: node.__target, title: node.__title }, node.__key); }
  static importJSON(value: SerializedLinkNode) { return new RelativeLinkNode(value.url).updateFromJSON(value); }
  sanitizeUrl(url: string) { return relativePath(url) || url.startsWith('#') ? url : super.sanitizeUrl(url); }
  constructor(url = '', attributes: LinkAttributes = {}, key?: NodeKey) { super(url, attributes, key); }
}

type CalloutJSON = SerializedElementNode & { kind: string };
export class CalloutNode extends ElementNode {
  __kind: string;
  constructor(kind = 'info', key?: NodeKey) { super(key); this.__kind = kind; }
  static getType() { return 'wl-callout'; }
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
  static getType() { return 'wl-annotation'; }
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

type ReferenceJSON = SerializedLexicalNode & { target: string; label: string; card: boolean };
function ReferenceView({ target, label, card }: { target: string; label: string; card: boolean }) {
  const features = useContext(EditorContext);
  const title = (card ? features.children : features.notes)?.find(note => note.id === target)?.title || label || '제목 없음';
  return <a className={card ? 'child-page-card' : 'doc-link-chip'} href={`#${card ? 'sub' : 'doc'}:${target}`}
    {...(card ? { 'data-child-page': target } : { 'data-doc-link': target })} onClick={event => {
      event.preventDefault(); event.stopPropagation();
      if (card) features.openChild?.(target); else if (event.metaKey || event.ctrlKey) {
        if (features.notes?.some(note => note.id === target)) features.openNote?.(target);
        else void features.host?.warn('노트를 찾을 수 없어요');
      }
    }}>{card && <span aria-hidden="true">↳ </span>}{title}</a>;
}
export class ReferenceNode extends DecoratorNode<ReactNode> {
  __target: string; __label: string; __card: boolean;
  constructor(target = '', label = '', card = false, key?: NodeKey) { super(key); this.__target = target; this.__label = label; this.__card = card; }
  static getType() { return 'wl-reference'; }
  static clone(node: ReferenceNode) { return new ReferenceNode(node.__target, node.__label, node.__card, node.__key); }
  static importJSON(value: ReferenceJSON) { return new ReferenceNode(value.target, value.label, value.card); }
  exportJSON(): ReferenceJSON { return { ...super.exportJSON(), target: this.getTarget(), label: this.getLabel(), card: this.isCard() }; }
  getTarget() { return this.getLatest().__target; } getLabel() { return this.getLatest().__label; } isCard() { return this.getLatest().__card; }
  getTextContent() { return this.getLabel(); }
  isInline() { return !this.__card; }
  createDOM() { const element = document.createElement(this.__card ? 'div' : 'span'); element.contentEditable = 'false'; return element; }
  updateDOM() { return false; }
  exportDOM() { const element = document.createElement('a'); element.href = `#${this.isCard() ? 'sub' : 'doc'}:${this.getTarget()}`; element.textContent = this.getLabel(); return { element }; }
  decorate() { return <ReferenceView target={this.getTarget()} label={this.getLabel()} card={this.isCard()} />; }
}
export const $createReference = (target: string, label: string, card = false) => $applyNodeReplacement(new ReferenceNode(target, label, card));
export const $isReference = (node: unknown): node is ReferenceNode => node instanceof ReferenceNode;

type ImageJSON = SerializedLexicalNode & { source: string; alt: string };
export class ImageNode extends DecoratorNode<ReactNode> {
  __source: string; __alt: string;
  constructor(source = '', alt = '', key?: NodeKey) { super(key); this.__source = source; this.__alt = alt; }
  static getType() { return 'wl-image'; }
  static clone(node: ImageNode) { return new ImageNode(node.__source, node.__alt, node.__key); }
  static importJSON(value: ImageJSON) { return new ImageNode(value.source, value.alt); }
  exportJSON(): ImageJSON { return { ...super.exportJSON(), source: this.getSource(), alt: this.getAlt() }; }
  getSource() { return this.getLatest().__source; } getAlt() { return this.getLatest().__alt; }
  isInline() { return true; }
  createDOM() { const element = document.createElement('span'); element.className = 'lex-image'; element.contentEditable = 'false'; return element; }
  updateDOM() { return false; }
  exportDOM() { const element = document.createElement('img'); element.setAttribute('src', this.getSource()); element.alt = this.getAlt(); return { element }; }
  decorate() { return <ImageView source={this.getSource()} alt={this.getAlt()} />; }
}
export const $createImage = (source: string, alt = '') => $applyNodeReplacement(new ImageNode(source, alt));
export const $isImage = (node: unknown): node is ImageNode => node instanceof ImageNode;
