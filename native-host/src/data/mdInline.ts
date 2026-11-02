// One inline pairing for the editor import, the reading view and the screen title
// (9.5). It is the editor library's CommonMark pairing (Lexical 0.47 scanCodeSpans,
// scanDelimiters, processEmphasis) with the four changes appendix C decided:
// C178  Hangul, Han or kana outside a mark lets it open or close after punctuation.
// C199  CJK punctuation and fullwidth forms count as punctuation.
// C200  Marker placeholder characters count as punctuation.
// C197  A code span and an emphasis that do not overlap are taken in document order.

export type InlineFormat = 'bold' | 'italic' | 'strikethrough' | 'highlight' | 'code';
export interface FormatMatch { kind: 'code' | 'mark'; start: number; end: number; tag: string; content: string; }
export interface LinkMatch { kind: 'image' | 'chip' | 'link'; start: number; end: number; label: string; match: RegExpMatchArray; }

export const TAG_FORMATS: Record<string, InlineFormat[]> = {
  '`': ['code'], '***': ['bold', 'italic'], '___': ['bold', 'italic'], '**': ['bold'], '__': ['bold'],
  '*': ['italic'], '_': ['italic'], '~~': ['strikethrough'], '==': ['highlight'],
};
// The editor's image, note chip and link import patterns, in their winning order.
export const IMAGE_PATTERN = /!\[([^\]]*)\]\(([^\s)]+)\)/;
export const CHIP_PATTERN = /\[([^\]]+)\]\(#doc:([^\s)]+)\)/;
export const LINK_PATTERN = /(?:\[(.+?)\])(?:\((?:([^()\s]+)(?:\s"((?:[^"]*\\")*[^"]*)"\s*)?)\))/;

const WHITESPACE = /\s/;
const PUNCTUATION = /[!"#$%&'()*+,\-./:;<=>?@[\]^_`{|}~　-〿！-／：-＠［-｀｛-･-]/;
const CJK = /[\p{Script=Hangul}\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}　-〿＀-￯]/u;
const escaped = (text: string, index: number) => { let count = 0; for (let i = index - 1; i >= 0 && text[i] === '\\'; i--) count++; return count % 2 === 1; };

function flanking(text: string, index: number, length: number, left: boolean) {
  const before = text[index - 1], after = text[index + length];
  const [inner, outer] = left ? [after, before] : [before, after];
  if (inner === undefined || WHITESPACE.test(inner)) return false;
  if (!PUNCTUATION.test(inner)) return true;
  return outer === undefined || WHITESPACE.test(outer) || PUNCTUATION.test(outer) || CJK.test(outer);
}
function canEmphasis(char: string, text: string, index: number, length: number, open: boolean) {
  if (!flanking(text, index, length, open)) return false;
  if (char !== '_' || !flanking(text, index, length, !open)) return true;
  const adjacent = open ? text[index - 1] : text[index + length];
  return adjacent !== undefined && PUNCTUATION.test(adjacent);
}

export function codeSpans(text: string): { start: number; end: number; content: string }[] {
  const runs: { index: number; length: number }[] = [];
  for (let i = 0; i < text.length;) {
    if (text[i] !== '`') { i++; continue; }
    let length = 1; while (text[i + length] === '`') length++;
    runs.push({ index: i, length }); i += length;
  }
  const spans: { start: number; end: number; content: string }[] = [];
  for (let open = 0; open < runs.length;) {
    const opener = runs[open];
    if (escaped(text, opener.index)) { open++; continue; }
    const close = runs.findIndex((run, n) => n > open && run.length === opener.length);
    if (close === -1) { open++; continue; }
    let content = text.slice(opener.index + opener.length, runs[close].index);
    if (content.length >= 2 && content.startsWith(' ') && content.endsWith(' ') && /[^ ]/.test(content)) content = content.slice(1, -1);
    spans.push({ start: opener.index, end: runs[close].index + runs[close].length, content });
    open = close + 1;
  }
  return spans;
}

interface Delimiter { char: string; index: number; length: number; original: number; open: boolean; close: boolean; active: boolean; }
function outermostEmphasis(text: string, exclude: { start: number; end: number }[]): FormatMatch | null {
  const delimiters: Delimiter[] = [];
  for (let i = 0; i < text.length;) {
    const char = text[i];
    if (!'*_~='.includes(char) || escaped(text, i) || exclude.some(range => i >= range.start && i < range.end)) { i++; continue; }
    let length = 1; while (text[i + length] === char) length++;
    const open = canEmphasis(char, text, i, length, true), close = canEmphasis(char, text, i, length, false);
    if (open || close) delimiters.push({ char, index: i, length, original: length, open, close, active: true });
    i += length;
  }
  const bottoms: Record<string, number> = {};
  let result: FormatMatch | null = null;
  for (let at = 0; at < delimiters.length;) {
    const closer = delimiters[at];
    if (!closer.active || !closer.close || !closer.length) { at++; continue; }
    const key = closer.char + closer.open;
    let found = false;
    for (let n = at - 1; n > (bottoms[key] ?? -1); n--) {
      const opener = delimiters[n];
      if (!opener.active || !opener.open || !opener.length || opener.char !== closer.char) continue;
      if ((opener.close || closer.open) && (opener.original + closer.original) % 3 === 0 && opener.original % 3 && closer.original % 3) continue;
      const most = Math.min(opener.length, closer.length);
      const tag = Object.keys(TAG_FORMATS).filter(item => item[0] === opener.char && item.length <= most).sort((a, b) => b.length - a.length)[0];
      if (!tag) continue;
      found = true;
      const match: FormatMatch = { kind: 'mark', tag, start: opener.index + opener.length - tag.length, end: closer.index + tag.length,
        content: text.slice(opener.index + opener.length, closer.index) };
      if (!result || match.start < result.start || match.start === result.start && match.end > result.end) result = match;
      for (let j = n + 1; j < at; j++) delimiters[j].active = false;
      opener.length -= tag.length; closer.length -= tag.length; opener.active = opener.length > 0;
      if (closer.length > 0) closer.index += tag.length; else { closer.active = false; at++; }
      break;
    }
    if (!found) { bottoms[key] = at - 1; if (!closer.open) closer.active = false; at++; }
  }
  return result;
}

/** The code span or emphasis the editor applies first in `text` (C197: document order). */
export function outermostFormat(text: string): FormatMatch | null {
  const spans = codeSpans(text), code = spans[0] ? { kind: 'code' as const, tag: '`', ...spans[0] } : null;
  const mark = outermostEmphasis(text, spans);
  if (code && mark) return mark.start < code.start ? mark : code;
  return code || mark;
}

/** The image, note chip or link the editor applies first (library text-match order). */
export function outermostLink(text: string): LinkMatch | null {
  let found: LinkMatch | null = null;
  for (const [kind, pattern] of [['image', IMAGE_PATTERN], ['chip', CHIP_PATTERN], ['link', LINK_PATTERN]] as const) {
    const match = text.match(pattern);
    if (!match) continue;
    const start = match.index || 0, end = start + match[0].length;
    if (!found || start < found.start && (end > found.end || end <= found.start)) found = { kind, start, end, label: match[1] ?? '', match };
  }
  return found;
}

/** The library's choice between a format and a text match (code spans bind tighter). */
export function outermostInline(text: string): FormatMatch | LinkMatch | null {
  const format = outermostFormat(text), link = outermostLink(text);
  if (!format || !link) return format || link;
  if (format.kind === 'code') return link.start <= format.start && link.end >= format.end ? link : format;
  return format.start <= link.start && format.end >= link.end || link.start > format.end ? format : link;
}

const unescape = (text: string) => text.replace(/\\([!-/:-@[-`{-~])/g, '$1').replace(/&#(\d+);/g, (all, code) => {
  const value = Number(code); return value > 0 && value <= 0x10ffff ? String.fromCodePoint(value) : all;
});

/**
 * The text one markdown inline run shows in the editor: formats applied the same way,
 * escapes and decimal entities resolved, link and chip labels kept, images dropped.
 */
export function plainInline(source: string, first = true): string {
  const found = outermostInline(source);
  if (!found) return unescape(source);
  const before = source.slice(0, found.start), after = source.slice(found.end);
  let middle: string;
  if (found.kind === 'code') middle = first && found.start === 0 ? unescape(found.content) : found.content;
  else if (found.kind === 'mark') middle = plainInline(found.content, false);
  else middle = found.kind === 'image' ? '' : plainInline((found as LinkMatch).label, false);
  return (before ? plainInline(before, first) : '') + middle + (after ? plainInline(after, false) : '');
}

// Reading view: paired marks become private-use sentinels so the reading renderer
// draws exactly the editor's pairs and never pairs anything itself.
export const SENTINEL: Record<string, [string, string]> = {
  bold: ['', ''], italic: ['', ''], strikethrough: ['', ''], highlight: ['', ''],
};
export function sentinelInline(source: string): string {
  const found = outermostInline(source);
  if (!found) return source;
  const before = source.slice(0, found.start), after = source.slice(found.end), whole = source.slice(found.start, found.end);
  let middle = whole;
  if (found.kind === 'mark') {
    const formats = TAG_FORMATS[found.tag];
    middle = formats.map(format => SENTINEL[format][0]).join('') + sentinelInline(found.content) + formats.map(format => SENTINEL[format][1]).reverse().join('');
  } else if (found.kind !== 'code' && (found as LinkMatch).label) {
    const label = (found as LinkMatch).label, at = whole.indexOf(label);
    middle = whole.slice(0, at) + sentinelInline(label) + whole.slice(at + label.length);
  }
  return (before ? sentinelInline(before) : '') + middle + (after ? sentinelInline(after) : '');
}
