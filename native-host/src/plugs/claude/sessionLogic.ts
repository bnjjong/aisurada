// What the session screen and the note rail decide without touching React or the host: how the engine's
// `read-session` answer is read (7.7), the session header (9.3), whether the composer is open (10.1), the rail chat's
// question and turn (8.6 to 8.10) and the note a transcript is pinned into (9.8).
// Adheres to Chapters 7 to 10 of aisurada Part 4 Claude integration specifications.
import { cutText, noteHeading, TEXT_LIMIT } from './askText.ts';
import type { EngineTurn } from './transcriptLines.ts';
import type { ClaudeSession } from './sessionTypes.ts';

// --------------------------------------------------------------------------
// 7.7: the answer of `read-session`
// --------------------------------------------------------------------------

export interface ReadSessionView {
  sessionId: string;
  /** `null` when this machine has no record of the session (7.9). */
  file: string | null;
  source: { bytes: number; lines: number; skipped: number; messages: number };
  hasMore: boolean;
  urls: Array<{ url: string; count: number }>;
  runningAgents: Array<{ what?: string; at?: string }>;
  todos: Array<{ number: number; title: string; status: string }>;
  runState: { model?: string; effort?: string; contextTokens?: number; quotaBlocked?: boolean; quotaResetsAt?: number } | null;
  turns: EngineTurn[];
}

const asCount = (value: unknown): number => (typeof value === 'number' && Number.isFinite(value) && value > 0 ? value : 0);

/** Reads the engine's answer as it is. The names are the engine's (7.7): `source.bytes`, `source.lines`, `turns[].who`. */
export function normalizeReadSession(raw: any): ReadSessionView {
  const turns: EngineTurn[] = Array.isArray(raw?.turns)
    ? raw.turns.filter((turn: any) => turn && typeof turn === 'object' && (turn.who === 'user' || turn.who === 'assistant' || turn.who === 'tool'))
    : [];
  const state = raw?.runState && typeof raw.runState === 'object' ? raw.runState : null;
  return {
    sessionId: String(raw?.sessionId ?? ''),
    file: typeof raw?.file === 'string' ? raw.file : null,
    source: { bytes: asCount(raw?.source?.bytes), lines: asCount(raw?.source?.lines), skipped: asCount(raw?.source?.skipped), messages: asCount(raw?.source?.messages) },
    hasMore: raw?.hasMore === true,
    urls: Array.isArray(raw?.urls) ? raw.urls.filter((item: any) => item && typeof item.url === 'string').map((item: any) => ({ url: item.url, count: asCount(item.count) || 1 })) : [],
    runningAgents: Array.isArray(raw?.runningAgents)
      ? raw.runningAgents.filter((item: any) => item && typeof item === 'object').map((item: any) => ({
          ...(typeof item.what === 'string' && item.what ? { what: item.what } : {}),
          ...(typeof item.at === 'string' && item.at ? { at: item.at } : {}),
        }))
      : [],
    todos: Array.isArray(raw?.todos)
      ? raw.todos.filter((item: any) => item && typeof item === 'object').map((item: any) => ({ number: Number(item.number) || 0, title: String(item.title ?? ''), status: String(item.status ?? 'pending') }))
      : [],
    runState: state
      ? {
          ...(typeof state.model === 'string' ? { model: state.model } : {}),
          ...(typeof state.effort === 'string' ? { effort: state.effort } : {}),
          ...(typeof state.contextTokens === 'number' ? { contextTokens: state.contextTokens } : {}),
          ...(state.quotaBlocked === true ? { quotaBlocked: true } : {}),
          ...(typeof state.quotaResetsAt === 'number' ? { quotaResetsAt: state.quotaResetsAt } : {}),
        }
      : null,
    turns: turns.map((turn) => ({ ...turn, text: typeof turn.text === 'string' ? turn.text : '' })),
  };
}

// --------------------------------------------------------------------------
// 9.3, 9.4: header and transcript text
// --------------------------------------------------------------------------

/** 9.4: `YYYY-MM-DD HH:mm` in the machine's time zone. A text that is not a time keeps its first 16 characters with `T` as a space. */
export function formatTurnTime(iso: string | null | undefined): string {
  if (!iso) return '';
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return iso.slice(0, 16).replace('T', ' ');
  const pad = (value: number) => String(value).padStart(2, '0');
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** 9.4: under 60 seconds one decimal, under an hour whole minutes, above that hours with one decimal. */
export function formatDuration(seconds: number): string {
  if (seconds < 60) return `${seconds.toFixed(1)}초`;
  if (seconds < 3600) return `${Math.round(seconds / 60)}분`;
  return `${(seconds / 3600).toFixed(1)}시간`;
}

/** 9.4: 1,048,576 bytes and over is MB with one decimal, 1,024 and over is KB whole, else B. */
export function formatBytes(bytes: number): string {
  if (bytes >= 1_048_576) return `${(bytes / 1_048_576).toFixed(1)}MB`;
  if (bytes >= 1_024) return `${Math.round(bytes / 1_024)}KB`;
  return `${bytes}B`;
}

/** The elapsed time text of a session row: 「3분」, 「2시간」, 「5일」 (the clock is the moment of drawing, 9.3). */
export function sinceText(from: number, now: number = Date.now()): string {
  const seconds = Math.max(0, Math.floor((now - from) / 1000));
  if (seconds < 60) return '방금';
  const minutes = Math.floor(seconds / 60);
  if (minutes < 60) return `${minutes}분 전`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}시간 전`;
  return `${Math.floor(hours / 24)}일 전`;
}

export function kindLabel(kind: ClaudeSession['kind'] | undefined): string {
  return kind === 'interactive' ? '터미널' : kind === 'background' ? '백그라운드' : '알 수 없음';
}

/**
 * 9.3: the status cell of a header. An ended row reads 「<지난 시간>에 끝남」 and is the green label; a list row reads
 * its run state: running 도는 중, blocked 멈춤, idle 쉬는 중, unknown 모름. The note rail head uses the same function.
 */
export function statusCell(row: Pick<ClaudeSession, 'status' | 'isEndedRow' | 'endedAt'>, now: number = Date.now()): { text: string; ended: boolean } {
  if (row.isEndedRow) return { text: `${sinceText(row.endedAt ?? now, now)}에 끝남`, ended: true };
  const text = row.status === 'running' ? '도는 중' : row.status === 'blocked' ? '멈춤' : row.status === 'idle' ? '쉬는 중' : '모름';
  return { text, ended: false };
}

/** 9.5: the button of the sub agent bar. A session counts as alive only when its row exists and its pid is alive. */
export function agentBarText(count: number, alive: boolean): string {
  return alive ? `서브에이전트 ${count}개가 도는 중` : `결과가 안 온 서브에이전트 ${count}개, 지금 도는지는 몰라요`;
}

export function todoBarText(todos: Array<{ status: string }>): string {
  return `할 일 ${todos.length}개 가운데 ${todos.filter((todo) => todo.status === 'completed').length}개가 끝났어요`;
}

// --------------------------------------------------------------------------
// 10.1: is the composer open
// --------------------------------------------------------------------------

export type ComposerBlock = 'live-terminal' | 'live-background' | 'no-record';

/**
 * 10.1: a row that is alive and not held by an agent closes it (a background row by `live-background`, any other by
 * `live-terminal`); otherwise a read that finished with `file` null closes it as `no-record`; otherwise it is open.
 * Not knowing a thing opens it: no row, no pid, a list that could not be read, a conversation still being read.
 */
export function composerBlock(row: Pick<ClaudeSession, 'processAlive' | 'heldByAgent' | 'kind'> | undefined, read: { done: boolean; file: string | null | undefined }): ComposerBlock | null {
  if (row && row.processAlive === true && row.heldByAgent !== true) return row.kind === 'background' ? 'live-background' : 'live-terminal';
  if (read.done && read.file === null) return 'no-record';
  return null;
}

export const COMPOSER_BLOCK_TEXT: Record<ComposerBlock, string> = {
  'live-terminal': '이 세션은 다른 터미널에서 열려 있어요. 여기서도 보내면 대화가 둘로 갈라져서 입력창을 닫아 뒀어요. 그 터미널에서 claude 를 끝내고 돌아오면 여기서 이어 쓸 수 있어요.',
  'live-background': '이 세션은 백그라운드에서 돌고 있어요. 도는 동안에는 claude 가 이어 쓰기를 거절해서 입력창을 닫아 뒀어요. 끝나면 여기서 이어 쓸 수 있어요.',
  'no-record': '이 기계에 이 세션의 대화 기록이 없어서 이어 쓸 수 없어요.',
};

/** 10.6: the picture lines at the end of a sent text. */
export function pictureLines(paths: string[]): string {
  return [`[첨부 그림] 이 물음에 그림 ${paths.length}장을 붙였다. 답하기 전에 Read 도구로 하나씩 열어 본다.`, ...paths.map((path) => `- ${path}`)].join('\n');
}

// --------------------------------------------------------------------------
// 10.3 to 10.5: one send from the composer
// --------------------------------------------------------------------------

export interface SessionSendDeps {
  /** Reads the whole session list again (6.1). `null` when it could not be read. */
  readList(): Promise<Array<Pick<ClaudeSession, 'sessionId' | 'processAlive' | 'heldByAgent' | 'kind'>> | null>;
  /** `session-titles <id>` and its `cwd`; `null` when unreadable or absent. */
  firstCwd(sessionId: string): Promise<string | null>;
  writeImage(sessionId: string, name: string, bytes: Uint8Array): Promise<string>;
  /** Never throws (3.8). */
  askClaude(args: Record<string, unknown>): Promise<{ ok: boolean; answer?: string; error?: string | null; missing_cwd?: string | null }>;
}

export interface SessionSendInput {
  sessionId: string;
  text: string;
  images: Array<{ name: string; bytes: Uint8Array }>;
  vault: string;
  model: string;
  effort: string;
  timeoutSeconds: number;
}

export type SessionSendResult = { ok: true; notice: string | null; cancelled?: boolean } | { ok: false; error: string; cancelled?: boolean };

/**
 * 10.3 steps 4 to 9. The list and the first folder are read together; a session another terminal holds is refused
 * before any picture is written; a picture that cannot be written stops the send; then `ask_claude` goes with the
 * eight keys of 10.4 (and `cwd` only when the first folder was read).
 */
export async function sendToSession(deps: SessionSendDeps, input: SessionSendInput): Promise<SessionSendResult> {
  try {
    const [list, cwd] = await Promise.all([deps.readList().catch(() => null), deps.firstCwd(input.sessionId).catch(() => null)]);
    const row = list?.find((item) => item.sessionId === input.sessionId);
    if (composerBlock(row, { done: false, file: undefined }) !== null) {
      return { ok: false, error: '이 세션이 지금 다른 터미널에서 돌고 있어서 안 보냈어요. 그 터미널에서 이어 가 주세요. 여기서 보내면 대화가 갈라져요.' };
    }
    const written: string[] = [];
    for (const image of input.images) {
      try {
        written.push(await deps.writeImage(input.sessionId, image.name, image.bytes));
      } catch (error) {
        return { ok: false, error: `붙인 그림을 못 저장해서 안 보냈어요: ${error instanceof Error ? error.message : String(error)}` };
      }
    }
    if (input.model.toLowerCase().includes('gemini')) {
      return { ok: false, error: '세션 이어 쓰기는 Claude만 돼요' };
    }
    const text = input.text.trim();
    const answer = await deps.askClaude({
      vault: input.vault,
      sessionId: input.sessionId,
      prompt: written.length > 0 ? `${text}\n\n${pictureLines(written)}` : text,
      resume: true,
      resumeOnly: true,
      ...(cwd ? { cwd } : {}),
      model: input.model,
      effort: input.effort,
      timeoutSeconds: input.timeoutSeconds,
      keepAlive: true,
    });
    if (!answer.ok) return { ok: false, error: answer.error || '까닭을 못 받았다' };
    if (answer.answer?.includes('[사용자에 의해 중단됨]')) {
      return { ok: true, notice: '질문이 중단되었어요. 작성하던 글을 복원했어요.', cancelled: true };
    }
    return { ok: true, notice: answer.missing_cwd ? `이 세션이 처음 돌던 폴더(${answer.missing_cwd})가 없어서 vault 에서 이어 갔어요.` : null };
  } catch (error) {
    return { ok: false, error: error instanceof Error ? error.message : String(error) };
  }
}

// --------------------------------------------------------------------------
// 8.5 to 8.10: the rail chat
// --------------------------------------------------------------------------

/** 8.10: `rt<time in base 36>-<8 hex of a random UUID>`. */
export function railTurnId(now: number, uuid: string): string {
  return `rt${now.toString(36)}-${uuid.replace(/-/g, '').slice(0, 8)}`;
}

/**
 * 8.5: the session id a chat goes to claude with. A new chat's id is a UUID and goes as it is, in lower case. An old
 * chat id (including HTTP-time `rc<time>-<hex>`) becomes a UUID from the first 16 bytes of its SHA-256, with the version nibble set to 4
 * and the top two bits of byte 9 to 10, in lower case 8-4-4-4-12.
 */
export async function railChatSessionId(chatId: string): Promise<string> {
  if (/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(chatId)) return chatId.toLowerCase();
  const digest = new Uint8Array(await crypto.subtle.digest('SHA-256', new TextEncoder().encode(chatId))).slice(0, 16);
  digest[6] = (digest[6] & 0x0f) | 0x40;
  digest[8] = (digest[8] & 0x3f) | 0x80;
  const hex = [...digest].map((byte) => byte.toString(16).padStart(2, '0')).join('');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

export interface RailTurn {
  id: string;
  askedAt: string;
  question: string;
  answer: string;
  error?: string;
  model: string;
  effort: string;
  elapsedMs: number;
  images?: string[];
  /** Memory only. */
  unsaved?: boolean;
}

export interface RailChat {
  noteId: string;
  chatId: string;
  sourceSessionId: string;
  createdAt: string;
  turns: RailTurn[];
}

/** 8.10: the turn made from an answer. The text is cut to 20,000 with the rule of 3.10. */
export function railTurnFrom(input: { id: string; askedAt: string; question: string; model: string; effort: string; images: string[]; answer: { ok: boolean; answer?: string; error?: string | null; elapsed_ms?: number } }): RailTurn {
  const failed = !input.answer.ok;
  return {
    id: input.id,
    askedAt: input.askedAt,
    question: cutText(input.question, TEXT_LIMIT),
    answer: failed ? '' : cutText(input.answer.answer ?? '', TEXT_LIMIT),
    ...(failed ? { error: input.answer.error || '답변을 받지 못했어요' } : {}),
    model: input.model,
    effort: input.effort,
    elapsedMs: Math.max(0, Math.trunc(input.answer.elapsed_ms ?? 0) || 0),
    ...(input.images.length ? { images: input.images } : {}),
  };
}

/** 8.4 the note's name for the persona: the first `#` heading without HTML comments, or none (a json note has none). */
export function railHeading(markdown: string | undefined, format?: string): string | null {
  return noteHeading(markdown, format);
}

/** 8.7: the rail chat's persona. The second line's end depends on whether the bundle has anything in it. */
export function railPersona(input: { heading: string | null; noteId: string; subId?: string; sessionId: string; hasBundle: boolean }): string {
  const place = input.subId ? `노트 ${input.noteId}의 하위 노트 ${input.subId}` : input.noteId;
  const name = input.heading ? `「${input.heading}」(${place})` : place;
  const read = input.subId ? `ais-cli get_sub_note '{"parent":"${input.noteId}","id":"${input.subId}"}'` : `ais-cli get_doc '{"id":"${input.noteId}"}'`;
  const update = input.subId ? 'ais-cli update_sub_note' : 'ais-cli update_doc';
  return [
    `너는 사용자의 aisurada 앱에서, 노트 옆 세션 레일로 질문을 받는다. 노트는 ${name}이다.`,
    `이 대화는 Claude Code 세션 ${input.sessionId} 와 그 세션이 쓴 이 노트를 두고 묻는다. ${input.hasBundle ? '첫 물음 앞에 그 세션의 최근 대화가 실려 있다.' : '그 세션의 대화는 실리지 않았다.'}`,
    `노트 내용이 필요하면 \`${read}\` 로 읽는다. 노트를 고쳐 달라고 하면 그 응답의 precondition 값을 f1 으로 실어 \`${update}\` 로 고친다.`,
    '작업 폴더는 그 세션이 돌던 폴더다. 코드나 다른 파일을 고쳐 달라는 말이 없으면 노트 밖 파일은 고치지 않는다.',
  ].join('\n');
}

export interface SeedMessage {
  role: 'user' | 'assistant';
  content: string;
}

/**
 * 8.8: the earlier questions and answers for a conversation that is not at its first question. Every earlier turn that did
 * not fail and has an answer gives a user and an assistant message; the first message carries the session bundle before
 * its question. `undefined` when there is nothing to carry (the key is then not sent).
 */
export function railSeed(prior: RailTurn[], bundle: string): SeedMessage[] | undefined {
  const messages: SeedMessage[] = [];
  for (const turn of prior) {
    if (turn.error || turn.answer.trim() === '') continue;
    messages.push({ role: 'user', content: messages.length === 0 && bundle ? `${bundle}${turn.question}` : turn.question });
    messages.push({ role: 'assistant', content: turn.answer });
  }
  return messages.length ? messages : undefined;
}

/** 8.7: the text sent. The bundle is on the first question of a conversation only; the question is not cut. */
export function railPrompt(input: { bundle: string; question: string; first: boolean; imagePaths: string[] }): string {
  const question = input.first && input.bundle ? `${input.bundle}${input.question}` : input.question;
  return input.imagePaths.length > 0 ? `${question}\n\n${pictureLines(input.imagePaths)}` : question;
}

// --------------------------------------------------------------------------
// 9.8: the note a transcript becomes
// --------------------------------------------------------------------------

/** The longest run of backticks in a text. */
function longestBacktickRun(text: string): number {
  let longest = 0;
  for (const run of text.match(/`+/g) ?? []) longest = Math.max(longest, run.length);
  return longest;
}

/** 9.8: a text as inline code. Line breaks become one space; the fence is one backtick longer than the longest run. */
export function inlineCode(text: string): string {
  const flat = text.replace(/\r\n|\r|\n/g, ' ');
  const fence = '`'.repeat(longestBacktickRun(flat) + 1);
  const hasTick = flat.includes('`');
  const spaced = flat === '' || (/^\s/.test(flat) && /\s$/.test(flat));
  return hasTick || spaced ? `${fence} ${flat} ${fence}` : `${fence}${flat}${fence}`;
}

/** 9.8: a spoken turn is fenced; the fence is one longer than the longest backtick run of any spoken turn, never under 3. */
export function pinFence(turns: EngineTurn[]): string {
  let longest = 0;
  for (const turn of turns) if (turn.who !== 'tool') longest = Math.max(longest, longestBacktickRun(turn.text ?? ''));
  return '`'.repeat(Math.max(3, longest + 1));
}

export interface PinInput {
  title: string;
  sessionId: string;
  turns: EngineTurn[];
  source: { bytes: number; lines: number; messages: number };
  /** How many things the shape check found (`scan_text_shapes`). */
  shapeFindings: number;
  /** The shape check's names with counts, for the callout. */
  shapeLines?: string[];
}

/** 9.8: the line a spoken or tool turn adds. A tool turn without `tool` adds nothing. */
function pinTurn(turn: EngineTurn, fence: string): string[] {
  if (turn.who === 'tool') {
    if (!turn.tool) return [];
    const arg = turn.tool.arg ? ` — ${inlineCode(turn.tool.arg)}` : '';
    return [`- ${inlineCode(turn.tool.name)}${arg}`];
  }
  return [turn.who === 'user' ? '**나**' : '**클로드**', '', fence, turn.text ?? '', fence, ''];
}

/**
 * 9.8: the body sent to `create-doc`: heading, one sentence, the summary table, a callout and a blank line, `## 대화`, then
 * the turns. The first line is `# <title>`; the callout is there even at zero findings.
 */
export function pinBody(input: PinInput): string {
  const fence = pinFence(input.turns);
  const callout = input.shapeFindings > 0
    ? [`> [!danger] 표에 없는데 비밀처럼 생긴 것 ${input.shapeFindings} 건`, `> ${(input.shapeLines ?? []).join(', ')}`, '> 이 검사는 도구 결과 본문은 보지 못해요.']
    : ['> [!success] 모양 검사에 걸린 것이 0 건입니다', '> 도구 결과 본문은 애초에 안 담아서, 이 굳히기는 값 표를 안 써요.', '> 이 검사는 채택한 모양 밖과 도구 결과 본문은 보지 못해요.'];
  const lines = [
    `# ${input.title}`,
    '',
    'Claude Code 세션 대화를 굳힌 노트입니다. 도구 결과 본문은 안 담아서 자격증명이 안 실립니다.',
    '',
    '| 무엇 | 값 |',
    '| --- | --- |',
    `| 세션 id | \`${input.sessionId}\` |`,
    `| 담은 회차 | ${input.turns.length} |`,
    `| 원본 | ${formatBytes(input.source.bytes)}, ${input.source.lines.toLocaleString('en-US')}줄에서 ${input.source.messages.toLocaleString('en-US')}건 |`,
    `| 모양으로 걸린 것 | ${input.shapeFindings} 건 |`,
    '',
    ...callout,
    '',
    '## 대화',
    '',
  ];
  for (const turn of input.turns) lines.push(...pinTurn(turn, fence));
  return lines.join('\n');
}

/** 9.8: the lines a shape check reads: one per turn, speech as its text, a tool as its name and argument. */
export function pinScanText(turns: EngineTurn[]): string {
  return turns.map((turn) => (turn.who === 'tool' ? `${turn.tool?.name ?? ''}\n${turn.tool?.arg ?? ''}` : turn.text ?? '')).join('\n');
}

/** 9.8: the default title is `세션 대화 <HH:MM>` with the machine's clock. */
export function pinDefaultTitle(now: Date = new Date()): string {
  const pad = (value: number) => String(value).padStart(2, '0');
  return `세션 대화 ${pad(now.getHours())}:${pad(now.getMinutes())}`;
}
