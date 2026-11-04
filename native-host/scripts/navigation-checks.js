/* Chapter 3 acceptance driver; uses DOM and public host commands only. */
const cfg = window.__AIS_FIXTURE__;
const invoke = (...args) => window.__TAURI_INTERNALS__.invoke(...args);
const assert = (value, message) => { if (!value) throw Error(message || 'assertion failed'); };
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(predicate, timeout = 25000, label = 'navigation') {
  const end = Date.now() + timeout;
  while (Date.now() < end) { if (await predicate()) return; await wait(30); }
  const surface = document.querySelector('.doc-scroll');
  throw Error(`timed out waiting for ${label}; path=${location.pathname}; title=${document.querySelector('#screen-content h1')?.textContent}; scroll=${surface?.scrollTop}/${surface?.scrollHeight}/${surface?.clientHeight}`);
}
const post = (path, data) => fetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) });
const go = path => { history.pushState({}, '', path); dispatchEvent(new PopStateEvent('popstate')); };
const doc = id => `/doc/${encodeURIComponent(id)}`;
const file = path => `/file?path=${encodeURIComponent(path)}`;
const tabIds = () => [...document.querySelectorAll('.tab-item[data-tab-id]')].map(element => element.dataset.tabId);
const tab = id => [...document.querySelectorAll('.tab-item[data-tab-id]')].find(element => element.dataset.tabId === id);
const close = id => { assert(tab(id), 'missing tab ' + id); tab(id).querySelector('.tab-close').click(); };
const saved = () => JSON.parse(localStorage.getItem('aisurada.tabs.v1'));
const h1 = () => document.querySelector('#screen-content h1')?.textContent;
const ready = () => document.querySelector('.tab-bar');

if (location.pathname.startsWith('/note-window/')) {
  try {
    await until(() => document.querySelector('.note-body'));
    assert(!document.querySelector('.tab-bar'), 'note window has tabs');
    assert(!window.__AIS_WRITES__.length, 'note window wrote main storage');
    await post('/note-result', { ok: true });
  } catch (error) { await post('/note-result', { ok: false, error: String(error) }); }
} else {
  const reloaded = sessionStorage.getItem('navigation-reload');
  const results = reloaded ? JSON.parse(sessionStorage.getItem('navigation-results')) : [];
  const test = async (id, work) => {
    try { await work(); results.push({ id, ok: true }); }
    catch (error) { results.push({ id, ok: false, error: String(error) }); }
    await post('/progress', results.at(-1));
    if (!results.at(-1).ok) throw Error(id);
  };
  try {
    await until(ready);
    if (reloaded) {
      await test('reload-restores-only-note-tabs', async () => {
        const expected = JSON.parse(sessionStorage.getItem('navigation-expected'));
        assert(JSON.stringify(tabIds()) === JSON.stringify(expected), 'wrong restored order: ' + tabIds());
        assert(location.pathname === '/calendar', 'active route was restored');
        assert(document.querySelector('.tab-main [aria-selected="true"]'), 'main not active after reload');
        assert(document.querySelector('.calendar-controls [aria-pressed="true"]').textContent === '일', 'calendar mode was restored');
      });
    } else {
      await test('saved-tab-startup', async () => {
        assert(typeof window.__TAURI__ === 'undefined', 'global Tauri object present');
        assert(JSON.stringify(tabIds()) === JSON.stringify([cfg.id]), 'legacy transient ids not discarded');
        assert(document.querySelector('.tab-main [aria-selected="true"]'), 'saved note became active');
        assert(saved().state.workspaceRoot === cfg.raw, 'saved root spelling changed');
      });
      await test('screen-routes-and-rail', async () => {
        for (const [path, title] of [['/favorites','즐겨찾기'],['/agenda/demo','검증 아젠다'],['/connections','연결'],['/review','데일리 리포트'],
          ['/templates','템플릿'],['/trash','휴지통'],['/guide','연동 가이드'],['/session/demo','이 판에는 Claude 연동이 없어요']]) {
          go(path); await until(() => h1() === title); assert(document.querySelector('[data-rail-section="nav"]'), 'missing rail at ' + path);
        }
        go(doc(cfg.id)); await until(() => document.querySelector('.doc-scroll')); assert(!document.querySelector('.workspace-rail'), 'doc has rail');
        go('/file'); await until(() => h1() === '이 파일을 열 수 없어요'); assert(!document.querySelector('.workspace-rail'), 'file has rail');
      });
      await test('replacement-guards-preserve-workspace', async () => {
        const last = localStorage.getItem('aisurada.lastWorkspace.v1'); go('/typo'); await until(() => location.pathname === '/launcher');
        assert(localStorage.getItem('aisurada.lastWorkspace.v1') === last, 'unknown route cleared last workspace');
        go('/'); await until(() => location.pathname === '/calendar' && ready());
        go(doc('missing')); await until(() => location.pathname === doc(cfg.id));
      });
      await test('tab-click-keyboard-and-close-neighbor', async () => {
        const main = `/calendar?mode=month&date=${cfg.today}`;
        go(main); await until(() => h1() === '캘린더'); go(doc(cfg.id)); await until(() => document.querySelector('.doc-scroll'));
        go(doc(cfg.legacyId)); await until(() => tab(cfg.legacyId));
        tab(cfg.id).querySelector('.tab-button').click(); await until(() => location.pathname === doc(cfg.id));
        tab(cfg.id).querySelector('.tab-button').dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowRight', bubbles: true }));
        await until(() => location.pathname === doc(cfg.legacyId));
        tab(cfg.legacyId).querySelector('.tab-button').dispatchEvent(new KeyboardEvent('keydown', { key: 'Home', bubbles: true }));
        await until(() => location.pathname + location.search === main);
        document.querySelector('.tab-main .tab-button').dispatchEvent(new KeyboardEvent('keydown', { key: 'End', bubbles: true }));
        await until(() => location.pathname === doc(cfg.legacyId));
        tab(cfg.legacyId).querySelector('.tab-button').dispatchEvent(new KeyboardEvent('keydown', { key: 'Delete', bubbles: true }));
        await until(() => location.pathname === doc(cfg.id) && !tab(cfg.legacyId));
        go(doc(cfg.legacyId)); await until(() => tab(cfg.legacyId));
      });
      await test('external-file-tab-and-persistence', async () => {
        go(file(cfg.external)); await until(() => document.querySelector('.ext-md'));
        assert(document.querySelector('.ext-md').textContent.includes('별도의 Markdown 본문'), 'external body missing');
        assert(tab('ext:' + cfg.external), 'file tab missing');
        assert(saved().state.openIds.every(id => !id.startsWith('ext:')), 'external tab persisted');
        const before = location.pathname + location.search; close(cfg.id); await until(() => !tab(cfg.id));
        assert(location.pathname + location.search === before, 'nonactive close navigated');
        close('ext:' + cfg.external); await until(() => location.pathname === doc(cfg.legacyId));
        go(doc(cfg.id)); await until(() => tab(cfg.id));
      });
      await test('terminal-panels-survive-navigation', async () => {
        go('/terminal'); await until(() => document.querySelector('[data-terminal-key="term:main"]'));
        const panel = document.querySelector('[data-terminal-key="term:main"]');
        go(doc(cfg.id)); await until(() => document.querySelector('.doc-scroll'));
        assert(document.querySelector('[data-terminal-key="term:main"]') === panel && panel.hidden, 'terminal panel remounted');
        go('/terminal/session'); await until(() => tab('term:session'));
        assert(document.querySelector('.tab-main [aria-selected="true"]'), 'session terminal compatibility changed');
        close('term:session'); await until(() => !tab('term:session'));
        assert(location.pathname === '/terminal/session', 'inactive session close navigated');
        close('term:main'); go('/calendar'); await until(() => h1() === '캘린더');
      });
      await test('xterm-terminal-starts-a-shell-and-the-pin-dialog-reads-its-logical-lines', async () => {
        go('/terminal'); await until(() => document.querySelector('[data-terminal-key="term:main"] .xterm .xterm-screen'), 40000, 'xterm mounted');
        const bar = () => document.querySelector('[data-terminal-key="term:main"] .term-bar-btn');
        await until(() => bar() && !bar().disabled, 40000, 'shell started');
        for (const n of [1, 2, 3, 4, 5]) await invoke('pty_write', { key: 'term:main', data: `echo ais-xterm-marker-${n}\r` });
        const hinted = () => Number(/(\d+)줄이에요/.exec(document.querySelector('.pin-hint')?.textContent || '')?.[1] || 0);
        let lines = 0;
        await until(async () => {
          document.querySelector('.pin-hint') ? document.querySelector('.pin-modal .btn-secondary')?.click() : 0;
          await wait(300); bar().click(); await until(() => document.querySelector('.pin-hint'), 5000, 'pin dialog');
          lines = hinted(); return lines >= 10;
        }, 40000, 'shell output in the logical lines');
        assert(document.querySelector('.pin-selection-note')?.textContent.includes('드래그'), 'no reason for the disabled selection button');
        document.querySelector('.pin-modal .btn-secondary').click(); await until(() => !document.querySelector('.pin-modal'));
        close('term:main'); go('/calendar'); await until(() => h1() === '캘린더');
      });
      await test('agenda-query-and-template-initial-open', async () => {
        go(`/agenda/demo?open=${cfg.id},${cfg.legacyId}`); await until(() => document.querySelectorAll('.agenda-note').length === 2);
        assert(document.querySelector('.tab-main [aria-selected="true"]'), 'agenda pane made a note tab active');
        go('/templates?open=one'); await until(() => document.querySelector('.template-editor .lex-content')?.textContent.includes('본문 one'));
        go('/templates?open=two'); await until(() => location.search === '?open=two');
        assert(document.querySelector('.template-editor .lex-content').textContent.includes('본문 one'), 'template query was reread without leaving page');
        go('/calendar'); await until(() => h1() === '캘린더'); go('/templates?open=two');
        await until(() => document.querySelector('.template-editor .lex-content')?.textContent.includes('본문 two'));
      });
      await test('floating-note-removes-main-tab', async () => {
        go(doc(cfg.id)); await until(() => document.querySelector('.doc-scroll'));
        await invoke('open_note_window', { docId: cfg.id, title: '검증 노트' });
        let note; await until(async () => { note = await (await fetch('/note-result')).json(); return note; }); assert(note.ok, note.error);
        await until(() => !tab(cfg.id) && location.pathname !== doc(cfg.id));
        go(doc(cfg.id)); await until(() => location.pathname !== doc(cfg.id)); assert(!tab(cfg.id), 'floating note tab reopened');
        await invoke('close_note_windows'); await until(async () => !(await invoke('note_windows')).length);
        await until(() => { go(doc(cfg.id)); return !!tab(cfg.id); });
      });
      await test('scroll-restoration-and-user-release', async () => {
        go(doc(cfg.id)); await until(() => document.querySelector('.doc-scroll'));
        let surface = document.querySelector('.doc-scroll'); assert(surface.scrollHeight > surface.clientHeight + 700, 'fixture too short');
        surface.dispatchEvent(new WheelEvent('wheel')); surface.scrollTop = 600; surface.dispatchEvent(new Event('scroll'));
        go(doc(cfg.legacyId)); await until(() => h1() === '옛 노트'); go(doc(cfg.id));
        await until(() => document.querySelector('.doc-scroll')?.scrollTop === 600, 10000, 'restore 600'); surface = document.querySelector('.doc-scroll');
        surface.scrollTop = 20; surface.dispatchEvent(new Event('scroll')); await until(() => surface.scrollTop === 600, 10000, 'hold 600');
        surface.dispatchEvent(new WheelEvent('wheel')); surface.scrollTop = 40; surface.dispatchEvent(new Event('scroll'));
        go(doc(cfg.legacyId)); await until(() => h1() === '옛 노트'); go(doc(cfg.id));
        await until(() => document.querySelector('.doc-scroll')?.scrollTop === 40, 10000, 'restore user 40');
      });
      // A full page load must retain only the persisted note ids and their order.
      go('/terminal'); await until(() => tab('term:main'));
      sessionStorage.setItem('navigation-results', JSON.stringify(results));
      sessionStorage.setItem('navigation-expected', JSON.stringify(saved().state.openIds));
      sessionStorage.setItem('navigation-reload', 'yes'); location.assign('/');
    }
  } catch { /* A failing dependent step is already recorded. */ }
  if (reloaded || results.some(result => !result.ok)) await post('/result', results);
}
