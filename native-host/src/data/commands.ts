// Each item is an argv cell. Values pass directly to the host, without a shell.
export type Fields = Record<string, string | number | boolean | string[] | undefined>;
const cells = (fields: Fields) => Object.entries(fields).filter(([, value]) => value !== undefined)
  .map(([key, value]) => `${key}=${Array.isArray(value) ? JSON.stringify(value) : String(value)}`);
const command = (name: string, positional: string[] = [], fields: Fields = {}, token?: string) =>
  [name, ...positional, ...cells(fields), ...(token ? [`f1=${token}`] : [])];

/**
 * 엔진 호출 하나: 명령 이름(snake_case)과 인자 객체. Rust 엔진은 이 객체를 그대로 받고, Java 엔진으로 보낼 때는
 * `toArgv`로 `key=value` 문자열 목록으로 푼다. 옮겨진 명령부터 이 모양으로 만들고, 나머지는 아직 argv 목록이다.
 */
export interface Call { command: string; args?: Fields; }
/** Java 엔진이 `key=value`가 아니라 자리(positional)로 받는 인자: 호출 객체의 인자 이름을 그 차례로 앞에 둔다. */
const POSITIONAL: Record<string, string[]> = {
  get_doc: ['id'], list_sub_notes: ['parent'], get_sub_note: ['parent', 'id'],
  list_comments: ['id', 'subId'], list_attachments: ['id', 'subId'], get_agenda_facts: ['id'], history: ['id', 'number'],
};
/** 호출 객체를 Java 엔진이 받는 문자열 목록으로 푼다(`list_docs` → `list-docs`). */
export const toArgv = (call: Call): string[] => {
  const args = call.args ?? {}, positional = POSITIONAL[call.command] ?? [];
  const placed = positional.map(key => args[key]).filter(value => value !== undefined).map(String);
  const named = Object.fromEntries(Object.entries(args).filter(([key]) => !positional.includes(key)));
  return [call.command.replace(/_/g, '-'), ...placed, ...cells(named)];
};

export const read = {
  reindex: (): Call => ({ command: 'reindex' }),
  templates: (): Call => ({ command: 'list_templates', args: { bodies: true } }), trash: (): Call => ({ command: 'list_trash' }),
  agendas: (): Call => ({ command: 'list_agendas' }), projects: (): Call => ({ command: 'list_projects' }),
  docs: (): Call => ({ command: 'list_docs', args: { limit: 0, bodies: true } }),
  subNotes: (parent: string): Call => ({ command: 'list_sub_notes', args: { parent, bodies: true } }),
  comments: (root: string, sub?: string): Call => ({ command: 'list_comments', args: { id: root, subId: sub || undefined } }),
  doc: (id: string): Call => ({ command: 'get_doc', args: { id } }),
  subNote: (parent: string, id: string): Call => ({ command: 'get_sub_note', args: { parent, id } }),
  facts: (id: string): Call => ({ command: 'get_agenda_facts', args: { id } }),
  history: (id: string, fields: Fields = {}): Call => ({ command: 'history', args: { id, ...fields } }),
  commit: () => ['commit-vault'], schedule: () => ['schedule', 'status', '--json'],
  sessionTitles: (ids: string[]) => ['session-titles', ...new Set(ids)],
  readSession: (sessionId: string, limit = 100, urls = true) => ['read-session', sessionId, `limit=${limit}`, `urls=${urls}`],
  listChats: (noteId: string, subId?: string) => ['list-chats', noteId, ...(subId ? [subId] : [])],
};

export interface NewDoc {
  agenda: string; type: string; title: string; body: string; date: string; time: string;
  tags: string[]; template?: string; tier?: string;
}
export const write = {
  init: (name: string) => command('init-workspace', [], { name, templates: [] }),
  createDoc: (doc: NewDoc) => command('create-doc', [], { agenda: doc.agenda, type: doc.type, title: doc.title,
    body: doc.body, date: doc.date, time: doc.time, format: 'markdown', tags: doc.tags,
    template: doc.template, tier: doc.tier === 'record' ? 'record' : undefined }),
  updateDoc: (id: string, fields: Fields, token?: string) => command('update-doc', [id], fields, token),
  favorite: (id: string, fields: Fields, token?: string) => command('set-favorite', [id], fields, token),
  evaluate: (id: string, fields: Fields, token?: string) => command('evaluate-doc', [id], fields, token),
  link: (from: string, to: string, kind: string, remove: boolean, token?: string) => command('link-docs', [from, to], { kind, remove: remove || undefined }, token),
  createSub: (parent: string) => command('create-sub-note', [parent], { title: '새 후속 노트', body: '# 새 후속 노트\n\n', type: 'free' }),
  updateSub: (parent: string, id: string, fields: Fields, token?: string) => command('update-sub-note', [parent, id], fields, token),
  deleteSub: (parent: string, id: string) => ['delete-sub-note', parent, id],
  createComment: (root: string, sub: string | undefined, id: string, body: string, quote?: string) => command('create-comment', [root, ...(sub ? [sub] : [])], { body, comment_id: id, anchor_quote: quote || undefined }),
  updateComment: (root: string, sub: string | undefined, id: string, body: string) => command('update-comment', [root, ...(sub ? [sub] : [])], { comment_id: id, body }),
  deleteComment: (root: string, sub: string | undefined, id: string) => command('delete-comment', [root, ...(sub ? [sub] : [])], { comment_id: id }),
  deleteDoc: (id: string) => ['delete-doc', id], restore: (id: string) => ['restore-doc', id],
  purge: (id: string) => ['purge-doc', id], emptyTrash: () => ['empty-trash'],
  createAgenda: (fields: Fields) => command('create-agenda', [], fields), updateAgenda: (id: string, fields: Fields) => command('update-agenda', [id], fields),
  deleteAgenda: (id: string) => ['delete-agenda', id],
  createProject: (fields: Fields) => command('create-project', [], fields), updateProject: (id: string, fields: Fields) => command('update-project', [id], fields),
  deleteProject: (id: string) => ['delete-project', id],
  saveTemplate: (fields: Fields) => command('save-template', [], fields), saveRitual: (id: string, fields: Fields) => command('save-ritual-template', [id], fields),
  deleteTemplate: (id: string) => ['delete-doc-template', id],
  facts: (id: string, facts: string) => command('update-agenda-facts', [id], { facts: facts.trim() }),
  appendChatTurn: (noteId: string, subId: string | undefined, fields: Fields) =>
    command('append-chat-turn', [noteId, ...(subId ? [subId] : [])], fields),
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
