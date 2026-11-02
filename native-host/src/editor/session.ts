import type { WorkspaceController } from '../workspace/controller.ts';
import type { NoteRef } from '../data/vault.ts';
import { extractTitle } from '../data/title.ts';
import { asRow } from '../workspace/types.ts';

export type SaveResult = 'saved' | 'unsaved' | 'conflict';
export type RestoreResult = 'restored' | 'conflict' | 'unsaved' | 'not-recorded' | 'replaced-conflict' | 'replaced-unsaved' | 'unavailable';
type SaveMode = 'auto' | 'explicit' | 'dispose' | 'keep' | 'restore';
type SaveClass = 'conflict' | 'error' | 'saved' | 'dirty' | 'saving';
export interface SessionSnapshot {
  status: string; state: SaveClass; discarding: boolean;
  mirror: { revision: number; body: string; external: boolean };
}

/** One mounted note editor. Its buffer intentionally lives outside vault memory. */
export class EditorSession {
  private readonly controller: WorkspaceController;
  private readonly ref: NoteRef;
  private readonly epoch: number;
  private readonly format: string;
  private gate: string;
  private buffer: string;
  private ownEditing = false;
  private typed = false;
  private released = false;
  private discarding = false;
  private lastFailed = false;
  private cursor: 1 | 2 = 2;
  private jobs = 0;
  private work?: Promise<SaveResult>;
  private timer?: ReturnType<typeof setTimeout>;
  private unregister: () => void;
  private unrestore: () => void;
  private unlisten: () => void;
  private unbuffer: () => void;
  private listeners = new Set<() => void>();
  private state: SessionSnapshot;
  constructor(controller: WorkspaceController, ref: NoteRef, body: string) {
    this.controller = controller; this.ref = ref; this.epoch = controller.data.epoch();
    this.format = controller.data.note(ref)?.format || 'markdown';
    const memory = controller.data.getSnapshot(), clash = memory.conflicted.includes(this.key());
    this.buffer = clash ? memory.buffers[this.key()] ?? body : body;
    this.ownEditing = clash;
    this.gate = extractTitle(this.buffer, this.format);
    this.state = { status: clash ? '외부 변경됨' : '저장됨', state: clash ? 'conflict' : 'saved', discarding: false,
      mirror: { revision: 0, body: this.buffer, external: false } };
    const root = ref.parent ? controller.data.chain(ref.parent)?.split('/')[0] || ref.id : ref.id;
    this.unregister = controller.registerEditor(root, async () => { await this.flush(); });
    this.unrestore = controller.registerRestore(this.key(), this.restore);
    this.unbuffer = controller.registerBuffer?.(this.key(), this.body) || (() => {});
    this.unlisten = controller.subscribe(this.receiveStore);
    if (clash) queueMicrotask(() => { if (this.valid() && !this.released) controller.data.protect(ref, true); });
  }
  private key() { return this.ref.parent ? `${this.ref.parent}:${this.ref.id}` : this.ref.id; }
  private valid() { return this.epoch === this.controller.data.epoch(); }
  private conflicted() { return this.controller.data.getSnapshot().conflicted.includes(this.key()); }
  getSnapshot = () => this.state;
  body = () => this.buffer;
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  private emit() {
    const state: SaveClass = this.conflicted() ? 'conflict' : this.lastFailed ? 'error' : this.jobs ? 'saving'
      : this.ownEditing || this.buffer !== this.controller.data.note(this.ref)?.body ? 'dirty' : 'saved';
    const status = state === 'conflict' ? '외부 변경됨' : state === 'error' ? '저장 실패 — 다시 시도 중'
      : state === 'saved' ? '저장됨' : '저장 중…';
    this.state = { ...this.state, state, status, discarding: this.discarding };
    if (!this.released) this.listeners.forEach(listener => listener());
  }
  private stash() { if (this.ownEditing && !this.discarding) this.controller.data.stashConflict(this.ref, this.buffer, this.typed); }
  private receiveStore = () => {
    if (!this.valid()) { clearTimeout(this.timer); return; }
    if (this.conflicted()) { clearTimeout(this.timer); this.stash(); }
    else if (!this.ownEditing && !this.discarding) {
      const note = this.controller.data.note(this.ref);
      if (note) this.receive(note.body);
    }
    this.emit();
  };
  receive(body: string) {
    if (!this.valid() || this.discarding || this.ownEditing || body === this.buffer) return;
    this.buffer = body; this.gate = extractTitle(body, this.format); this.lastFailed = false;
    this.mirror(body, true);
  }
  private mirror(body: string, external: boolean) {
    this.state = { ...this.state, mirror: { revision: this.state.mirror.revision + 1, body, external } }; this.emit();
  }
  change = (body: string, cursor: 1 | 2) => {
    if (!this.valid() || this.discarding || this.released) return;
    this.buffer = body; this.cursor = cursor; this.typed = true;
    if (!this.ownEditing && body !== this.controller.data.note(this.ref)?.body) {
      this.ownEditing = true; this.controller.data.protect(this.ref, true);
    }
    this.stash(); this.emit(); this.schedule(600, 'auto');
  };
  private schedule(ms: number, mode: SaveMode) {
    clearTimeout(this.timer);
    if (!this.released && !this.discarding && !this.conflicted()) this.timer = setTimeout(() => { void this.flush(mode); }, ms);
  }
  // IME defers the editor mirror, not the autosave buffer. This preserves the
  // documented 10.15(5) ordering; an intervening edit is reported on compositionend.
  pause = (_pending: boolean) => {};
  conflict = () => {
    if (!this.valid() || this.discarding) return;
    this.ownEditing = true; this.controller.data.mirrorConflict(this.ref, this.buffer); this.stash(); clearTimeout(this.timer); this.emit();
  };
  async flush(mode: SaveMode = 'explicit', force = false): Promise<SaveResult> {
    clearTimeout(this.timer);
    if (!this.valid() || this.discarding) return 'saved';
    if (this.conflicted()) { this.stash(); return 'conflict'; }
    this.jobs++; this.emit();
    const prior = this.work;
    const run = (prior || Promise.resolve('saved')).catch(() => 'unsaved').then(() => this.save(mode, force));
    this.work = run.finally(() => { this.jobs--; if (this.work === task) this.work = undefined; this.emit(); });
    const task = this.work;
    return task;
  }
  private async title(mode: SaveMode): Promise<boolean> {
    if (mode === 'auto' || mode === 'keep') { if (this.cursor === 1) return true; }
    const note = this.controller.data.note(this.ref); if (!note) return true;
    // C90: leaving compares with the gate like window blur. The mirror moves the gate,
    // so a heading changed elsewhere is never committed as this editor's title.
    const title = extractTitle(note.body, this.format);
    if (!title || title === this.gate) return true;
    const previous = this.gate; this.gate = title;
    if (note.title === title) return true;
    const result = await this.controller.data.updateNote(this.ref, { title });
    if (!this.valid()) return false;
    // A sub-note title failure keeps its optimistic title and the advanced gate.
    if (this.ref.parent) return true;
    if (!result.ok) this.gate = previous;
    return result.ok;
  }
  private async save(mode: SaveMode, force: boolean): Promise<SaveResult> {
    if (!this.valid() || this.discarding) return 'saved';
    if (this.conflicted()) { this.stash(); return 'conflict'; }
    const body = this.buffer;
    const result = await this.controller.data.saveBody(this.ref, body, force);
    if (!this.valid() || this.discarding) return 'saved';
    if (!result.ok) {
      this.ownEditing = true; this.lastFailed = true; this.controller.data.protect(this.ref, true); this.stash();
      if (this.released && !this.conflicted()) this.controller.data.retryBody(this.ref);
      else this.schedule(4000, mode);
      this.emit(); return this.conflicted() ? 'conflict' : 'unsaved';
    }
    this.lastFailed = false;
    if (this.buffer === this.controller.data.note(this.ref)?.body) { this.ownEditing = false; this.controller.data.protect(this.ref, false); }
    const titled = await this.title(mode);
    if (this.released && !this.conflicted() && this.controller.data.getSnapshot().unsaved.includes(this.key())) this.controller.data.retryBody(this.ref);
    this.emit();
    return this.conflicted() ? 'conflict' : titled ? 'saved' : 'unsaved';
  }
  keep = async (): Promise<SaveResult> => {
    if (!this.valid() || this.discarding) return 'unsaved';
    clearTimeout(this.timer); await this.controller.data.refreshToken(this.ref);
    if (!this.valid()) return 'unsaved';
    this.controller.data.clearConflict(this.ref); this.lastFailed = false;
    return this.flush('keep', true);
  };
  discard = async () => {
    if (!this.valid() || this.discarding) return;
    clearTimeout(this.timer); this.discarding = true; this.ownEditing = false; this.emit();
    this.controller.data.protect(this.ref, false);
    if (this.ref.parent) await this.controller.data.loadSubNotes(this.ref.parent); else await this.controller.data.reconcile();
    if (!this.valid()) return;
    this.buffer = this.controller.data.note(this.ref)?.body || '';
    this.gate = extractTitle(this.buffer, this.format); this.lastFailed = false; this.typed = false;
    this.mirror(this.buffer, false); this.controller.data.clearConflict(this.ref); this.discarding = false; this.emit();
  };
  diskBody = () => this.controller.data.diskBody(this.ref);
  restore = async (body: string): Promise<RestoreResult> => {
    if (!this.valid() || this.released || this.discarding) return 'unavailable';
    const before = await this.flush('restore'); if (before !== 'saved') return before;
    try {
      const commit = asRow(await this.controller.callEngine(['commit-vault']));
      this.controller.lastRestoreCommit = commit;
      if (!commit || !['committed', 'nothing'].includes(String(commit.outcome))) return 'not-recorded';
    } catch { this.controller.lastRestoreCommit = { outcome: 'failed' }; return 'not-recorded'; }
    if (!this.valid() || this.released) return 'unavailable';
    this.buffer = body; this.typed = true; this.ownEditing = true; this.controller.data.protect(this.ref, true); this.mirror(body, false);
    const result = await this.flush('restore', true);
    return result === 'saved' ? 'restored' : result === 'conflict' ? 'replaced-conflict' : 'replaced-unsaved';
  };
  dispose() {
    if (this.released) return;
    clearTimeout(this.timer); this.stash(); this.unlisten(); this.unregister(); this.unrestore(); this.unbuffer();
    if (!this.discarding && this.valid() && !this.conflicted()) {
      const bodyNeeded = this.buffer !== this.controller.data.note(this.ref)?.body || this.ownEditing && !this.jobs;
      if (bodyNeeded) void this.flush('dispose');
      else if (this.work) void this.work.then(result => { if (result === 'saved' && this.valid() && !this.conflicted()) void this.title('dispose'); });
      else void this.title('dispose');
    }
    this.released = true; this.listeners.clear();
  }
}
