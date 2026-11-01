export function foldParagraphs(input: string): string {
  const lines = input.replace(/\r\n?/g, '\n').split('\n'), output: string[] = [];
  let fenced = false;
  const block = (line: string) => /^(?:#{1,6}\s|>|`{3}|\s*(?:[-*+]|\d+\.)\s|\s*\|.*\|\s*$)/.test(line)
    || /^\s*(?:(?:-\s*){3,}|(?:\*\s*){3,}|(?:_\s*){3,})$/.test(line);
  for (const line of lines) {
    if (/^\s*`{3}/.test(line)) { fenced = !fenced; output.push(line); continue; }
    const previous = output.at(-1);
    if (!fenced && previous !== undefined && previous.trim() && line.trim() && !block(previous) && !block(line)
      && !/(?: {2,}|\\)$/.test(previous)) output[output.length - 1] += ' ' + line.trimStart();
    else output.push(line);
  }
  return output.join('\n');
}

export function doubleListIndent(text: string): string {
  return text.replace(/^( +)(?=(?:[-*+]|\d+[.)])\s)/gm, indent => indent + indent);
}
export function halveListIndent(text: string): string {
  return text.replace(/^( +)(?=(?:[-*+]|\d+[.)])\s)/gm, indent => ' '.repeat(Math.round(indent.length / 2)));
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
export const markerPattern = /<!--(\/?)(wl[ca]):((?:cm|ak)[a-z0-9]+-[a-z0-9]{1,8})-->/gi;
export const stripMarkers = (text: string) => text.replace(markerPattern, '');
export interface MarkerPair { open: number; close: number; id: string; kind: 'c' | 'a'; }
export function markerPlaceholders(input: string): { text: string; pairs: MarkerPair[]; marked: boolean } {
  let count = 0;
  const opens = new Map<string, { index: number; kind: 'c' | 'a' }>(), pairs: MarkerPair[] = [];
  const text = input.replace(markerPattern, (_all, closing: string, type: string, id: string) => {
    const index = count++, kind = type.toLowerCase().at(-1) as 'c' | 'a';
    const valid = kind === 'c' ? /^cm/i.test(id) : /^ak/i.test(id);
    if (valid && !closing) opens.set(id, { index, kind });
    if (valid && closing) {
      const open = opens.get(id);
      if (open && open.kind === kind) { pairs.push({ open: open.index, close: index, id, kind }); opens.delete(id); }
    }
    return `\ue000${closing ? 'c' : 'o'}:${index}:${id}\ue000`;
  });
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
