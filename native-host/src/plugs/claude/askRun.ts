// One drag question from the press to the saved turn (3.4 to 3.10), and the old thread move (2.10).
// It has no React in it and reaches the host only through `AskHost`, so the node tests drive it with a fake.
// Adheres to Chapters 2 and 3 of aisurada Part 4 Claude integration specifications.
import {
  IMAGE_MIME_EXTENSIONS,
  QUOTE_LIMIT,
  TEXT_LIMIT,
  askPersona,
  buildAskPrompt,
  cutText,
  groupTurns,
  isAnswered,
  mergeUnsaved,
  newUnderlineId,
  normalizeEffort,
  normalizeModel,
  randomUuid,
  targetArgs,
} from './askText.ts';
import type { AskTarget, AskThread, AskTurn } from './askText.ts';
import { askNow, markNoteUnseen, noteAskOutcome, setAskRuntime, setAskThread, setPendingAsk } from './state.ts';
import type { AskRuntime, PendingAsk } from './state.ts';
import { authorBundleText } from './transcriptLines.ts';
import type { EngineAuthor } from './transcriptLines.ts';

/** 3.8: what the host answers. A refusal of the whole command arrives as `ok: false` too (the bridge converts). */
export interface AskAnswer {
  ok: boolean;
  answer: string;
  error?: string | null;
  elapsed_ms: number;
}

export interface EngineReply {
  status: number;
  stdout: string;
  stderr: string;
}

export interface PictureSource {
  name: string;
  type: string;
  arrayBuffer(): Promise<ArrayBuffer>;
}

export interface AskHost {
  native: boolean;
  engine(argv: string[]): Promise<EngineReply>;
  /** Never throws: a refused or failed command comes back as `{ok: false, error}` (3.8). */
  askClaude(args: AskClaudeArgs): Promise<AskAnswer>;
  askRuntime(): Promise<string>;
  writeImage(noteDir: string, name: string, bytes: Uint8Array): Promise<void>;
  announceSelfWrite(vault: string, relPath: string): void;
}

/** 3.8: the nine keys a drag question sends. Nothing more, or the way back to the flags is lost. */
export interface AskClaudeArgs {
  vault: string;
  sessionId: string;
  model: string;
  effort: string;
  prompt: string;
  resume: boolean;
  timeoutSeconds: number;
  keepAlive: false;
  persona: string;
}

export interface AskRequest {
  target: AskTarget;
  /** The root note id: the note itself, or the root of a sub note. */
  rootId: string;
  /** The absolute path of the vault. Empty when no vault is open. */
  vault: string;
  /** The absolute path of the note's folder, when it is known. */
  noteDir?: string;
  /** The selected text; empty for a follow-up. */
  quote: string;
  question: string;
  /** The underline this question belongs to. */
  markId: string;
  /** The turn id: the underline id for the first question of an underline, a fresh `ak…` id for a follow-up. */
  turnId: string;
  images?: PictureSource[];
  /** The attachment path under the note folder when this is a question about an HTML box. */
  htmlRel?: string;
  /** The first `#` heading of the editor buffer, or null (3.6). */
  heading: string | null;
  /** The `repo` of the note's agenda. */
  repo?: string;
  /** The author session ids to try: the note's own, then the root note's for a sub note (3.7). */
  authorSession?: string;
  /** Model, effort and wait limit as they were at the moment of the press (3.4, 3.12). */
  model: string;
  effort: string;
  timeoutSeconds: number;
}

// --------------------------------------------------------------------------
// The per-note line (3.5)
// --------------------------------------------------------------------------

const lines = new Map<string, Promise<void>>();

/** A later question waits for the end of the last one of the same key. A throw does not stop the line. */
function enqueue(key: string, task: () => Promise<void>): Promise<void> {
  const tail = lines.get(key) ?? Promise.resolve();
  const next = tail.then(task).catch((error) => {
    console.error('ask line failed on', key, error);
  });
  lines.set(key, next);
  void next.finally(() => {
    if (lines.get(key) === next) lines.delete(key);
  });
  return next;
}

export function isLineBusy(key: string): boolean {
  return lines.has(key);
}

// --------------------------------------------------------------------------
// Pictures (2.8, 3.4)
// --------------------------------------------------------------------------

export interface PictureWrite {
  /** `attach/<name>` for every picture that was written. */
  written: string[];
  /** The reason one could not be written. A question with a failed picture is not asked (3.9). */
  failure?: string;
}

function extensionOf(file: PictureSource): string {
  const known = IMAGE_MIME_EXTENSIONS[file.type];
  if (known) return known;
  const dot = file.name.lastIndexOf('.');
  return dot >= 0 ? file.name.slice(dot + 1).toLowerCase() : 'png';
}

/** 2.8: `ask-<time in base 36>-<8 hex of a UUID>.<extension from the MIME>`. */
export function pictureName(file: PictureSource, now: number = Date.now(), uuid: string = randomUuid()): string {
  return `ask-${now.toString(36)}-${uuid.replace(/-/g, '').slice(0, 8)}.${extensionOf(file)}`;
}

export async function writePictures(host: AskHost, noteDir: string | undefined, files: PictureSource[] | undefined): Promise<PictureWrite> {
  const result: PictureWrite = { written: [] };
  if (!files || files.length === 0) return result;
  if (!noteDir) return { written: [], failure: '이 노트의 폴더를 몰라요.' };
  for (const file of files) {
    const name = pictureName(file);
    try {
      await host.writeImage(noteDir, name, new Uint8Array(await file.arrayBuffer()));
      result.written.push(`attach/${name}`);
    } catch (error) {
      return { ...result, failure: error instanceof Error ? error.message : String(error) };
    }
  }
  return result;
}

// --------------------------------------------------------------------------
// The author session bundle (3.7)
// --------------------------------------------------------------------------

/**
 * 3.7: one `read-session` with the note ids to try. When the first note has nothing to carry, the next id after
 * `author.note` is asked for. A failed read means no bundle, never a blocked question.
 */
export async function authorBundleFor(host: AskHost, sessionId: string, ids: string[]): Promise<{ text: string; lines: number }> {
  let rest = ids;
  while (rest.length > 0) {
    let author: EngineAuthor | null = null;
    try {
      const reply = await host.engine(['read-session', sessionId, 'limit=200', 'before=0', `author=${rest.join(',')}`]);
      if (reply.status !== 0) return { text: '', lines: 0 };
      author = JSON.parse(reply.stdout).author ?? null;
    } catch {
      return { text: '', lines: 0 };
    }
    if (!author || !author.mentions || author.mentions.length === 0) return { text: '', lines: 0 };
    const bundle = authorBundleText(sessionId, author);
    if (bundle.text) return bundle;
    const at = rest.indexOf(author.note);
    rest = at >= 0 ? rest.slice(at + 1) : [];
  }
  return { text: '', lines: 0 };
}

// --------------------------------------------------------------------------
// The engine argv of a turn (2.7)
// --------------------------------------------------------------------------

/** 3.10: the `append-ask-turn` argv. The turn's text is cut before it gets here. */
export function appendArgv(target: AskTarget, sessionId: string, turn: AskTurn): string[] {
  const argv = [
    'append-ask-turn',
    ...targetArgs(target),
    `session-id=${sessionId}`,
    `turn-id=${turn.id}`,
    `mark-id=${turn.markId || turn.id}`,
    `asked-at=${turn.askedAt}`,
  ];
  if (turn.quote) argv.push(`quote=${turn.quote}`);
  argv.push(`question=${turn.question}`, `answer=${turn.answer}`);
  if (turn.error) argv.push(`error=${turn.error}`);
  argv.push(`model=${turn.model}`, `effort=${turn.effort}`, `elapsed-ms=${Math.max(0, Math.min(2_147_483_647, Math.trunc(turn.elapsedMs) || 0))}`);
  if (turn.sessionLines) argv.push(`session-lines=${turn.sessionLines}`);
  if (turn.images && turn.images.length > 0) argv.push(`images=${turn.images.join(',')}`);
  return argv;
}

/** Saves one turn and returns the engine's thread, or null with the reason when it could not. */
export async function saveTurn(host: AskHost, vault: string, target: AskTarget, sessionId: string, turn: AskTurn): Promise<{ thread: AskThread } | { failed: string }> {
  try {
    const reply = await host.engine(appendArgv(target, sessionId, turn));
    if (reply.status !== 0) return { failed: reply.stderr.trim() || '저장하지 못했어요.' };
    const body = JSON.parse(reply.stdout);
    if (typeof body.path === 'string' && vault) host.announceSelfWrite(vault, body.path);
    const { path: _path, ...thread } = body;
    return { thread: thread as AskThread };
  } catch (error) {
    return { failed: error instanceof Error ? error.message : String(error) };
  }
}

// --------------------------------------------------------------------------
// One question
// --------------------------------------------------------------------------

/** 3.9: the cases that are not asked at all, in their order. Null when the question may go. */
export function refusalBeforeAsk(input: { picture: PictureWrite; hasPictures: boolean; htmlRel?: string; noteDir?: string; runtime: string }): string | null {
  if (input.picture.failure) return `붙인 그림을 못 저장해서 묻지 않았어요: ${input.picture.failure}`;
  if (input.hasPictures && input.runtime === 'http') return '지금 묻기 길(HTTP)은 그림을 못 실어서 묻지 않았어요.';
  if (input.htmlRel && input.runtime === 'http') return '지금 묻기 길(HTTP)은 HTML 문서를 못 열어서 묻지 않았어요.';
  if (input.htmlRel && !input.noteDir) return '이 노트의 폴더를 몰라서 HTML 문서를 못 실었어요.';
  return null;
}

async function knownRuntime(host: AskHost): Promise<string> {
  const known = askNow().runtime;
  if (known) return known;
  let runtime: string = 'cli';
  try {
    const asked = await host.askRuntime();
    if (asked === 'cli' || asked === 'agent-sdk' || asked === 'http') runtime = asked;
  } catch {
    /* outside the app, or a failure: cli (3.8) */
  }
  setAskRuntime(runtime as AskRuntime);
  return runtime;
}

/**
 * 3.4: what is decided at the press, before the first wait: the key, a thread in memory, the in-flight record,
 * the model, effort and limit (already in `request`), and the start of the picture writes. Then the question
 * joins the line of its note.
 */
export function startAsk(host: AskHost, request: AskRequest): Promise<void> {
  const key = request.target.id;
  let thread = askNow().threads[key];
  if (!thread) {
    thread = { noteId: request.target.id, sessionId: randomUuid(), turns: [] };
    setAskThread(key, thread);
  }
  const record: PendingAsk = {
    at: Date.now(),
    sessionId: thread.sessionId,
    timeoutSeconds: request.timeoutSeconds,
    ...(askNow().runtime ? { runtime: askNow().runtime as string } : {}),
    ...(isLineBusy(key) ? { queued: true } : {}),
  };
  setPendingAsk(key, request.markId, record);
  const pictures = writePictures(host, request.noteDir, request.images);
  return enqueue(key, () => depart(host, request, pictures));
}

async function depart(host: AskHost, request: AskRequest, picturesPromise: Promise<PictureWrite>): Promise<void> {
  const key = request.target.id;
  try {
    // 3.5: when its turn comes the queued mark is removed and the start time becomes the departure.
    const at = Date.now();
    const before = askNow().pending[key]?.[request.markId];
    const departed: PendingAsk = { ...(before ?? { sessionId: '' }), at };
    delete departed.queued;
    // The thread is read now, so a question behind another sees the turn the first one added.
    const thread = askNow().threads[key] ?? { noteId: request.target.id, sessionId: randomUuid(), turns: [] };
    departed.sessionId = thread.sessionId;
    setPendingAsk(key, request.markId, departed);

    const runtime = await knownRuntime(host);
    if (!departed.runtime) setPendingAsk(key, request.markId, { ...departed, runtime });
    const picture = await picturesPromise;
    const refusal = refusalBeforeAsk({ picture, hasPictures: (request.images?.length ?? 0) > 0, htmlRel: request.htmlRel, noteDir: request.noteDir, runtime });

    let result: AskAnswer;
    let sessionLines: number | undefined;
    if (refusal) {
      result = { ok: false, answer: '', error: refusal, elapsed_ms: 0 };
    } else {
      // 3.7: a bundle goes before the question when no turn of the thread has been answered yet.
      let bundle = '';
      if (!thread.turns.some(isAnswered) && request.authorSession) {
        const ids = request.target.parent ? [request.target.id, request.target.parent] : [request.target.id];
        const found = await authorBundleFor(host, request.authorSession, ids);
        bundle = found.text;
        if (found.lines > 0) sessionLines = found.lines;
      }
      const openingQuote = request.quote ? undefined : groupTurns(thread.turns).find((group) => group.markId === request.markId)?.quote;
      const prompt = buildAskPrompt({
        target: request.target,
        quote: request.quote,
        question: request.question,
        openingQuote,
        repo: request.repo,
        bundle,
        htmlPath: request.htmlRel ? `${request.noteDir}/${request.htmlRel}` : undefined,
        imagePaths: picture.written.map((relative) => `${request.noteDir}/${relative}`),
      });
      result = await host.askClaude({
        vault: request.vault,
        sessionId: thread.sessionId,
        model: request.model,
        effort: request.effort,
        prompt,
        resume: thread.turns.length > 0,
        timeoutSeconds: request.timeoutSeconds,
        keepAlive: false,
        persona: askPersona(request.target, request.heading),
      });
    }

    // 3.10: the turn is made now, after the answer, so askedAt is the time the answer arrived.
    const failed = !result.ok;
    noteAskOutcome(failed ? 'error' : 'ok');
    const turn: AskTurn = {
      id: request.turnId,
      markId: request.markId,
      askedAt: new Date().toISOString(),
      ...(request.quote.trim() ? { quote: cutText(request.quote, QUOTE_LIMIT) } : {}),
      question: cutText(request.question, TEXT_LIMIT),
      answer: failed ? '' : cutText(result.answer ?? '', TEXT_LIMIT),
      ...(failed ? { error: result.error || '까닭을 못 받았다' } : {}),
      model: request.model,
      effort: request.effort,
      elapsedMs: Math.max(0, Math.trunc(result.elapsed_ms) || 0),
      ...(sessionLines ? { sessionLines } : {}),
      ...(picture.written.length > 0 ? { images: picture.written } : {}),
    };

    const saved = request.vault ? await saveTurn(host, request.vault, request.target, thread.sessionId, turn) : { failed: 'vault 가 열려 있지 않아요.' };
    const memory = askNow().threads[key] ?? thread;
    if ('thread' in saved) {
      setAskThread(key, mergeUnsaved(saved.thread, memory));
    } else {
      // The turn stays in memory with `unsaved` and is never sent again (3.10).
      setAskThread(key, { ...memory, turns: [...memory.turns, { ...turn, unsaved: true }] });
    }
  } finally {
    setPendingAsk(key, request.markId, null);
    markNoteUnseen(key, request.rootId);
  }
}

// --------------------------------------------------------------------------
// The old thread move (2.10)
// --------------------------------------------------------------------------

export interface LegacyStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/**
 * 2.10: moves one note's old thread (kept under the legacy key) into the vault, turn by turn, with the model and
 * effort brought back to a pickable value. The note leaves the key only after every turn went through; a failure
 * keeps the original and returns what was received so far.
 */
export async function migrateLegacyThread(host: AskHost, storage: LegacyStorage, legacyKey: string, vault: string, target: AskTarget): Promise<AskThread | null> {
  let all: Record<string, any>;
  try {
    all = JSON.parse(storage.getItem(legacyKey) ?? 'null');
  } catch {
    return null;
  }
  if (!all || typeof all !== 'object') return null;
  const old = all[target.id];
  if (!old || typeof old !== 'object' || !Array.isArray(old.turns)) return null;
  const sessionId = typeof old.sessionId === 'string' ? old.sessionId : '';
  let latest: AskThread | null = null;
  for (const item of old.turns as any[]) {
    if (!item || typeof item !== 'object') continue;
    const turn: AskTurn = {
      id: String(item.id ?? newUnderlineId()),
      markId: typeof item.markId === 'string' ? item.markId : undefined,
      askedAt: String(item.askedAt ?? new Date().toISOString()),
      quote: typeof item.quote === 'string' && item.quote ? item.quote : undefined,
      question: String(item.question ?? ''),
      answer: String(item.answer ?? ''),
      error: typeof item.error === 'string' && item.error ? item.error : undefined,
      model: normalizeModel(item.model),
      effort: normalizeEffort(item.effort),
      elapsedMs: Number(item.elapsedMs) || 0,
      ...(typeof item.sessionLines === 'number' ? { sessionLines: item.sessionLines } : {}),
      ...(Array.isArray(item.images) ? { images: item.images.filter((value: unknown) => typeof value === 'string') } : {}),
    };
    const saved = await saveTurn(host, vault, target, sessionId, turn);
    if ('failed' in saved) return latest; // the original stays; what arrived is shown
    latest = saved.thread;
  }
  delete all[target.id];
  if (Object.keys(all).length === 0) storage.removeItem(legacyKey);
  else storage.setItem(legacyKey, JSON.stringify(all));
  return latest;
}

