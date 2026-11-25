// Each item is an argv cell. Values pass directly to the host, without a shell.
export type Fields = Record<string, string | number | boolean | string[] | object[] | undefined>;
const cells = (fields: Fields) => Object.entries(fields).filter(([, value]) => value !== undefined)
  .map(([key, value]) => `${key}=${Array.isArray(value) ? JSON.stringify(value) : String(value)}`);

/**
 * 엔진 호출 하나: 명령 이름(snake_case)과 인자 객체. `engine_call`은 이 객체를 그대로 받고, 명령줄 길로 보낼 때는
 * `toArgv`로 `key=value` 문자열 목록으로 푼다. 옮겨진 명령부터 이 모양으로 만들고, 나머지는 아직 argv 목록이다.
 */
export interface Call { command: string; args?: Fields; }
/** 명령줄이 `key=value`가 아니라 자리(positional)로 받는 인자: 호출 객체의 인자 이름을 그 차례로 앞에 둔다. */
const POSITIONAL: Record<string, string[]> = {
  get_doc: ['id'], list_sub_notes: ['parent'], get_sub_note: ['parent', 'id'],
  list_comments: ['id', 'subId'], list_decisions: ['id', 'subId'], list_attachments: ['id', 'subId'], get_agenda_facts: ['id'], history: ['id', 'number'],
  update_agenda: ['id'], delete_agenda: ['id'], update_project: ['id'], delete_project: ['id'],
  save_ritual_template: ['id'], delete_doc_template: ['id'], update_agenda_facts: ['id'],
  set_favorite: ['id'], evaluate_doc: ['id'], link_docs: ['from', 'to'], update_doc: ['id'],
  create_sub_note: ['parent'], update_sub_note: ['parent', 'id'], delete_sub_note: ['parent', 'id'],
  create_comment: ['id', 'subId'], update_comment: ['id', 'subId'], delete_comment: ['id', 'subId'],
  delete_doc: ['id'], restore_doc: ['trashId'], purge_doc: ['trashId'],
  get_task: ['id'], update_task: ['id'], transition_task: ['id'], add_task_comment: ['id'], delete_task: ['id'], restore_task: ['id'],
  link_tasks: ['from', 'to'],
  get_resource: ['id'], delete_resource: ['id'], restore_resource: ['id'], usable_resources: ['project'], list_resource_versions: ['id'],
  set_resource_ref: ['project', 'id'], get_resource_change: ['id'], approve_resource_change: ['id'], reject_resource_change: ['id'],
  resolve_resource_edit: ['id'],
  get_ask: ['id', 'subId'], append_ask_turn: ['id', 'subId'], list_chats: ['id', 'subId'], append_chat_turn: ['id', 'subId'],
  read_session: ['id'],
  get_c4_model: ['project'], apply_c4_changes: ['project'],
  list_c4_diagrams: ['project'], create_c4_diagram: ['project'], rename_c4_diagram: ['project'], delete_c4_diagram: ['project'],
};
/** 자리 인자를 여럿 받는 명령: 호출 객체에서는 그 인자들을 이 이름의 배열로 둔다. */
const VARIADIC: Record<string, string> = { session_titles: 'ids' };
/** 호출 객체를 명령줄이 받는 문자열 목록으로 푼다(`list_docs` → `list-docs`). */
export const toArgv = (call: Call): string[] => {
  const args = call.args ?? {}, positional = POSITIONAL[call.command] ?? [];
  const variadic = VARIADIC[call.command];
  const placed = variadic
    ? (Array.isArray(args[variadic]) ? (args[variadic] as string[]) : []).map(String)
    : positional.map(key => args[key]).filter(value => value !== undefined).map(String);
  const named = Object.fromEntries(Object.entries(args).filter(([key]) => !positional.includes(key) && key !== variadic));
  return [call.command.replace(/_/g, '-'), ...placed, ...cells(named)];
};

/** 이름 인자 칸(`key=value`)의 앞부분 */
const NAMED_CELL = /^[A-Za-z][A-Za-z0-9_-]*=/;
/**
 * `toArgv`의 거꾸로: 옛 `key=value` 문자열 목록을 호출 객체로 바꾼다. 명령 이름은 snake_case로, `POSITIONAL`에 있는 인자는 앞의 자리 값으로 읽고,
 * 나머지 칸은 `key=value`를 처음 `=`에서 가른다(값은 글 그대로). 태스크 화면이 옛 인자 목록 만드는 함수를 그대로 두고 Rust 길로 보낼 때 쓴다.
 */
export const fromArgv = (argv: string[]): Call => {
  const command = argv[0].replace(/-/g, '_'), positional = POSITIONAL[command] ?? [], args: Fields = {};
  let rest = argv.slice(1);
  // 자리 인자는 앞에서부터 `key=value`가 아닌 칸이다. 뒤쪽 자리 인자가 선택(후속 노트 id)이어도 이름 칸을 자리로 읽지 않는다.
  let used = 0;
  for (const key of positional) {
    if (used >= rest.length || NAMED_CELL.test(rest[used])) break;
    args[key] = rest[used++];
  }
  const variadic = VARIADIC[command];
  if (variadic) {
    args[variadic] = rest.filter(cell => !NAMED_CELL.test(cell));
    rest = rest.filter(cell => NAMED_CELL.test(cell));
    used = 0;
  }
  for (const cell of rest.slice(used)) {
    const at = cell.indexOf('=');
    args[at < 0 ? cell : cell.slice(0, at)] = at < 0 ? '' : cell.slice(at + 1);
  }
  return Object.keys(args).length ? { command, args } : { command };
};

export const read = {
  reindex: (): Call => ({ command: 'reindex' }),
  templates: (): Call => ({ command: 'list_templates', args: { bodies: true } }), trash: (): Call => ({ command: 'list_trash' }),
  agendas: (): Call => ({ command: 'list_agendas' }), projects: (): Call => ({ command: 'list_projects' }),
  docs: (): Call => ({ command: 'list_docs', args: { limit: 0, bodies: true } }),
  subNotes: (parent: string): Call => ({ command: 'list_sub_notes', args: { parent, bodies: true } }),
  comments: (root: string, sub?: string): Call => ({ command: 'list_comments', args: { id: root, subId: sub || undefined } }),
  doc: (id: string): Call => ({ command: 'get_doc', args: { id } }),
  subNote: (parent: string, id: string, includeIdPath?: true): Call => ({ command: 'get_sub_note',
    args: includeIdPath ? { parent, id, includeIdPath } : { parent, id } }),
  facts: (id: string): Call => ({ command: 'get_agenda_facts', args: { id } }),
  history: (id: string, fields: Fields = {}): Call => ({ command: 'history', args: { id, ...fields } }),
  commit: (): Call => ({ command: 'commit_vault' }),
  sessionTitles: (ids: string[]) => ['session-titles', ...new Set(ids)],
};

export interface NewDoc {
  agenda: string; type: string; title: string; body: string; date: string; time: string;
  tags: string[]; subAgendaIds?: string[]; template?: string; tier?: string;
}
export const write = {
  init: (name: string): Call => ({ command: 'init_workspace', args: { name, templates: '[]' } }),
  createDoc: (doc: NewDoc): Call => ({ command: 'create_doc', args: { agenda: doc.agenda, type: doc.type, title: doc.title,
    body: doc.body, date: doc.date, time: doc.time, format: 'markdown', tags: doc.tags,
    subAgendaIds: doc.subAgendaIds, template: doc.template, tier: doc.tier === 'record' ? 'record' : undefined } }),
  updateDoc: (id: string, fields: Fields, token?: string): Call => ({ command: 'update_doc', args: { id, ...fields, f1: token || undefined } }),
  favorite: (id: string, fields: Fields, token?: string): Call => ({ command: 'set_favorite', args: { id, ...fields, f1: token || undefined } }),
  evaluate: (id: string, fields: Fields, token?: string): Call => ({ command: 'evaluate_doc', args: { id, ...fields, f1: token || undefined } }),
  link: (from: string, to: string, kind: string, remove: boolean, token?: string): Call =>
    ({ command: 'link_docs', args: { from, to, kind, remove: remove || undefined, f1: token } }),
  createSub: (parent: string): Call => ({ command: 'create_sub_note', args: { parent, title: '새 후속 노트', body: '# 새 후속 노트\n\n', type: 'free' } }),
  updateSub: (parent: string, id: string, fields: Fields, token?: string): Call => ({ command: 'update_sub_note', args: { parent, id, ...fields, f1: token || undefined } }),
  deleteSub: (parent: string, id: string): Call => ({ command: 'delete_sub_note', args: { parent, id } }),
  createComment: (root: string, sub: string | undefined, id: string, body: string, quote?: string): Call =>
    ({ command: 'create_comment', args: { id: root, subId: sub || undefined, body, comment_id: id, anchor_quote: quote || undefined } }),
  updateComment: (root: string, sub: string | undefined, id: string, body: string): Call =>
    ({ command: 'update_comment', args: { id: root, subId: sub || undefined, comment_id: id, body } }),
  deleteComment: (root: string, sub: string | undefined, id: string): Call =>
    ({ command: 'delete_comment', args: { id: root, subId: sub || undefined, comment_id: id } }),
  deleteDoc: (id: string): Call => ({ command: 'delete_doc', args: { id } }),
  restore: (id: string): Call => ({ command: 'restore_doc', args: { trashId: id } }),
  purge: (id: string): Call => ({ command: 'purge_doc', args: { trashId: id } }),
  emptyTrash: (): Call => ({ command: 'empty_trash' }),
  createSubAgenda: (agendaId: string, fields: Fields): Call => ({ command: 'create_sub_agenda', args: { agendaId, ...fields } }),
  updateSubAgenda: (agendaId: string, id: string, fields: Fields): Call => ({ command: 'update_sub_agenda', args: { agendaId, id, ...fields } }),
  createAgenda: (fields: Fields): Call => ({ command: 'create_agenda', args: fields }),
  updateAgenda: (id: string, fields: Fields): Call => ({ command: 'update_agenda', args: { id, ...fields } }),
  deleteAgenda: (id: string): Call => ({ command: 'delete_agenda', args: { id } }),
  createProject: (fields: Fields): Call => ({ command: 'create_project', args: fields }),
  updateProject: (id: string, fields: Fields): Call => ({ command: 'update_project', args: { id, ...fields } }),
  deleteProject: (id: string): Call => ({ command: 'delete_project', args: { id } }),
  saveTemplate: (fields: Fields): Call => ({ command: 'save_template', args: fields }),
  saveRitual: (id: string, fields: Fields): Call => ({ command: 'save_ritual_template', args: { id, ...fields } }),
  deleteTemplate: (id: string): Call => ({ command: 'delete_doc_template', args: { id } }),
  facts: (id: string, facts: string): Call => ({ command: 'update_agenda_facts', args: { id, facts: facts.trim() } }),
};

export function cleanTags(value: unknown): string[] {
  return Array.isArray(value) ? [...new Set(value.filter((tag): tag is string => typeof tag === 'string').map(tag => tag.trim()).filter(Boolean))].slice(0, 50) : [];
}
export const commentId = (now: number, uuid: string) => `cm${now.toString(36)}-${uuid.slice(0, 8)}`;
export function commitOutcome(value: Record<string, unknown>) {
  const outcome = ['committed', 'nothing', 'skipped', 'failed', 'disabled'].includes(String(value.outcome)) ? String(value.outcome) : 'failed';
  const blocked = ['off', 'not-repo', 'index-lock', 'in-progress', 'not-main'].includes(String(value.blocked)) ? String(value.blocked) : undefined;
  return { outcome, blocked };
}
