import { asRow, asText, rows } from '../workspace/types.ts';
import type { Note, Row, Workspace, WorkspaceServices } from '../workspace/types.ts';
import { engineCall, engineRefusal, readWorkspace, SidecarCommandError } from '../workspace/engine.ts';
import { cleanTags, commentId, read, write } from './commands.ts';
import type { Fields, NewDoc } from './commands.ts';
import { SelfWrites } from './selfWrites.ts';
import { WriteQueues } from './writeQueues.ts';
import type { WriteStamp } from './writeQueues.ts';
import { readJSON, writeJSON, writeSampleTemplates } from '../workspace/storage.ts';
import { extractTitle } from './title.ts';
import { withoutTitleMismatch } from './titleWarning.ts';

export interface NoteRef { id: string; parent?: string; }
export interface FailureLine { text: string; key?: string; kind: 'failed' | 'conflict' | 'reloaded'; }
export interface CommentState { state: string; comments: Row[]; reason?: string; }
export interface VaultMemory {
  subNotes: Record<string, Note[]>; comments: Record<string, CommentState>; facts: Record<string, Row>;
  editing: string[]; unsaved: string[]; conflicted: string[]; buffers: Record<string, string>;
  failures: FailureLine[]; relations: { from: string; to: string; kind: string }[];
}
export interface WriteResult { ok: boolean; conflict?: boolean; reloaded?: boolean; reason?: string; value?: Row; }
const keyOf = (ref: NoteRef) => ref.parent ? `${ref.parent}:${ref.id}` : ref.id;
const empty = (): VaultMemory => ({ subNotes: {}, comments: {}, facts: {}, editing: [], unsaved: [], conflicted: [], buffers: {}, failures: [], relations: [] });
const has = (value: object, key: string) => Object.prototype.hasOwnProperty.call(value, key);
const folder = (note?: Note) => !!note && !!asText(note.path) && !/\.(md|json|html)$/i.test(asText(note.path));
const heading = extractTitle;
// Match the engine's card-id whitespace set; FEFF is deliberately excluded.
const trimCardId = (value: string) => value.replace(/^[\u0009-\u000d\u001c-\u0020\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]+|[\u0009-\u000d\u001c-\u0020\u00a0\u1680\u2000-\u200a\u2028\u2029\u202f\u205f\u3000]+$/g, '');
export function relations(notes: Note[]): VaultMemory['relations'] {
  const result: VaultMemory['relations'] = [], seen = new Set<string>();
  for (const note of notes) for (const link of rows(note.links)) {
    const to = asText(link.to), kind = asText(link.kind) || 'direct', key = JSON.stringify([note.id, to, kind]);
    if (to && to !== note.id && !seen.has(key)) { seen.add(key); result.push({ from: note.id, to, kind }); }
  }
  return result;
}

export class VaultData {
  private memory = empty();
  private generation = 0;
  private queues = new WriteQueues();
  private reads = new Map<string, number>();
  private visibleParents = new Map<string, number>();
  private reconciliation?: Promise<void>;
  private requested = false;
  private reports = new Map<string, ReturnType<typeof setTimeout>>();
  private retries = new Map<string, ReturnType<typeof setTimeout>>();
  private savedTitles = new Map<string, string>();
  private unsavedTemplates = new Map<string, Row>();
  readonly selfWrites: SelfWrites;
  private services: WorkspaceServices;
  private workspace: () => Workspace | null;
  private changed: (memory: VaultMemory, workspace?: Workspace) => void;
  constructor(services: WorkspaceServices, workspace: () => Workspace | null,
    changed: (memory: VaultMemory, workspace?: Workspace) => void) {
    this.services = services; this.workspace = workspace; this.changed = changed;
    this.selfWrites = new SelfWrites(() => services.now().getTime());
  }
  getSnapshot = () => this.memory;
  epoch() { return this.generation; }
  mirrorConflict(ref: NoteRef, body: string) {
    this.setFlag('editing', keyOf(ref), true); this.conflict(ref, body); this.emit();
  }
  reset(workspace?: Workspace): VaultMemory {
    this.generation++; this.requested = false;
    for (const timer of this.reports.values()) clearTimeout(timer);
    for (const timer of this.retries.values()) clearTimeout(timer);
    this.retries.clear();
    this.savedTitles = new Map((workspace?.notes || []).map(note => [note.id, note.title]));
    this.reports.clear(); this.queues = new WriteQueues(); this.reads.clear(); this.visibleParents.clear(); this.selfWrites.clear(); this.unsavedTemplates.clear();
    this.memory = { ...empty(), relations: relations(workspace?.notes || []) };
    return this.memory;
  }
  dispose() { this.reset(); }
  invalidate() { this.generation++; this.requested = false; for (const timer of this.retries.values()) clearTimeout(timer); this.retries.clear(); }
  idle() { return this.reconciliation || Promise.resolve(); }
  unsavedForRoot(id: string) {
    return [...new Set([...this.memory.unsaved, ...this.memory.conflicted, ...Object.keys(this.memory.buffers)])]
      .filter(key => { const [parent, child] = key.split(':'); return child ? this.chain(parent)?.split('/')[0] === id : parent === id; });
  }
  private emit(workspace?: Workspace) {
    this.memory = { ...this.memory, ...(workspace ? { relations: relations(workspace.notes) } : {}) };
    this.changed(this.memory, workspace);
  }
  private valid(generation: number) { return generation === this.generation && !!this.workspace(); }
  private stamp(key: string) { const next = (this.reads.get(key) || 0) + 1; this.reads.set(key, next); return next; }
  private setFlag(name: 'editing' | 'unsaved' | 'conflicted', key: string, enabled: boolean) {
    this.memory = { ...this.memory, [name]: enabled ? [...new Set([...this.memory[name], key])] : this.memory[name].filter(id => id !== key) };
  }
  note(ref: NoteRef): Note | undefined { return (ref.parent ? this.memory.subNotes[ref.parent] : this.workspace()?.notes)?.find(note => note.id === ref.id); }
  private replaceNote(ref: NoteRef, fields: Row) {
    if (ref.parent) {
      this.stamp('sub:' + ref.parent);
      this.memory = { ...this.memory, subNotes: { ...this.memory.subNotes, [ref.parent]: (this.memory.subNotes[ref.parent] || []).map(note => note.id === ref.id ? { ...note, ...fields } : note) } };
      this.emit();
    } else {
      const workspace = this.workspace();
      if (workspace) this.emit({ ...workspace, notes: workspace.notes.map(note => note.id === ref.id ? { ...note, ...fields } : note) });
    }
  }
  protect(ref: NoteRef, editing: boolean, buffer?: string) {
    const key = keyOf(ref); this.setFlag('editing', key, editing);
    if (!editing) this.setFlag('unsaved', key, false);
    if (!editing && !ref.parent) this.clearConflict(ref);
    if (buffer !== undefined && this.memory.conflicted.includes(key)) this.memory = { ...this.memory, buffers: { ...this.memory.buffers, [key]: buffer } };
    this.emit();
  }
  edit(ref: NoteRef, fields: Row) { this.protect(ref, true, typeof fields.body === 'string' ? fields.body : undefined); this.replaceNote(ref, fields); }
  clearConflict(ref: NoteRef) {
    const key = keyOf(ref), buffers = { ...this.memory.buffers }; delete buffers[key];
    this.setFlag('conflicted', key, false); this.memory = { ...this.memory, buffers, failures: this.memory.failures.filter(line => !(line.key === key && line.kind === 'conflict')) }; this.emit();
  }
  stashConflict(ref: NoteRef, body: string, typed: boolean) {
    const key = keyOf(ref);
    if (!this.memory.conflicted.includes(key) || !typed && key in this.memory.buffers || this.memory.buffers[key] === body) return;
    this.memory = { ...this.memory, buffers: { ...this.memory.buffers, [key]: body } }; this.emit();
  }
  dismissFailures() { this.memory = { ...this.memory, failures: [] }; this.emit(); }
  async diskBody(ref: NoteRef): Promise<string> {
    const chain = ref.parent ? this.chain(ref.parent) : null;
    if (ref.parent && !chain) throw new Error('부모 사슬이 없어요');
    return asText(asRow(await engineCall(this.services.host, ref.parent ? read.subNote(chain!, ref.id) : read.doc(ref.id)))?.body);
  }
  async refreshToken(ref: NoteRef): Promise<void> {
    const generation = this.generation;
    const receive = async () => {
      const chain = ref.parent ? this.chain(ref.parent) : null;
      if (ref.parent && !chain) return;
      try {
        const row = asRow(await engineCall(this.services.host, ref.parent ? read.subNote(chain!, ref.id) : read.doc(ref.id)));
        if (row && has(row, 'precondition') && this.valid(generation)) this.replaceNote(ref, { precondition: row.precondition });
      } catch { /* The user's forced write retains its old token if refresh fails. */ }
    };
    if (ref.parent) await receive(); else await this.queues.run(keyOf(ref), receive, false);
  }
  async saveBody(ref: NoteRef, body: string, force = false): Promise<WriteResult> {
    const note = this.note(ref), key = keyOf(ref);
    if (!note) return { ok: true };
    if (this.memory.conflicted.includes(key)) return { ok: false, conflict: true };
    if (!force && note.body === body && !this.memory.unsaved.includes(key)) return { ok: true };
    if (ref.parent && !this.chain(ref.parent)) { this.replaceNote(ref, { body }); return { ok: true }; }
    return this.updateNote(ref, { body });
  }
  retryBody(ref: NoteRef) {
    // Compatibility: sub-note retry identity deliberately omits the parent.
    if (this.retries.has(ref.id)) return;
    const generation = this.generation;
    const run = async () => {
      this.retries.delete(ref.id);
      const note = this.note(ref);
      if (!this.valid(generation) || !note || this.memory.conflicted.includes(keyOf(ref))) return;
      const body = note.body, result = await this.saveBody(ref, body);
      if (!this.valid(generation)) return;
      if (result.ok) { if (this.note(ref)?.body === body) this.protect(ref, false); }
      else if (!result.conflict) this.retryBody(ref);
    };
    this.retries.set(ref.id, setTimeout(() => { void run(); }, 4000));
  }
  private conflict(ref: NoteRef, body?: string) {
    const key = keyOf(ref); this.setFlag('conflicted', key, true);
    if (body !== undefined || this.memory.editing.includes(key) && !(key in this.memory.buffers)) this.memory = { ...this.memory, buffers: { ...this.memory.buffers, [key]: body ?? this.note(ref)?.body ?? '' } };
  }
  private failure(text: string, key?: string, kind: FailureLine['kind'] = 'failed') {
    this.memory = { ...this.memory, failures: [...this.memory.failures.filter(line => line.text !== text), { text, key, kind }].slice(-10) }; this.emit();
  }
  private success() {
    this.memory = { ...this.memory, failures: this.memory.failures.filter(line => line.kind === 'conflict' && line.key && this.memory.conflicted.includes(line.key)) }; this.emit();
  }
  markOwn(relative: string) {
    const root = this.workspace()?.path;
    if (!root || !relative) return;
    this.selfWrites.record(root, relative);
    if (this.services.native !== false) void this.services.host.invoke('announce_self_write', { root, relPath: relative }).catch(() => {});
  }
  receiveOwn = (event: { root: string; relPath: string; from: string }) => {
    if (event.from === this.services.windowLabel || event.root !== this.workspace()?.path) return;
    this.selfWrites.record(event.root, event.relPath);
  };

  // A chain is derived from loaded parent lists, never guessed from a path.
  chain(parent: string): string | null {
    const ids: string[] = [], visited = new Set<string>();
    let id = parent;
    for (let depth = 0; depth <= 20; depth++) {
      if (visited.has(id)) return null;
      visited.add(id); ids.unshift(id);
      if (this.workspace()?.notes.some(note => note.id === id)) return ids.join('/');
      const owner = Object.entries(this.memory.subNotes).find(([, notes]) => notes.some(note => note.id === id));
      if (!owner) return null;
      id = owner[0];
    }
    return null;
  }
  private refFor(id: string): NoteRef | null {
    if (this.workspace()?.notes.some(note => note.id === id)) return { id };
    const parent = Object.entries(this.memory.subNotes).find(([, notes]) => notes.some(note => note.id === id))?.[0];
    return parent ? { id, parent } : null;
  }
  private root(ref: NoteRef): string | null { return ref.parent ? this.chain(ref.parent)?.split('/')[0] || null : ref.id; }
  retainSubNotes(parent: string): () => void {
    const generation = this.generation;
    this.visibleParents.set(parent, (this.visibleParents.get(parent) || 0) + 1);
    return () => { if (generation !== this.generation) return; const count = (this.visibleParents.get(parent) || 1) - 1; if (count) this.visibleParents.set(parent, count); else this.visibleParents.delete(parent); };
  }
  async loadSubNotes(parent: string): Promise<Note[]> {
    const generation = this.generation, chain = this.chain(parent), ref = this.refFor(parent), workspace = this.workspace();
    const ticket = this.stamp('sub:' + parent), snapshot = this.queues.snapshot();
    if (!workspace || !ref || !chain || !folder(this.note(ref))) {
      this.memory = { ...this.memory, subNotes: { ...this.memory.subNotes, [parent]: [] } }; this.emit(); return [];
    }
    if (workspace.sample) return this.memory.subNotes[parent] || [];
    try {
      const value = await engineCall(this.services.host, read.subNotes(chain));
      if (!this.valid(generation) || this.reads.get('sub:' + parent) !== ticket) return this.memory.subNotes[parent] || [];
      const root = workspace.notes.find(note => note.id === chain.split('/')[0])!;
      const disk = rows(value, 'subNotes').map(row => this.toNote({ ...row, agenda: root.agenda }, root.agendaId));
      const merged = this.mergeNotes(disk, this.memory.subNotes[parent] || [], snapshot, parent).sort((a, b) => a.created < b.created ? -1 : a.created > b.created ? 1 : 0);
      this.memory = { ...this.memory, subNotes: { ...this.memory.subNotes, [parent]: merged } }; this.prune(); this.emit(); return merged;
    } catch (error) { this.services.warn('후속 노트를 읽지 못했어요', error); return this.memory.subNotes[parent] || []; }
  }
  async loadComments(ref: NoteRef): Promise<CommentState | undefined> {
    const key = keyOf(ref), root = this.root(ref), generation = this.generation, ticket = this.stamp('comments:' + key);
    if (!this.workspace() || !root || !folder(this.note(ref))) return undefined;
    if (this.workspace()?.sample) return this.memory.comments[key];
    try {
      const value = asRow(await engineCall(this.services.host, read.comments(root, ref.parent ? ref.id : undefined)));
      if (this.valid(generation) && this.reads.get('comments:' + key) === ticket) {
        const item = { state: asText(value?.state), comments: rows(value?.comments), reason: asText(value?.reason) };
        this.memory = { ...this.memory, comments: { ...this.memory.comments, [key]: item } }; this.emit();
        if (item.state === 'unreadable') this.services.warn('코멘트 파일을 읽지 못했어요', item.reason);
      }
    } catch (error) { this.services.warn('코멘트를 읽지 못했어요', error); }
    return this.memory.comments[key];
  }
  async loadFacts(id: string): Promise<Row | undefined> {
    const generation = this.generation, ticket = this.stamp('facts:' + id);
    if (!this.workspace() || this.workspace()?.sample) return this.memory.facts[id];
    try {
      const value = asRow(await engineCall(this.services.host, read.facts(id)));
      if (value && this.valid(generation) && this.reads.get('facts:' + id) === ticket) { this.memory = { ...this.memory, facts: { ...this.memory.facts, [id]: value } }; this.emit(); return value; }
    } catch (error) { this.services.warn('사실층을 읽지 못했어요', error); }
    return undefined;
  }
  private toNote(row: Row, agendaId: string): Note {
    const created = asText(row.created);
    return { ...row, id: asText(row.id), title: asText(row.title), body: asText(row.body), agenda: asText(row.agenda), agendaId,
      created, date: created.slice(0, 10), time: /T((?:[01]\d|2[0-3]):[0-5]\d)/.exec(created)?.[1] || '00:00', format: asText(row.format) || 'markdown' };
  }
  private mergeNotes(disk: Note[], memory: Note[], snapshot: WriteStamp, parent?: string): Note[] {
    const merged = disk.flatMap(note => {
      const ref = { id: note.id, parent }, key = keyOf(ref), old = memory.find(item => item.id === note.id);
      // C175: a note trashed while this read was in flight is not revived.
      if (!old && this.queues.overlapped(key, snapshot)) return [];
      if (!old) { this.setFlag('unsaved', key, false); if (parent) this.setFlag('editing', key, false); else this.savedTitles.set(note.id, note.title); return [note]; }
      if (this.queues.overlapped(key, snapshot)) return [old];
      if (this.memory.editing.includes(key) || this.memory.unsaved.includes(key)) {
        if (old.body !== note.body && (!asText(old.precondition) || !asText(note.precondition) || old.precondition !== note.precondition)) this.conflict(ref);
        // C45: protected text stays in memory, but the folder always follows the disk.
        return [{ ...old, path: note.path }];
      }
      if (!parent) this.savedTitles.set(note.id, note.title);
      return [note];
    });
    for (const old of memory) if (!disk.some(note => note.id === old.id)) {
      if (!parent && (this.memory.editing.includes(old.id) || this.queues.overlapped(old.id, snapshot))) merged.push(old);
      else if (parent) this.setFlag('editing', keyOf({ id: old.id, parent }), false);
    }
    return merged;
  }
  private prune(notes = this.workspace()?.notes || []) {
    const live = new Set(notes.map(note => note.id)), subNotes = { ...this.memory.subNotes };
    for (let depth = 0; depth < 21; depth++) for (const [parent, notes] of Object.entries(subNotes)) if (live.has(parent)) notes.forEach(note => live.add(note.id));
    for (const parent of Object.keys(subNotes)) if (!live.has(parent)) delete subNotes[parent];
    const aliveKey = (key: string) => key.includes(':') ? key.split(':').every(id => live.has(id)) : live.has(key);
    const comments = Object.fromEntries(Object.entries(this.memory.comments).filter(([key]) => aliveKey(key)));
    for (const key of this.reads.keys()) {
      if (key.startsWith('sub:') && !live.has(key.slice(4)) || key.startsWith('comments:') && !aliveKey(key.slice(9))) this.reads.delete(key);
    }
    this.memory = { ...this.memory, subNotes, comments };
  }
  reconcile = (): Promise<void> => {
    if (!this.workspace() || this.workspace()?.sample) return Promise.resolve();
    this.requested = true;
    if (this.reconciliation) return this.reconciliation;
    const run = async () => {
      while (this.requested) {
        this.requested = false;
        const generation = this.generation, workspace = this.workspace(), snapshot = this.queues.snapshot();
        if (!workspace || workspace.sample) break;
        const data = await readWorkspace(this.services.host, true, workspace, this.services.sanitizeHTML,
          () => this.valid(generation) && this.workspace()?.path === workspace.path);
        if (!this.valid(generation) || this.workspace()?.path !== workspace.path || data.noteReadFailed) continue;
        const current = this.workspace()!;
        const notes = this.mergeNotes(data.notes, current.notes, snapshot);
        // C95: a template whose save failed keeps its typed values over the disk copy.
        const templates = data.templates.map(row => ({ ...row, ...this.unsavedTemplates.get(asText(row.id)) }));
        this.prune(notes); this.emit({ ...current, ...data, notes, templates });
        const parents = Object.keys(this.memory.subNotes).filter(parent => this.visibleParents.has(parent) || (this.memory.subNotes[parent] || [])
          .some(note => ['editing', 'unsaved', 'conflicted'].some(flag => this.memory[flag as 'editing'].includes(keyOf({ id: note.id, parent })))));
        await Promise.allSettled(parents.map(parent => this.loadSubNotes(parent)));
      }
    };
    this.reconciliation = run().finally(() => { this.reconciliation = undefined; });
    return this.reconciliation;
  };

  async waitForWrites(): Promise<string[]> {
    await this.queues.drain();
    return this.unsavedKeys();
  }
  unsavedKeys() { return [...new Set([...this.memory.unsaved, ...this.memory.conflicted, ...Object.keys(this.memory.buffers)])]; }
  async settled() { await this.queues.drain(); return !this.queues.pending(); }
  private async mutate(ref: NoteRef, fields: Row, build: (note: Note) => string[] | null,
    label: string, bodyWrite: boolean, suppressReloadLine = false): Promise<WriteResult> {
    const original = this.note(ref), workspace = this.workspace(), generation = this.generation, key = keyOf(ref);
    if (!original || !workspace) return { ok: false };
    if (!ref.parent && !this.savedTitles.has(key)) this.savedTitles.set(key, original.title);
    this.replaceNote(ref, fields);
    if (workspace.sample) return { ok: true };
    let reload = false;
    const result = await this.queues.run(key, async (): Promise<WriteResult> => {
      if (!this.valid(generation)) return { ok: false };
      const note = this.note(ref); if (!note) return { ok: false };
      const argv = build(note); if (!argv) return { ok: false };
      try {
        const response = asRow(await engineCall(this.services.host, argv)) || {};
        if (!this.valid(generation)) return { ok: false };
        const update: Row = {};
        if (has(response, 'precondition')) update.precondition = response.precondition;
        if (has(response, 'path')) { update.path = response.path; this.markOwn(asText(response.path)); }
        if (response.renamed === true || ref.parent && asText(response.path) && response.path !== note.path) this.markOwn(asText(note.path));
        if (has(response, 'evaluation')) update.evaluation = response.evaluation;
        const titleCell = !ref.parent && argv.find(cell => cell.startsWith('title='));
        if (titleCell) this.savedTitles.set(key, titleCell.slice(6));
        this.replaceNote(ref, update);
        // 10.4: the unsaved mark comes down only when a write that carried the body succeeds.
        // A sub-note's title-only write succeeding leaves it as it was.
        if (has(fields, 'body')) this.setFlag('unsaved', key, false);
        this.success();
        const warning = argv.some(cell => cell.startsWith('title=')) ? withoutTitleMismatch(asText(response.warning)) : asText(response.warning);
        if (response.renamed === true && warning) this.failure(warning);
        return { ok: true, value: response };
      } catch (error) {
        if (!this.valid(generation)) return { ok: false };
        const clash = error instanceof SidecarCommandError && error.status === 5;
        this.services.warn(label, error);
        if (bodyWrite) {
          this.setFlag('unsaved', key, true);
          if (clash) this.conflict(ref, this.note(ref)?.body);
        } else {
          const rollback: Row = {};
          for (const field of Object.keys(fields)) if (this.note(ref)?.[field] === fields[field]) rollback[field] = !ref.parent && field === 'title' ? this.savedTitles.get(key) ?? original.title : original[field];
          this.replaceNote(ref, rollback);
          if (clash && this.memory.unsaved.includes(key)) this.conflict(ref);
          else if (clash && !this.memory.editing.includes(key)) reload = true;
        }
        if (!(reload && suppressReloadLine)) this.failure(clash && !reload ? `다른 쪽이 이 노트를 먼저 고쳤어요. 저장을 멈췄습니다: ${original.title || '제목 없음'}` : label, key, clash && !reload ? 'conflict' : 'failed');
        return { ok: false, conflict: clash };
      }
    });
    if (reload && this.valid(generation)) {
      if (ref.parent) await this.loadSubNotes(ref.parent); else await this.reconcile();
      if (this.valid(generation) && !suppressReloadLine) this.failure(`다른 쪽이 이 노트를 먼저 고쳐서 되돌렸어요. 디스크 글을 다시 읽었어요: ${original.title || '제목 없음'}`, key, 'reloaded');
      return { ...result, reloaded: true };
    }
    return result;
  }
  async updateNote(ref: NoteRef, input: Fields, retryTitle = true): Promise<WriteResult> {
    const fields: Fields = { ...input };
    if (has(fields, 'tags')) fields.tags = cleanTags(fields.tags);
    if (has(fields, 'mins')) fields.mins = Math.max(0, Math.round(Number(fields.mins) || 0));
    const bodyWrite = has(fields, 'body') || !!ref.parent && has(fields, 'title');
    const title = this.note(ref)?.title || '제목 없음', beforeHeading = heading(this.note(ref)?.body || '');
    const existing = this.note(ref);
    const memoryFields = has(fields, 'time') && existing ? { ...fields, created: `${existing.date}T${asText(fields.time)}${existing.created.slice(16) || ':00'}` } : fields;
    const result = await this.mutate(ref, memoryFields, note => {
      const values = Object.fromEntries(Object.keys(fields).map(field => [field, note[field]])) as Fields;
      if (ref.parent && has(values, 'body')) values.title = note.title;
      const token = asText(note.precondition);
      if (!ref.parent) return write.updateDoc(ref.id, values, token);
      const chain = this.chain(ref.parent); return chain ? write.updateSub(chain, ref.id, values, token) : null;
    }, `${ref.parent ? '후속 노트 저장' : has(fields, 'body') ? '자동 저장' : has(fields, 'time') ? '시각 변경' : '문서 저장'}: ${title}`, bodyWrite, retryTitle && !ref.parent && Object.keys(fields).join() === 'title');
    if (retryTitle && !ref.parent && result.reloaded && !this.memory.editing.includes(keyOf(ref)) && Object.keys(fields).join() === 'title' && heading(this.note(ref)?.body || '') === beforeHeading) return this.updateNote(ref, fields, false);
    return result;
  }
  favorite(id: string, value: boolean | string) {
    const fields = typeof value === 'boolean' ? { favorite: value } : { favoriteCategory: value.trim() };
    return this.mutate({ id }, fields, note => write.favorite(id, typeof value === 'boolean' ? { favorite: note.favorite === true } : { category: asText(note.favoriteCategory) }, asText(note.precondition)), `문서 저장: ${this.note({ id })?.title || '제목 없음'}`, false);
  }
  evaluate(id: string, score: number | null, memo = '') {
    const evaluation = score === null ? null : { score: Math.max(0, Math.min(100, Math.round(score))), note: memo.trim() };
    return this.mutate({ id }, { evaluation }, note => write.evaluate(id, note.evaluation === null ? { clear: true } : {
      score: Number(asRow(note.evaluation)?.score), note: asText(asRow(note.evaluation)?.note),
    }, asText(note.precondition)), `문서 저장: ${this.note({ id })?.title || '제목 없음'}`, false);
  }
  link(from: string, to: string, kind = 'direct', remove = false): Promise<WriteResult> {
    const note = this.note({ id: from }), links = rows(note?.links), exists = links.some(link => link.to === to && link.kind === kind);
    if (!note || from === to || exists && !remove || !exists && remove) return Promise.resolve({ ok: true });
    return this.mutate({ id: from }, { links: remove ? links.filter(link => link.to !== to || link.kind !== kind) : [...links, { to, kind }] },
      current => write.link(from, to, kind, remove, asText(current.precondition)), `문서 저장: ${note.title || '제목 없음'}`, false);
  }

  private async perform(argv: string[], label: string, accepted: (value: Row) => void): Promise<WriteResult> {
    const generation = this.generation, workspace = this.workspace();
    if (!workspace) return { ok: false };
    try {
      const value = workspace.sample ? {} : asRow(await engineCall(this.services.host, argv)) || {};
      if (!this.valid(generation)) return { ok: false };
      accepted(value); this.success(); return { ok: true, value };
    } catch (error) { if (this.valid(generation)) { this.services.warn(label, error); this.failure(label); } return { ok: false }; }
  }
  async createDoc(input: Omit<NewDoc, 'agenda'> & { agendaId: string }): Promise<WriteResult> {
    const workspace = this.workspace(); if (!workspace) return { ok: false };
    const agenda = workspace.agendas.find(item => item.id === input.agendaId)?.slug || '';
    const id = `nd${this.services.now().getTime().toString(36)}-${this.services.uuid().slice(0, 8)}`;
    const doc = { ...input, agenda, mins: Math.max(0, Math.round(input.mins)), tags: cleanTags(input.tags) };
    const note = this.toNote({ ...doc, id, created: `${doc.date}T${doc.time}:00`, format: 'markdown' }, input.agendaId);
    // C32: the note enters memory only after the engine accepted it, so a
    // refusal leaves no local-only note behind and the caller sees the failure.
    const result = await this.perform(write.createDoc(doc), `문서 저장: ${doc.title || '제목 없음'}`, response => {
      const current = this.workspace()!, actualId = asText(response.id) || id;
      this.queues.advance(actualId);
      const created = { ...note, id: actualId,
        ...(has(response, 'path') ? { path: response.path } : {}), ...(has(response, 'precondition') ? { precondition: response.precondition } : {}) };
      this.emit({ ...current, notes: [...current.notes.filter(item => item.id !== actualId), created] });
      this.markOwn(asText(response.path));
    });
    if (!result.ok) return result;
    if (result.value?.agendaCreated === true || asText(result.value?.agenda) && result.value?.agenda !== agenda) this.failure(`아젠다가 달라졌어요: ${doc.title || '제목 없음'}`);
    return { ...result, value: { ...result.value, id: asText(result.value?.id) || id } };
  }
  async createSubNote(parent: string): Promise<WriteResult> {
    const chain = this.chain(parent), ref = this.refFor(parent);
    if (!chain || !ref || !folder(this.note(ref))) return { ok: false };
    return this.perform(write.createSub(chain), '후속 노트 저장: 새 후속 노트', response => {
      const id = asText(response.id) || `sn${this.services.now().getTime().toString(36)}-${this.services.uuid().slice(0, 8)}`;
      const root = this.workspace()!.notes.find(note => note.id === chain.split('/')[0])!;
      const note = this.toNote({ ...response, id, title: asText(response.title) || '새 후속 노트', body: '# 새 후속 노트\n\n', agenda: root.agenda,
        created: this.services.now().toISOString() }, root.agendaId);
      this.stamp('sub:' + parent);
      this.memory = { ...this.memory, subNotes: { ...this.memory.subNotes, [parent]: [...(this.memory.subNotes[parent] || []), note] } };
      this.markOwn(asText(response.path)); this.emit();
    });
  }
  private acceptDerived(ref: NoteRef, before: unknown, after: unknown, strip: (body: string) => string) {
    const note = this.note(ref);
    if (!note || !asText(before) || note.precondition !== before || !asText(after)) return;
    this.queues.advance(keyOf(ref));
    this.replaceNote(ref, { precondition: after, body: strip(note.body) });
  }
  async deleteSubNote(parent: string, id: string): Promise<WriteResult> {
    const chain = this.chain(parent), ref = this.refFor(parent), generation = this.generation;
    const old = this.memory.subNotes[parent] || [], note = old.find(item => item.id === id);
    if (!chain || !ref || !note) return { ok: false };
    this.stamp('sub:' + parent);
    this.memory = { ...this.memory, subNotes: { ...this.memory.subNotes, [parent]: old.filter(item => item.id !== id) } }; this.emit();
    const result = await this.queues.run(keyOf(ref), () => !this.valid(generation) ? Promise.resolve({ ok: false }) : this.perform(write.deleteSub(chain, id), `후속 노트 삭제: ${note.title || '제목 없음'}`, response => {
      this.markOwn(asText(response.path));
      if (response.cardRemoved === true) {
        this.markOwn(asText(this.note(ref)?.path));
        this.acceptDerived(ref, response.containerBefore, response.containerAfter, body => body.split('\n').filter(line => {
          const match = /^\[[^\]]+\]\(#sub:([^)]+)\)[ \t\v\f\r]*$/.exec(line); return !match || trimCardId(match[1]) !== id;
        }).join('\n'));
      }
      this.setFlag('editing', keyOf({ parent, id }), false); this.setFlag('unsaved', keyOf({ parent, id }), false); this.emit();
    }), false);
    if (!result.ok && this.valid(generation)) {
      this.stamp('sub:' + parent);
      const current = this.memory.subNotes[parent] || [], index = old.indexOf(note);
      if (!current.some(item => item.id === id)) this.memory = { ...this.memory, subNotes: { ...this.memory.subNotes, [parent]: [...current.slice(0, index), note, ...current.slice(index)] } };
      this.emit();
    }
    return result;
  }
  async comment(ref: NoteRef, action: 'create' | 'update' | 'delete', body = '', id?: string, quote?: string): Promise<WriteResult> {
    const key = keyOf(ref), root = this.root(ref), state = this.memory.comments[key], generation = this.generation;
    if (!root || !folder(this.note(ref)) || state?.state === 'unreadable' || action !== 'delete' && !body.trim()) return { ok: false };
    const cid = id || commentId(this.services.now().getTime(), this.services.uuid()), before = state?.comments || [];
    const comments = action === 'create' ? [...before, { id: cid, body: body.trim(), createdAt: this.services.now().toISOString(), ...(quote ? { anchor: { quote } } : {}) }]
      : action === 'update' ? before.map(item => item.id === cid ? { ...item, body: body.trim(), editedAt: this.services.now().toISOString() } : item) : before.filter(item => item.id !== cid);
    this.stamp('comments:' + key);
    this.memory = { ...this.memory, comments: { ...this.memory.comments, [key]: { state: 'loaded', comments } } }; this.emit();
    const work = (): Promise<WriteResult> => !this.valid(generation) ? Promise.resolve({ ok: false }) : this.perform(action === 'create' ? write.createComment(root, ref.parent ? ref.id : undefined, cid, body.trim(), quote)
      : action === 'update' ? write.updateComment(root, ref.parent ? ref.id : undefined, cid, body.trim()) : write.deleteComment(root, ref.parent ? ref.id : undefined, cid), '코멘트 저장', response => {
      this.markOwn(asText(response.path));
      if (action === 'delete' && response.markerStripped === true) {
        this.markOwn(asText(this.note(ref)?.path));
        this.acceptDerived(ref, response.bodyBefore, response.bodyAfter, text => text.replace(/<!--\/?aisc:(cm[a-z0-9]+-[a-z0-9]{1,8})-->/gi, (marker, found: string) => found === cid ? '' : marker));
      }
    });
    const result = await this.queues.run('comments:' + key, () => action === 'delete' ? this.queues.run(key, work, false) : work());
    if (!result.ok && action === 'delete' && this.valid(generation)) {
      this.stamp('comments:' + key);
      const current = this.memory.comments[key]?.comments || [], removed = before.find(item => item.id === cid), index = before.findIndex(item => item.id === cid);
      const restored = removed && !current.some(item => item.id === cid) ? [...current.slice(0, index), removed, ...current.slice(index)] : current;
      this.memory = { ...this.memory, comments: { ...this.memory.comments, [key]: { state: 'loaded', comments: restored } } }; this.emit();
    }
    return { ...result, value: { ...result.value, id: cid } };
  }
  async saveFacts(id: string, facts: string, startedWith: string): Promise<WriteResult> {
    if (!this.workspace()) return { ok: false };
    if (!this.workspace()?.sample) {
      const disk = await this.loadFacts(id);
      if (!disk) { this.failure(`사실층 저장: ${this.workspace()?.agendas.find(item => item.id === id)?.title || id}`); return { ok: false }; }
      if (asText(disk.facts) !== startedWith || disk.tooLargeToEdit === true) return { ok: false, conflict: true, value: disk };
    }
    return this.perform(write.facts(id, facts), `사실층 저장: ${this.workspace()?.agendas.find(item => item.id === id)?.title || id}`, response => {
      this.stamp('facts:' + id); this.memory = { ...this.memory, facts: { ...this.memory.facts, [id]: { ...response, facts: facts.trim() } } };
      this.markOwn(asText(response.path)); this.emit();
    });
  }
  saveReport(id: string, body: string, memo: string) {
    const workspace = this.workspace(); if (!workspace) return;
    const report = { id, body, memo }, existing = workspace.reports.some(item => item.id === id);
    this.emit({ ...workspace, reports: existing ? workspace.reports.map(item => item.id === id ? report : item) : [...workspace.reports, report] });
    if (workspace.sample) {
      for (const [key, value] of [['aisurada.reportEdits.v1', body], ['aisurada.reportMemos.v1', memo]]) {
        writeJSON(this.services.storage, key, { ...asRow(readJSON(this.services.storage, key)), [id]: value });
      }
      return;
    }
    clearTimeout(this.reports.get(id));
    this.reports.set(id, setTimeout(() => { void this.sendReport(id); }, 400));
  }
  private async sendReport(id: string): Promise<WriteResult> {
    clearTimeout(this.reports.get(id)); this.reports.delete(id);
    const generation = this.generation;
    return this.queues.run('report:' + id, async () => {
      if (!this.valid(generation)) return { ok: false };
      const report = this.workspace()?.reports.find(item => item.id === id); if (!report) return { ok: false };
      let argv: string[];
      try { argv = write.report(id, asText(report.body), asText(report.memo)); }
      catch (error) { this.services.warn('리포트 저장', error); this.failure(`리포트 저장: ${id}`); return { ok: false }; }
      return this.perform(argv, `리포트 저장: ${id}`, response => this.markOwn(asText(response.path) || `.aisurada/reports/${id}.json`));
    });
  }
  async flushReports() { await Promise.allSettled([...this.reports.keys()].map(id => this.sendReport(id))); }

  async deleteDoc(id: string): Promise<WriteResult> {
    const workspace = this.workspace(), note = this.note({ id }), generation = this.generation;
    if (!workspace || !note) return { ok: false };
    const index = workspace.notes.indexOf(note);
    this.emit({ ...workspace, notes: workspace.notes.filter(item => item.id !== id) });
    const result = await this.queues.run(id, () => !this.valid(generation) ? Promise.resolve({ ok: false }) : this.perform(write.deleteDoc(id), `휴지통 이동: ${note.title || '제목 없음'}`, response => {
      this.markOwn(asText(note.path)); this.markOwn(`.aisurada/trash/files/${asText(response.file)}`); this.markOwn('.aisurada/trash/trash.json');
      this.setFlag('editing', id, false); this.setFlag('unsaved', id, false); this.clearConflict({ id });
      const current = this.workspace()!;
      this.emit({ ...current, trash: [...current.trash, { ...note, ...response, docId: id }] });
    }));
    if (!result.ok && this.valid(generation)) {
      const current = this.workspace()!;
      if (!current.notes.some(item => item.id === id)) this.emit({ ...current, notes: [...current.notes.slice(0, index), note, ...current.notes.slice(index)] });
    }
    return result;
  }
  async trashAction(id: string, action: 'restore' | 'purge'): Promise<WriteResult> {
    const workspace = this.workspace(), generation = this.generation, item = workspace?.trash.find(row => row.trashId === id);
    if (!workspace || !item) return { ok: false };
    const index = workspace.trash.indexOf(item);
    this.emit({ ...workspace, trash: workspace.trash.filter(row => row.trashId !== id) });
    const result = await this.perform(action === 'restore' ? write.restore(id) : write.purge(id), `${action === 'restore' ? '복구' : '완전 삭제'}: ${asText(item.title) || '제목 없음'}`, () => {
      if (action === 'purge') { this.markOwn(`.aisurada/trash/files/${asText(item.file)}`); this.markOwn('.aisurada/trash/trash.json'); }
    });
    if (!this.valid(generation)) return result;
    if (!result.ok) { const current = this.workspace()!; this.emit({ ...current, trash: [...current.trash.slice(0, index), item, ...current.trash.slice(index)] }); }
    else if (action === 'restore') await this.reconcile();
    return result;
  }
  async emptyTrash(): Promise<WriteResult> {
    const workspace = this.workspace(), generation = this.generation; if (!workspace) return { ok: false };
    const original = workspace.trash; this.emit({ ...workspace, trash: [] });
    const result = await this.perform(write.emptyTrash(), '휴지통 비우기', response => {
      const failed = new Set(rows(response.failed).map(row => row.trashId));
      for (const item of original) if (!failed.has(item.trashId)) this.markOwn(`.aisurada/trash/files/${asText(item.file)}`);
      this.markOwn('.aisurada/trash/trash.json');
    });
    if (!this.valid(generation)) return result;
    if (!result.ok) this.emit({ ...this.workspace()!, trash: original });
    else if (rows(result.value?.failed).length) { this.failure(`휴지통 비우기: ${rows(result.value?.failed).length}개를 못 지웠어요`); await this.reconcile(); }
    return result;
  }
  async saveEntity(kind: 'agenda' | 'project', input: Fields, id?: string): Promise<WriteResult> {
    const workspace = this.workspace(); if (!workspace) return { ok: false };
    const fields = { ...input };
    for (const field of ['title', 'summary', 'repo']) if (typeof fields[field] === 'string') fields[field] = fields[field].trim();
    if (!id) for (const field of ['hue', 'project', 'repo']) if (!fields[field]) delete fields[field];
    const key = kind === 'agenda' ? 'agendas' : 'projects', nid = id || `${kind === 'agenda' ? 'ag' : 'pr'}${this.services.now().getTime().toString(36)}-${this.services.uuid().slice(0, 8)}`;
    const local = { ...fields, id: nid, slug: asText(fields.title).toLowerCase().replace(/[^\p{L}\p{N}]+/gu, '-') || nid };
    const old = workspace[key].find(row => row.id === nid);
    this.emit({ ...workspace, [key]: old ? workspace[key].map(row => row.id === nid ? { ...row, ...fields } : row) : [...workspace[key], local] } as Workspace);
    const argv = kind === 'agenda' ? id ? write.updateAgenda(id, fields) : write.createAgenda(fields) : id ? write.updateProject(id, fields) : write.createProject(fields);
    const result = await this.perform(argv, `${kind === 'agenda' ? '아젠다' : '프로젝트'} 저장: ${asText(fields.title) || '제목 없음'}`, response => {
      const current = this.workspace()!;
      this.emit({ ...current, [key]: current[key].map(row => row.id === nid ? { ...row, ...response } : row) } as Workspace);
      if (asText(response.slug)) this.markOwn(`.aisurada/${key}/${asText(response.slug)}.md`);
    });
    return { ...result, value: { ...result.value, id: asText(result.value?.id) || nid } };
  }
  async deleteEntity(kind: 'agenda' | 'project', id: string): Promise<WriteResult> {
    const workspace = this.workspace(), generation = this.generation, key = kind === 'agenda' ? 'agendas' : 'projects', item = workspace?.[key].find(row => row.id === id);
    if (!workspace || !item) return { ok: false };
    if (kind === 'agenda') for (const note of workspace.notes.filter(note => note.agendaId === id)) { await this.deleteDoc(note.id); if (!this.valid(generation)) return { ok: false }; }
    else for (const agenda of workspace.agendas.filter(agenda => agenda.project === item.slug)) { await this.saveEntity('agenda', { project: '' }, agenda.id); if (!this.valid(generation)) return { ok: false }; }
    try {
      const result = workspace.sample ? { ok: true as const, value: {} } : await engineRefusal(this.services.host, kind === 'agenda' ? write.deleteAgenda(id) : write.deleteProject(id));
      if (!this.valid(generation)) return { ok: false };
      if (!result.ok) return { ok: false, reason: result.reason };
      const response = asRow(result.value) || {};
      this.emit({ ...this.workspace()!, [key]: this.workspace()![key].filter(row => row.id !== id) });
      if (asText(response.slug)) this.markOwn(`.aisurada/${key}/${asText(response.slug)}.md`);
      this.markOwn(asText(response.archivedPath)); this.success(); return { ok: true };
    } catch (error) { this.services.warn('지우지 못했어요', error); this.failure(`${kind === 'agenda' ? '아젠다' : '프로젝트'} 삭제: ${asText(item.title)}`); return { ok: false, reason: `${kind === 'agenda' ? '아젠다' : '프로젝트'}를 지우지 못했어요. 잠시 뒤 다시 시도해 주세요.` }; }
  }
  async saveTemplate(input: Fields, ritualId?: string): Promise<WriteResult> {
    const workspace = this.workspace(), generation = this.generation; if (!workspace) return { ok: false };
    const id = ritualId || asText(input.id) || `tpl-${this.services.now().getTime().toString(36)}-${this.services.uuid().slice(0,8)}`, old = workspace.templates.find(item => item.id === id);
    if (old) this.emit({ ...workspace, templates: workspace.templates.map(row => row.id === id ? { ...row, ...input } : row) });
    const result = await this.perform(ritualId ? write.saveRitual(ritualId, input) : write.saveTemplate(input), `${ritualId ? '리추얼 ' : ''}템플릿 저장: ${asText(input.name) || '제목 없음'}`, response => {
      const current = this.workspace()!, value = { ...old, ...input, ...response, id: asText(response.id) || id, category: ritualId ? 'ritual' : 'doc' };
      this.emit({ ...current, templates: old ? current.templates.map(row => row.id === id ? value : row) : [...current.templates, value] });
      this.markOwn(asText(response.path));
      if (current.sample) writeSampleTemplates(this.services.storage, this.workspace()!.templates);
    });
    if (result.ok) this.unsavedTemplates.delete(id);
    else if (old && this.valid(generation)) this.unsavedTemplates.set(id, { ...this.unsavedTemplates.get(id), ...input });
    return { ...result, value: { ...result.value, id: asText(result.value?.id) || id } };
  }
  deleteTemplate(id: string): Promise<WriteResult> {
    return this.perform(write.deleteTemplate(id), `템플릿 삭제: ${id}`, response => {
      const current = this.workspace()!; this.emit({ ...current, templates: current.templates.filter(row => row.id !== id) });
      if (Array.isArray(response.removed)) response.removed.forEach(path => { if (typeof path === 'string') this.markOwn(path); });
      if (current.sample) writeSampleTemplates(this.services.storage, this.workspace()!.templates);
    });
  }
}
