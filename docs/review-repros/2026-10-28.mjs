import fs from 'node:fs';
import assert from 'node:assert/strict';
import ts from 'typescript';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
const root=path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../..');
const cache=new Map();
async function load(file){
  file=file.endsWith('.ts')?file:file+'.ts';
  if(cache.has(file))return cache.get(file);
  let code=ts.transpileModule(fs.readFileSync(file,'utf8'),{compilerOptions:{target:ts.ScriptTarget.ES2020,module:ts.ModuleKind.ES2020}}).outputText;
  const imports=[...code.matchAll(/from ['"]([^'"]+)['"]/g)];
  const path=await import('node:path');
  for(const match of imports){if(match[1].startsWith('.')){const dep=await load(path.resolve(path.dirname(file),match[1]));code=code.replace(match[0],`from '${dep}'`);}}
  const url='data:text/javascript;base64,'+Buffer.from(code).toString('base64'); cache.set(file,url);return url;
}
const pre=await import(await load(root+'/src/engine/write/preconditions.ts'));
const target={kind:'root_note',vaultId:'vault-a',docId:'note-a'};
const token=pre.createScopedToken(target,'token-for-note-a');target.vaultId='vault-b';target.docId='note-b';
let rebound;try{rebound=pre.combinePrecondition({id:'note-b'},token,{kind:'root_note',vaultId:'vault-b',docId:'note-b'});}catch(e){rebound=e.name;}
console.log('MUTABLE_SCOPED_TOKEN',JSON.stringify({tokenTarget:token.target,result:rebound}));
assert.equal(token.target.vaultId,'vault-a');
assert.equal(token.target.docId,'note-a');
assert.equal(rebound,'TargetMismatchError');
const {createReindexCoordinator}=await import(await load(root+'/src/engine/indexing/coordinator.ts'));
let dispatched=0,notifiedAfterDispose=0,disposed=false;
const coordinator=createReindexCoordinator({getClient:()=>({reindex:async()=>{dispatched++;return {kind:'transport_failure',reason:'other',phase:'pre_dispatch',message:'synthetic',recoveryAction:{kind:'unspecified',description:'synthetic'}};}}),schedule:()=>{}});
coordinator.subscribe('v',snapshot=>{if(snapshot.phase==='running'){disposed=true;coordinator.dispose();}});
coordinator.subscribe('v',()=>{if(disposed)notifiedAfterDispose++;});
coordinator.requestRefresh({vaultId:'v',reason:'manual'});await coordinator.flush('v');
console.log('DISPOSE_REENTRANCY',JSON.stringify({dispatched,notifiedAfterDispose}));
assert.equal(dispatched,0);
assert.equal(notifiedAfterDispose,0);
const {decodeResponse}=await import(await load(root+'/src/engine/decodeResponse.ts'));
const head=Buffer.from('{"body":"'),tail=Buffer.from('","precondition":"token"}\n');
const raw={mode:'cli',exitCode:0,stdout:new Uint8Array(Buffer.concat([head,Buffer.from([255]),tail])),stderr:new Uint8Array()};
const decoded=decodeResponse(raw,data=>data&&typeof data.body==='string'&&typeof data.precondition==='string');
console.log('INVALID_UTF8',JSON.stringify({kind:decoded.kind,violation:decoded.violation?.violationKind}));
assert.equal(decoded.kind,'contract_violation');
assert.equal(decoded.violation.violationKind,'invalid_utf8');
console.log('All fixed regression assertions passed');
