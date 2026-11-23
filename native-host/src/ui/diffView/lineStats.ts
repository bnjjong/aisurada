import { diffLines } from "diff";

export const INLINE_DIFF_MAX = 2000;

export function comparisonTexts(before: string, after: string): [string, string] {
  if (before && after && !before.endsWith("\n") && !after.endsWith("\n")) {
    return [before + "\n", after + "\n"];
  }
  return [before, after];
}

export function lineStats(before: string, after: string) {
  if (before === after) return { deleted: 0, added: 0, long: 0, same: true };
  let deleted = 0;
  let added = 0;
  let long = 0;
  for (const chunk of diffLines(...comparisonTexts(before, after))) {
    if (!chunk.added && !chunk.removed) continue;
    const lines = chunk.value.split("\n");
    if (lines.at(-1) === "") lines.pop();
    if (chunk.added) added += lines.length;
    else deleted += lines.length;
    long += lines.filter((line) => line.length > INLINE_DIFF_MAX).length;
  }
  return { deleted, added, long, same: false };
}
