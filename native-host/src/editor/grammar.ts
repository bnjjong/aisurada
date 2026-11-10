import { markPairs, charFormats, IMAGE_PATTERN, CHIP_PATTERN, LINK_PATTERN } from '../data/mdInline.ts';

// 9.9: a fence opens on a line whose text after leading spaces and tabs is three
// or more backticks, and closes only on a line of backticks at least as long. A
// one-line fence closes on its own line. Line joining, callout lifting, list
// indents and marker placeholders all ask this one judge (C88, C179, C181, C186, C187).
export function fenceTracker() {
  let open = 0;
  return (raw: string): boolean => {
    const line = raw.replace(/\r$/, '');
    if (open) {
      if (new RegExp(`^[ \\t]*\`{${open},}[ \\t]*$`).test(line)) open = 0;
      return true;
    }
    const start = /^[ \t]*(`{3,})(.*)$/.exec(line);
    if (!start) return false;
    const rest = start[2].trimEnd(), close = new RegExp(`\`{${start[1].length},}$`).exec(rest);
    if (!(close && close.index > 0)) open = start[1].length;
    return true;
  };
}
export const fenceLine = (line: string) => /^[ \t]*`{3,}/.test(line);

// 9.4: the lines that open a block and so never join the line before them.
const blockLine = (line: string) => /^(?:#{1,6}\s|>|\s*(?:[-*+]|\d+[.)])\s|\s*\|.*\|\s*$)/.test(line) || fenceLine(line)
  || /^\s*(?:(?:-\s*){3,}|(?:\*\s*){3,}|(?:_\s*){3,})$/.test(line);
const hardBreak = (line: string) => {
  if (/ {2,}$/.test(line)) return true;
  const slashes = line.match(/\\+$/)?.[0].length || 0;
  return slashes % 2 === 1;
};

export function foldParagraphs(input: string): string {
  const lines = input.replace(/\r\n?/g, '\n').split('\n'), output: string[] = [], fenced = fenceTracker();
  const block = blockLine;
  for (const line of lines) {
    if (fenced(line)) { output.push(line); continue; }
    const previous = output.at(-1);
    if (previous !== undefined && previous.trim() && line.trim() && !block(previous) && !block(line)
      && !hardBreak(previous)) output[output.length - 1] += ' ' + line;
    else output.push(line);
  }
  return output.join('\n');
}

// C198: within one block (following quote lines including a callout body, a list item
// with its indented continuation lines, a paragraph joined by hard breaks) a format
// that crosses a line break is closed at the line end and reopened on the next line,
// because the editor imports line by line. The editor import and the reading view
// both pass here. A blank > line, and a heading or list shaped line in a quote, end
// the block; an unindented continuation line does not join it. A block stays as it
// came if pairing the rewritten lines one by one shows different formats, if a
// crossing mark sits inside link syntax, or if the block is over 10,000 characters.
// `paragraphs` also treats soft-wrapped paragraph lines as one block (reading an
// external file, which is not folded first).
export function closeBlockEmphasis(markdown: string, paragraphs = false): string {
  const lines = markdown.split('\n'), fenced = fenceTracker();
  type Part = { line: number; prefix: number; end: number };
  const blocks: Part[][] = [];
  let kind = '', current: Part[] = [];
  const close = () => { if (current.length > 1) blocks.push(current); current = []; kind = ''; };
  const part = (line: number, prefix: number) => {
    const text = lines[line], suffix = /(?: {2,}|\\)$/.exec(text.slice(prefix));
    return { line, prefix, end: suffix ? text.length - suffix[0].length : text.length };
  };
  lines.forEach((line, n) => {
    if (fenced(line)) return close();
    const quote = /^> ?/.exec(line);
    if (quote) {
      const content = line.slice(quote[0].length);
      if (!content.trim() || /^(?:#{1,6}\s|\s*(?:[-*+]|\d+\.)\s|\[!|>)/.test(content)) return close();
      if (kind !== 'quote') { close(); kind = 'quote'; }
      return void current.push(part(n, quote[0].length));
    }
    const item = /^\s*(?:[-*+]|\d+\.)\s+(?:\[[ xX/-]\]\s+)?/.exec(line);
    if (item) { close(); kind = 'list'; return void current.push(part(n, item[0].length)); }
    if (kind === 'list' && /^\s+\S/.test(line) && !blockLine(line)) return void current.push(part(n, /^\s*/.exec(line)![0].length));
    if (line.trim() && !blockLine(line)) {
      const previous = current.at(-1);
      if (kind === 'para' && previous && (paragraphs || hardBreak(lines[previous.line]))) return void current.push(part(n, 0));
      close(); kind = 'para'; return void current.push(part(n, 0));
    }
    close();
  });
  close();
  for (const block of blocks) {
    const contents = block.map(({ line, prefix, end }) => lines[line].slice(prefix, end)), joined = contents.join('\n');
    if (joined.length > 10000) continue;
    const starts: number[] = [];
    contents.reduce((at, text) => { starts.push(at); return at + text.length + 1; }, 0);
    const lineOf = (offset: number) => starts.filter(start => start <= offset).length - 1;
    const crossing = markPairs(joined).filter(pair => lineOf(pair.open) !== lineOf(pair.close));
    if (!crossing.length) continue;
    const links = [IMAGE_PATTERN, CHIP_PATTERN, LINK_PATTERN].flatMap(pattern => [...joined.matchAll(new RegExp(pattern.source, 'g'))])
      .map(match => [match.index!, match.index! + match[0].length]);
    if (crossing.some(pair => links.some(([from, to]) => [pair.open, pair.close].some(at => at >= from && at < to)))) continue;
    const opens = contents.map(() => [] as string[]), ends = contents.map(() => [] as string[]);
    for (const pair of crossing) {
      for (let n = lineOf(pair.open); n < lineOf(pair.close); n++) ends[n].unshift(pair.tag);
      for (let n = lineOf(pair.open) + 1; n <= lineOf(pair.close); n++) opens[n].push(pair.tag);
    }
    const rewritten = contents.map((text, n) => opens[n].join('') + text + ends[n].join(''));
    const want = charFormats(joined).filter(item => item.ch !== '\n'), got = rewritten.flatMap(text => charFormats(text));
    if (want.length !== got.length || want.some((item, n) => item.ch !== got[n].ch || item.formats !== got[n].formats)) continue;
    block.forEach(({ line, prefix, end }, n) => { lines[line] = lines[line].slice(0, prefix) + rewritten[n] + lines[line].slice(end); });
  }
  return lines.join('\n');
}

// C186: code lines that look like list items keep their spaces.
const isHrLine = (line: string) => /^\s*(?:(?:-\s*){3,}|(?:\*\s*){3,}|(?:_\s*){3,})$/.test(line);
export function doubleListIndent(text: string): string {
  const fenced = fenceTracker();
  return text.split('\n').map(line => (fenced(line) || isHrLine(line)) ? line : line.replace(/^( +)(?=(?:[-*+]|\d+[.)])\s)/, indent => indent + indent)).join('\n');
}
export function halveListIndent(text: string): string {
  return text.split('\n').map(line => isHrLine(line) ? line : line.replace(/^( +)(?=(?:[-*+]|\d+[.)])\s)/, indent => ' '.repeat(Math.round(indent.length / 2)))).join('\n');
}
export function tableCells(line: string): string[] {
  let text = line.trim().slice(1);
  if (text.endsWith('|') && text.at(-2) !== '\\') text = text.slice(0, -1);
  const cells: string[] = []; let start = 0;
  for (let i = 0; i < text.length; i++) if (text[i] === '|' && text[i - 1] !== '\\') { cells.push(text.slice(start, i)); start = i + 1; }
  cells.push(text.slice(start));
  return cells.map(cell => cell.trim().replace(/\\\|/g, '|'));
}
export function dividerWidths(line: string): number[] | null {
  if (!/^\|.*\|\s?$/.test(line)) return null;
  const cells = line.replace(/\s$/, '').slice(1, -1).split('|');
  const matches = cells.map(cell => /^ ?:?(-+):? ?$/.exec(cell));
  return matches.every(Boolean) ? matches.map(match => match![1].length) : null;
}
export function widthHint(widths: number[]): number[] | undefined {
  return widths.length > 1 && Math.max(...widths) - Math.min(...widths) >= 3 ? widths : undefined;
}
export function widthDashes(widths: readonly number[] | undefined, count: number): number[] {
  if (!widths || widths.length !== count || !widths.every(n => Number.isFinite(n) && n > 0)) return Array(count).fill(3);
  const total = widths.reduce((a, b) => a + b, 0), budget = Math.max(count * 16, 40);
  const dashes = widths.map(width => Math.max(3, Math.round(width / total * budget)));
  return widthHint(dashes) || Array(count).fill(3);
}
export const markerPattern = /<!--(\/?)(ais[ca]):((?:cm|ak)[a-z0-9]+-[a-z0-9]{1,8})-->/gi;
// C182: marker syntax written in a code fence or an inline code span is code text.
// `map` sees only the text outside code, in document order.
export function mapOutsideCode(text: string, map: (part: string) => string): string {
  const fenced = fenceTracker();
  return text.split('\n').map(line => {
    if (fenced(line)) return line;
    let out = '', plain = '', i = 0;
    while (i < line.length) {
      if (line[i] !== '`') { plain += line[i++]; continue; }
      const run = /^`+/.exec(line.slice(i))![0];
      let end = line.indexOf(run, i + run.length);
      while (end !== -1 && (line[end - 1] === '`' || line[end + run.length] === '`')) end = line.indexOf(run, end + 1);
      if (end === -1) { plain += run; i += run.length; continue; }
      out += map(plain) + line.slice(i, end + run.length); plain = ''; i = end + run.length;
    }
    return out + map(plain);
  }).join('\n');
}
export const stripMarkers = (text: string) => mapOutsideCode(text, part => part.replace(markerPattern, ''));
export interface MarkerPair { open: number; close: number; id: string; kind: 'c' | 'a'; }
export function markerPlaceholders(input: string): { text: string; pairs: MarkerPair[]; marked: boolean } {
  let count = 0;
  const opens = new Map<string, { index: number; kind: 'c' | 'a' }>(), pairs: MarkerPair[] = [];
  const text = mapOutsideCode(input, part => part.replace(markerPattern, (_all, closing: string, type: string, id: string) => {
    const index = count++, kind = type.toLowerCase().at(-1) as 'c' | 'a';
    const valid = kind === 'c' ? /^cm/i.test(id) : /^ak/i.test(id);
    if (valid && !closing) opens.set(id, { index, kind });
    if (valid && closing) {
      const open = opens.get(id);
      if (open && open.kind === kind) { pairs.push({ open: open.index, close: index, id, kind }); opens.delete(id); }
    }
    return `\ue000${closing ? 'c' : 'o'}:${index}:${id}\ue000`;
  }));
  return { text, pairs: pairs.sort((a, b) => a.open - b.open), marked: count > 0 };
}
export function relativePath(raw: string): string | null {
  try {
    const value = decodeURIComponent(raw.split(/[?#]/)[0]);
    if (!value || /^[a-z][a-z\d+.-]*:/i.test(value) || /^[/#]/.test(value) || /[\\\x00-\x1f\x7f]/.test(value)
      || value.split('/').some(part => !part || part === '..')) return null;
    return value;
  } catch { return null; }
}
export const calloutStyle = (kind: string) => /^(tip|success|check|done)$/i.test(kind) ? 'success'
  : /^(warning|warn|caution|attention)$/i.test(kind) ? 'warn' : /^(danger|error|fail|bug)$/i.test(kind) ? 'danger' : 'info';
export const headingSlug = (text: string) => text.trim().toLowerCase().replace(/[^\p{L}\p{N}\s_-]/gu, '').replace(/\s+/g, '-');
