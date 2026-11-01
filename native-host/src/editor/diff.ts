import { diffLines, diffWordsWithSpace } from 'diff';
export interface DiffRow { kind: 'same' | 'del' | 'add'; left?: number; right?: number; text: string; pieces?: { text: string; changed: boolean }[]; }
export function compareBodies(left: string, right: string, budget = 5_000_000) {
  const rows: DiffRow[] = []; let a = 0, b = 0, deleted = 0, added = 0, skipped = 0;
  for (const chunk of diffLines(left, right)) {
    const lines = chunk.value.split('\n'); if (lines.at(-1) === '') lines.pop();
    for (const text of lines) {
      const kind = chunk.added ? 'add' : chunk.removed ? 'del' : 'same';
      if (kind !== 'add') a++; if (kind !== 'del') b++;
      if (kind === 'del') deleted++; if (kind === 'add') added++;
      rows.push({ kind, text, ...(kind !== 'add' ? { left: a } : {}), ...(kind !== 'del' ? { right: b } : {}) });
    }
  }
  for (let i = 0; i < rows.length;) {
    if (rows[i].kind !== 'del') { i++; continue; }
    const old: DiffRow[] = [], next: DiffRow[] = [];
    while (rows[i]?.kind === 'del') old.push(rows[i++]);
    while (rows[i]?.kind === 'add') next.push(rows[i++]);
    for (let j = 0; j < Math.min(old.length, next.length); j++) {
      const cost = old[j].text.length * next[j].text.length;
      if (cost > budget) { skipped += 2; continue; } budget -= cost;
      const pieces = diffWordsWithSpace(old[j].text, next[j].text);
      old[j].pieces = pieces.filter(piece => !piece.added).map(piece => ({ text: piece.value, changed: !!piece.removed }));
      next[j].pieces = pieces.filter(piece => !piece.removed).map(piece => ({ text: piece.value, changed: !!piece.added }));
    }
  }
  return { rows, deleted, added, skipped };
}
