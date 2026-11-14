import { Component, useEffect, useState, useSyncExternalStore } from 'react';
import type { ErrorInfo, FormEvent, ReactNode } from 'react';
import { WorkspaceController } from './workspace/controller.ts';
import { workspaceServices } from './workspace/platform.ts';
import { windowContext } from './workspace/types.ts';
import { applyTypographyVars, TYPOGRAPHY_EVENT } from './workspace/typography.ts';
import type { WorkspaceState } from './workspace/types.ts';
import { WorkspaceShell } from './navigation/WorkspaceShell.tsx';
import { NoteScreen } from './navigation/NoteReading.tsx';
import { InteractionOverlays } from './interaction/Overlays.tsx';
import { VaultBanner } from './data/VaultBanner.tsx';
import { captureEscape } from './ui/layers.ts';
import { Preview } from './ui/Preview.tsx';
import { Modal } from './ui/Modal.tsx';
import { BrandMark } from './ui/BrandMark.tsx';
import type { BrandState } from './ui/BrandMark.tsx';
import { formatRelativeTime } from './ui/dates.ts';
import { refreshGithub } from './ui/githubStore.ts';
import { applyChanged, refreshNotifications, toItem } from './ui/notificationsStore.ts';
import { refreshApprovalCounts, resetApprovalsStore, startApprovalsWatch } from './ui/approvalsStore.ts';
import { refreshRunner, runnerChanged } from './ui/runnerStore.ts';
import { NotificationToasts } from './ui/NotificationCenter.tsx';

// Window identity is decided once, before any internal navigation.
const context = windowContext(window.location.pathname);

class ScreenBoundary extends Component<{ children: ReactNode }, { failed: boolean }> {
  state = { failed: false };
  static getDerivedStateFromError() { return { failed: true }; }
  componentDidCatch(error: Error, info: ErrorInfo) { console.error('화면을 열지 못했어요', error, info); }
  render() {
    return this.state.failed ? <main className="status-screen"><h1>앱을 여는 중 문제가 생겼어요</h1>
      <button onClick={() => location.reload()}>새로고침</button></main> : this.props.children;
  }
}

export function App() { return <ScreenBoundary><WorkspaceApp /></ScreenBoundary>; }

function WorkspaceApp() {
  const [controller] = useState(() => new WorkspaceController(workspaceServices(), context, location.pathname + location.search));
  const state = useSyncExternalStore(controller.subscribe, controller.getSnapshot);
  useEffect(() => {
    const changed = () => controller.locationChanged(location.pathname + location.search);
    const focus = () => { controller.refreshTheme(); void controller.refresh(); };
    const visible = () => { if (document.visibilityState === 'visible') focus(); };
    const storage = (event: StorageEvent) => { if (event.key === 'aisurada.prefs.v1') controller.refreshTheme(); };
    const keydown = (event: KeyboardEvent) => { controller.handleKey(event); };
    let unlistenClose: (() => void) | undefined;
    if (context.kind === 'note') {
      void controller.listenCloseRequested(() => { void controller.closeNoteWindow(); })?.then(fn => { unlistenClose = fn; });
    } else if (context.kind === 'main') {
      void controller.listenCloseRequested(() => { void controller.closeMainWindow(); })?.then(fn => { unlistenClose = fn; });
    }
    let unlistenGeometry: (() => void) | undefined;
    if (context.kind === 'note') void controller.listenNoteGeometry()?.then(fn => { unlistenGeometry = fn; });
    addEventListener('popstate', changed);
    addEventListener('focus', focus);
    addEventListener('storage', storage);
    addEventListener('keydown', keydown); addEventListener('keydown', captureEscape, true);
    document.addEventListener('visibilitychange', visible);
    void controller.boot(context.kind === 'main' && import.meta.env.DEV && new URLSearchParams(location.search).get('seed') === 'sample',
      context.kind === 'main' && import.meta.env.DEV ? new URLSearchParams(location.search).get('deeplink') || undefined : undefined);
    return () => {
      unlistenClose?.(); unlistenGeometry?.();
      removeEventListener('popstate', changed); removeEventListener('focus', focus); removeEventListener('storage', storage);
      removeEventListener('keydown', keydown); removeEventListener('keydown', captureEscape, true);
      document.removeEventListener('visibilitychange', visible); controller.dispose();
    };
  }, [controller]);
  useEffect(() => { document.documentElement.dataset.theme = state.theme; }, [state.theme]);
  useEffect(() => { document.documentElement.dataset.skin = state.skin; }, [state.skin]);
  // 글꼴은 CSS 변수로 걸고, 열려 있는 터미널에는 이벤트로 알린다. 화면 배율은 웹뷰 확대로 맞춘다.
  const { uiFont, terminalFont, scale } = state.typography;
  useEffect(() => {
    applyTypographyVars(document.documentElement, { scale, uiFont, terminalFont });
    window.dispatchEvent(new Event(TYPOGRAPHY_EVENT));
  }, [uiFont, terminalFont, scale]);
  useEffect(() => { controller.applyScale(scale); }, [controller, scale]);
  // 아젠다의 저장소 폴더(GitHub 전용 항목은 앱이 받는 자리)를 풀려면 연결 상태가 필요하다. 호스트가 있을 때만 묻는다.
  useEffect(() => { if (controller.isNative()) void refreshGithub(controller.callHost); }, [controller]);
  // 알림: 처음에 목록과 설정을 읽고, 호스트가 알려 주는 사건을 받는다. 토스트는 메인 창만 띄운다(노트 창은 목록만 따라간다).
  useEffect(() => {
    if (!controller.isNative()) return;
    void refreshNotifications(controller.callHost);
    let unlisten: (() => void) | undefined, gone = false;
    void controller.listenNotifications(event => {
      applyChanged(event, context.kind === 'main');
      // 승인을 기다리는 변경이 알려지면 레일의 배지도 바로 맞춘다(#53)
      if (toItem(event.pushed)?.kind === 'approval') void refreshApprovalCounts(controller.callEngine);
    })?.then(fn => { if (gone) fn(); else unlisten = fn; });
    return () => { gone = true; unlisten?.(); };
  }, [controller]);
  // 러너: 처음에 상태를 읽고, 호스트가 알려 주는 사건(집었다, 질문했다, 끝났다)마다 다시 읽는다. 메인 창만 맡는다.
  // vault가 바뀌면 러너 상태도 그 vault의 것으로 다시 읽는다.
  const vaultPath = state.workspace?.path;
  useEffect(() => {
    if (!controller.isNative() || context.kind !== 'main' || !vaultPath) return;
    void refreshRunner(controller.callHost);
    let unlisten: (() => void) | undefined, gone = false;
    void controller.listenRunner(() => { void runnerChanged(controller.callHost); })?.then(fn => { if (gone) fn(); else unlisten = fn; });
    return () => { gone = true; unlisten?.(); };
  }, [controller, vaultPath]);
  // 승인이 필요한 스킬·에이전트 변경의 수(#53): 처음에 읽고, 창이 눈에 띌 때와 일정한 간격으로 다시 읽는다. 메인 창만 맡는다. 샘플에는 엔진이 없다.
  const sample = state.workspace?.sample;
  useEffect(() => {
    resetApprovalsStore();
    if (context.kind !== 'main' || !vaultPath || sample) return;
    return startApprovalsWatch(controller.callEngine, window);
  }, [controller, vaultPath, sample]);
  if (state.booting) return <main className="status-screen" role="status"><Brand state="thinking" />
    <p className="ais-bootsplash-msg">{context.kind === 'note' ? '노트를 여는 중…' : '워크스페이스를 여는 중…'}</p></main>;
  if (state.fatal) {
    if (context.kind === 'note') {
      return (
        <main className="status-screen note-window tabwin">
          <div className="doc-route-empty-card">
            <h2>노트를 열지 못했어요</h2>
            <p role="alert">{state.fatal}</p>
          </div>
        </main>
      );
    }
    return <main className="status-screen"><Brand state="error" /><p role="alert">{state.fatal}</p></main>;
  }
  if (context.kind === 'note') {
    const note = state.workspace?.notes.find(n => n.id === context.docId);
    const preview = state.workspace?.notes.find(n => n.id === state.preview);
    return <main className="note-window tabwin">
      <VaultBanner state={state} dismiss={() => controller.data.dismissFailures()} />
      {note ? <NoteScreen note={note} controller={controller} /> : (
        <div className="doc-route-empty-card">
          <h2>이 노트를 찾을 수 없어요</h2>
          <p>삭제됐거나 옮겨졌을 수 있어요. 이 창을 닫고 메인 창에서 다시 찾아보세요.</p>
        </div>
      )}
      {preview && <Preview key={preview.id} controller={controller} state={state} />}
      <InteractionOverlays controller={controller} state={state} />
      {state.noteCloseDialog && (
        <Modal
          title="창을 닫으면 사라지는 것이 있어요"
          close={() => controller.dismissNoteCloseDialog()}
        >
          <ul className="note-window-dialog-list">
            {state.noteCloseDialog.reasons.map((r, i) => <li key={i}>{r}</li>)}
          </ul>
          <footer>
            <button type="button" onClick={() => controller.dismissNoteCloseDialog()}>남겨 두기</button>
            <button type="button" className="btn-danger" onClick={() => void controller.forceCloseNoteWindow()}>그래도 닫기</button>
          </footer>
        </Modal>
      )}
    </main>;
  }
  return <>
    {!state.workspace || state.route.split('?')[0] === '/launcher'
      ? <Launcher controller={controller} state={state} /> : <WorkspaceShell controller={controller} state={state} />}
    {state.notice && <NoticeBox notice={state.notice} onClose={controller.clearNotice} />}
    {state.workspace && <NotificationToasts controller={controller} />}
  </>;
}

function NoticeBox({ notice, onClose }: { notice: string; onClose: () => void }) {
  const colonIndex = notice.indexOf(': ');
  const hasDetails = colonIndex !== -1 && notice.length > colonIndex + 2;
  const summaryText = hasDetails ? notice.slice(0, colonIndex) : notice;
  const detailText = hasDetails ? notice.slice(colonIndex + 2) : null;

  return (
    <div className="notice" role="status">
      <div className="notice-inner">
        <div className="notice-body">
          <span>{summaryText}</span>
          {detailText && (
            <details className="notice-details">
              <summary>자세히</summary>
              <pre>{detailText}</pre>
            </details>
          )}
        </div>
        <button type="button" className="notice-close" onClick={onClose} aria-label="알림 닫기">
          ×
        </button>
      </div>
    </div>
  );
}

function Brand({ state = 'idle' }: { state?: BrandState }) { return <div className="brand" role="img" aria-label="aisurada"><BrandMark state={state} /></div>; }

function Launcher({ controller, state }: { controller: WorkspaceController; state: WorkspaceState }) {
  const [mode, setMode] = useState<'create' | 'open' | null>(() => new URLSearchParams(state.route.split('?')[1]).get('mode') === 'create' ? 'create' : null);
  const [name, setName] = useState('');
  const [path, setPath] = useState('');
  useEffect(() => {
    if (new URLSearchParams(state.route.split('?')[1]).get('mode') === 'create') setMode('create');
  }, [state.route]);
  const choose = async () => {
    const selected = await controller.chooseDirectory(mode === 'create');
    if (selected && mode === 'open') setPath(selected);
  };
  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (mode === 'create') void controller.create(name); else void controller.open(path);
  };
  return <main className="launcher">
    <section className="launcher-intro"><Brand /><p className="eyebrow">YOUR WORK, IN CONTEXT</p>
      <h1>작업의 흐름을<br />기록으로 이어가세요.</h1>
      <p className="intro-copy">오늘의 생각과 작업을 담고,<br />다음 걸음을 계획하는 나만의 공간.</p>
      <div className="intro-footnote"><span aria-hidden="true">↳</span> 기록은 내 컴퓨터의 폴더에 남아요.</div>
    </section>
    <section className="launcher-content" aria-label="워크스페이스 시작">
      <header><p className="eyebrow">WORKSPACE</p><h2>어디서 시작할까요?</h2><p className="muted">새 공간을 만들거나, 이어서 작업하세요.</p></header>
      <div className="launcher-actions">
        <button className={`workspace-option ${mode === 'create' ? 'selected' : ''}`} onClick={() => setMode(mode === 'create' ? null : 'create')} aria-expanded={mode === 'create'} disabled={state.busy}>
          <span className="option-symbol" aria-hidden="true">＋</span><span><strong>새 워크스페이스</strong><small>기록을 모을 새 폴더를 만들어요</small></span><span aria-hidden="true">↗</span></button>
        <button className={`workspace-option ${mode === 'open' ? 'selected' : ''}`} onClick={() => setMode(mode === 'open' ? null : 'open')} aria-expanded={mode === 'open'} disabled={state.busy}>
          <span className="option-symbol" aria-hidden="true">▱</span><span><strong>폴더 열기</strong><small>내 컴퓨터의 작업 공간을 열어요</small></span><span aria-hidden="true">↗</span></button>
      </div>
      {mode && <form className="workspace-form" onSubmit={submit} aria-label={mode === 'create' ? '워크스페이스 만들기' : '워크스페이스 열기'}>
        {mode === 'create' && <label>이름<input name="name" value={name} onChange={e => setName(e.target.value)} placeholder="나의 작업 기록" autoFocus disabled={state.busy} required /></label>}
        <label>{mode === 'create' ? '저장 위치' : '폴더 경로'}<span className="path-input">
          <input name={mode === 'create' ? 'parent' : 'path'} value={mode === 'create' ? state.home : path}
            onChange={e => mode === 'create' ? controller.setHome(e.target.value) : setPath(e.target.value)}
            placeholder="/Users/me/Documents/aisurada" autoFocus={mode === 'open'} disabled={state.busy} required />
          <button type="button" className="button-subtle" onClick={() => void choose()} disabled={state.busy}>찾아보기</button>
        </span></label>
        <button className="button-primary" type="submit" disabled={state.busy || !(mode === 'create' ? name.trim() && state.home : path.trim())}>
          {state.busy ? '워크스페이스를 여는 중…' : mode === 'create' ? '만들기' : '열기'}</button>
      </form>}
      {state.workspace && (
        <button
          type="button"
          className="sample-button"
          style={{ marginBottom: 8 }}
          onClick={() => controller.navigate('/calendar')}
          disabled={state.busy}
        >
          열려 있는 워크스페이스로 돌아가기 <span aria-hidden="true">→</span>
        </button>
      )}
      <button className="sample-button" onClick={() => void controller.sample()} disabled={state.busy}>샘플로 둘러보기 <span aria-hidden="true">→</span></button>
      {state.recent.length > 0 && <section className="recent-workspaces" aria-label="최근 워크스페이스"><h3>최근 워크스페이스</h3>
        <ul>{state.recent.map(item => <li key={item.path}>
          <button className="recent-open" onClick={() => void controller.openRecent(item)} disabled={state.busy}>
            <span className="recent-icon" aria-hidden="true">▱</span><span><strong>{item.name}</strong><small>{item.path}</small></span><span className="recent-time">{formatRelativeTime(item.opened)}</span>
          </button><button className="recent-remove" aria-label={`${item.name} 최근 목록에서 제거`} onClick={() => controller.forgetRecent(item.path)} disabled={state.busy}>×</button>
        </li>)}</ul>
      </section>}
    </section>
  </main>;
}
