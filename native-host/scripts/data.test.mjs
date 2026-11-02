import test from 'node:test';
import assert from 'node:assert/strict';
import { VaultData, relations } from '../src/data/vault.ts';
import { read, write, cleanTags, commentId, commitOutcome } from '../src/data/commands.ts';
import { SelfWrites } from '../src/data/selfWrites.ts';
import { WriteQueues } from '../src/data/writeQueues.ts';
import { engineCall, engineRefusal, SidecarCommandError, readWorkspace } from '../src/workspace/engine.ts';
import { sampleWorkspace } from '../src/workspace/sample.ts';
import { TITLE_MISMATCH_HEAD } from '../src/data/titleWarning.ts';
import { readFileSync } from 'node:fs';

const copy = value => JSON.parse(JSON.stringify(value));
const tick = () => new Promise(resolve => setImmediate(resolve));
const deferred = () => { let resolve; const promise = new Promise(r => { resolve = r; }); return { resolve, promise }; };
const ok = value => ({ status: 0, stdout: JSON.stringify(value), stderr: '' });
const fail = status => ({ status, stdout: '', stderr: '  WL-412: changed\n' });
const note = (id='a', fields={}) => ({ id, title:id, body:'# '+id+'\n\nold', agenda:'demo', agendaId:'ag', created:'2026-11-01T09:00:00+09:00', date:'2026-11-01', time:'09:00', format:'markdown', path:'notes/demo/'+id, precondition:'T0', ...fields });

function fixture(t, options={}) {
  let workspace = { ...sampleWorkspace(), sample:false, path:'/fixture', name:'fixture', agendas:[{id:'ag',slug:'demo',title:'Demo'}], notes:[note(),note('b')],
    projects:[{id:'pr',slug:'p',title:'Project'}], templates:[{id:'tp',category:'doc',name:'T'}], reports:[{id:'rv-morning-2026-11-01',body:'before',memo:'memo'}], trash:[{trashId:'tr1',file:'gone',title:'Gone'}], ...options.workspace };
  const disk = copy(workspace), calls=[], warnings=[], commits=[], values=new Map();
  const sub={a:[note('child',{created:'2026-11-01T10:00:00Z',path:'notes/demo/a/sub/child',precondition:'S0'})]};
  let now=1000, handler=options.handler;
  const services={native:true,windowLabel:'main',storage:{getItem:k=>values.get(k)||null,setItem:(k,v)=>values.set(k,v)},now:()=>new Date(now),uuid:()=> 'abcdef01-1234-5678-9876-abcdefabcdef',warn:(...args)=>warnings.push(args),
    host:{invoke:async(command,args)=>{
      calls.push({command,args:copy(args||{})}); if(command!=='sidecar_call')return;
      const argv=args.argv, override=handler ? await handler(argv) : undefined;
      if(override!==undefined)return override;
      const cmd=argv[0];
      if(cmd==='list-docs')return ok(copy(disk.notes));
      if(cmd==='list-agendas')return ok(copy(disk.agendas));
      if(cmd==='list-projects')return ok(copy(disk.projects));
      if(cmd==='list-templates')return ok(copy(disk.templates));
      if(cmd==='list-reports')return ok(copy(disk.reports));
      if(cmd==='list-trash')return ok(copy(disk.trash));
      if(cmd==='reindex')return ok({skipped:copy(disk.skipped),conflicts:copy(disk.conflicts)});
      if(cmd==='list-sub-notes')return ok({subNotes:copy(sub[argv[1]]||[])});
      if(cmd==='list-comments')return ok({state:'loaded',comments:[{id:'cmtest-1',body:'before'}]});
      if(cmd==='get-agenda-facts')return ok({id:argv[1],facts:'facts',path:'.withlog/agendas/demo.facts.md'});
      return ok({path:'notes/demo/a',precondition:'T1'});
    }}};
  const data=new VaultData(services,()=>workspace,(state,next)=>{if(next)workspace=next;commits.push({state,workspace});});
  data.reset(workspace);t.after(()=>data.dispose());
  return {data,services,disk,sub,calls,warnings,commits,values,get workspace(){return workspace;},setWorkspace:value=>{workspace=value;data.reset(value);},setHandler:value=>{handler=value;},advance:ms=>{now+=ms;},argv:()=>calls.filter(call=>call.command==='sidecar_call').map(call=>call.args.argv)};
}

test('argv preserves whitespace, newlines, quotes, equals, empty fields and places opaque tokens last',()=>{
  assert.deepEqual(write.updateDoc(' id ',{body:'a\n"b"=c',tags:[],mins:0},'opaque token'),['update-doc',' id ','body=a\n"b"=c','tags=[]','mins=0','f1=opaque token']);
  assert.deepEqual(write.updateSub('a/c','d',{title:'',body:''},''),['update-sub-note','a/c','d','title=','body=']);
  assert.deepEqual(write.createDoc({agenda:'s',type:'free',title:'',body:'',date:'d',time:'t',mins:0,tags:[],tier:'other'}),['create-doc','agenda=s','type=free','title=','body=','date=d','time=t','format=markdown','mins=0','tags=[]']);
  assert.deepEqual(read.docs(),['list-docs','limit=0','bodies=true']);
  assert.deepEqual(read.comments('root','child'),['list-comments','root','child']);
  assert.deepEqual(write.report('rv-weekly-2026-11-01','  ','\n'),['save-report','kind=weekly','date=2026-11-01']);
  assert.throws(()=>write.report('invalid','a','b'));
});
test('read/history and remaining write command surfaces retain exact positional and named cells',()=>{
  const cases=[
    [read.subNote('r/c','d'),['get-sub-note','r/c','d']],
    [read.history('a',{commit:'c=1'}),['history','a','commit=c=1']],
    [read.sessionTitles(['a','a','b']),['session-titles','a','b']],
    [read.schedule(),['schedule','status','--json']],
    [write.createSub('r/c'),['create-sub-note','r/c','title=새 후속 노트','body=# 새 후속 노트\n\n','type=free']],
    [write.createComment('r','c','cm1-a','text','quote'),['create-comment','r','c','body=text','comment_id=cm1-a','anchor_quote=quote']],
    [write.updateComment('r',undefined,'cm1-a','new'),['update-comment','r','comment_id=cm1-a','body=new']],
    [write.deleteComment('r','c','cm1-a'),['delete-comment','r','c','comment_id=cm1-a']],
    [write.link('a','b','attach',true,'T'),['link-docs','a','b','kind=attach','remove=true','f1=T']],
    [write.evaluate('a',{clear:true},'T'),['evaluate-doc','a','clear=true','f1=T']],
    [write.facts('ag',' text '),['update-agenda-facts','ag','facts=text']],
    [write.saveTemplate({id:'t',name:'',type:'free',desc:'',body:''}),['save-template','id=t','name=','type=free','desc=','body=']],
    [write.saveRitual('t',{body:''}),['save-ritual-template','t','body=']],
  ];for(const [actual,expected]of cases)assert.deepEqual(actual,expected);
});
test('normalization and comment ids preserve their bounded contracts',()=>{
  assert.deepEqual(cleanTags([' a ','a',1,'','b']),['a','b']);assert.equal(cleanTags(Array.from({length:60},(_,i)=>String(i))).length,50);
  assert.match(commentId(123456,'abcdef01-1234'),/^cm[a-z0-9]+-[a-z0-9]{1,8}$/);
  assert.deepEqual(commitOutcome({outcome:'unexpected',blocked:'unknown'}),{outcome:'failed',blocked:undefined});
});
test('bridge errors retain status and trimmed stderr while refusal commands return every nonzero status verbatim',async()=>{
  for(const status of [1,2,3,4,5,-1]){
    const host={invoke:async()=>fail(status)};
    await assert.rejects(engineCall(host,['update-doc','a']),error=>error instanceof SidecarCommandError&&error.status===status&&error.stderr==='WL-412: changed');
    assert.deepEqual(await engineRefusal(host,['delete-agenda','a']),{ok:false,reason:'WL-412: changed'});
  }
  await assert.rejects(engineCall({invoke:async()=>({status:0,stdout:'not json',stderr:''})},['list-docs']));
  await assert.rejects(engineCall({invoke:async()=>{throw 'host rejected';}},['list-docs']),e=>e==='host rejected');
});
test('initial reads derive date, agenda and report normalization without using date/time overrides',async t=>{
  const h=fixture(t);h.disk.notes[0]={...note(),created:'xT25:30',date:'WRONG',time:'WRONG'};h.disk.reports=[{id:'x',body:'<p>safe<script>bad</script></p>',memo:'  '}];
  const data=await readWorkspace(h.services.host,true,undefined,html=>html.replace(/<script>.*?<\/script>/g,''));
  assert.equal(data.notes[0].date,'xT25:30');assert.equal(data.notes[0].time,'00:00');assert.equal(data.notes[0].agendaId,'ag');
  assert.equal(data.reports[0].body,'<p>safe</p>');assert.equal(data.reports[0].memo,'');
});
test('clean reconciliation updates content and deduplicated relations in one publication',async t=>{
  const h=fixture(t);h.disk.notes=[note('a',{body:'external',precondition:'NEW',links:[{to:'a',kind:'direct'},{to:'b',kind:'direct'},{to:'b',kind:'direct'}]})];
  await h.data.reconcile();assert.equal(h.workspace.notes[0].body,'external');assert.deepEqual(h.data.getSnapshot().relations,[{from:'a',to:'b',kind:'direct'}]);
  assert.equal(h.commits.length,1);
  assert.equal(relations([note('a',{links:[{to:'b',kind:'attach'},{to:'b',kind:'direct'}]})]).length,2);
});
test('failed auxiliary reads preserve old values but a failed reindex clears diagnostic lists',async t=>{
  const h=fixture(t,{workspace:{skipped:[{path:'old'}],conflicts:[{path:'copy'}]},handler:async argv=>['list-reports','list-projects','list-agendas','list-templates','list-trash','reindex'].includes(argv[0])?fail(4):undefined});
  h.disk.notes=[note('a',{body:'new'})];await h.data.reconcile();
  for(const field of ['reports','projects','agendas','templates','trash'])assert.equal(h.workspace[field].length,1,field);
  assert.equal(h.workspace.notes[0].agendaId,'ag');assert.deepEqual(h.workspace.skipped,[]);assert.deepEqual(h.workspace.conflicts,[]);
});
test('failed note read leaves every workspace field untouched',async t=>{
  const h=fixture(t,{handler:async argv=>argv[0]==='list-docs'?fail(4):undefined}), before=h.workspace;
  h.disk.agendas=[];h.disk.reports=[];await h.data.reconcile();assert.equal(h.workspace,before);assert.equal(h.commits.length,0);
});
test('editing/unsaved notes retain memory content and token but main paths follow disk moves',async t=>{
  const h=fixture(t);h.data.edit({id:'a'},{body:'local'});h.disk.notes=[note('a',{body:'external',path:'notes/demo/moved',precondition:'NEW'})];
  await h.data.reconcile();assert.equal(h.workspace.notes[0].body,'local');assert.equal(h.workspace.notes[0].precondition,'T0');assert.equal(h.workspace.notes[0].path,'notes/demo/moved');
  assert.deepEqual(h.data.getSnapshot().conflicted,['a']);assert.equal(h.data.getSnapshot().buffers.a,'local');
});
test('local body difference with the same token is not an external conflict',async t=>{
  const h=fixture(t);h.data.edit({id:'a'},{body:'local'});await h.data.reconcile();assert.deepEqual(h.data.getSnapshot().conflicted,[]);
});
test('main deletion retains editing notes but removes notes with only unsaved status',async t=>{
  const h=fixture(t,{handler:async argv=>argv[0]==='update-doc'?fail(4):undefined});
  h.data.edit({id:'a'},{body:'local'});await h.data.updateNote({id:'b'},{body:'failed'});h.disk.notes=[];await h.data.reconcile();
  assert.deepEqual(h.workspace.notes.map(n=>n.id),['a']);
});
test('reconciliation coalesces repeated triggers into one queued round and callers await it',async t=>{
  const gate=deferred();let reads=0;
  const h=fixture(t,{handler:async argv=>{if(argv[0]==='list-docs'){reads++;if(reads===1)await gate.promise;}}});
  const first=h.data.reconcile();await tick();const second=h.data.reconcile(),third=h.data.reconcile();let finished=false;second.then(()=>finished=true);
  assert.equal(reads,1);assert.equal(finished,false);gate.resolve();await Promise.all([first,second,third]);assert.equal(reads,2);assert.equal(finished,true);
});
test('late reconciliation results are discarded after workspace reset',async t=>{
  const gate=deferred();const h=fixture(t,{handler:async argv=>{if(argv[0]==='list-docs')await gate.promise;}});
  const pending=h.data.reconcile();await tick();h.setWorkspace({...h.workspace,path:'/other',notes:[note('new')]});gate.resolve();await pending;assert.equal(h.workspace.notes[0].id,'new');
});
test('queued writes read latest field values and the previous response token at dispatch time',async t=>{
  const gate=deferred();let writes=0;
  const h=fixture(t,{handler:async argv=>{if(argv[0]==='update-doc'){writes++;if(writes===1)await gate.promise;return ok({path:'notes/demo/a',precondition:'T'+writes});}}});
  const one=h.data.updateNote({id:'a'},{body:'one'});await tick();const two=h.data.updateNote({id:'a'},{body:'two'});const three=h.data.updateNote({id:'a'},{body:'last'});
  gate.resolve();await Promise.all([one,two,three]);
  const calls=h.argv().filter(a=>a[0]==='update-doc');assert.equal(calls[0].at(-1),'f1=T0');assert.equal(calls[1].at(-1),'f1=T1');assert.equal(calls[2].at(-1),'f1=T2');
  assert.equal(calls[1][2],'body=last');assert.equal(calls[2][2],'body=last');assert.equal(h.workspace.notes[0].precondition,'T3');
});
test('body status 5 preserves local text, marks unsaved/conflict and never auto-retries with a fresh token',async t=>{
  const h=fixture(t,{handler:async argv=>argv[0]==='update-doc'?fail(5):undefined});
  const result=await h.data.updateNote({id:'a'},{body:'local'});assert.equal(result.conflict,true);assert.equal(h.workspace.notes[0].body,'local');
  assert.deepEqual(await h.data.waitForWrites(),['a']);assert.equal(h.argv().filter(a=>a[0]==='update-doc').length,1);assert.equal(h.workspace.notes[0].precondition,'T0');
});
test('metadata failures roll back only metadata and do not mark the whole note unsaved',async t=>{
  const h=fixture(t,{handler:async argv=>argv[0]==='set-favorite'?fail(4):undefined});await h.data.favorite('a',true);
  assert.equal(h.workspace.notes[0].favorite,undefined);assert.deepEqual(h.data.getSnapshot().unsaved,[]);assert.equal(h.data.getSnapshot().failures.length,1);
});
test('clean metadata status 5 reloads body and token after its queue finishes',async t=>{
  const h=fixture(t,{handler:async argv=>argv[0]==='set-favorite'?fail(5):undefined});h.disk.notes[0]={...note(),body:'external',precondition:'EXT'};
  const result=await h.data.favorite('a',true);assert.equal(result.reloaded,true);assert.equal(h.workspace.notes[0].body,'external');assert.equal(h.workspace.notes[0].precondition,'EXT');
  assert.equal(h.data.getSnapshot().failures.at(-1).kind,'reloaded');
});
test('metadata status 5 during editing retains its token without reloading or raising a conflict',async t=>{
  const h=fixture(t,{handler:async argv=>argv[0]==='set-favorite'?fail(5):undefined});h.data.edit({id:'a'},{body:'local'});await h.data.favorite('a',true);
  assert.equal(h.workspace.notes[0].precondition,'T0');assert.equal(h.argv().length,1);assert.deepEqual(h.data.getSnapshot().conflicted,[]);
});
test('metadata status 5 with unsaved text raises a conflict without refreshing the token',async t=>{
  const h=fixture(t,{handler:async argv=>['update-doc','set-favorite'].includes(argv[0])?fail(argv[0]==='update-doc'?4:5):undefined});
  await h.data.updateNote({id:'a'},{body:'failed'});await h.data.favorite('a',true);assert.deepEqual(h.data.getSnapshot().conflicted,['a']);assert.equal(h.workspace.notes[0].precondition,'T0');
});
test('main title commit retries at most once after a clean conflict only if the heading remains the same',async t=>{
  let writes=0;const h=fixture(t,{handler:async argv=>argv[0]==='update-doc'?(++writes===1?fail(5):ok({precondition:'DONE'})):undefined});h.disk.notes[0].precondition='EXT';
  assert.equal((await h.data.updateNote({id:'a'},{title:'new'})).ok,true);assert.equal(writes,2);assert.equal(h.argv().filter(a=>a[0]==='update-doc')[1].at(-1),'f1=EXT');
});
test('reconciliation cannot roll back a token when an own write started during the read',async t=>{
  const gate=deferred();const h=fixture(t,{handler:async argv=>{if(argv[0]==='list-docs'){const old=copy(h.disk.notes);await gate.promise;return ok(old);}}});
  const reading=h.data.reconcile();await tick();await h.data.favorite('a',true);gate.resolve();await reading;
  assert.equal(h.workspace.notes[0].favorite,true);assert.equal(h.workspace.notes[0].precondition,'T1');
});
test('an own write already in flight at read start is also protected',async t=>{
  const gate=deferred();const h=fixture(t,{handler:async argv=>{if(argv[0]==='set-favorite')await gate.promise;}});
  const writing=h.data.favorite('a',true);await tick();await h.data.reconcile();assert.equal(h.workspace.notes[0].favorite,true);gate.resolve();await writing;
});
test('missing response tokens retain existing tokens and JSON notes omit f1',async t=>{
  const h=fixture(t,{handler:async argv=>argv[0]==='set-favorite'?ok({changed:false}):undefined});await h.data.favorite('a',true);assert.equal(h.workspace.notes[0].precondition,'T0');
  h.setWorkspace({...h.workspace,notes:[note('j',{format:'json',precondition:undefined})]});await h.data.updateNote({id:'j'},{body:'json'});assert.ok(!h.argv().at(-1).some(cell=>cell.startsWith('f1=')));
});
test('sub-note reads sort by created, inherit agenda and only refresh retained parents',async t=>{
  const h=fixture(t);h.sub.a=[note('z',{created:'2026-11-02',path:'notes/demo/a/sub/z'}),...h.sub.a];
  await h.data.loadSubNotes('a');assert.deepEqual(h.data.getSnapshot().subNotes.a.map(n=>n.id),['child','z']);
  const release1=h.data.retainSubNotes('a'),release2=h.data.retainSubNotes('a');release1();await h.data.reconcile();assert.equal(h.argv().filter(a=>a[0]==='list-sub-notes').length,2);
  release2();await h.data.reconcile();assert.equal(h.argv().filter(a=>a[0]==='list-sub-notes').length,2);
});
test('a retention disposer from an old workspace does not clear a new workspace parent subscription',async t=>{
  const h=fixture(t);await h.data.loadSubNotes('a');const old=h.data.retainSubNotes('a');h.setWorkspace({...h.workspace,path:'/new'});
  await h.data.loadSubNotes('a');const release=h.data.retainSubNotes('a');old();await h.data.reconcile();release();
  assert.equal(h.argv().filter(a=>a[0]==='list-sub-notes').length,3);
});
test('C45: protected sub-notes keep their text but follow the disk path; disk deletion removes even editing sub-notes',async t=>{
  const h=fixture(t);await h.data.loadSubNotes('a');h.data.edit({parent:'a',id:'child'},{body:'local'});
  h.sub.a[0]={...h.sub.a[0],path:'moved',body:'external',precondition:'EXT'};await h.data.loadSubNotes('a');
  assert.equal(h.data.getSnapshot().subNotes.a[0].path,'moved');assert.equal(h.data.getSnapshot().subNotes.a[0].body,'local');assert.deepEqual(h.data.getSnapshot().conflicted,['a:child']);
  h.sub.a=[];await h.data.loadSubNotes('a');assert.deepEqual(h.data.getSnapshot().subNotes.a,[]);assert.ok(!h.data.getSnapshot().editing.includes('a:child'));
});
test('sub-note body writes include the current title and token; subtitle failure is unsaved',async t=>{
  const h=fixture(t);await h.data.loadSubNotes('a');await h.data.updateNote({parent:'a',id:'child'},{body:'new'});
  assert.deepEqual(h.argv().at(-1),['update-sub-note','a','child','body=new','title=child','f1=S0']);
  h.setHandler(async argv=>argv[0]==='update-sub-note'?fail(4):undefined);await h.data.updateNote({parent:'a',id:'child'},{title:'kept'});
  assert.equal(h.data.getSnapshot().subNotes.a[0].title,'kept');assert.ok(h.data.getSnapshot().unsaved.includes('a:child'));
});
test('invalid or over-deep parent chains never invoke the engine',async t=>{
  const h=fixture(t);assert.equal(h.data.chain('missing'),null);await h.data.loadSubNotes('missing');assert.equal(h.argv().length,0);
  let parent='a';
  for(let i=0;i<21;i++){const id='s'+i;h.sub[h.data.chain(parent)]=[note(id,{path:'notes/demo/'+id})];await h.data.loadSubNotes(parent);parent=id;}
  assert.equal(h.data.chain(parent),null);const before=h.argv().length;await h.data.loadSubNotes(parent);assert.equal(h.argv().length,before);
});
test('late sub/comment/facts reads cannot replace mutations or more recent reads',async t=>{
  const h=fixture(t);const comments=deferred();h.setHandler(async argv=>argv[0]==='list-comments'?comments.promise:undefined);
  const reading=h.data.loadComments({id:'a'});await tick();await h.data.comment({id:'a'},'create','new','cmnew-1');comments.resolve(ok({state:'loaded',comments:[{id:'old'}]}));await reading;
  assert.equal(h.data.getSnapshot().comments.a.comments[0].id,'cmnew-1');
  const facts=deferred();let reads=0;h.setHandler(async argv=>argv[0]==='get-agenda-facts'?(++reads===1?facts.promise:ok({facts:'new'})):undefined);
  const old=h.data.loadFacts('ag');await h.data.loadFacts('ag');facts.resolve(ok({facts:'old'}));await old;assert.equal(h.data.getSnapshot().facts.ag.facts,'new');
  const sub=deferred();h.setHandler(async argv=>argv[0]==='list-sub-notes'?sub.promise:argv[0]==='create-sub-note'?ok({id:'added',path:'notes/demo/a/sub/added'}):undefined);
  const list=h.data.loadSubNotes('a');await h.data.createSubNote('a');sub.resolve(ok({subNotes:[]}));await list;assert.equal(h.data.getSnapshot().subNotes.a[0].id,'added');
});
test('unreadable comments block all three writes and failed on-demand reads preserve cached data',async t=>{
  const h=fixture(t,{handler:async argv=>argv[0]==='list-comments'?ok({state:'unreadable',reason:'bad',comments:[]}):undefined});await h.data.loadComments({id:'a'});
  for(const action of ['create','update','delete'])assert.equal((await h.data.comment({id:'a'},action,'x','cmtest-1')).ok,false);
  assert.equal(h.argv().length,1);h.setHandler(async()=>fail(4));await h.data.loadComments({id:'a'});assert.equal(h.data.getSnapshot().comments.a.state,'unreadable');
});
test('derived deletion accepts a matching before token and strips only the specified marker',async t=>{
  const h=fixture(t,{workspace:{notes:[note('a',{body:'<!--wlc:cmtest-1-->keep<!--/wlc:cmtest-1-->\n<!--wlc:cmother-1-->other'})]},handler:async argv=>argv[0]==='delete-comment'?ok({path:'notes/demo/a/comments.json',markerStripped:true,bodyBefore:'T0',bodyAfter:'T2'}):undefined});
  await h.data.loadComments({id:'a'});await h.data.comment({id:'a'},'delete','','cmtest-1');assert.equal(h.workspace.notes[0].precondition,'T2');assert.equal(h.workspace.notes[0].body,'keep\n<!--wlc:cmother-1-->other');
});
test('failed comment deletion restores its position without dropping a newer queued comment',async t=>{
  const gate=deferred(),h=fixture(t,{handler:async argv=>argv[0]==='delete-comment'?gate.promise:undefined});await h.data.loadComments({id:'a'});
  const deleting=h.data.comment({id:'a'},'delete','','cmtest-1');await tick();const creating=h.data.comment({id:'a'},'create','new','cmnew-1');
  gate.resolve(fail(4));await Promise.all([deleting,creating]);assert.deepEqual(h.data.getSnapshot().comments.a.comments.map(row=>row.id),['cmtest-1','cmnew-1']);
});
test('derived deletion with a different before token does not mask an overlapping external edit',async t=>{
  const gate=deferred();const h=fixture(t,{handler:async argv=>argv[0]==='delete-comment'?ok({path:'comments.json',markerStripped:true,bodyBefore:'EXT',bodyAfter:'NEW'}):argv[0]==='list-docs'?(await gate.promise,ok([note('a',{body:'external',precondition:'EXT'})])):undefined});
  await h.data.loadComments({id:'a'});const reading=h.data.reconcile();await tick();await h.data.comment({id:'a'},'delete','','cmtest-1');gate.resolve();await reading;assert.equal(h.workspace.notes[0].body,'external');assert.equal(h.workspace.notes[0].precondition,'EXT');
});
test('sub-note deletion strips cards only when its parent token matches and restores list order on failure',async t=>{
  const h=fixture(t,{workspace:{notes:[note('a',{body:'before\n[Child](#sub:\u001cchild\u001f)\n[BOM](#sub:\ufeffchild\ufeff)\nafter'})]},handler:async argv=>argv[0]==='delete-sub-note'?ok({path:'sub/child',cardRemoved:true,containerBefore:'T0',containerAfter:'T2'}):undefined});
  await h.data.loadSubNotes('a');await h.data.deleteSubNote('a','child');assert.equal(h.workspace.notes[0].body,'before\n[BOM](#sub:\ufeffchild\ufeff)\nafter');assert.equal(h.workspace.notes[0].precondition,'T2');
  await h.data.loadSubNotes('a');h.setHandler(async argv=>argv[0]==='delete-sub-note'?fail(4):undefined);await h.data.deleteSubNote('a','child');assert.equal(h.data.getSnapshot().subNotes.a[0].id,'child');
});
test('facts save reads immediately before writing and refuses changed or unreadable facts',async t=>{
  const h=fixture(t);assert.equal((await h.data.saveFacts('ag','local','old')).conflict,true);assert.ok(!h.argv().some(a=>a[0]==='update-agenda-facts'));
  h.setHandler(async argv=>argv[0]==='get-agenda-facts'?fail(4):undefined);assert.equal((await h.data.saveFacts('ag','local','facts')).ok,false);
  h.setHandler(undefined);assert.equal((await h.data.saveFacts('ag',' local ','facts')).ok,true);assert.deepEqual(h.argv().at(-1),['update-agenda-facts','ag','facts=local']);
});
test('report edits debounce into one latest write and flush before workspace departure',async t=>{
  const h=fixture(t);h.data.saveReport('rv-morning-2026-11-01','one','memo');h.data.saveReport('rv-morning-2026-11-01','last','');assert.equal(h.argv().length,0);
  await h.data.flushReports();assert.deepEqual(h.argv(),[['save-report','kind=morning','date=2026-11-01','body=last']]);
  assert.equal(h.data.selfWrites.has('/fixture/.withlog/reports/rv-morning-2026-11-01.json'),false); // Response path takes precedence.
  h.data.saveReport('rv-morning-2026-11-01','abandoned','');h.setWorkspace({...h.workspace,path:'/other'});await h.data.flushReports();assert.equal(h.argv().length,1);
});
test('trash writes restore optimistic removals on failure and restoration waits for reconciliation',async t=>{
  const h=fixture(t,{handler:async argv=>['delete-doc','purge-doc'].includes(argv[0])?fail(4):undefined});
  await h.data.deleteDoc('a');assert.equal(h.workspace.notes[0].id,'a');await h.data.trashAction('tr1','purge');assert.equal(h.workspace.trash[0].trashId,'tr1');
  await h.data.trashAction('tr1','restore');assert.ok(h.argv().some(a=>a[0]==='reindex'));
});
test('C28: entity updates retain optimistic values on failure and send a given empty repo= to clear it, while creation omits it',async t=>{
  const h=fixture(t,{handler:async argv=>argv[0]==='update-agenda'?fail(4):undefined});await h.data.saveEntity('agenda',{title:' New ',repo:'',project:'',summary:''},'ag');
  assert.equal(h.workspace.agendas[0].title,'New');assert.deepEqual(h.argv().at(-1),['update-agenda','ag','title=New','repo=','project=','summary=']);await h.data.reconcile();assert.equal(h.workspace.agendas[0].title,'Demo');
  await h.data.saveEntity('agenda',{title:'Plain',summary:''},'ag');assert.deepEqual(h.argv().at(-1),['update-agenda','ag','title=Plain','summary=']);
  await h.data.saveEntity('agenda',{title:'Fresh',repo:'',hue:'',project:'',summary:''});assert.ok(!h.argv().at(-1).some(cell=>cell.startsWith('repo=')));
});
test('entity deletion refusal exposes full stderr and keeps preceding note deletions',async t=>{
  const h=fixture(t,{handler:async argv=>argv[0]==='delete-doc'?ok({trashId:'tr2',file:'a'}):argv[0]==='delete-agenda'?fail(-1):undefined});
  const result=await h.data.deleteEntity('agenda','ag');assert.equal(result.reason,'WL-412: changed');assert.equal(h.workspace.agendas.length,1);assert.equal(h.workspace.notes.length,0);
});
test('C32: a refused creation never enters memory and returns the failure; template creation failure leaves templates unchanged',async t=>{
  const gate=deferred(),h=fixture(t,{handler:async argv=>argv[0]==='create-doc'?(await gate.promise,fail(4)):argv[0]==='save-template'?fail(4):undefined});
  const creating=h.data.createDoc({agendaId:'ag',type:'free',title:'Local',body:'body',date:'2026-11-01',time:'12:30',mins:0,tags:[]});
  await tick();assert.deepEqual(h.workspace.notes.map(n=>n.id),['a','b']);gate.resolve();
  const result=await creating;assert.equal(result.ok,false);assert.equal(result.value,undefined);assert.deepEqual(h.workspace.notes.map(n=>n.id),['a','b']);
  assert.deepEqual(h.data.getSnapshot().failures.map(line=>line.text),['문서 저장: Local']);
  await h.data.saveTemplate({name:'New',type:'free',desc:'',body:''});assert.equal(h.workspace.templates.length,1);
});
test('C175: a note trashed while a reconciliation read is in flight is not revived by that read',async t=>{
  const gate=deferred(),h=fixture(t,{handler:async argv=>argv[0]==='list-docs'?(await gate.promise,ok([note(),note('b')])):argv[0]==='delete-doc'?ok({trashId:'tr2',file:'a'}):undefined});
  const reading=h.data.reconcile();await tick();await h.data.deleteDoc('a');assert.deepEqual(h.workspace.notes.map(n=>n.id),['b']);
  gate.resolve();await reading;assert.deepEqual(h.workspace.notes.map(n=>n.id),['b']);
  h.setHandler(undefined);h.disk.notes=[note('b')];await h.data.reconcile();assert.deepEqual(h.workspace.notes.map(n=>n.id),['b']);
});
test('C95: a template whose save failed keeps the typed text across reconciliation until a later save succeeds',async t=>{
  const h=fixture(t,{workspace:{templates:[{id:'tp',category:'doc',name:'T',type:'free',desc:'',body:'disk'}]},handler:async argv=>argv[0]==='save-template'?fail(-1):undefined});
  await h.data.saveTemplate({id:'tp',name:'T',type:'free',desc:'',body:'typed'});assert.equal(h.workspace.templates[0].body,'typed');
  await h.data.reconcile();assert.equal(h.workspace.templates[0].body,'typed');
  h.setHandler(undefined);await h.data.saveTemplate({id:'tp',name:'T',type:'free',desc:'',body:'typed again'});
  h.disk.templates=[{id:'tp',category:'doc',name:'T',type:'free',desc:'',body:'external'}];await h.data.reconcile();assert.equal(h.workspace.templates[0].body,'external');
});
test('a successful create registers its real id even if an older reconciliation pruned the provisional note',async t=>{
  const gate=deferred(),h=fixture(t,{handler:async argv=>argv[0]==='create-doc'?gate.promise:undefined});
  const creating=h.data.createDoc({agendaId:'ag',type:'free',title:'New',body:'new',date:'2026-11-01',time:'12:30',mins:0,tags:[]});
  await h.data.reconcile();assert.equal(h.workspace.notes.length,2);
  gate.resolve(ok({id:'real',path:'notes/demo/real',precondition:'CREATED'}));await creating;
  assert.equal(h.workspace.notes.at(-1).id,'real');assert.equal(h.workspace.notes.at(-1).precondition,'CREATED');
});
test('a read started before creation cannot remove the real id after the create response arrives',async t=>{
  const gate=deferred(),h=fixture(t,{handler:async argv=>argv[0]==='list-docs'?(await gate.promise,ok([note(),note('b')])):argv[0]==='create-doc'?ok({id:'real',path:'notes/demo/real',precondition:'CREATED'}):undefined});
  const reading=h.data.reconcile();await tick();await h.data.createDoc({agendaId:'ag',type:'free',title:'New',body:'new',date:'2026-11-01',time:'12:30',mins:0,tags:[]});
  gate.resolve();await reading;assert.ok(h.workspace.notes.some(note=>note.id==='real'&&note.precondition==='CREATED'));
});
test('C188: a renaming title write drops only the engine title-mismatch notice, whose head matches the engine source',async t=>{
  const mismatch='본문 첫 헤딩("snake\\_case")이 제목("snake_case")과 달라요 — 앱에서 고치면 제목이 바뀝니다.';
  let warning=mismatch+'\n옛 노트 삭제 실패 — 같은 id가 둘 남았을 수 있어요(정리 필요): notes/demo/a';
  const h=fixture(t,{handler:async argv=>argv[0]==='update-doc'?ok({path:'notes/demo/moved',precondition:'T1',renamed:true,warning}):undefined});
  await h.data.updateNote({id:'a'},{title:'snake_case'});assert.deepEqual(h.data.getSnapshot().failures.map(line=>line.text),['옛 노트 삭제 실패 — 같은 id가 둘 남았을 수 있어요(정리 필요): notes/demo/a']);
  h.data.dismissFailures();warning=mismatch;await h.data.updateNote({id:'a'},{title:'snake_case 2'});assert.deepEqual(h.data.getSnapshot().failures,[]);
  const engine=readFileSync(new URL('../../src/withlog/write/WarningsDetector.java',import.meta.url),'utf8');
  assert.ok(engine.includes('return "'+TITLE_MISMATCH_HEAD.replace('"','\\"')+'"'),'engine title-mismatch sentence head changed');
});
test('failure banner deduplicates to ten lines and success retains only active conflict failures',async t=>{
  const h=fixture(t,{handler:async argv=>argv[0]==='update-doc'?fail(4):undefined});
  for(let i=0;i<12;i++)await h.data.updateNote({id:'a'},{body:'body',title:'title'+i});assert.equal(h.data.getSnapshot().failures.length,10);
  h.setHandler(async argv=>argv[0]==='update-doc'?fail(5):undefined);await h.data.updateNote({id:'a'},{body:'conflict'});h.setHandler(undefined);await h.data.favorite('b',true);
  assert.equal(h.data.getSnapshot().failures.length,1);assert.equal(h.data.getSnapshot().failures[0].kind,'conflict');
});
test('reset discards queued writes and late results from the previous workspace',async t=>{
  const gate=deferred();const h=fixture(t,{handler:async argv=>{if(argv[0]==='update-doc')await gate.promise;}});
  const first=h.data.updateNote({id:'a'},{body:'one'});await tick();const queued=h.data.updateNote({id:'a'},{body:'two'});h.setWorkspace({...h.workspace,path:'/new',notes:[note('new')]});gate.resolve();await Promise.all([first,queued]);
  assert.equal(h.argv().filter(a=>a[0]==='update-doc').length,1);assert.equal(h.workspace.notes[0].id,'new');
});
test('samples mutate only memory and report storage, never invoke the host',async t=>{
  const h=fixture(t,{workspace:{sample:true,path:''}});await h.data.favorite('a',true);await h.data.updateNote({id:'a'},{body:'sample'});h.data.saveReport('rv-morning-2026-11-01','sample','memo');await h.data.reconcile();
  assert.deepEqual(h.calls,[]);assert.equal(JSON.parse(h.values.get('withlog.reportEdits.v1'))['rv-morning-2026-11-01'],'sample');
});
test('self-write paths honor expiry, segment boundaries and exact root spelling',()=>{
  let now=0;const own=new SelfWrites(()=>now);own.record('/v','');assert.equal(own.has('/v/anything'),false);own.record('/v','notes/a');
  assert.equal(own.has('/v/notes/a/note.md'),true);assert.equal(own.has('/v/notes/a-2'),false);now=2000;assert.equal(own.has('/v/notes/a'),false);
  own.record('/v/','notes/a');assert.equal(own.has('/v/notes/a'),false);assert.equal(own.has('/v//notes/a'),true);
  assert.deepEqual(own.external(['/v/x.tmp','/v/.git','/v/.git/a','/v/.withlog/cache/a','/v/.gitignore']),['/v/.gitignore']);own.clear();assert.equal(own.has('/v//notes/a'),false);
});
test('cross-window self-write receipts do not echo and ignore own labels or different roots',t=>{
  const h=fixture(t);h.data.receiveOwn({root:'/fixture',relPath:'notes/a',from:'note-a'});assert.equal(h.data.selfWrites.has('/fixture/notes/a'),true);assert.equal(h.calls.length,0);
  h.data.receiveOwn({root:'/fixture',relPath:'notes/b',from:'main'});h.data.receiveOwn({root:'/other',relPath:'notes/c',from:'note-c'});assert.equal(h.data.selfWrites.has('/fixture/notes/b'),false);assert.equal(h.data.selfWrites.has('/other/notes/c'),false);
});
test('queue draining includes newly queued work and runs independent notes concurrently',async()=>{
  const queues=new WriteQueues(),gate=deferred();let b=false,second=false;
  const first=queues.run('a',async()=>{await gate.promise;void queues.run('a',async()=>{second=true;});});
  await queues.run('b',async()=>{b=true;});assert.equal(b,true);const draining=queues.drain();gate.resolve();await Promise.all([first,draining]);assert.equal(second,true);assert.equal(queues.pending(),false);
});
