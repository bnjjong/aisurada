import { asRow, asText, rows } from './types.ts';
import type { Agenda, HostPort, Note, Workspace } from './types.ts';
import { read } from '../data/commands.ts';

interface SidecarResult { status: number; stdout: string; stderr: string; }

export class SidecarCommandError extends Error {
  readonly status: number;
  readonly command: string;
  readonly stderr: string;
  constructor(command: string, result: SidecarResult) {
    super(`사이드카 명령이 실패했어요(${command}, 종료 코드 ${result.status}): ${result.stderr.trim()}`);
    this.name = 'SidecarCommandError'; this.command = command; this.status = result.status; this.stderr = result.stderr.trim();
  }
}

export async function engineRefusal(host: HostPort, argv: string[]): Promise<{ ok: true; value: unknown } | { ok: false; reason: string }> {
  const result = await host.invoke<SidecarResult>('sidecar_call', { argv });
  return result.status === 0 ? { ok: true, value: JSON.parse(result.stdout) } : { ok: false, reason: result.stderr.trim() };
}

export async function engineCall(host: HostPort, argv: string[]): Promise<unknown> {
  const result = await host.invoke<SidecarResult>('sidecar_call', { argv });
  if (result.status !== 0) {
    throw new SidecarCommandError(argv[0], result);
  }
  return JSON.parse(result.stdout);
}

export async function readWorkspace(host: HostPort, reindex: boolean, previous?: Workspace, sanitizeHTML: (html: string) => string = value => value,
  current: () => boolean = () => true): Promise<Pick<Workspace,
  'agendas' | 'projects' | 'notes' | 'reports' | 'templates' | 'trash' | 'skipped' | 'conflicts' | 'noteReadFailed'>> {
  const first = await Promise.allSettled([
    reindex ? engineCall(host, read.reindex()) : Promise.resolve(null),
    engineCall(host, read.reports()), engineCall(host, read.templates()), engineCall(host, read.trash()),
  ]);
  if (!current()) return { agendas: [], projects: [], notes: [], reports: [], templates: [], trash: [], skipped: [], conflicts: [], noteReadFailed: true };
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
    reports: rows(fallback(first[1], previous?.reports), 'reports').map(row => ({ ...row, body: asText(row.body).trim() ? /^\s*<(!DOCTYPE|[a-z])/i.test(asText(row.body)) ? sanitizeHTML(asText(row.body)) : asText(row.body) : '', memo: asText(row.memo).trim() ? asText(row.memo) : '' })),
    templates: rows(fallback(first[2], previous?.templates), 'templates').filter(r => r.category === 'doc' || r.category === 'ritual'),
    trash: rows(fallback(first[3], previous?.trash), 'trash'), skipped: rows(index?.skipped), conflicts: rows(index?.conflicts),
    noteReadFailed: second[2].status === 'rejected',
  };
}
