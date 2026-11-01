import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHeadlessEditor } from '@lexical/headless';
import { $getSelection } from 'lexical';
import { EDITOR_NODES, $importDocument, $exportDocument, $repairDocument } from '../src/editor/markdown.ts';
import { relativePath, widthDashes, markerPlaceholders, calloutStyle } from '../src/editor/grammar.ts';
import { frameHeight, openEditorLink } from '../src/editor/media.tsx';
import { tablesOnlyJSON } from '../src/editor/tableTools.tsx';
const cases=JSON.parse(readFileSync(new URL('../fixtures/editor-roundtrip/cases.json',import.meta.url),'utf8'));
for(const c of cases) test(`PDF에서 독립 재구성한 왕복 ${c.id}: headless`,()=>{
 const editor=createHeadlessEditor({namespace:c.id,nodes:EDITOR_NODES,onError:e=>{throw e}});
 editor.update(()=>{$importDocument(`# 왕복 ${c.id}\n\n${c.input}\n\n왕복 사례 끝`); assert.equal($getSelection(),null)},{discrete:true});
 assert.equal(editor.getEditorState().read($exportDocument),`# 왕복 ${c.id}\n\n${c.headless??c.expected}\n\n왕복 사례 끝`);
});
test('mounted 표 정리는 짧은 행을 패딩하고 마지막 폭을 반복한다',()=>{
 const editor=createHeadlessEditor({namespace:'repair',nodes:EDITOR_NODES,onError:e=>{throw e}});
 editor.update(()=>{$importDocument('| a | b |\n| - | ----------- |\n| 1 |\n| x | y | z |');$repairDocument();},{discrete:true});
 const text=editor.getEditorState().read($exportDocument);assert.match(text,/\| 1 \| \| \|/);assert.match(text,/\| --- \| -{20,} \| -{20,} \|/);
});
test('상대 첨부 경로는 한 번만 디코드하고 위험 세그먼트를 거절한다',()=>{
 for(const path of ['https://a/x','/x','#x','a\\b','a//b','a/../b','%2e%2e/a','a/%00','a/%2f/b'])assert.equal(relativePath(path),null,path);
 assert.equal(relativePath('이미지%20하나.png'),'이미지 하나.png');assert.equal(relativePath('./a.png'),'./a.png');assert.equal(relativePath('%252e%252e/a'),'%2e%2e/a');
});
test('표 폭은 픽셀 대신 구분선 비율로 저장된다',()=>{assert.deepEqual(widthDashes([100,100],2),[3,3]);assert.deepEqual(widthDashes([100,400,400],3),[5,21,21]);});
test('콜아웃 표시 계열',()=>{assert.equal(calloutStyle('TIP'),'success');assert.equal(calloutStyle('fail'),'danger');assert.equal(calloutStyle('caution'),'warn');assert.equal(calloutStyle('unknown'),'info');});
test('틀 높이 캐시는 올림·범위·URL별 보관을 적용한다',()=>{assert.equal(frameHeight('a'),480);assert.equal(frameHeight('a',159.1),160);assert.equal(frameHeight('a',400.1),401);assert.equal(frameHeight('a',9999),1600);assert.equal(frameHeight('b'),480);});
test('첨부 HTML/SVG 외부 열기는 확인 실패 시 닫힌다',async()=>{
 const calls=[];const f={noteDirectory:'/vault/note',host:{confirm:async()=>{throw Error('dialog')},openFile:async p=>calls.push(p),openURL:async p=>calls.push(p)}};
 await openEditorLink('x.html',f);await openEditorLink('x.svg',f);assert.deepEqual(calls,[]);
 await openEditorLink('x.pdf',f);await openEditorLink('https://example.com',f);await openEditorLink('javascript:alert(1)',f);assert.deepEqual(calls,['/vault/note/x.pdf','https://example.com']);
});
test('주석은 마지막 같은 ID 열기와 첫 닫기를 짝짓는다',()=>{const p=markerPlaceholders('<!--wlc:cm1-a-->a<!--wlc:cm1-a-->b<!--/wlc:cm1-a-->c');assert.equal(p.pairs.length,1);assert.equal(p.pairs[0].open,1);});
test('표 전용 복사는 앱 JSON에서도 문단을 걷고 표 노드만 남긴다',()=>{const table={type:'table',children:[{type:'tablerow'}]};assert.deepEqual(JSON.parse(tablesOnlyJSON(JSON.stringify({namespace:'withlog-editor',nodes:[{type:'paragraph',text:'제외'},table]}))),{namespace:'withlog-editor',nodes:[table]});assert.equal(tablesOnlyJSON('broken'),undefined);});
