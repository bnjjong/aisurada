// Runs only in the isolated macOS acceptance fixture, never in the product.
const cfg=window.__WL_FIXTURE__, invoke=(...args)=>window.__TAURI_INTERNALS__.invoke(...args);
const pause=ms=>new Promise(r=>setTimeout(r,ms));
const assert=(yes,message)=>{if(!yes)throw Error(message)};
const post=(path,data)=>fetch(path,{method:'POST',headers:{'Content-Type':'application/json'},body:JSON.stringify(data)});
async function until(fn,label,timeout=20000){const end=Date.now()+timeout;while(Date.now()<end){if(await fn())return;await pause(40)}throw Error('timeout: '+label)}
const engine=async argv=>{const r=await invoke('sidecar_call',{argv});assert(r.status===0,`${argv[0]} status=${r.status} ${r.stderr}`);return JSON.parse(r.stdout)};
const go=path=>{history.pushState({},'',path);dispatchEvent(new PopStateEvent('popstate'))};
const content=()=>document.querySelector('#screen-content .lex-content');
function edit(text,suffix='왕복 사례 끝',root=content()){
 const walker=document.createTreeWalker(root,NodeFilter.SHOW_TEXT);let node;
 while(node=walker.nextNode())if(node.textContent.includes(suffix))break;
 assert(node,'text target absent '+suffix);root.focus({preventScroll:true});const range=document.createRange();range.setStart(node,node.textContent.indexOf(suffix)+suffix.length);range.collapse(true);const sel=getSelection();sel.removeAllRanges();sel.addRange(range);assert(document.execCommand('insertText',false,text),'insertText failed');
}
async function open(c){go('/doc/'+c.noteId);await until(()=>content()?.querySelector('h1')?.textContent===`왕복 ${c.id}`&&content().textContent.includes('왕복 사례 끝'),'open '+c.id)}
const results=[];
async function test(id,work){try{await work();results.push({id,ok:true})}catch(error){results.push({id,ok:false,error:String(error)})}await post('/progress',results.at(-1));if(!results.at(-1).ok)throw Error(id)}
try{
 await until(()=>document.querySelector('.tab-bar'),'boot');
 const cases=await fetch('/editor-cases').then(r=>r.json());
 await test('engine-preserves-seeded-body-bytes',async()=>{for(const c of cases){const n=await engine(['get-doc',c.noteId]);assert(n.body===c.original,'seed bytes changed '+c.id);c.token=n.precondition}});
 let saves=0;
 for(const c of cases)await test('native-roundtrip-'+c.id,async()=>{
  await open(c);await pause(1500);const n=await engine(['get-doc',c.noteId]);assert(n.precondition===c.token,'open wrote '+c.id);assert(!content().contains(document.activeElement),'open stole focus '+c.id);
  edit('편집됨');let saved;await until(async()=>{saved=await engine(['get-doc',c.noteId]);return saved.body.includes('왕복 사례 끝편집됨')},'save '+c.id);
  const want=`# 왕복 ${c.id}\n\n${c.expected}\n\n왕복 사례 끝편집됨`;assert(saved.body===want,`${c.id} expected=${JSON.stringify(want)} got=${JSON.stringify(saved.body)}`);saves++;
  if(c.second!==undefined){go('/trash');await until(()=>!content(),'unmount');await open(c);await pause(400);edit('다시','왕복 사례 끝편집됨');await until(async()=>{saved=await engine(['get-doc',c.noteId]);return saved.body.endsWith('편집됨다시')},'second '+c.id);assert(saved.body===`# 왕복 ${c.id}\n\n${c.second}\n\n왕복 사례 끝편집됨다시`,'second bytes '+c.id+' '+JSON.stringify(saved.body));saves++}
 });
 await test('body-write-count-has-no-open-or-repair-writes',async()=>{const calls=await fetch('/calls').then(r=>r.json());const writes=calls.filter(c=>c.args.argv[0]==='update-doc'&&c.args.argv.some(a=>a.startsWith('body=')));assert(writes.length===saves,`expected ${saves} body writes, got ${writes.length}`)});
 const c=cases[0];await open(c);
 await test('native-toolbar-table-and-ten-menu-actions',async()=>{
  edit('추가');document.querySelector('[title="표 삽입"]').click();await until(()=>content().querySelector('table'),'table');assert(content().querySelectorAll('table tr').length===3,'rows');assert(content().querySelectorAll('table th').length===3,'header');
  content().querySelector('td,th').dispatchEvent(new MouseEvent('mouseover',{bubbles:true}));await until(()=>document.querySelector('.lex-cell-menu-btn'),'cell hover');document.querySelector('.lex-cell-menu-btn').click();await until(()=>document.querySelector('.lex-table-menu'),'table menu');assert(document.querySelectorAll('.lex-table-menu button').length===10,'menu order/count');
  document.querySelector('.lex-table-menu button').click();await until(()=>content().querySelectorAll('table tr').length===4,'insert row');
 });
 await test('plain-text-paste-keeps-markdown-literal',async()=>{
  edit('貼');const data=new DataTransfer();data.setData('text/plain','**그대로**');content().dispatchEvent(new ClipboardEvent('paste',{bubbles:true,cancelable:true,clipboardData:data}));await until(()=>content().textContent.includes('**그대로**'),'plain paste');assert(![...content().querySelectorAll('strong')].some(n=>n.textContent==='그대로'),'plain text became bold');
 });
 await test('image-paste-writes-native-bytes-before-inserting-reference',async()=>{
  edit('첨부');const bytes=Uint8Array.from(atob('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a6n8AAAAASUVORK5CYII='),c=>c.charCodeAt(0));const data=new DataTransfer();data.items.add(new File([bytes],'테스트.png',{type:'image/png'}));content().dispatchEvent(new ClipboardEvent('paste',{bubbles:true,cancelable:true,clipboardData:data}));await until(()=>content().querySelector('.lex-image img'),'image reference');assert(content().querySelector('.lex-image img').getAttribute('src').includes('img-'),'attachment name');
 });
 await test('selection-comment-wraps-before-engine-thread',async()=>{
  const root=content(),node=root.querySelector('h1').firstChild.firstChild;root.focus();const range=document.createRange();range.setStart(node,0);range.setEnd(node,2);getSelection().removeAllRanges();getSelection().addRange(range);document.dispatchEvent(new Event('selectionchange'));await until(()=>document.querySelector('.sel-bar .comment-add-btn'),'selection bar');document.querySelector('.comment-add-btn').click();await until(()=>document.querySelector('.comment-compose-popover textarea'),'composer');const textarea=document.querySelector('.comment-compose-popover textarea'),setter=Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype,'value').set;setter.call(textarea,'실제 코멘트');textarea.dispatchEvent(new Event('input',{bubbles:true}));await until(()=>!document.querySelector('.link-popover-btn.primary').disabled,'comment enabled');document.querySelector('.link-popover-btn.primary').click();await until(()=>content().querySelector('[data-comment-id]'),'annotation');await until(()=>document.querySelector('.note-comment-thread')?.textContent.includes('실제 코멘트'),'engine thread');
 });
 await test('table-copy-preserves-html-and-strips-markdown-annotations',async()=>{
  const root=content(),table=root.querySelector('table');root.focus();const r=document.createRange();r.selectNodeContents(table);getSelection().removeAllRanges();getSelection().addRange(r);document.dispatchEvent(new Event('selectionchange'));await pause(60);
  const clipboard=new DataTransfer();root.dispatchEvent(new ClipboardEvent('copy',{bubbles:true,cancelable:true,clipboardData:clipboard}));assert(clipboard.getData('text/plain').includes('| --- |'),'copy lacks GFM divider');assert(clipboard.getData('text/html').includes('<table'),'copy lost HTML');assert(!clipboard.getData('text/plain').includes('<!--wlc:'),'copy leaked annotations');
 });
 await test('table-pair-resize-keeps-total-and-persists-ratio',async()=>{
  const table=content().querySelector('table');table.querySelector('th,td').dispatchEvent(new MouseEvent('mouseover',{bubbles:true}));await until(()=>document.querySelector('.lex-col-grip'),'resize handle');const before=[...table.querySelectorAll('tr:first-of-type>*')].map(cell=>cell.getBoundingClientRect().width),grip=document.querySelector('.lex-col-grip');grip.dispatchEvent(new PointerEvent('pointerdown',{bubbles:true,clientX:100}));document.dispatchEvent(new PointerEvent('pointermove',{bubbles:true,clientX:160}));document.dispatchEvent(new PointerEvent('pointerup',{bubbles:true}));await pause(100);const after=[...table.querySelectorAll('tr:first-of-type>*')].map(cell=>cell.getBoundingClientRect().width);assert(Math.abs(before.reduce((a,b)=>a+b,0)-after.reduce((a,b)=>a+b,0))<2,'pair resize changed total');assert(after.every(width=>width>=47),'column below minimum');assert(after[0]>before[0]+40,'resize did not move boundary');
 });
 await test('html-frame-sandbox-height-source-and-hidden-ask',async()=>{
  go('/doc/'+cfg.id);await until(()=>content()?.querySelector('iframe'),'HTML frame');const frame=content().querySelector('iframe');assert(frame.getAttribute('sandbox')==='allow-scripts','sandbox');assert(frame.getAttribute('referrerpolicy')==='no-referrer','referrer');assert(frame.getAttribute('loading')==='lazy','lazy');assert(frame.src.startsWith('wlframe://localhost/'),'protocol');assert(document.querySelector('.wl-html-frame-ask').hidden,'ask visible');
  await pause(150);const initial=frame.style.height;dispatchEvent(new MessageEvent('message',{source:window,data:{wlframeHeight:999}}));await pause(30);assert(frame.style.height===initial,'accepted foreign window');dispatchEvent(new MessageEvent('message',{source:frame.contentWindow,data:{wlframeHeight:400.1}}));await until(()=>frame.style.height==='401px','height rounding');dispatchEvent(new MessageEvent('message',{source:frame.contentWindow,data:{wlframeHeight:9999}}));await until(()=>frame.style.height==='1600px','height clamp');
 });
 await test('mermaid-renders-svg-labels-outside-editable',async()=>{await until(()=>document.querySelector('.mermaid-svg svg text'),'Mermaid labels',30000);const svg=document.querySelector('.mermaid-svg svg');assert(!content().contains(svg),'preview inside editable');assert(svg.textContent.includes('A')&&svg.textContent.includes('B'),'missing labels');assert(!svg.querySelector('foreignObject,image,a'),'unsafe SVG nodes');assert(content().querySelector('.lex-code').classList.contains('wl-mermaid-source-folded'),'not collapsed by default');});
 await test('child-overlay-edits-through-update-sub-note',async()=>{await until(()=>document.querySelector('.child-note-list button'),'child navigation');document.querySelector('.child-note-list button').click();await until(()=>document.querySelector('.subnote-page .lex-content h1')?.textContent==='하위 파일 노트','child editor');const root=document.querySelector('.subnote-page .lex-content');edit('편집검증','하위 Markdown 파일의 본문이에요.',root);await until(async()=>{const n=await engine(['get-sub-note',cfg.id,cfg.childId]);return n.body.includes('편집검증')},'child engine write');document.querySelector('[aria-label="하위 페이지 닫기"]').click();await until(()=>!document.querySelector('.subnote-page'),'child close');});
}catch(error){if(!results.some(result=>!result.ok))results.push({id:'editor-driver',ok:false,error:String(error)});console.error(error)}
await post('/result',results);
