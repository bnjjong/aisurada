// Each item is an argv cell. Values pass directly to the host, without a shell.
export type Fields = Record<string, string | number | boolean | string[] | undefined>;
const cells = (fields: Fields) => Object.entries(fields).filter(([, value]) => value !== undefined)
  .map(([key, value]) => `${key}=${Array.isArray(value) ? JSON.stringify(value) : String(value)}`);
const command = (name: string, positional: string[] = [], fields: Fields = {}, token?: string) =>
  [name, ...positional, ...cells(fields), ...(token ? [`f1=${token}`] : [])];

export const read = {
  reindex: () => ['reindex'], reports: () => ['list-reports', 'bodies=true'],
  templates: () => ['list-templates', 'bodies=true'], trash: () => ['list-trash'],
  agendas: () => ['list-agendas'], projects: () => ['list-projects'], docs: () => ['list-docs', 'limit=0', 'bodies=true'],
  subNotes: (parent: string) => ['list-sub-notes', parent, 'bodies=true'],
  comments: (root: string, sub?: string) => ['list-comments', root, ...(sub ? [sub] : [])],
  doc: (id: string) => ['get-doc', id], subNote: (parent: string, id: string) => ['get-sub-note', parent, id],
  facts: (id: string) => ['get-agenda-facts', id], history: (id: string, fields: Fields = {}) => command('history', [id], fields),
  commit: () => ['commit-vault'], schedule: () => ['schedule', 'status', '--json'],
  sessionTitles: (ids: string[]) => ['session-titles', ...new Set(ids)],
  readSession: (sessionId: string, limit = 100, urls = true) => ['read-session', sessionId, `limit=${limit}`, `urls=${urls}`],
  listChats: (noteId: string, subId?: string) => ['list-chats', noteId, ...(subId ? [subId] : [])],
};

export interface NewDoc {
  agenda: string; type: string; title: string; body: string; date: string; time: string;
  mins: number; tags: string[]; template?: string; tier?: string;
}
export const write = {
  init: (name: string) => command('init-workspace', [], { name, templates: [] }),
  createDoc: (doc: NewDoc) => command('create-doc', [], { agenda: doc.agenda, type: doc.type, title: doc.title,
    body: doc.body, date: doc.date, time: doc.time, format: 'markdown', mins: doc.mins, tags: doc.tags,
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
  report: (id: string, body: string, memo: string) => {
    const match = /^rv-(morning|evening|weekly)-(\d{4}-\d{2}-\d{2})$/.exec(id);
    if (!match) throw new Error('리포트 id가 올바르지 않아요.');
    return command('save-report', [], { kind: match[1], date: match[2], body: body.trim() ? body : undefined, memo: memo.trim() ? memo : undefined });
  },
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
