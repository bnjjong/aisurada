// C188: the engine keeps its own first-heading rule (part 1, 1.11), so a title
// the screen unescaped can differ from that heading and the engine says so in
// `warning`. On a write that carries the title this notice is expected, not a failure.
export const TITLE_MISMATCH_HEAD = '본문 첫 헤딩("';

export function withoutTitleMismatch(warning: string): string {
  return warning.split('\n').filter(line => !line.startsWith(TITLE_MISMATCH_HEAD)).join('\n');
}
