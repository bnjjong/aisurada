/* Injected by the isolated host harness; never shipped with the app. */
const cfg = window.__AIS_FIXTURE__;
const invoke = (...args) => window.__TAURI_INTERNALS__.invoke(...args);
const assert = (value, message) => { if (!value) throw Error(message || 'assertion failed'); };
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(predicate, label = 'interaction') {
  const end = Date.now() + 25000;
  while (Date.now() < end) { if (await predicate()) return; await wait(30); }
  throw Error(`timed out: ${label}; path=${location.pathname}; title=${document.querySelector('#screen-content h1')?.textContent}`);
}
const post = (path, data) => fetch(path, {method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(data)});
const menu = (payload, label = 'main') => invoke('plugin:event|emit_to', {target:{kind:'AnyLabel',label},event:'ais://menu',payload});
const links = payload => invoke('plugin:event|emit', {event:'deep-link://new-url',payload});
const link = (id, ws = cfg.raw, extra = '') => `aisurada://doc/${encodeURIComponent(id)}?ws=${encodeURIComponent(ws)}${extra}`;
const file = path => 'file://' + path.split('/').map(encodeURIComponent).join('/');
const go = path => {history.pushState({},'',path);dispatchEvent(new PopStateEvent('popstate'));};
const doc = id => `/doc/${encodeURIComponent(id)}`;
const tab = id => [...document.querySelectorAll('.tab-item[data-tab-id]')].find(item=>item.dataset.tabId===id);
const press = options => dispatchEvent(new KeyboardEvent('keydown',{metaKey:true,bubbles:true,cancelable:true,...options}));

if(location.pathname.startsWith('/note-window/')) {
  try { await until(()=>document.querySelector('.note-body')); assert(!document.querySelector('.tab-bar'),'note has tabs'); assert(!window.__AIS_WRITES__.length,'note wrote main storage'); await post('/note-result',{ok:true}); }
  catch(error){await post('/note-result',{ok:false,error:String(error)});}
} else {
  const results=[];
  const test=async(id,work)=>{try{await work();results.push({id,ok:true});}catch(error){results.push({id,ok:false,error:String(error)});}await post('/progress',results.at(-1));if(!results.at(-1).ok)throw Error(id);};
  try {
    await until(()=>document.querySelector('[data-rail-section="nav"]'));
    await test('main-menu-terminal-and-close',async()=>{
      await menu('ais_open_terminal');await until(()=>tab('term:main'));
      await menu('ais_close_tab');await until(()=>!tab('term:main'));
      await menu('ais_close_tab');assert(document.querySelector('.tab-main'),'main frame disappeared');
    });
    await test('global-shortcuts-and-search',async()=>{
      go(doc(cfg.id));await until(()=>tab(cfg.id));go(doc(cfg.legacyId));await until(()=>tab(cfg.legacyId));
      press({key:'!',code:'Digit1'});await until(()=>location.pathname==='/calendar');
      press({key:'}',code:'BracketRight',shiftKey:true});await until(()=>location.pathname===doc(cfg.id));
      press({key:'9',code:'Numpad9'});await until(()=>location.pathname===doc(cfg.legacyId));
      press({key:'w',code:'KeyW'});assert(tab(cfg.legacyId),'native Cmd+W handled twice');
      press({key:'k',code:'KeyK'});await until(()=>document.querySelector('.cmdk-input'));
      assert(document.activeElement===document.querySelector('.cmdk-input'),'search not focused');
      document.querySelector('.cmdk-input').dispatchEvent(new KeyboardEvent('keydown',{key:'Escape',bubbles:true,cancelable:true}));
      await until(()=>!document.querySelector('.cmdk-input'));assert(location.pathname===doc(cfg.legacyId),'search changed route');
    });
    await test('window-link-and-window-scoped-close',async()=>{
      await links([link(cfg.id,cfg.raw,'&window=1')]);
      await until(async()=> (await invoke('note_windows')).includes(cfg.id));
      let note;await until(async()=>{note=await(await fetch('/note-result')).json();return note;});assert(note.ok,note.error);
      const before=location.pathname;assert(tab(cfg.legacyId),'main tab missing before close');
      await menu('ais_new_workspace','note-'+cfg.id);assert(location.pathname===before,'note accepted main-only menu');
      await menu('ais_close_tab','note-'+cfg.id);
      await until(async()=>!(await invoke('note_windows')).length,'close target note window');
      assert(tab(cfg.legacyId)&&location.pathname===before,'note Cmd+W also closed main tab');
    });
    await test('live-links-select-last-valid-url',async()=>{
      await links([link(cfg.id),'unsupported://ignored',link(cfg.legacyId),'not a link']);await until(()=>location.pathname===doc(cfg.legacyId));
      await links([`AISURADA://AGENDA/demo?ws=${encodeURIComponent(cfg.raw)}`]);await until(()=>location.pathname==='/agenda/demo');
      assert(document.querySelector('#screen-content h1').textContent==='검증 아젠다','agenda slug failed');
    });
    await test('markdown-note-and-child-chain',async()=>{
      const sub=(...ids)=>'/sub/'+ids.map(encodeURIComponent).join('/'), grandId='snfixture-00000002';
      await links([file(cfg.rootNote)]);await until(()=>location.pathname===doc(cfg.id));
      await links([file(cfg.childNote)]);await until(()=>location.pathname===sub(cfg.id,cfg.childId)&&document.querySelector('.note-body')?.textContent.includes('하위 Markdown 파일의 본문'));
      assert(!document.querySelector('.subnote-page'),'child file opened a modal in the main window');
      assert(tab(cfg.id)&&tab(`sub:${cfg.id}/${cfg.childId}`),'root and child tabs are not both open');
      await links([file(cfg.grandchildNote)]);await until(()=>location.pathname===sub(cfg.id,cfg.childId,grandId)&&document.querySelector('.note-body')?.textContent.includes('두 단계 사슬의 본문'));
      assert(tab(`sub:${cfg.id}/${cfg.childId}/${grandId}`),'grandchild has no tab');
      press({key:'Escape'});await wait(150);assert(location.pathname===sub(cfg.id,cfg.childId,grandId),'Escape closed a follow-up tab');
      for(const id of [`sub:${cfg.id}/${cfg.childId}/${grandId}`,`sub:${cfg.id}/${cfg.childId}`]){tab(id).querySelector('.tabx-close').click();await until(()=>!tab(id),'close '+id);}
    });
    await test('external-markdown-through-the-shared-gate',async()=>{
      await links([file(cfg.external)]);await until(()=>document.querySelector('.ext-md'));
      assert(document.querySelector('.ext-md').textContent.includes('별도의 Markdown 본문'),'external body missing');
    });
    await test('same-folder-different-spelling-reopens-workspace',async()=>{
      const alias=cfg.raw+'/';await links([link(cfg.legacyId,alias)]);
      await until(()=>JSON.parse(localStorage.getItem('aisurada.lastWorkspace.v1')).path===alias&&location.pathname===doc(cfg.legacyId));
      assert(await invoke('vault_root')===alias,'host path normalized');
    });
    await test('non-workspace-links-do-not-change-host-root',async()=>{
      const before=await invoke('vault_root');await links([link(cfg.id,cfg.parent+'/not-a-workspace')]);await until(()=>location.pathname==='/launcher');
      assert(await invoke('vault_root')===before,'invalid workspace changed scope root');
      await links([link(cfg.legacyId,before)]);await until(()=>location.pathname===doc(cfg.legacyId));
    });
    await test('new-workspace-keeps-vault-and-disables-tab-menus',async()=>{
      const before=localStorage.getItem('aisurada.lastWorkspace.v1');await menu('ais_new_workspace');await until(()=>location.pathname+location.search==='/launcher?mode=create');
      await menu('ais_open_terminal');await menu('ais_close_tab');assert(location.pathname==='/launcher','tab menu ran in launcher');
      assert(localStorage.getItem('aisurada.lastWorkspace.v1')===before,'new-workspace menu cleared vault');
      press({key:'k',code:'KeyK'});assert(!document.querySelector('.cmdk-panel'),'launcher accepted Cmd+K');
      await links([link(cfg.legacyId,await invoke('vault_root'))]);await until(()=>location.pathname===doc(cfg.legacyId));
    });
    await test('welcome-and-external-file-without-vault',async()=>{
      const root=await invoke('vault_root');await menu('ais_welcome');await until(()=>location.pathname==='/launcher'&&localStorage.getItem('aisurada.lastWorkspace.v1')==='null');
      assert(await invoke('vault_root')===root,'welcome changed host root');
      await links([file(cfg.external)]);await until(()=>document.querySelector('.notice')?.textContent.includes('워크스페이스가 필요'));
      assert(location.pathname==='/launcher'&&!document.querySelector('.ext-md'),'external file bypassed workspace gate');
    });
  }catch{/* Stop after a dependent failure. */}
  await post('/result',results);
}
