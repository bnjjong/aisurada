import test from 'node:test';
import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { handleDeepLinkRequest } from './deeplink-bridge.ts';

async function server(t) {
  let connected=false; const sent=[];
  const http=createServer((req,res)=>handleDeepLinkRequest(req,res,()=>connected,url=>sent.push(url)));
  await new Promise(resolve=>http.listen(0,'127.0.0.1',resolve)); t.after(()=>new Promise(resolve=>http.close(resolve)));
  const address=`http://127.0.0.1:${http.address().port}/__deeplink`;
  return {sent, connect:()=>{connected=true;}, request:(method,body)=>fetch(address,{method,body})};
}

test('bridge rejects wrong methods and invalid or empty bodies with the specified statuses', async t=>{
  const h=await server(t);
  assert.equal((await h.request('GET')).status,405);
  for(const body of ['  ','file:///a.md','withlog://doc/a/'])assert.equal((await h.request('POST',body)).status,400);
  assert.deepEqual(h.sent,[]);
});

test('bridge distinguishes no connected page from successful broadcast',async t=>{
  const h=await server(t),link='withlog://doc/a?ws=%2Fvault';
  assert.equal((await h.request('POST',link)).status,503); h.connect();
  assert.equal((await h.request('POST','  '+link+'\n')).status,200);assert.deepEqual(h.sent,[link]);
});

test('bridge enforces the 4096-byte UTF-8 boundary rather than character count',async t=>{
  const h=await server(t);h.connect();const prefix='withlog://doc/a?ws=/';
  const exact=prefix+'x'.repeat(4096-Buffer.byteLength(prefix));assert.equal((await h.request('POST',exact)).status,200);
  assert.equal((await h.request('POST',exact+'x')).status,413);
  assert.equal((await h.request('POST',prefix+'한'.repeat(1400))).status,413);
});
