// The engine's session turns turned into prompt lines, and the two ways the lines are cut into a bundle:
// the author session of a drag question (3.7) and the rail chat's recent conversation (8.7).
// Both use the same line rules, so this is the one place that writes them.

/** One turn of `read-session` (7.4). Tool turns carry `tool`, others carry `text`. */
export interface EngineTurn {
  index: number;
  who: 'user' | 'assistant' | 'tool';
  at?: string | null;
  chars?: number;
  truncated?: boolean;
  text?: string;
  tool?: { name: string; arg?: string; seconds?: number; failed?: boolean };
}

/** The `author` object of `read-session author=<note id>,…` (7.8). */
export interface EngineAuthor {
  note: string;
  mentions: number[];
  creation?: EngineTurn[];
  latest: EngineTurn[];
}

export interface BundleLine {
  /** The text of the line, without a line break. */
  text: string;
  /** The first and last engine turn index this line stands for. */
  first: number;
  last: number;
}

export const BUNDLE_BUDGET = 24_000;
export const CREATION_BUDGET = 16_000;
const TURN_TEXT_LIMIT = 2_000;

/** 3.7: a turn's text is trimmed; one cut at 2,000 characters carries `…(잘림)`. */
function spoken(turn: EngineTurn): string {
  const text = (turn.text ?? '').trim();
  if (!text) return '';
  return turn.truncated || text.length > TURN_TEXT_LIMIT ? `${text.slice(0, TURN_TEXT_LIMIT)}…(잘림)` : text;
}

/**
 * 3.7, 8.7: people speak as `사용자: <글>`, the model as `클로드: <글>`, a run of tool calls is one line
 * `(도구: <이름>, <이름> ×<횟수>)` where the same name in a row folds into a count. Arguments and results
 * never go in. An empty speech is dropped.
 */
export function turnsToLines(turns: EngineTurn[]): BundleLine[] {
  const lines: BundleLine[] = [];
  for (let at = 0; at < turns.length; at++) {
    const turn = turns[at];
    if (turn.who === 'tool') {
      const first = turn.index;
      const groups: Array<{ name: string; count: number }> = [];
      let last = turn.index;
      while (at < turns.length && turns[at].who === 'tool') {
        const name = turns[at].tool?.name ?? '';
        if (name) {
          const tail = groups[groups.length - 1];
          if (tail && tail.name === name) tail.count += 1;
          else groups.push({ name, count: 1 });
        }
        last = turns[at].index;
        at += 1;
      }
      at -= 1;
      const names = groups.map((group) => (group.count > 1 ? `${group.name} ×${group.count}` : group.name)).join(', ');
      if (groups.length) lines.push({ text: `(도구: ${names})`, first, last });
      continue;
    }
    const text = spoken(turn);
    if (!text) continue;
    lines.push({ text: `${turn.who === 'user' ? '사용자' : '클로드'}: ${text}`, first: turn.index, last: turn.index });
  }
  return lines;
}

/**
 * Fills from the end up to `budget`. A line costs its length plus 1. The first line that does not fit stops
 * the fill; shorter older lines are not picked up around it, so the conversation stays continuous.
 */
export function fillFromEnd(lines: BundleLine[], budget: number): { picked: BundleLine[]; left: number; cut: boolean } {
  const picked: BundleLine[] = [];
  let left = budget;
  for (let at = lines.length - 1; at >= 0; at--) {
    const cost = lines[at].text.length + 1;
    if (cost > left) return { picked, left, cut: true };
    picked.unshift(lines[at]);
    left -= cost;
  }
  return { picked, left, cut: false };
}

export interface BundlePart {
  label: '노트를 만들기까지' | '노트를 만들고 고치기까지' | '마지막으로 고치기까지';
  lines: string[];
}

/**
 * 3.7 「창 고르기」. The budget is 24,000 characters.
 * - One mention: the lines up to it, filled from the end (`노트를 만들기까지`).
 * - Several mentions, and the lines up to the last one reach back to the first one: that one window
 *   (`노트를 만들고 고치기까지`).
 * - Otherwise two windows: up to the first mention within 16,000 characters (`노트를 만들기까지`), and the
 *   lines after the first mention up to the last one with what is left (`마지막으로 고치기까지`).
 * Returns no part when no line can be carried.
 */
export function authorParts(author: EngineAuthor): BundlePart[] {
  const mentions = author.mentions;
  if (mentions.length === 0) return [];
  const firstMention = mentions[0];
  const latestLines = turnsToLines(author.latest);
  if (mentions.length === 1) {
    const { picked } = fillFromEnd(latestLines, BUNDLE_BUDGET);
    return picked.length ? [{ label: '노트를 만들기까지', lines: picked.map((line) => line.text) }] : [];
  }
  const whole = fillFromEnd(latestLines, BUNDLE_BUDGET);
  if (whole.picked.length && whole.picked[0].first <= firstMention) {
    return [{ label: '노트를 만들고 고치기까지', lines: whole.picked.map((line) => line.text) }];
  }
  const creationLines = turnsToLines(author.creation ?? author.latest.filter((turn) => turn.index <= firstMention));
  const creation = fillFromEnd(creationLines, CREATION_BUDGET);
  const after = latestLines.filter((line) => line.first > firstMention);
  const latest = fillFromEnd(after, BUNDLE_BUDGET - (CREATION_BUDGET - creation.left));
  const parts: BundlePart[] = [];
  if (creation.picked.length) parts.push({ label: '노트를 만들기까지', lines: creation.picked.map((line) => line.text) });
  if (latest.picked.length) parts.push({ label: '마지막으로 고치기까지', lines: latest.picked.map((line) => line.text) });
  return parts;
}

/** The author session bundle that goes before the question (3.7). Empty when nothing can be carried. */
export function authorBundleText(sessionId: string, author: EngineAuthor | null | undefined): { text: string; lines: number } {
  if (!author) return { text: '', lines: 0 };
  const parts = authorParts(author);
  if (parts.length === 0) return { text: '', lines: 0 };
  const body = parts.map((part) => `<session-transcript part="${part.label}">\n${part.lines.join('\n')}\n</session-transcript>`).join('\n\n');
  const lines = parts.reduce((sum, part) => sum + part.lines.length, 0);
  const text = [
    '## 이 노트를 쓴 세션의 대화',
    '',
    `이 노트를 만든 Claude Code 세션 \`${sessionId}\` 의 대화에서 노트를 쓰던 무렵만 잘라 실었다. 도구 호출은 이름만 남기고 도구 결과는 뺐다.`,
    '아래 대화만으로 답이 모자라면 짐작하지 말고 모자라다고 말하라.',
    '',
    body,
    '',
    '## 물음',
    '',
    '',
  ].join('\n');
  return { text, lines };
}

/**
 * 8.7: the rail chat's bundle, made from the 100 speech turns the rail already read, filled from the end
 * with 24,000 characters. The third line of the header says when the front was cut off.
 */
export function railBundleText(sessionId: string, turns: EngineTurn[]): string {
  const { picked, cut } = fillFromEnd(turnsToLines(turns), BUNDLE_BUDGET);
  if (picked.length === 0) return '';
  const loaded = cut ? '실었다 (앞쪽은 뺐다).' : '실었다.';
  return [
    '## 이 세션의 최근 대화',
    '',
    `사용자가 노트 옆에서 연 Claude Code 세션 \`${sessionId}\` 의 대화다. 최신 쪽부터 24,000자까지 ${loaded} 도구 호출은 이름만 남겼다.`,
    '이것은 읽은 순간의 스냅샷이라 가장 최근 활동이 빠졌을 수 있다.',
    '아래 대화만으로 답이 모자라면 짐작하지 말고 모자라다고 말하라.',
    '',
    '<session-transcript>',
    ...picked.map((line) => line.text),
    '</session-transcript>',
    '',
    '## 물음',
    '',
    '',
  ].join('\n');
}
