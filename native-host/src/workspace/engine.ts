import { asRow, asText, rows } from './types.ts';
import type { Agenda, HostPort, Note, Row, Workspace } from './types.ts';
import { read, toArgv } from '../data/commands.ts';
import type { Call } from '../data/commands.ts';

interface SidecarResult { status: number; stdout: string; stderr: string; }

export class SidecarCommandError extends Error {
  readonly status: number;
  readonly command: string;
  readonly stderr: string;
  /** Rust 엔진이 낸 오류의 종류(`conflict`, `not_found` …). Java 엔진의 오류에는 없다. */
  readonly kind?: string;
  constructor(command: string, result: SidecarResult, kind?: string) {
    super(`동기화 엔진 명령이 실패했어요(${command}, 종료 코드 ${result.status}): ${result.stderr.trim()}`);
    this.name = 'SidecarCommandError'; this.command = command; this.status = result.status; this.stderr = result.stderr.trim();
    this.kind = kind;
  }
}

export async function engineRefusal(host: HostPort, argv: string[]): Promise<{ ok: true; value: unknown } | { ok: false; reason: string }> {
  const result = await host.invoke<SidecarResult>('sidecar_call', { argv });
  return result.status === 0 ? { ok: true, value: JSON.parse(result.stdout) } : { ok: false, reason: result.stderr.trim() };
}

/**
 * 엔진 호출의 길목. 호출 객체(`Call`)는 호스트가 Rust 엔진으로 처리한다고 알린 명령이면 Rust 엔진으로, 아니면 Java 엔진으로 보낸다.
 * 문자열 목록(argv)은 늘 Java 엔진이다. 어느 쪽이든 화면은 같은 모양의 응답을 받는다(Rust 응답은 어댑터가 기존 모양으로 바꾼다).
 */
export async function engineCall(host: HostPort, request: string[] | Call): Promise<unknown> {
  if (Array.isArray(request)) return javaCall(host, request);
  const supported = await host.engineCommands?.().catch(() => undefined);
  return supported?.has(request.command) ? rustCall(host, request) : javaCall(host, toArgv(request));
}

async function javaCall(host: HostPort, argv: string[]): Promise<unknown> {
  const result = await host.invoke<SidecarResult>('sidecar_call', { argv });
  if (result.status !== 0) {
    throw new SidecarCommandError(argv[0], result);
  }
  return JSON.parse(result.stdout);
}

/** Rust 엔진의 오류 종류를 옛 종료 코드에 맞춘다(화면이 쓰기 충돌을 종료 코드 5로 가르는 곳이 아직 있다). */
const LEGACY_STATUS: Record<string, number> = { precondition_failed: 5, internal: 1 };

async function rustCall(host: HostPort, call: Call): Promise<unknown> {
  let value: unknown;
  try {
    value = await host.invoke('engine_call', { command: call.command, args: call.args ?? {} });
  } catch (error) {
    const failure = asRow(error);
    const kind = typeof failure?.kind === 'string' ? failure.kind : undefined;
    const text = failure ? `${asText(failure.code)} ${asText(failure.message)}`.trim() : String(error);
    throw new SidecarCommandError(call.command, { status: LEGACY_STATUS[kind ?? ''] ?? (kind ? 3 : 1), stdout: '', stderr: text }, kind);
  }
  return (ADAPTERS[call.command] ?? (v => v))(value);
}

const withDateAndTime = (row: Row): Row => {
  const created = asText(row.created);
  return { ...row, date: created.slice(0, 10), time: /T((?:[01]\d|2[0-3]):[0-5]\d)/.exec(created)?.[1] || '00:00' };
};
const items = (value: unknown): Row[] => rows(value, 'items');

/** Rust 엔진의 새 응답을 화면이 지금 쓰는 모양으로 바꾼다. 화면 타입을 새 모양으로 조이면 이 어댑터를 걷어낸다. */
const ADAPTERS: Record<string, (value: unknown) => unknown> = {
  list_docs: value => items(value).map(withDateAndTime),
  list_agendas: items, list_projects: items, list_templates: items, list_trash: items,
};

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
  const agendas = rows(fallback(second[0], previous?.agendas)).map(r => ({ ...r, id: asText(r.id), slug: asText(r.slug), title: asText(r.title) })) as Agenda[];
  const notes: Note[] = rows(value(second[2])).map(r => {
    const created = asText(r.created);
    return { ...r, id: asText(r.id), title: asText(r.title), body: asText(r.body), agenda: asText(r.agenda),
      agendaId: agendas.find(a => a.slug === r.agenda)?.id || '', format: asText(r.format) || 'markdown',
      created, date: created.slice(0, 10), time: /T((?:[01]\d|2[0-3]):[0-5]\d)/.exec(created)?.[1] || '00:00' };
  });
  const index = asRow(value(first[0]));
  return {
    agendas, projects: rows(fallback(second[1], previous?.projects)), notes,
    templates: rows(fallback(first[1], previous?.templates), 'templates').filter(r => r.category === 'doc' || r.category === 'ritual'),
    trash: rows(fallback(first[2], previous?.trash), 'trash'), skipped: rows(index?.skipped), conflicts: rows(index?.conflicts),
    noteReadFailed: second[2].status === 'rejected',
  };
}
