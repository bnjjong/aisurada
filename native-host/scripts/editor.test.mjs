import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { createHeadlessEditor } from '@lexical/headless';
import { $getSelection, $getRoot, $createRangeSelection, $setSelection } from 'lexical';
import { extractTitle } from '../src/data/title.ts';
import { $insertBlock, $formatSelection, $selectionHasFormat } from '../src/editor/wrapBlock.ts';
import { EDITOR_NODES, $importDocument, $exportDocument, $repairDocument, codeInfo, transformers } from '../src/editor/markdown.ts';
import { $convertFromMarkdownString } from '@lexical/markdown';
import { $createCodeNode } from '@lexical/code';
import { $createTextNode } from 'lexical';
import { relativePath, widthDashes, markerPlaceholders, calloutStyle, fenceTracker, stripMarkers, foldParagraphs, closeBlockEmphasis } from '../src/editor/grammar.ts';
import { frameHeight, openEditorLink, FRAME_LINK, imageFileName } from '../src/editor/media.tsx';
import { REPORT_HTML } from '../src/workspace/engine.ts';
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
 const text='`<!--aisc:cmab-12345678-->` 앞 <!--aisc:cmcd-12345678-->글<!--/aisc:cmcd-12345678-->\n```\n<!--aisa:akab-12345678-->x<!--/aisa:akab-12345678-->\n```';
 const placed=markerPlaceholders(text);assert.deepEqual(placed.pairs.map(pair=>pair.id),['cmcd-12345678']);assert.ok(placed.text.startsWith('`<!--aisc:cmab-12345678-->`'));assert.ok(placed.text.includes('<!--aisa:akab-12345678-->x'));
 assert.equal(stripMarkers(text),'`<!--aisc:cmab-12345678-->` 앞 글\n```\n<!--aisa:akab-12345678-->x<!--/aisa:akab-12345678-->\n```');
});
test('C74: the language is the first word in the reading-view shape, else the old [\\w-] run, and the rest stays on the fence',()=>{
 assert.deepEqual(codeInfo('c++'),{language:'c++',tail:''});assert.deepEqual(codeInfo('ts title="x"'),{language:'ts',tail:' title="x"'});
 assert.deepEqual(codeInfo('   js'),{language:'js',tail:''});assert.deepEqual(codeInfo('{.python}'),{language:'',tail:'{.python}'});assert.deepEqual(codeInfo(''),{language:'',tail:''});
});
test('C178/C197: on text with no CJK the inline rule saves exactly what the library import saves (18 chosen, 3,000 random lines)',()=>{
 const make=()=>createHeadlessEditor({namespace:'diff',nodes:EDITOR_NODES,onError:e=>{throw e}});
 const ours=line=>{const e=make();e.update(()=>$importDocument(line),{discrete:true});return e.getEditorState().read($exportDocument);};
 const library=line=>{const e=make();e.update(()=>{$convertFromMarkdownString(line,transformers(),$getRoot());},{discrete:true});return e.getEditorState().read($exportDocument);};
 const chosen=['a **b** c','*a* _b_ ~~c~~ ==d==','**a `b` c**','x `*` y','[**a**](u) b','**[a](u)**','***x***','**a*b**','_a_b_','snake_case_x','\\*not\\*','`a\\.b` x','![i](a.png) **b**','[n](#doc:nd1) *c*','__init__ f','1~2kg','x**(a)**y','2 * 3 * 4'];
 let seed=7;const next=n=>{seed=(seed*1103515245+12345)&0x7fffffff;return seed%n;};
 const pieces=['a','b',' ','*','**','_','__','~~','==','`','[',']','(u)','!','\\','.','(',')','x y','#doc:'];
 const random=Array.from({length:3000},()=>Array.from({length:2+next(9)},()=>pieces[next(pieces.length)]).join('')).filter(line=>line.trim()&&!/^[#>\-+*\d\s|`]/.test(line));
 for(const line of [...chosen,...random])assert.equal(ours(line),library(line),line);
});
test('C198: a format crossing a line break inside one block is closed and reopened per line; block edges and unsafe rewrites stay',()=>{
 const same=['> **a\n>\n> b**','> **a\n> # b**','> **a\n> - b**','- **a\n이어짐**','> **a \n> b**','```\n> **a\n> b**\n```','긴 **문단\n이어짐**'];
 for(const text of same)assert.equal(closeBlockEmphasis(text),text,text);
 assert.equal(closeBlockEmphasis('> ~~a\n> b~~ ==c\n> d=='),'> ~~a~~\n> ~~b~~ ==c==\n> ==d==');
 assert.equal(closeBlockEmphasis('> ***x\n> y***'),'> ***x***\n> ***y***');
 assert.equal(closeBlockEmphasis('**굵게\\\n이어짐**'),'**굵게**\\\n**이어짐**');
 assert.equal(closeBlockEmphasis('긴 **문단\n이어짐**',true),'긴 **문단**\n**이어짐**');
 const long='> **'+'가'.repeat(10001)+'\n> 끝**';assert.equal(closeBlockEmphasis(long),long);
});
test('C201: B and I format the chips in the selection with the text, and pressed means every text and chip has the format',()=>{
 const editor=createHeadlessEditor({namespace:'chip-format',nodes:EDITOR_NODES,onError:e=>{throw e}});
 editor.update(()=>$importDocument('앞 [노트](#doc:nd1) 뒤'),{discrete:true});
 const selectAll=()=>{const texts=$getRoot().getAllTextNodes(),range=$createRangeSelection();range.anchor.set(texts[0].getKey(),0,'text');range.focus.set(texts.at(-1).getKey(),texts.at(-1).getTextContentSize(),'text');$setSelection(range);};
 const save=()=>editor.getEditorState().read($exportDocument);
 editor.update(()=>{selectAll();assert.equal($selectionHasFormat('bold'),false);assert.equal($formatSelection('bold'),true);},{discrete:true});
 assert.equal(save(),'**앞 [노트](#doc:nd1) 뒤**');
 editor.update(()=>{selectAll();assert.equal($selectionHasFormat('bold'),true);$formatSelection('italic');},{discrete:true});
 assert.equal(save(),'***앞 [노트](#doc:nd1) 뒤***');
 editor.update(()=>{selectAll();$formatSelection('bold');$formatSelection('italic');},{discrete:true});
 assert.equal(save(),'앞 [노트](#doc:nd1) 뒤');
 editor.update(()=>{selectAll();assert.equal($formatSelection('code'),false,'code stays with the library');},{discrete:true});
 const half=createHeadlessEditor({namespace:'chip-half',nodes:EDITOR_NODES,onError:e=>{throw e}});
 half.update(()=>$importDocument('**앞**[노트](#doc:nd1)**뒤**'),{discrete:true});
 half.update(()=>{const texts=$getRoot().getAllTextNodes(),range=$createRangeSelection();range.anchor.set(texts[0].getKey(),0,'text');range.focus.set(texts.at(-1).getKey(),texts.at(-1).getTextContentSize(),'text');$setSelection(range);
  assert.equal($selectionHasFormat('bold'),false,'an unformatted chip keeps the button off');},{discrete:true});
});
test('목록 6번과 7번: 회고 본문은 태그 이름 뒤가 빈칸, >, /일 때만 HTML이고, HTML 상자는 네 가지 주소만 연다',()=>{
 for(const html of ['<p>안녕</p>','  <div class="a">x','</div>','<br/>','<br>','<h1-x>','<My-Tag a=1>'])assert.equal(REPORT_HTML.test(html),true,html);
 for(const markdown of ['<https://example.com> 링크','<!-- 메모 -->','<!DOCTYPE html>','<3 마음','글 <b>굵게</b>','','<a','<mailto:a@b.c>'])assert.equal(REPORT_HTML.test(markdown),false,markdown);
 for(const link of ['http://a.b','https://a.b/c?d','mailto:a@b.c','tel:+8210','HTTPS://A.B'])assert.equal(FRAME_LINK.test(link),true,link);
 for(const link of ['attach/report.pdf','javascript:alert(1)','file:///etc/hosts','//a.b','aisframe://localhost/x','data:text/html,x',' https://a.b'])assert.equal(FRAME_LINK.test(link),false,link);
});
test('mounted 표 정리는 짧은 행을 패딩하고 마지막 폭을 반복한다',()=>{
 const editor=createHeadlessEditor({namespace:'repair',nodes:EDITOR_NODES,onError:e=>{throw e}});
 editor.update(()=>{$importDocument('| a | b |\n| - | ----------- |\n| 1 |\n| x | y | z |');$repairDocument();},{discrete:true});
 const text=editor.getEditorState().read($exportDocument);assert.match(text,/\| 1 \|  \|/);assert.match(text,/\| --- \| -{20,} \| -{20,} \|/);
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
test('절대 경로와 file:// 링크는 openPath 또는 openFile로 전달된다',async()=>{
 const calls=[];const f={host:{openPath:async p=>calls.push(p),warn:async()=>{}}};
 await openEditorLink('/Users/df/file.txt',f);
 await openEditorLink('file:///Users/df/My%20Doc.md',f);
 assert.deepEqual(calls,['/Users/df/file.txt','/Users/df/My Doc.md']);
});
test('주석은 마지막 같은 ID 열기와 첫 닫기를 짝짓는다',()=>{const p=markerPlaceholders('<!--aisc:cm1-a-->a<!--aisc:cm1-a-->b<!--/aisc:cm1-a-->c');assert.equal(p.pairs.length,1);assert.equal(p.pairs[0].open,1);});
test('표 전용 복사는 앱 JSON에서도 문단을 걷고 표 노드만 남긴다',()=>{const table={type:'table',children:[{type:'tablerow'}]};assert.deepEqual(JSON.parse(tablesOnlyJSON(JSON.stringify({namespace:'aisurada-editor',nodes:[{type:'paragraph',text:'제외'},table]}))),{namespace:'aisurada-editor',nodes:[table]});assert.equal(tablesOnlyJSON('broken'),undefined);});
test('이미지 파일명 추출은 상대 경로, URL, 쿼리, 인코딩을 처리하고 data URL은 제외한다', () => {
  assert.equal(imageFileName('attach/yofidra-kit-bag.svg'), 'yofidra-kit-bag.svg');
  assert.equal(imageFileName('attach/assembly%20steps.svg'), 'assembly steps.svg');
  assert.equal(imageFileName('images/sub/photo.png?v=123#anchor'), 'photo.png');
  assert.equal(imageFileName('https://example.com/assets/logo.png'), 'logo.png');
  assert.equal(imageFileName('file:///Users/df/vault/attach/test.png'), 'test.png');
  assert.equal(imageFileName('attach\\sub\\test.png'), 'test.png');
  assert.equal(imageFileName('data:image/svg+xml;utf8,...'), '');
  assert.equal(imageFileName(''), '');
});

test('표 칸 역슬래시는 \\n만 개행으로 처리하고 구두점 앞 역슬래시나 경로를 이중 해제하지 않는다', () => {
  // 마크다운 파일 내용: | 경로 | 줄바꿈 |\n| --- | --- |\n| ..\\..\\foo | a\nb |
  const tableMarkdown = '| 경로 | 줄바꿈 |\n| --- | --- |\n| ..\\\\..\\\\foo | a\\nb |';
  const editor = createHeadlessEditor({ namespace: 'table-slash', nodes: EDITOR_NODES, onError: e => { throw e; } });
  editor.update(() => $importDocument(tableMarkdown), { discrete: true });
  const exported = editor.getEditorState().read($exportDocument);
  assert.match(exported, /\.\.\\\\\.\.\\\\foo/);
  assert.match(exported, /a\\nb/);
});
