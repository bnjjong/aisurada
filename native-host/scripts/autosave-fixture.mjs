import {VaultData} from '../src/data/vault.ts';import {EditorSession} from '../src/editor/session.ts';import {sampleWorkspace} from '../src/workspace/sample.ts';
export const ok=value=>({status:0,stdout:JSON.stringify(value),stderr:''}),fail=status=>({status,stdout:'',stderr:'AIS-fixture failure'});
export const spin=async()=>{for(let i=0;i<50;i++)await Promise.resolve()},deferred=()=>{let resolve;const promise=new Promise(r=>resolve=r);return{resolve,promise}};
const clone=x=>JSON.parse(JSON.stringify(x));
export function fixture(t,options={}){
 const note=(id,title,body)=>({id,title,body,format:'markdown',agenda:'demo',agendaId:'ag',created:'2026-11-01T09:00:00Z',date:'2026-11-01',time:'09:00',path:'notes/demo/'+id,precondition:'T0'});
 let ws={...sampleWorkspace(),path:'/fixture',sample:false,notes:[note('a','A','# A\n\nold')],agendas:[{id:'ag',slug:'demo',title:'demo'}],...options.workspace};
 const disk=clone(ws),sub={a:[note('child','Child','# Child\n\nold')]},listeners=new Set(),calls=[],flushers=new Map(),restorers=new Map(),sessions=[];let handler=options.handler,count=0;
 const invoke=async(command,args)=>{
  if(command!=='sidecar_call')return;const argv=args.argv;calls.push([...argv]);const intercepted=await handler?.(argv);if(intercepted!==undefined)return intercepted;const cmd=argv[0];
  if(cmd==='get-doc')return ok(clone(disk.notes.find(n=>n.id===argv[1])||{}));
  if(cmd==='get-sub-note')return ok(clone(sub[argv[1]]?.find(n=>n.id===argv[2])||{}));
  if(cmd==='list-sub-notes')return ok({subNotes:clone(sub[argv[1]]||[])});
  if(cmd==='list-docs')return ok(clone(disk.notes));if(cmd==='reindex')return ok({skipped:[],conflicts:[]});if(cmd==='commit-vault')return ok({outcome:'committed'});if(cmd.startsWith('list-'))return ok(clone(disk[cmd.slice(5)]||[]));
  if(cmd==='update-doc'||cmd==='update-sub-note'){
   const parent=cmd==='update-sub-note',n=parent?sub[argv[1]]?.find(n=>n.id===argv[2]):disk.notes.find(n=>n.id===argv[1]);if(!n)return fail(3);
   const fields=Object.fromEntries(argv.slice(parent?3:2).map(v=>[v.slice(0,v.indexOf('=')),v.slice(v.indexOf('=')+1)]));if(fields.f1!==n.precondition)return fail(5);if(n.format==='json'&&'body'in fields)return fail(3);
   Object.assign(n,Object.fromEntries(Object.entries(fields).filter(([k])=>k!=='f1')));n.precondition='T'+(++count);return ok({path:n.path,precondition:n.precondition});
  }return ok({});
 };
 const services={native:false,windowLabel:'main',storage:{getItem:()=>null,setItem:()=>{}},now:()=>new Date(),uuid:()=>crypto.randomUUID(),warn:()=>{},host:{invoke}};
 const data=new VaultData(services,()=>ws,(memory,next)=>{if(next)ws=next;for(const fn of listeners)fn()});data.reset(ws);
 const controller={data,getSnapshot:()=>({workspace:ws,vault:data.getSnapshot()}),subscribe:fn=>{listeners.add(fn);return()=>listeners.delete(fn)},registerEditor:(root,fn)=>{const key=Symbol();flushers.set(key,{root,fn});return()=>flushers.delete(key)},registerRestore:(key,fn)=>{const token=Symbol();restorers.set(token,{key,fn});return()=>restorers.delete(token)},callEngine:async argv=>{const r=await invoke('sidecar_call',{argv});if(r.status)throw Error('engine');return JSON.parse(r.stdout)}};
 const session=(ref={id:'a'})=>{const s=new EditorSession(controller,ref,data.note(ref)?.body||'');sessions.push(s);return s};
 t.after(()=>{data.invalidate();for(const s of sessions)s.dispose();data.dispose()});
 return{data,controller,disk,sub,calls,session,flushers,restorers,setHandler:fn=>handler=fn,get ws(){return ws},bodyCalls:()=>calls.filter(a=>a[0].startsWith('update-')&&a.some(v=>v.startsWith('body='))),titles:()=>calls.filter(a=>a[0].startsWith('update-')&&a.some(v=>v.startsWith('title='))&&!a.some(v=>v.startsWith('body=')))};
}
