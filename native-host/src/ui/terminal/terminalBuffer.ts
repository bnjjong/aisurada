// 14.9: logical lines read from the terminal's normal buffer, and the user's selection
// turned into a range of those lines. The terminal library (xterm 6.0.0) owns what a
// buffer row holds; this file only reads it through the small interface below.

export interface BufferLineLike { isWrapped: boolean; translateToString(trimRight?: boolean): string; }
export interface BufferLike { length: number; getLine(row: number): BufferLineLike | undefined; }
export interface SelectionPosition { start: { x: number; y: number }; end: { x: number; y: number }; }

/**
 * The buffer read from the top down. A row the terminal wrapped is joined to the end of the
 * row before it, so a secret split across the wrap is still one line. Only the right end of
 * a logical line loses its cells that never held a character; a space the shell printed
 * stays. `rowToLine[row]` is the logical line each buffer row belongs to.
 */
export function readLogicalLines(buffer: BufferLike): { lines: string[]; rowToLine: number[] } {
  const lines: string[] = [], rowToLine: number[] = [];
  for (let row = 0; row < buffer.length; row++) {
    const line = buffer.getLine(row);
    if (!line) { rowToLine.push(Math.max(0, lines.length - 1)); continue; }
    const followedByWrap = buffer.getLine(row + 1)?.isWrapped === true;
    const text = line.translateToString(!followedByWrap);
    if (line.isWrapped && lines.length) { lines[lines.length - 1] += text; rowToLine.push(lines.length - 1); }
    else { lines.push(text); rowToLine.push(lines.length - 1); }
  }
  return { lines, rowToLine };
}

export const SELECTION_REASONS = {
  tui: 'TUI 가 떠 있어서 고른 부분은 못 써요. 「마지막 N줄」은 그대로 쓸 수 있어요.',
  none: '터미널에서 굳힐 부분을 먼저 드래그해 주세요.',
  missing: '고른 자리를 스크롤백에서 못 찾았어요. 다시 골라 주세요.',
} as const;

/**
 * The logical lines a selection touches, widened to whole lines so a secret is never cut in
 * half. Rows count from 0, and an end column of 0 means the row before. While a TUI is up
 * the selection is in the alternate screen's rows but the lines read are the normal
 * buffer's, so the same row would be another text: it is refused.
 */
export function selectionToLines(position: SelectionPosition | undefined, rowToLine: readonly number[], tuiUp: boolean):
  { range: [number, number] } | { reason: string } {
  if (tuiUp) return { reason: SELECTION_REASONS.tui };
  if (!position) return { reason: SELECTION_REASONS.none };
  const startRow = position.start.y, endRow = position.end.x === 0 && position.end.y > startRow ? position.end.y - 1 : position.end.y;
  if (startRow < 0 || startRow >= rowToLine.length || endRow < startRow) return { reason: SELECTION_REASONS.missing };
  return { range: [rowToLine[startRow], rowToLine[Math.min(endRow, rowToLine.length - 1)]] };
}
