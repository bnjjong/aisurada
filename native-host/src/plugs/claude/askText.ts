// Pure helpers for the drag-ask screen: model and effort lists, text cutting, the question text and the
// persona. Adheres to Chapters 2, 3 of aisurada Part 4 Claude integration specifications.
// Nothing here touches React or the host, so the node tests can run it as it is.

// 3.3: the two lists are contract, the labels are not.
export const ASK_MODELS = [
  { id: 'claude-opus-5-5', label: 'opus 5.5' },
  { id: 'claude-opus-5', label: 'opus 5' },
  { id: 'claude-sonnet-5-5', label: 'sonnet 5.5' },
  { id: 'claude-sonnet-5', label: 'sonnet 5' },
  { id: 'claude-fable-5-1', label: 'fable 5.1' },
  { id: 'claude-fable-5', label: 'fable 5' },
] as const;
export const ASK_EFFORTS = [
  { id: 'low', label: 'low' },
  { id: 'medium', label: 'medium' },
  { id: 'high', label: 'high' },
  { id: 'xhigh', label: 'xhigh' },
  { id: 'max', label: 'max' },
] as const;
export const DEFAULT_MODEL = 'claude-sonnet-5-5';
export const DEFAULT_EFFORT = 'low';

// 3.3: old aliases move to the newest of their family; anything else outside the list is the first value.
const MODEL_ALIASES: Record<string, string> = {
  opus: 'claude-opus-5-5',
  sonnet: 'claude-sonnet-5-5',
  fable: 'claude-fable-5-1',
};

/** Brings a stored or received model id back to a value the picker can show. */
export function normalizeModel(value: unknown): string {
  if (typeof value !== 'string') return DEFAULT_MODEL;
  if (ASK_MODELS.some((model) => model.id === value)) return value;
  return MODEL_ALIASES[value] ?? DEFAULT_MODEL;
}

/** Brings a stored or received effort back to one of the five values. */
export function normalizeEffort(value: unknown): string {
  return typeof value === 'string' && ASK_EFFORTS.some((effort) => effort.id === value) ? value : DEFAULT_EFFORT;
}

/** The label shown in the meta line; it is the same label the picker shows. */
export function modelLabel(id: string): string {
  return ASK_MODELS.find((model) => model.id === id)?.label ?? id;
}

// 3.2: the picture rules the picker enforces when a picture is pasted.
export const IMAGE_MIME_EXTENSIONS: Record<string, string> = {
  'image/png': 'png',
  'image/jpeg': 'jpg',
  'image/gif': 'gif',
  'image/webp': 'webp',
  'image/avif': 'avif',
  'image/bmp': 'bmp',
  'image/svg+xml': 'svg',
};
export const MAX_IMAGE_BYTES = 5_000_000;
export const MAX_IMAGES = 8;

// 3.10 and 2.7: the limits the engine enforces on a turn.
export const QUOTE_LIMIT = 2_000;
export const TEXT_LIMIT = 20_000;

/**
 * 3.10: text at or under the UTF-16 limit stays as it is. Longer text is cut to limit - 1 units, a trailing
 * high surrogate is dropped with it, and an ellipsis is appended. A half character would become a broken
 * character once the engine decodes the JSON.
 */
export function cutText(text: string, limit: number): string {
  if (text.length <= limit) return text;
  let cut = text.slice(0, limit - 1);
  const last = cut.charCodeAt(cut.length - 1);
  if (last >= 0xd800 && last <= 0xdbff) cut = cut.slice(0, -1);
  return `${cut}…`;
}

/** 3.1 and 3.6: the quote for a table row is its cells joined by tabs with a line break at the end. */
export function rowQuote(cells: string[]): string {
  return `${cells.join('\t')}\n`;
}

/**
 * 3.6: the folded quote of a follow-up. Tabs become ` | `, every other run of blanks becomes one space,
 * the ends are trimmed, and more than 200 code points keep 200 and an ellipsis.
 */
export function foldQuote(quote: string): string {
  const folded = quote.replace(/\t/g, ' | ').replace(/\s+/g, ' ').trim();
  const points = [...folded];
  return points.length > 200 ? `${points.slice(0, 200).join('')}…` : folded;
}

/** 3.1: the quote of an HTML box question. An empty label falls back to the attachment path. */
export function htmlQuote(label: string, path: string): string {
  return `HTML 문서: ${label || path}`;
}

export const HTML_ID_PREFIX = 'html:';

/** 2.9: the question id of an HTML box is built from its attachment path. */
export function htmlAskId(path: string): string {
  return `${HTML_ID_PREFIX}${path}`;
}

/**
 * 2.1: `ak<time in base 36>-<first 8 hex of a random UUID>`. The tail is hexadecimal, so it always has
 * 8 characters and matches the form part 3 accepts.
 */
export function newUnderlineId(now: number = Date.now(), random: () => string = randomUuid): string {
  return `ak${now.toString(36)}-${random().replace(/-/g, '').slice(0, 8)}`;
}

/** A random UUID (version 4); `claude --session-id` accepts nothing else. */
export function randomUuid(): string {
  const cryptoApi = (globalThis as { crypto?: Crypto }).crypto;
  if (cryptoApi && typeof cryptoApi.randomUUID === 'function') return cryptoApi.randomUUID();
  const bytes = new Uint8Array(16);
  if (cryptoApi && typeof cryptoApi.getRandomValues === 'function') cryptoApi.getRandomValues(bytes);
  else for (let i = 0; i < bytes.length; i++) bytes[i] = Math.floor(Math.random() * 256);
  bytes[6] = (bytes[6] & 0x0f) | 0x40;
  bytes[8] = (bytes[8] & 0x3f) | 0x80;
  const hex = [...bytes].map((byte) => byte.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

// --------------------------------------------------------------------------
// Targets: a note, or a sub note under a root note (3.13)
// --------------------------------------------------------------------------

export interface AskTarget {
  /** The note id, or the sub note id when the target is a sub note. It is the store key. */
  id: string;
  /** The root note id of a sub note. */
  parent?: string;
}

/** 3.4: the key is the target's own id, the sub note id for a sub note. */
export function targetKey(target: AskTarget): string {
  return target.id;
}

/** The engine argv head for a target: `<root id>` or `<root id> <sub id>`. */
export function targetArgs(target: AskTarget): string[] {
  return target.parent ? [target.parent, target.id] : [target.id];
}

/** 3.6: the place text of the persona and the note id line. */
export function targetPlace(target: AskTarget): string {
  return target.parent ? `노트 ${target.parent}의 하위 노트 ${target.id}` : target.id;
}

/**
 * 3.6: the title of the persona is the first `#` heading of the editor buffer with HTML comments removed.
 * A non-markdown buffer or a buffer with no usable heading has none.
 */
export function noteHeading(markdown: string | undefined, format: string | undefined): string | null {
  if (!markdown || (format && format !== 'markdown')) return null;
  for (const line of markdown.split('\n')) {
    const match = /^#[ \t]+(.*)$/.exec(line.replace(/\r$/, ''));
    if (!match) continue;
    const text = match[1].replace(/<!--[\s\S]*?-->/g, '').trim();
    return text || null;
  }
  return null;
}

/** 3.6: the three line persona, sent apart from the question text as `persona`. */
export function askPersona(target: AskTarget, heading: string | null): string {
  const place = targetPlace(target);
  const first = heading
    ? `너는 사용자의 aisurada 노트 안에서 질문을 받는다. 노트는 「${heading}」(${place})이다.`
    : `너는 사용자의 aisurada 노트 안에서 질문을 받는다. 노트는 ${place}이다.`;
  return [
    first,
    '질문은 늘 이 노트의 특정 문장이나 사용자가 드래그한 대목에 대한 것이다.',
    '질문마다 [대목] 줄이 붙는다. 그 대목을 기준으로 답하고, 같은 대화에서 다른 대목을 두고 오간 문답과 섞지 않는다.',
  ].join('\n');
}

/** 3.6: the note id line, in its note form or its sub note form. */
export function noteIdLine(target: AskTarget): string {
  if (target.parent) {
    return `이 대목을 고른 노트: 노트 ${target.parent} 의 하위 노트 ${target.id}. 답하기 전에 \`ais-cli get_sub_note '{"parent":"${target.parent}","id":"${target.id}"}'\` 로 이 노트를 먼저 읽고, 대목의 앞뒤 글을 보고 답해라.`;
  }
  return `이 대목을 고른 노트의 id: ${target.id}. 답하기 전에 \`ais-cli get_doc '{"id":"${target.id}"}'\` 로 이 노트를 먼저 읽고, 대목의 앞뒤 글을 보고 답해라.`;
}

export interface AskPromptInput {
  target: AskTarget;
  /** The selected text, or empty for a follow-up. */
  quote: string;
  question: string;
  /** For a follow-up: the quote of the turn that opened the underline, if the thread has it. */
  openingQuote?: string;
  /** The `repo` value of the note's agenda. */
  repo?: string;
  /** The ready-made session bundle with its own `## 물음` header, for a first question only. */
  bundle?: string;
  /** The question id when it starts with `html:`; the absolute path of that HTML file. */
  htmlPath?: string;
  /** Absolute paths of the pictures that were written. */
  imagePaths?: string[];
}

/**
 * 3.6: the text sent as `prompt`. A new underline carries the session bundle (3.7), the repository line,
 * the note id line, then the quote and the question. A follow-up carries only the folded quote of the
 * underline it continues. Both end with the HTML and picture lines when they apply.
 */
export function buildAskPrompt(input: AskPromptInput): string {
  const question = input.question.trim();
  const quote = input.quote.trim();
  const lines: string[] = [];
  if (quote) {
    const head: string[] = [];
    if (input.repo) head.push(`이 노트가 속한 아젠다의 소스 저장소: ${input.repo}`);
    head.push(noteIdLine(input.target));
    lines.push(...head, '');
    lines.push(`[대목] 「${quote}」`, '', '이 대목에 대한 질문이다. 짧게 답해라.', '', question);
  } else {
    if (input.openingQuote !== undefined && input.openingQuote.trim()) {
      lines.push(`[대목] 「${foldQuote(input.openingQuote)}」`, '');
    }
    lines.push(question);
  }
  if (input.htmlPath) {
    lines.push('', '[HTML 문서] 이 물음은 고른 대목이 아니라 노트에 넣은 HTML 문서 전체에 대한 것이다. 이 대화에서 아직 안 읽었으면 답하기 전에 Read 도구로 이 파일을 연다.', `- ${input.htmlPath}`);
  }
  const images = input.imagePaths ?? [];
  if (images.length > 0) {
    lines.push('', `[첨부 그림] 이 물음에 그림 ${images.length}장을 붙였다. 답하기 전에 Read 도구로 하나씩 열어 본다.`, ...images.map((path) => `- ${path}`));
  }
  const text = lines.join('\n');
  return input.bundle ? `${input.bundle}${text}` : text;
}

// --------------------------------------------------------------------------
// Turns and the underline each one belongs to (2.4)
// --------------------------------------------------------------------------

export interface AskTurn {
  id: string;
  markId?: string;
  askedAt: string;
  quote?: string;
  question: string;
  answer: string;
  error?: string;
  model: string;
  effort: string;
  elapsedMs: number;
  sessionLines?: number;
  images?: string[];
  /** Memory only; it never goes to the engine or the file. */
  unsaved?: boolean;
}

export interface AskThread {
  v?: number;
  noteId: string;
  sessionId: string;
  turns: AskTurn[];
}

/**
 * 2.4: a turn's underline is its markId; without one it is the id of the nearest earlier turn that has a
 * quote; without that it is the turn's own id. Only turns with a quote advance the "nearest underline".
 */
export function turnUnderlines(turns: AskTurn[]): string[] {
  const marks: string[] = [];
  let nearest: string | null = null;
  for (const turn of turns) {
    if (turn.quote) nearest = turn.id;
    marks.push(turn.markId || nearest || turn.id);
  }
  return marks;
}

export interface UnderlineGroup {
  markId: string;
  /** The quote of the turn that opened the underline. */
  quote?: string;
  turns: AskTurn[];
}

/** 2.4, 4.3: groups in the order their underline was first asked, turns inside in the order asked. */
export function groupTurns(turns: AskTurn[]): UnderlineGroup[] {
  const marks = turnUnderlines(turns);
  const groups: UnderlineGroup[] = [];
  const byMark = new Map<string, UnderlineGroup>();
  turns.forEach((turn, index) => {
    const markId = marks[index];
    let group = byMark.get(markId);
    if (!group) {
      group = { markId, turns: [] };
      byMark.set(markId, group);
      groups.push(group);
    }
    if (turn.quote && group.quote === undefined) group.quote = turn.quote;
    group.turns.push(turn);
  });
  return groups;
}

/**
 * 4.5: a card receives an underline id, or the id of a follow-up turn; the latter moves to the underline it
 * belongs to so the whole conversation is shown.
 */
export function turnsForUnderline(turns: AskTurn[], id: string): AskTurn[] {
  const marks = turnUnderlines(turns);
  const direct = marks.some((mark) => mark === id);
  const underline = direct ? id : marks[turns.findIndex((turn) => turn.id === id)];
  if (!underline) return [];
  return turns.filter((_, index) => marks[index] === underline);
}

/** 3.10: a turn that answered, with no failure and a non-empty answer. */
export function isAnswered(turn: AskTurn): boolean {
  return !turn.error && turn.answer.trim() !== '';
}

/**
 * 3.10: after a save the memory takes the engine's thread as it is, then keeps the turns that exist only in
 * memory (`unsaved`) and are not in the response, in order, behind it.
 */
export function mergeUnsaved(saved: AskThread, memory: AskThread | null): AskThread {
  const known = new Set(saved.turns.map((turn) => turn.id));
  const kept = (memory?.turns ?? []).filter((turn) => turn.unsaved && !known.has(turn.id));
  return kept.length === 0 ? saved : { ...saved, turns: [...saved.turns, ...kept] };
}
