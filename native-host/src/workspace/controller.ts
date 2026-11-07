import { engineCall, readWorkspace } from './engine.ts';
import { migrateLegacy } from './migrateLegacy.ts';
import { sampleWorkspace } from './sample.ts';
import { addRecent, homePath, KEYS, lastWorkspace, readJSON, readSampleTemplates, readSkin, readTheme, recentWorkspaces,
  removeRecent, savedOpenIds, writeJSON } from './storage.ts';
import { waitingAnswers, unsavedAnswers, noteRailNow } from '@claude-plug/state';
import { asRow, asText, workspaceName } from './types.ts';
import { TabModel } from '../navigation/tabs.ts';
import { NoteScrollMemory } from '../navigation/scroll.ts';
import { activeTab, agendaAddress, docAddress, fileAddress, parseRoute, tabAddress } from '../navigation/routes.ts';
import { connectLinkSource, latestInput } from '../interaction/links.ts';
import type { AppLink } from '../interaction/links.ts';
import { shortcutFor } from '../interaction/shortcuts.ts';
import type { KeyInput } from '../interaction/shortcuts.ts';
import type { RecentWorkspace, Skin, StoragePort, WindowContext, Workspace, WorkspaceServices, WorkspaceState } from './types.ts';
import { claudeSelfWrites } from '../data/selfWrites.ts';
import { VaultData } from '../data/vault.ts';
import { write } from '../data/commands.ts';
import { readPreferences, writePreferences, readNoteWindowGeometry, writeNoteWindowGeometry } from './storage.ts';
import type { Preferences } from '../ui/model.ts';
import { RecentNotes } from '../ui/recent.ts';
import { blocksEscape } from '../ui/layers.ts';
import type { RestoreResult } from '../editor/session.ts';

export class WorkspaceController {
  readonly context: WindowContext;
  private services: WorkspaceServices;
  private state: WorkspaceState;
  private listeners = new Set<() => void>();
  private queue: Promise<unknown> = Promise.resolve();
  private bootPromise?: Promise<void>;
  private watchGeneration = 0;
  private unwatch?: () => void;
  private restored = false;
  private disposed = false;
  private noticeTimer?: ReturnType<typeof setTimeout>;
  private tabs: TabModel;
  private unlistenWindows?: () => void;
  private inputsStarted = false;
  private inputDisposers = new Set<() => void>();
  private linkRevision = 0;
  private pendingLink: { link: AppLink; revision: number } | null = null;
  private drainingLinks = false;
  private closingNote = false;
  private unlistenSelf?: () => void;
  private selfListenerStarted = false;
  private editorFlushers = new Map<symbol, { root: string; flush: () => Promise<void> }>();
  private restorers = new Map<symbol, { key: string; restore: (body: string) => Promise<RestoreResult> }>();
  readonly data: VaultData;
  readonly scrollMemory = new NoteScrollMemory();
  readonly recentNotes: RecentNotes;
  private childRemovers = new Map<symbol, { id: string; remove: (child: string) => void }>();
  private buffers = new Map<symbol, { key: string; read: () => string }>();
  private moving = new Set<string>();
  lastRestoreCommit: import('./types.ts').Row | null = null;

  constructor(services: WorkspaceServices, context: WindowContext, route: string) {
    this.services = services;
    this.recentNotes = new RecentNotes(services.storage);
    this.context = context;
    this.tabs = new TabModel(services.storage, context.kind === 'main');
    this.data = new VaultData(services, () => this.state.workspace, (vault, workspace) => this.patch({ vault, ...(workspace ? { workspace } : {}) }));
    this.state = { booting: true, busy: false, route, workspace: null, notice: null, fatal: null,
      prefs: readPreferences(services.storage, context.kind === 'main') as unknown as Preferences, settingsOpen: false, form: null, hiddenAgendas: [], previewOpened: null,
      recent: recentWorkspaces(services.storage), home: homePath(readJSON(services.storage, KEYS.home)) || '~/Documents',
      theme: readTheme(services.storage), skin: readSkin(services.storage), tabs: this.tabs.snapshot(), noteWindows: [], searchOpen: false, preview: null, subNotes: null, vault: this.data.getSnapshot() };
  }

  getSnapshot = () => this.state;
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };

  private patch(update: Partial<WorkspaceState>, location?: 'push' | 'replace' | 'external') {
    if (this.disposed) return;
    const next = { ...this.state, ...update };
    if (update.workspace && update.workspace.path !== this.state.workspace?.path) {
      next.form = null; next.preview = null; next.previewOpened = null; next.hiddenAgendas = [];
    }
    if (update.noteWindows?.includes(next.preview || '')) { next.preview = null; next.previewOpened = null; }
    if (next.subNotes && update.noteWindows?.includes(next.subNotes.root)) next.subNotes = null;
    if (this.context.kind === 'main') {
      const requested = next.route;
      if (update.workspace) this.tabs.attach(update.workspace);
      if (update.workspace || update.noteWindows) {
        for (const id of next.noteWindows) next.route = this.tabs.close(id, next.route);
      }
      if (!next.booting) {
        const register = location !== undefined || this.state.booting || (!!update.workspace && !this.state.workspace);
        for (let n = 0; n < 3; n++) {
          const resolved = this.tabs.visit(next.route, next.workspace, register);
          if (resolved === next.route) break;
          next.route = resolved;
        }
      }
      if (location === 'push' || location === 'replace') this.services.navigate(next.route, location === 'replace' || next.route !== requested);
      else if (next.route !== requested) this.services.navigate(next.route, true);
    }
    // 11.7: a sub-page closes on every address change, counted by the router's location key,
    // so going to the address it is already at (Cmd+K on the same note) counts too.
    const moved = next.route !== this.state.route || (update.route !== undefined && location !== undefined);
    if (moved && next.subNotes) next.subNotes = next.subNotes.arrival === next.route ? { ...next.subNotes, arrival: null } : null;
    if (next.route !== this.state.route) next.preview = null;
    if (!next.workspace || parseRoute(next.route).kind === 'launcher') next.searchOpen = false;
    this.state = { ...next, tabs: this.tabs.snapshot() };
    for (const listener of this.listeners) listener();
  }

  navigate = (path: string, replace = false) => {
    this.changeLocation(path, replace ? 'replace' : 'push');
  };
  locationChanged = (path: string) => { if (this.context.kind === 'main') this.changeLocation(path, 'external'); };
  private changeLocation(path: string, location: 'push' | 'replace' | 'external') {
    if (this.context.kind === 'note') {
      const noteRoute = `/note-window/${encodeURIComponent(this.context.docId)}`;
      const docPath = docAddress(this.context.docId);
      if (path !== noteRoute && path !== docPath) {
        void this.services.host.invoke('show_in_main', { path, rail: null, sub: null }).catch(err => {
          this.services.warn('show_in_main failed', err);
        });
        return;
      }
    }
    const route = parseRoute(path);
    if (this.context.kind === 'main' && this.state.workspace && route.kind === 'doc' && this.state.noteWindows.includes(route.id)) {
      const previous = this.tabs.close(route.id, this.state.route);
      this.focusNoteWindow(route.id);
      this.patch({ route: previous }, location === 'external' ? 'replace' : undefined);
      return;
    }
    this.patch({ route: path }, location);
  }
  setNoteWindows = (ids: string[]) => { this.patch({ noteWindows: ids.filter(id => typeof id === 'string') }); };
  focusNoteWindow = (id: string) => {
    void this.services.host.invoke('open_note_window', { docId: id, title: null, geometry: this.noteGeometry() }).catch(error => this.services.warn('노트 창을 열지 못했어요', error));
  };
  selectTab = (id: string | null) => this.navigate(id === null ? this.state.tabs.lastMain : tabAddress(id));
  closeTab = (id: string) => {
    if (this.context.kind !== 'main') return;
    const present = this.state.tabs.openIds.includes(id);
    const next = this.tabs.close(id, this.state.route);
    if (present && id.startsWith('term:')) void this.services.host.invoke('pty_detach', { key: id }).catch(() => {});
    this.patch({ route: next }, next !== this.state.route ? 'push' : undefined);
  };
  closeActiveTab = () => { const id = activeTab(this.state.route); if (id !== null) this.closeTab(id); };
  reorderTab = (id: string, index: number) => { this.tabs.reorder(id, index); this.patch({}); };
  refreshTheme = () => this.patch({ theme: readTheme(this.services.storage), skin: readSkin(this.services.storage), ...(this.context.kind === 'main' ? { prefs: readPreferences(this.services.storage, true) as unknown as Preferences } : {}) });
  // 15.14 (5), C153: a setting changed in a note window lives only in that window. It
  // is applied to this window's memory and is never written to the shared store.
  private windowPrefs?: StoragePort;
  preference = (update: Partial<Preferences> & { theme?: 'light' | 'dark'; skin?: Skin }) => {
    if (this.context.kind !== 'main') {
      if (!this.windowPrefs) {
        const values = new Map<string, string>(), saved = this.services.storage.getItem(KEYS.prefs);
        if (saved !== null) values.set(KEYS.prefs, saved);
        this.windowPrefs = { getItem: key => values.get(key) ?? null, setItem: (key, value) => { values.set(key, value); } };
      }
      writePreferences(this.windowPrefs, update as any, true);
      this.patch({ prefs: readPreferences(this.windowPrefs, false) as unknown as Preferences });
      return;
    }
    writePreferences(this.services.storage, update as any, true);
    this.refreshTheme();
  };
  settings = (open: boolean) => { if (this.context.kind === 'main') this.patch({ settingsOpen: open }); };
  form = (form: WorkspaceState['form']) => { if (this.context.kind === 'main') this.patch({ form }); };
  filterAgendas = (hidden: string[]) => this.patch({ hiddenAgendas: [...new Set(hidden)] });
  isNative = () => !!this.services.native;
  confirm = (text: string) => this.services.host.confirmLeave(text);

  notify = (notice: string) => {
    clearTimeout(this.noticeTimer);
    this.patch({ notice });
    this.noticeTimer = setTimeout(() => this.patch({ notice: null }), 2600);
  };

  private enqueue<T>(action: () => Promise<T>): Promise<T> {
    const result = this.queue.then(action, action);
    this.queue = result.catch(() => undefined);
    return result;
  }

  private action(work: () => Promise<boolean>, failure: string, onFailure?: () => void): Promise<boolean> {
    if (this.context.kind === 'note' || this.disposed) return Promise.resolve(false);
    return this.enqueue(async () => {
      this.patch({ busy: true });
      try { return await work(); }
      catch (error) { this.notify(`${failure}: ${String(error)}`); onFailure?.(); return false; }
      finally { this.patch({ busy: false }); }
    });
  }

  boot(seedSample = false, developmentLink?: string): Promise<void> {
    if (this.bootPromise) return this.bootPromise;
    this.connectInputs(developmentLink);
    this.bootPromise = this.enqueue(async () => {
      try {
        if (this.context.kind === 'note') {
          const root = await this.services.host.invoke<string | null>('vault_root');
          if (!root) { this.patch({ fatal: 'main 창에 열린 vault가 없어요. main 창에서 vault를 연 뒤 다시 열어 주세요.' }); return; }
          const data = await readWorkspace(this.services.host, false, undefined, this.services.sanitizeHTML);
          const workspace: Workspace = { ...data, path: root, name: workspaceName(root), sample: false, today: this.today() };
          this.patch({ workspace, vault: this.data.reset(workspace) });
          this.listenNoteWindows();
          this.installWatch(root);
        } else {
          this.listenNoteWindows();
          void this.discoverHome();
          if (seedSample) { this.openSample(); }
          else {
            const last = lastWorkspace(this.services.storage);
            if (last) {
              try { await this.openWorkspace(last.path, last.name); }
              catch {
                writeJSON(this.services.storage, KEYS.last, null);
                this.notify(`이전 워크스페이스를 열 수 없어 시작 화면으로 돌아왔어요: ${last.path}`);
              }
            }
          }
          if (this.state.route === '/') this.navigate(this.state.workspace ? '/calendar' : '/launcher', true);
          else if (!this.state.workspace && this.state.route.split('?')[0] !== '/launcher') this.navigate('/launcher', true);
        }
      } catch (error) {
        this.patch({ fatal: `vault를 읽지 못했어요: ${String(error)}` });
      } finally { this.patch({ booting: false }); this.scheduleLinks(); }
    });
    return this.bootPromise;
  }

  private connectInputs(developmentLink?: string) {
    if (this.inputsStarted) return;
    this.inputsStarted = true;
    const keep = (dispose: () => void) => { if (this.disposed) dispose(); else this.inputDisposers.add(dispose); };
    if (this.services.menu) void this.services.menu(id => { void this.handleMenu(id); }).then(keep)
      .catch(error => this.services.warn('메뉴를 듣지 못했어요', error));
    if (this.context.kind !== 'main') return;
    if (this.services.navigateRequested) void this.services.navigateRequested(nav => {
      if (this.disposed || !nav?.path) return;
      this.navigate(nav.path);
      const route = parseRoute(nav.path);
      if (nav.sub) {
        const parts = nav.sub.split('/').filter(Boolean);
        if (parts.length > 0) {
          const root = route.kind === 'doc' ? route.id : parts[0];
          const chain = route.kind === 'doc' ? parts : parts.slice(1);
          if (root && chain.length) this.openSubNotes(root, chain);
        }
      }
      if (nav.rail) {
        const docId = route.kind === 'doc' ? route.id : '';
        if (docId) {
          const railState = noteRailNow();
          railState.openRail?.(docId, nav.rail);
        }
      }
    }).then(keep).catch(error => this.services.warn('이동 요청을 듣지 못했어요', error));
    if (this.services.deepLinks) void connectLinkSource(this.services.deepLinks, urls => { void this.receiveURLs(urls); },
      () => this.linkRevision, error => this.services.warn('앱을 연 주소를 읽지 못했어요', error)).then(keep)
      .catch(error => this.services.warn('딥링크를 듣지 못했어요', error));
    if (this.services.developmentLinks) keep(this.services.developmentLinks(urls => { void this.receiveURLs(urls); }));
    if (developmentLink) void this.receiveURLs([developmentLink]);
  }

  private showingTabs() {
    return this.context.kind === 'main' && !this.state.booting && this.state.workspace !== null && parseRoute(this.state.route).kind !== 'launcher';
  }

  handleMenu = async (id: string): Promise<boolean> => {
    if (this.disposed) return false;
    // 15.14 (1), C149: the menu is heard only once the window is attached to the vault;
    // the waiting and failure screens close with the window's own button.
    if (this.context.kind === 'note') return id === 'ais_close_tab' && this.state.workspace ? this.closeNoteWindow() : false;
    if (id === 'ais_new_workspace') { this.navigate('/launcher?mode=create'); return true; }
    if (id === 'ais_welcome') return this.launcher();
    if (id === 'ais_open_workspace') {
      try {
        const path = await this.services.host.chooseDirectory();
        return path ? this.open(path, true) : false;
      } catch (error) { this.notify(`폴더를 열 수 없어요: ${String(error)}`); this.navigate('/launcher'); return false; }
    }
    if (!this.showingTabs()) return false;
    if (id === 'ais_open_terminal') { this.navigate('/terminal'); return true; }
    if (id === 'ais_close_tab') {
      const active = activeTab(this.state.route);
      if (active !== null) this.closeTab(active);
      else if (this.services.native) { try { await this.services.closeWindow?.(); } catch { /* The host intentionally denies main close. */ } }
      return true;
    }
    return false;
  };

  handleKey = (event: KeyInput): boolean => {
    if (event.defaultPrevented) return false;
    if (event.key === 'Escape') {
      if (this.state.noteCloseDialog) { this.dismissNoteCloseDialog(); event.preventDefault(); return true; }
      if (this.state.searchOpen) this.patch({ searchOpen: false });
      else if (blocksEscape(event)) return false;
      else if (this.state.settingsOpen) this.settings(false);
      else if (this.state.form) this.form(null);
      else if (this.state.preview) this.showPreview(null);
      else if (this.state.subNotes) this.popSubNote();
      else return false;
      event.preventDefault(); return true;
    }
    if (!this.showingTabs()) return false;
    const shortcut = shortcutFor(event, this.services.mac ?? true, this.services.native ?? false);
    if (!shortcut) return false;
    if (shortcut.kind === 'search') { event.preventDefault(); this.patch({ searchOpen: !this.state.searchOpen }); return true; }
    if (shortcut.kind === 'terminal') { event.preventDefault(); void this.handleMenu('ais_open_terminal'); return true; }
    if (shortcut.kind === 'close') { event.preventDefault(); void this.handleMenu('ais_close_tab'); return true; }
    const tabs: (string | null)[] = [null, ...this.state.tabs.openIds];
    let target: string | null | undefined;
    if (shortcut.kind === 'cycle') {
      if (!this.state.tabs.openIds.length) return false;
      target = tabs[(tabs.indexOf(activeTab(this.state.route)) + shortcut.direction + tabs.length) % tabs.length];
    } else target = shortcut.index === -1 ? tabs.at(-1) : tabs[shortcut.index];
    if (target === undefined) return false;
    event.preventDefault();
    if (target !== activeTab(this.state.route)) this.selectTab(target);
    return true;
  };

  isPoppedOut = (id: string): boolean => {
    if (!id || !this.state.noteWindows.includes(id)) return false;
    if (this.context.kind === 'note' && this.context.docId === id) return false;
    return true;
  };
  closeSearch = () => this.patch({ searchOpen: false });
  showPreview = (id: string | null) => {
    if (id && this.isPoppedOut(id)) { this.focusNoteWindow(id); return; }
    this.patch({ preview: id, previewOpened: this.state.workspace?.notes.find(n => n.id === id) || null });
  };
  registerChildRemover = (id: string, remove: (child: string) => void) => { const token = Symbol(); this.childRemovers.set(token, { id, remove }); return () => { this.childRemovers.delete(token); }; };
  removeChildCards = (parent: string, child: string) => { for (const port of this.childRemovers.values()) if (port.id === parent) port.remove(child); };
  registerBuffer = (key: string, read: () => string) => { const token = Symbol(); this.buffers.set(token, { key, read }); return () => { this.buffers.delete(token); }; };
  readBuffer = (key: string, fallback = '') => [...this.buffers.values()].reverse().find(entry => entry.key === key)?.read() ?? fallback;
  moveNoteWindow = async (id: string): Promise<'opened' | 'unsaved' | 'failed'> => {
    if (this.moving.has(id)) return 'opened'; this.moving.add(id);
    try {
      await this.flushEditors(); await this.data.settled();
      if (this.data.unsavedForRoot(id).length) return 'unsaved';
      const note = this.state.workspace?.notes.find(n => n.id === id);
      await this.services.host.invoke('open_note_window', { docId: id, title: note?.title || null, geometry: this.noteGeometry() });
      // 15.14 (8), C156: from a note tab the open rule sends open_note_window once more (the agenda
      // pane gets its second from the list update). The host only brings the window forward.
      if (this.state.tabs.openIds.includes(id)) this.focusNoteWindow(id);
      if (this.state.preview === id) this.showPreview(null); return 'opened';
    } catch { return 'failed'; } finally { this.moving.delete(id); }
  };
  registerEditor = (root: string, flush: () => Promise<void>): (() => void) => {
    const key = Symbol(); this.editorFlushers.set(key, { root, flush }); return () => { this.editorFlushers.delete(key); };
  };
  registerRestore = (key: string, restore: (body: string) => Promise<RestoreResult>) => {
    const token = Symbol(); this.restorers.set(token, { key, restore }); return () => { this.restorers.delete(token); };
  };
  restoreNote = (key: string, body: string): Promise<RestoreResult> => {
    const target = [...this.restorers.values()].reverse().find(entry => entry.key === key);
    return target ? target.restore(body) : Promise.resolve('unavailable');
  };
  flushNoteEditors = (root?: string) => this.flushEditors(root);
  private flushEditors = async (root?: string) => {
    await Promise.allSettled([...this.editorFlushers.values()].filter(editor => !root || editor.root === root).map(editor => editor.flush()));
  };
  // 4.8: a link that opens sub-pages always writes the arrival address /doc/<root>, so the one
  // address change its own navigation makes is forgiven. Other ways write it only when not already there.
  openSubNotes = (root: string, ids: string[], alwaysArrive = false) => { if (!ids.length) { this.patch({ subNotes: null }); return; } if (this.isPoppedOut(root)) { this.focusNoteWindow(root); return; } this.patch({ subNotes: {
    root, ids, arrival: !alwaysArrive && this.state.route === docAddress(root) ? null : docAddress(root),
  } }); };
  popSubNote = () => {
    const current = this.state.subNotes;
    this.patch({ subNotes: current && current.ids.length > 1 ? { ...current, ids: current.ids.slice(0, -1) } : null });
  };

  private unlistenCloseRequested?: () => void;
  private geometryTimer?: ReturnType<typeof setTimeout>;
  /** 15.8: a move or a resize is written 400ms after the last change. */
  listenNoteGeometry = () => this.services.noteGeometryChanged?.(geometry => {
    clearTimeout(this.geometryTimer);
    this.geometryTimer = setTimeout(() => this.saveNoteGeometry(geometry), 400);
  });
  listenCloseRequested = (handler: () => void) => {
    const promise = this.services.closeRequested?.(handler);
    void promise?.then(fn => { this.unlistenCloseRequested = fn; });
    return promise;
  };
  saveNoteGeometry = (geo: { width: number; height: number; x: number; y: number }) => {
    if (!this.state.workspace) return; // C149: those screens do not record the window's place
    writeNoteWindowGeometry(this.services.storage, geo);
    void this.services.host.invoke('remember_note_window', { geometry: geo }).catch(err => {
      console.error('remember_note_window failed', err);
    });
  };

  closeNoteWindow = async (): Promise<boolean> => {
    if (this.context.kind !== 'note' || this.closingNote || this.disposed) return false;
    this.closingNote = true;
    try {
      let settled = false;
      let timer: ReturnType<typeof setTimeout> | undefined;
      try {
        settled = await Promise.race([
          (async () => {
            await this.services.pending.flushEditors();
            await this.flushEditors();
            return (await Promise.all([
              this.services.pending.waitForWrites(10_000),
              this.data.settled(),
            ])).every(Boolean);
          })(),
          new Promise<boolean>(resolve => { timer = setTimeout(() => resolve(false), 10_000); }),
        ]);
      } catch {
        settled = false;
      } finally {
        clearTimeout(timer);
      }

      const count = this.services.pending.unsavedNotes() + this.data.unsavedKeys().length;
      const waiting = waitingAnswers();
      const unsavedA = unsavedAnswers();

      if (settled && count === 0 && waiting === 0 && unsavedA === 0) {
        this.unlistenCloseRequested?.();
        this.stopWatch();
        await this.services.host.invoke('destroy_current_window');
        this.dispose();
        return true;
      }

      const reasons: string[] = [];
      if (!settled) {
        reasons.push('저장이 끝났는지 확인하지 못했어요. 사이드카가 답하지 않아요.');
      }
      if (count > 0) {
        reasons.push(`디스크에 아직 못 쓴 노트가 ${count}개 있어요. 충돌 배너가 떠 있거나 저장이 실패한 노트예요.`);
      }
      if (waiting > 0) {
        reasons.push(`답을 기다리는 물음이 ${waiting}개 있어요. 답은 이 창이 노트에 붙여서, 창을 닫으면 답이 노트에 안 남아요.`);
      }
      if (unsavedA > 0) {
        reasons.push(`답은 왔는데 노트에 못 적은 회차가 ${unsavedA}개 있어요.`);
      }

      this.patch({ noteCloseDialog: { reasons } });
      return false;
    } catch (error) {
      this.services.warn('노트 창을 닫지 못했어요', error);
      if (this.state.workspace) this.installWatch(this.state.workspace.path);
      return false;
    } finally {
      this.closingNote = false;
    }
  };

  dismissNoteCloseDialog = () => {
    this.patch({ noteCloseDialog: null });
  };

  forceCloseNoteWindow = async (): Promise<boolean> => {
    this.patch({ noteCloseDialog: null });
    this.unlistenCloseRequested?.();
    this.stopWatch();
    try {
      await this.services.host.invoke('destroy_current_window');
      this.dispose();
      return true;
    } catch (error) {
      this.services.warn('노트 창을 닫지 못했어요', error);
      return false;
    }
  };

  receiveURLs = async (urls: string[]): Promise<void> => {
    if (this.context.kind !== 'main' || this.disposed) return;
    const input = latestInput(urls);
    if (!input) return;
    // Claim file URLs before the asynchronous host lookup so stale cold-start
    // reads and older lookups cannot overtake a more recent user request.
    const revision = ++this.linkRevision;
    this.pendingLink = null;
    let link: AppLink;
    if ('link' in input) link = input.link;
    else {
      const root = this.state.workspace?.path;
      const knownRoots = [...(root ? [root] : []), ...recentWorkspaces(this.services.storage).map(item => item.path)];
      try {
        const target = asRow(await this.services.host.invoke('resolve_md_target', { path: input.file, knownRoots }));
        if (revision !== this.linkRevision || this.disposed) return;
        if (target?.kind === 'note' && Array.isArray(target.chain) && target.chain.length > 0 && target.chain.every(id => typeof id === 'string' && id.length > 0) && asText(target.ws)) {
          link = { kind: 'doc', id: target.chain[0], chain: target.chain.slice(1), ws: asText(target.ws), window: false };
        } else if (target?.kind === 'external' && asText(target.path)) link = { kind: 'external', path: asText(target.path), ws: null };
        else return;
      } catch { return; }
    }
    if (revision !== this.linkRevision || this.disposed) return;
    this.pendingLink = { link, revision };
    this.scheduleLinks();
    await this.waitForIdle();
  };

  waitForIdle = (): Promise<void> => this.queue.then(() => this.data.idle());

  private scheduleLinks() {
    if (this.drainingLinks || this.state.booting || this.disposed || !this.pendingLink) return;
    this.drainingLinks = true;
    void this.enqueue(async () => {
      try {
        while (!this.disposed && this.pendingLink) {
          const pending = this.pendingLink;
          this.pendingLink = null;
          await this.openLink(pending.link, pending.revision);
        }
      } finally { this.drainingLinks = false; this.scheduleLinks(); }
    });
  }

  private async openLink(link: AppLink, revision: number) {
    if (link.ws && this.state.workspace?.path !== link.ws) {
      this.patch({ busy: true });
      try {
        if (!await this.services.host.invoke<boolean>('is_workspace', { root: link.ws })) {
          if (revision === this.linkRevision) { this.notify(`딥링크가 가리키는 폴더가 워크스페이스가 아니에요: ${link.ws}`); this.navigate('/launcher'); }
          return;
        }
        if (!await this.canLeave()) return;
        this.data.invalidate();
        await this.data.idle();
        await this.authorize(link.ws);
        await this.openWorkspace(link.ws);
        this.patch({ recent: addRecent(this.services.storage, { path: link.ws, name: workspaceName(link.ws) }) });
      } catch (error) {
        if (revision === this.linkRevision) { this.notify(`딥링크 워크스페이스를 열 수 없어요: ${String(error)}`); this.navigate('/launcher'); }
        return;
      } finally { this.patch({ busy: false }); }
    }
    if (this.disposed || revision !== this.linkRevision) return;
    if (!this.state.workspace) {
      this.notify('딥링크를 열려면 워크스페이스가 필요해요. 먼저 워크스페이스를 열어 주세요.');
      this.navigate('/launcher'); return;
    }
    if (link.kind === 'agenda') { this.navigate(agendaAddress(link.slug)); return; }
    if (link.kind === 'external') { this.navigate(fileAddress(link.path)); return; }
    if (link.window) {
      const opened = await this.openLinkWindow(link.id, revision);
      if (opened || revision !== this.linkRevision) return;
    }
    if (link.chain.length && !this.state.noteWindows.includes(link.id)) this.openSubNotes(link.id, link.chain, true);
    this.navigate(docAddress(link.id));
  }

  private noteGeometry() {
    const geo = readNoteWindowGeometry(this.services.storage);
    if (!geo) return null;
    return {
      width: geo.width,
      height: geo.height,
      x: geo.x ?? null,
      y: geo.y ?? null,
    };
  }

  private async openLinkWindow(id: string, revision: number): Promise<boolean> {
    const geometry = this.noteGeometry();
    try {
      if (this.state.noteWindows.includes(id)) {
        await this.services.host.invoke('open_note_window', { docId: id, title: null, geometry }); return true;
      }
      const note = this.state.workspace?.notes.find(item => item.id === id);
      if (!note) return false;
      if (this.services.pending.flushNote) await this.services.pending.flushNote(id); else await this.services.pending.flushEditors();
      await this.flushEditors(id);
      if (!await this.services.pending.waitForWrites(10_000) || !await this.data.settled() || this.data.unsavedForRoot(id).length || (this.services.pending.unsavedForNote?.(id) ?? this.services.pending.unsavedNotes()) > 0) {
        this.services.warn('못 쓴 글이 남아서 노트를 새 창으로 옮기지 않았어요'); return false;
      }
      if (revision !== this.linkRevision || this.disposed) return false;
      await this.services.host.invoke('open_note_window', { docId: id, title: note.title, geometry });
      return true;
    } catch (error) { this.services.warn('노트를 새 창으로 옮기지 못했어요', error); return false; }
  }

  private today() {
    const date = this.services.now();
    return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
  }

  private listenNoteWindows() {
    if (!this.services.noteWindowsChanged) return;
    let heard = false;
    void this.services.noteWindowsChanged(ids => {
      heard = true;
      this.setNoteWindows(ids);
    }).then(unlisten => {
      if (this.disposed) unlisten(); else this.unlistenWindows = unlisten;
    }).catch(error => this.services.warn('노트 창 목록을 듣지 못했어요', error));
    void this.services.host.invoke<string[]>('note_windows').then(ids => {
      if (!heard && Array.isArray(ids)) this.setNoteWindows(ids);
    }).catch(() => {});
  }

  callHost = <T,>(command: string, args?: Record<string, unknown>): Promise<T> => this.services.host.invoke<T>(command, args);
  callEngine = (argv: string[]): Promise<unknown> => engineCall(this.services.host, argv);

  private async discoverHome() {
    try {
      const home = homePath(await this.services.host.documentDir());
      if (home && this.state.home === '~/Documents') this.patch({ home });
    } catch { /* Browser previews retain the documented fallback. */ }
  }

  setHome = (home: string) => this.patch({ home });
  chooseDirectory = async (forCreation: boolean): Promise<string | null> => {
    if (this.context.kind === 'note') return null;
    try {
      const path = await this.services.host.chooseDirectory();
      if (!path) throw new Error('취소');
      if (forCreation) {
        this.patch({ home: path });
        writeJSON(this.services.storage, KEYS.home, homePath(path));
      }
      return path;
    } catch { this.notify('폴더 선택 창을 열 수 없어요'); return null; }
  };

  private async canLeave(): Promise<boolean> {
    if (!this.state.workspace || this.state.workspace.sample) return true;
    const pending = this.services.pending;
    let settled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    try {
      settled = await Promise.race([
        (async () => { await pending.flushEditors(); await this.flushEditors();
          return (await Promise.all([pending.waitForWrites(10_000), this.data.settled()])).every(Boolean); })(),
        new Promise<boolean>(resolve => { timer = setTimeout(() => resolve(false), 10_000); }),
      ]);
    } catch { /* A failed flush must still ask before discarding data. */ }
    finally { clearTimeout(timer); }
    const count = pending.unsavedNotes() + this.data.unsavedKeys().length;
    // Part 4's outstanding questions and unpersisted answers are both zero.
    return settled && count === 0 || await this.services.host.confirmLeave(
      `아직 저장하지 못한 노트가 ${count}개 있어요.${settled ? '' : ' 저장을 마치지 못했어요.'} 워크스페이스를 바꿀까요?`);
  }

  private async authorize(path: string) {
    try {
      const notes = await this.services.host.invoke<string[]>('note_windows');
      if (notes.length) {
        const current = await this.services.host.invoke<string | null>('vault_root');
        if (current && current !== path) {
          await this.services.host.invoke('close_note_windows');
          for (let n = 0; n < 40; n++) {
            await this.services.sleep(100);
            if (!(await this.services.host.invoke<string[]>('note_windows')).length) break;
          }
        }
      }
    } catch { /* Only allow_workspace's string rejection is fatal. */ }
    try { await this.services.host.invoke('allow_workspace', { path }); }
    catch (error) { if (typeof error === 'string') throw new Error(error); }
  }

  private async openWorkspace(path: string, requestedName = '') {
    const name = requestedName || workspaceName(path);
    this.data.invalidate();
    await this.data.idle();
    await this.authorize(path);
    this.stopWatch();
    try { await engineCall(this.services.host, write.init(name)); }
    catch { /* Engine startup errors do not prevent opening existing workspaces. */ }
    try { await migrateLegacy(path, this.services.files, this.services.now(), this.services.uuid, this.services.warn); }
    catch (error) { this.services.warn('옛 노트 이관을 마치지 못했어요', error); }
    const data = await readWorkspace(this.services.host, true, undefined, this.services.sanitizeHTML);
    const workspace: Workspace = { ...data, path, name, sample: false, today: this.today() };
    this.patch({ workspace, fatal: null, vault: this.data.reset(workspace), subNotes: null, searchOpen: false });
    writeJSON(this.services.storage, KEYS.last, { path, name });
    this.installWatch(path);
    await this.restoreWindows(workspace);
  }

  open = (rawPath: string, menu = false): Promise<boolean> => this.action(async () => {
    const path = rawPath.trim();
    if (!path || !await this.canLeave()) return false;
    await this.openWorkspace(path);
    this.patch({ recent: addRecent(this.services.storage, { path, name: workspaceName(path) }) });
    this.navigate('/calendar');
    return true;
  }, '폴더를 열 수 없어요', menu ? () => this.navigate('/launcher') : undefined);

  create = (rawName: string): Promise<boolean> => this.action(async () => {
    const name = rawName.trim(), parent = this.state.home;
    if (!name || !await this.canLeave()) return false;
    const path = await this.services.host.invoke<string>('create_workspace_dir', { parent, name });
    const before = await this.services.host.invoke<string | null>('vault_root').catch(() => null);
    this.data.invalidate();
    await this.data.idle();
    await this.authorize(path);
    this.stopWatch();
    // Creation reports initialization failure instead of claiming success.
    try { await engineCall(this.services.host, write.init(name)); }
    catch (error) {
      // C202: the host goes back to the vault it had when creation started, so the
      // vault still on screen and the host's vault never part. The first error is
      // the one reported.
      if (before) { try { await this.services.host.invoke('allow_workspace', { path: before }); } catch { /* reported below */ } }
      const shown = this.state.workspace;
      if (shown && !shown.sample) this.installWatch(shown.path);
      throw error;
    }
    await this.openWorkspace(path, name);
    writeJSON(this.services.storage, KEYS.home, homePath(parent));
    this.patch({ recent: addRecent(this.services.storage, { path, name, created: true }) });
    this.navigate('/calendar');
    return true;
  }, '워크스페이스 생성 실패');

  // 5.4: a way that only lets go of the vault (the sample) also sends the retrospective
  // writes it has been collecting for 400ms, so a note typed just before is not lost.
  private openSample() {
    this.stopWatch();
    const workspace = sampleWorkspace();
    workspace.templates = readSampleTemplates(this.services.storage).map(item => ({ category: 'doc', ...item }));
    this.patch({ workspace, fatal: null, vault: this.data.reset(workspace), subNotes: null, searchOpen: false });
  }

  sample = (): Promise<boolean> => this.action(async () => {
    let documents = '~/Documents';
    try { documents = (await this.services.host.documentDir()).replace(/\/+$/, '') || documents; } catch { /* fallback */ }
    this.patch({ recent: addRecent(this.services.storage,
      { path: `${documents}/aisurada-sample`, name: '샘플 워크스페이스', sample: true }) });
    if (!await this.canLeave()) return false;
    this.stopWatch(); this.data.invalidate(); await this.data.idle();
    this.openSample();
    this.navigate('/calendar');
    return true;
  }, '워크스페이스를 열 수 없어요');

  openRecent = (item: RecentWorkspace): Promise<boolean> => this.action(async () => {
    if (!await this.canLeave()) return false;
    if (item.sample) { this.stopWatch(); this.data.invalidate(); await this.data.idle(); this.openSample(); }
    else await this.openWorkspace(item.path, item.name);
    this.navigate('/calendar');
    return true;
  }, '워크스페이스를 열 수 없어요');

  forgetRecent = (path: string) => {
    if (this.context.kind === 'main') this.patch({ recent: removeRecent(this.services.storage, path) });
  };

  launcher = (): Promise<boolean> => this.action(async () => {
    if (!await this.canLeave()) return false;
    this.stopWatch();
    this.data.invalidate(); await this.data.idle();
    void this.services.host.invoke('close_note_windows').catch(() => {});
    this.patch({ workspace: null, vault: this.data.reset() });
    writeJSON(this.services.storage, KEYS.last, null);
    this.navigate('/launcher');
    return true;
  }, '시작 화면으로 돌아갈 수 없어요');

  private async restoreWindows(workspace: Workspace) {
    if (this.restored || this.context.kind !== 'main') return;
    this.restored = true;
    try {
      const saved = await this.services.host.invoke<string[]>('saved_note_windows') || [];
      const mainTabs = savedOpenIds(this.services.storage, workspace.path);
      const notesMap = new Map(workspace.notes.map(note => [note.id, note]));
      const seen = new Set<string>();
      const docs: { docId: string; title: string }[] = [];
      for (const id of saved) {
        if (seen.has(id)) continue;
        seen.add(id);
        const note = notesMap.get(id);
        if (note && !mainTabs.has(id)) {
          docs.push({ docId: note.id, title: note.title });
        }
      }
      if (docs.length) await this.services.host.invoke('restore_note_windows', { docs });
    } catch (error) { this.services.warn('노트 창을 되살리지 못했어요', error); }
  }

  private stopWatch() {
    this.watchGeneration++;
    this.unwatch?.();
    this.unwatch = undefined;
    this.data.selfWrites.clear();
  }

  private installWatch(root: string) {
    this.stopWatch();
    if (!this.selfListenerStarted && this.services.selfWritesChanged) {
      this.selfListenerStarted = true;
      void this.services.selfWritesChanged(this.data.receiveOwn).then(unlisten => {
        if (this.disposed) unlisten(); else this.unlistenSelf = unlisten;
      }).catch(error => this.services.warn('다른 창의 쓰기를 듣지 못했어요', error));
    }
    const generation = this.watchGeneration;
    void this.services.watch(root, paths => {
      if (generation === this.watchGeneration && this.data.selfWrites.external(claudeSelfWrites.external(paths)).length) void this.refresh();
    }).then(unwatch => {
      if (this.disposed || generation !== this.watchGeneration) unwatch();
      else this.unwatch = unwatch;
    }).catch(error => this.services.warn('vault 감시를 걸지 못했어요', error));
  }

  refresh = (): Promise<void> => {
    if (this.disposed) return Promise.resolve();
    if (this.state.busy) return this.queue.then(() => this.refresh());
    let workspace = this.state.workspace;
    if (!workspace || workspace.sample || this.disposed) return Promise.resolve();
    const today = this.today();
    if (workspace.today !== today) { workspace = { ...workspace, today }; this.patch({ workspace }); }
    return this.data.reconcile();
  };

  dispose() {
    this.disposed = true;
    clearTimeout(this.geometryTimer);
    this.stopWatch();
    this.unlistenWindows?.();
    this.unlistenSelf?.();
    this.data.dispose();
    this.editorFlushers.clear();
    this.restorers.clear();
    this.buffers.clear(); this.childRemovers.clear();
    for (const dispose of this.inputDisposers) dispose();
    this.inputDisposers.clear();
    clearTimeout(this.noticeTimer);
    this.listeners.clear();
  }
}
