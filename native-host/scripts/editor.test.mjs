import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHeadlessEditor } from '@lexical/headless';
import { $getSelection, $getRoot } from 'lexical';
import { extractTitle } from '../src/data/title.ts';
import { $insertBlock } from '../src/editor/wrapBlock.ts';
import { EDITOR_NODES, $importDocument, $exportDocument, $repairDocument, codeInfo } from '../src/editor/markdown.ts';
import { $createCodeNode } from '@lexical/code';
import { $createTextNode } from 'lexical';
import { relativePath, widthDashes, markerPlaceholders, calloutStyle, fenceTracker, stripMarkers, foldParagraphs } from '../src/editor/grammar.ts';
import { frameHeight, openEditorLink } from '../src/editor/media.tsx';
import { tablesOnlyJSON } from '../src/editor/tableTools.tsx';
const cases=JSON.parse(readFileSync(new URL('../fixtures/editor-roundtrip/cases.json',import.meta.url),'utf8'));
for(const c of cases) test(`PDF에서 독립 재구성한 왕복 ${c.id}: headless`,()=>{
 const editor=createHeadlessEditor({namespace:c.id,nodes:EDITOR_NODES,onError:e=>{throw e}});
 editor.update(()=>{$importDocument(`# 왕복 ${c.id}\n\n${c.input}\n\n왕복 사례 끝`); assert.equal($getSelection(),null)},{discrete:true});
 assert.equal(editor.getEditorState().read($exportDocument),`# 왕복 ${c.id}\n\n${c.headless??c.expected}\n\n왕복 사례 끝`);
});
test('C188: the screen title equals the heading text the editor shows, before and after a save',()=>{
 const headings=['snake\\_case 정리','snake_case 정리','a_b_c','**굵은** 제목','*기울임*과 _기울임_','__init__ 함수','~~취소~~ 됨','==강조== 됨','`코드` 제목','1\\~2kg','a * b * c','값 &#35; 끝','\\*literal\\*','[링크](https://x.y) 글','[노트](#doc:nd1) 칩','**(주의)**를 확인','\\# 아님','***둘 다***','C:\\\\temp','앞 `a\\.b` 뒤','2 * 3 = 6','**a*b**','_a_b_','~a~','=a='];
 for(const heading of headings){
  const editor=createHeadlessEditor({namespace:'title',nodes:EDITOR_NODES,onError:e=>{throw e}});editor.update(()=>$importDocument('# '+heading),{discrete:true});
  const shown=editor.getEditorState().read(()=>$getRoot().getFirstChild().getTextContent()),saved=editor.getEditorState().read($exportDocument);
  assert.equal(extractTitle('# '+heading),shown,heading);assert.equal(extractTitle(saved),shown,'saved '+heading);
 }
});
test('C190: code block and callout inside a list wrap only the cursor item and split up to the outermost list',()=>{
 const wrap=(markdown,target,kind)=>{const editor=createHeadlessEditor({namespace:'wrap',nodes:EDITOR_NODES,onError:e=>{throw e}});
  editor.update(()=>$importDocument(markdown),{discrete:true});
  editor.update(()=>{$getRoot().getAllTextNodes().find(node=>node.getTextContent()===target).select(1,1);$insertBlock(kind);},{discrete:true});
  return editor.getEditorState().read($exportDocument);};
 assert.equal(wrap('- [ ] 할 일\n- [x] 끝난 일','할 일','code'),'```plaintext\n할 일\n```\n\n- [x] 끝난 일');
 assert.equal(wrap('- [ ] 할 일\n- [x] 끝난 일','할 일','callout'),'> [!info] 할 일\n\n- [x] 끝난 일');
 assert.equal(wrap('1. a\n2. b\n3. c','b','code'),'1. a\n\n```plaintext\nb\n```\n\n3. c');
 assert.equal(wrap('- a\n- b\n  - b1\n- c','b','code'),'- a\n\n```plaintext\nb\n```\n\n  - b1\n- c');
 assert.equal(wrap('- a\n  - x\n  - y\n    - z\n  - w\n- c','y','callout'),'- a\n  - x\n\n> [!info] y\n\n    - z\n  - w\n- c');
 assert.equal(wrap('문단 하나','문단 하나','callout'),'> [!info] 문단 하나');
});
test('C186: list-like lines typed in a code block are saved with their own spaces',()=>{
 const editor=createHeadlessEditor({namespace:'code',nodes:EDITOR_NODES,onError:e=>{throw e}});
 editor.update(()=>{$importDocument('글');$getRoot().append($createCodeNode('yaml').append($createTextNode('key:\n  - a\n    - b')));},{discrete:true});
 assert.equal(editor.getEditorState().read($exportDocument),'글\n\n```yaml\nkey:\n  - a\n    - b\n```');
 const opened=createHeadlessEditor({namespace:'code-open',nodes:EDITOR_NODES,onError:e=>{throw e}});
 opened.update(()=>$importDocument('```yaml\nkey:\n  - a\n```'),{discrete:true});
 assert.equal(opened.getEditorState().read(()=>$getRoot().getFirstChild().getTextContent()),'key:\n  - a','the editor shows code lines with their file spaces');
});
test('9.9 fence judge: one-line fences close themselves, closers must be at least as long, indents allowed',()=>{
 const judge=lines=>{const fenced=fenceTracker();return lines.map(line=>fenced(line));};
 assert.deepEqual(judge(['```한 줄```','x','```js','x','```']),[true,false,true,true,true]);
 assert.deepEqual(judge(['````md','```','a','```','````','b']),[true,true,true,true,true,false]);
 assert.deepEqual(judge(['글','  ```js','  a','  ```','뒤']),[false,true,true,true,false]);
 assert.equal(foldParagraphs('설명\n  ```js\n  a\n  b\n  ```\n뒤\n줄'),'설명\n  ```js\n  a\n  b\n  ```\n뒤 줄');
});
test('C182: marker syntax inside code survives placeholders and stripping; markers outside still pair',()=>{
 const text='`<!--wlc:cmab-12345678-->` 앞 <!--wlc:cmcd-12345678-->글<!--/wlc:cmcd-12345678-->\n```\n<!--wla:akab-12345678-->x<!--/wla:akab-12345678-->\n```';
 const placed=markerPlaceholders(text);assert.deepEqual(placed.pairs.map(pair=>pair.id),['cmcd-12345678']);assert.ok(placed.text.startsWith('`<!--wlc:cmab-12345678-->`'));assert.ok(placed.text.includes('<!--wla:akab-12345678-->x'));
 assert.equal(stripMarkers(text),'`<!--wlc:cmab-12345678-->` 앞 글\n```\n<!--wla:akab-12345678-->x<!--/wla:akab-12345678-->\n```');
});
test('C74: the language is the first word in the reading-view shape, else the old [\\w-] run, and the rest stays on the fence',()=>{
 assert.deepEqual(codeInfo('c++'),{language:'c++',tail:''});assert.deepEqual(codeInfo('ts title="x"'),{language:'ts',tail:' title="x"'});
 assert.deepEqual(codeInfo('   js'),{language:'js',tail:''});assert.deepEqual(codeInfo('{.python}'),{language:'',tail:'{.python}'});assert.deepEqual(codeInfo(''),{language:'',tail:''});
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
