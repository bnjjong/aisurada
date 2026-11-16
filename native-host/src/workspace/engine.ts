import { asRow, asText, rows } from './types.ts';
import type { Agenda, HostPort, Note, Workspace } from './types.ts';
import { fromArgv, read, toArgv } from '../data/commands.ts';
import type { Call } from '../data/commands.ts';

interface SidecarResult { status: number; stdout: string; stderr: string; }
/** 엔진을 부르는 데 필요한 호스트의 부분: 명령 호출과 Rust 엔진이 처리하는 명령 목록. */
export type EnginePort = Pick<HostPort, 'invoke'> & Partial<Pick<HostPort, 'engineCommands'>>;

export class SidecarCommandError extends Error {
  readonly status: number;
  readonly command: string;
  readonly stderr: string;
  /** `engine_call` 길의 오류 종류(`conflict`, `not_found` …). 명령줄 길(`sidecar_call`)의 오류에는 없다. */
  readonly kind?: string;
  constructor(command: string, result: SidecarResult, kind?: string) {
    super(`동기화 엔진 명령이 실패했어요(${command}, 종료 코드 ${result.status}): ${result.stderr.trim()}`);
    this.name = 'SidecarCommandError'; this.command = command; this.status = result.status; this.stderr = result.stderr.trim();
    this.kind = kind;
  }
}

export async function engineRefusal(host: HostPort, request: string[] | Call): Promise<{ ok: true; value: unknown } | { ok: false; reason: string }> {
  if (!Array.isArray(request)) {
    try { return { ok: true, value: await engineCall(host, request) }; }
    catch (error) { if (error instanceof SidecarCommandError) return { ok: false, reason: error.stderr }; throw error; }
  }
  const result = await host.invoke<SidecarResult>('sidecar_call', { argv: request });
  return result.status === 0 ? { ok: true, value: JSON.parse(result.stdout) } : { ok: false, reason: result.stderr.trim() };
}

/**
 * 엔진 호출의 길목. 호출 객체(`Call`)는 호스트가 Rust 엔진으로 처리한다고 알린 명령이면 Rust 엔진으로, 아니면 명령줄 길(`sidecar_call`, Rust `ais-cli`)로 보낸다.
 * 문자열 목록(argv)은 늘 명령줄 길이다. 응답은 길마다 모양이 다르다(Rust 엔진은 목록을 `{items, total, …}`로, 명령줄 길은 옛 모양으로 준다).
 * 받는 쪽이 `rows(value, ['items', '옛 이름'])`처럼 둘 다 읽는다.
 */
export async function engineCall(host: EnginePort, request: string[] | Call): Promise<unknown> {
  if (Array.isArray(request)) return cliCall(host, request);
  const supported = await host.engineCommands?.().catch(() => undefined);
  return supported?.has(request.command) ? rustCall(host, request) : cliCall(host, toArgv(request));
}

/**
 * 옛 `{status, stdout, stderr}` 응답이 필요한 호출자(Claude 연동의 묻기·대화)를 위한 길. 인자 목록을 호출 객체로 바꿔,
 * 호스트가 Rust 엔진이 처리한다고 알린 명령이면 Rust 엔진으로 보내고 결과를 옛 응답 모양으로 되돌린다.
 * 알리지 않은 명령은 인자 목록 그대로 명령줄 길로 보내고 응답을 건드리지 않는다.
 */
export async function engineReply(host: EnginePort, argv: string[]): Promise<SidecarResult> {
  const call = fromArgv(argv);
  const supported = await host.engineCommands?.().catch(() => undefined);
  if (!supported?.has(call.command)) return host.invoke<SidecarResult>('sidecar_call', { argv });
  try {
    return { status: 0, stdout: JSON.stringify(await rustCall(host, call)), stderr: '' };
  } catch (error) {
    if (error instanceof SidecarCommandError) return { status: error.status, stdout: '', stderr: error.stderr };
    throw error;
  }
}

async function cliCall(host: EnginePort, argv: string[]): Promise<unknown> {
  const result = await host.invoke<SidecarResult>('sidecar_call', { argv });
  if (result.status !== 0) {
    throw new SidecarCommandError(argv[0], result);
  }
  return JSON.parse(result.stdout);
}

async function rustCall(host: EnginePort, call: Call): Promise<unknown> {
  try {
    return await host.invoke('engine_call', { command: call.command, args: call.args ?? {} });
  } catch (error) {
    const failure = asRow(error);
    const kind = typeof failure?.kind === 'string' ? failure.kind : undefined;
    const text = failure ? `${asText(failure.code)} ${asText(failure.message)}`.trim() : String(error);
    // 오류는 `kind`로 가른다. 종료 코드 칸은 옛 `{status, stdout, stderr}` 응답을 받는 쪽(Claude 연동)을 위한 것이라 종류가 있으면 3, 없으면 1이다.
    throw new SidecarCommandError(call.command, { status: kind ? 3 : 1, stdout: '', stderr: text }, kind);
  }
}

/**
 * 쓰기가 「읽은 뒤 다른 쪽이 먼저 고쳤다」(AIS-412)로 거절됐는가. Rust 엔진 길은 오류 종류(`precondition_failed`)로,
 * 명령줄 길(`ais-cli`)은 종료 코드 5로 알린다.
 */
export const isPreconditionFailure = (error: unknown): boolean =>
  error instanceof SidecarCommandError && (error.kind === 'precondition_failed' || (error.kind === undefined && error.status === 5));

// 6.2: a report body is HTML only when it starts with a tag name followed by a space, > or /.
// `<https://…>` (an autolink) is markdown.
export const REPORT_HTML = /^\s*<\/?[a-zA-Z][a-zA-Z0-9-]*(?=[\s>/])/;

export async function readWorkspace(host: HostPort, reindex: boolean, previous?: Workspace, _sanitizeHTML: (html: string) => string = value => value,
  current: () => boolean = () => true): Promise<Pick<Workspace,
  'agendas' | 'projects' | 'notes' | 'templates' | 'trash' | 'skipped' | 'conflicts' | 'noteReadFailed'>> {
  const first = await Promise.allSettled([
    reindex ? engineCall(host, read.reindex()) : Promise.resolve(null),
    engineCall(host, read.templates()), engineCall(host, read.trash()),
  ]);
  if (!current()) return { agendas: [], projects: [], notes: [], templates: [], trash: [], skipped: [], conflicts: [], noteReadFailed: true };
  const second = await Promise.allSettled([
    engineCall(host, read.agendas()), engineCall(host, read.projects()), engineCall(host, read.docs()),
  ]);
  const value = (result: PromiseSettledResult<unknown>) => result.status === 'fulfilled' ? result.value : null;
  const fallback = (result: PromiseSettledResult<unknown>, old: unknown) => result.status === 'fulfilled' ? result.value : old;
  const agendas = rows(fallback(second[0], previous?.agendas), 'items').map(r => ({ ...r, id: asText(r.id), slug: asText(r.slug), title: asText(r.title) })) as Agenda[];
  const notes: Note[] = rows(value(second[2]), 'items').map(r => {
    const created = asText(r.created);
    return { ...r, id: asText(r.id), title: asText(r.title), body: asText(r.body), agenda: asText(r.agenda),
      agendaId: agendas.find(a => a.slug === r.agenda)?.id || '', format: asText(r.format) || 'markdown',
      created, date: created.slice(0, 10), time: /T((?:[01]\d|2[0-3]):[0-5]\d)/.exec(created)?.[1] || '00:00' };
  });
  const index = asRow(value(first[0]));
  return {
    agendas, projects: rows(fallback(second[1], previous?.projects), 'items'), notes,
    templates: rows(fallback(first[1], previous?.templates), ['items', 'templates']).filter(r => r.category === 'doc' || r.category === 'ritual'),
    trash: rows(fallback(first[2], previous?.trash), ['items', 'trash']), skipped: rows(index?.skipped), conflicts: rows(index?.conflicts),
    noteReadFailed: second[2].status === 'rejected',
  };
}
