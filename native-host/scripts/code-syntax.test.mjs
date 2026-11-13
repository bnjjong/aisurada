import test from 'node:test';
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import Prism from 'prismjs';
import { createHeadlessEditor } from '@lexical/headless';
import { $getRoot } from 'lexical';
import { $isCodeHighlightNode } from '@lexical/code';
import { EDITOR_NODES, $importDocument, $exportDocument } from '../src/editor/markdown.ts';
import { highlightCode, registerCodeSyntax, codeHighlightTheme } from '../src/editor/codeSyntax.ts';

// 편집기의 코드 언어 선택 목록(tools.tsx의 languages)을 소스에서 그대로 읽는다. 목록에 언어를 더하고
// 문법을 빠뜨리면 아래 시험이 잡는다. plaintext는 색이 없고 mermaid는 다이어그램이라 문법이 필요 없다.
const picker = () => {
  const source = readFileSync(new URL('../src/editor/tools.tsx', import.meta.url), 'utf8');
  const list = /const languages = \[([^\]]+)\]/.exec(source)?.[1].match(/'([^']+)'/g)?.map(name => name.slice(1, -1));
  assert.ok(list?.length, 'tools.tsx에서 언어 목록을 읽지 못했다');
  return list.filter(name => name !== 'plaintext' && name !== 'mermaid');
};

const open = markdown => {
  const editor = createHeadlessEditor({ namespace: 'code-syntax', nodes: EDITOR_NODES, onError: error => { throw error; } });
  const unregister = registerCodeSyntax(editor);
  editor.update(() => $importDocument(markdown), { discrete: true });
  return { editor, unregister };
};

test('Kotlin code is split into keyword, function, annotation and string tokens', () => {
  const html = highlightCode('@Deprecated\nfun main() { println("hi") }', 'kotlin');
  assert.match(html, /<span class="token keyword">fun<\/span>/);
  assert.match(html, /<span class="token function">main<\/span>/);
  assert.match(html, /<span class="token annotation[^"]*">@Deprecated<\/span>/);
  assert.match(html, /<span class="token string-literal[^"]*">/);
});

test('every language in the editor picker has a grammar', () => {
  for (const language of picker()) {
    const html = highlightCode('x', language);
    assert.notEqual(html, null, `${language}에 문법이 없다`);
  }
});

test('fence names people actually write resolve to a grammar', () => {
  for (const name of ['kt', 'kts', 'sh', 'shell', 'zsh', 'yml', 'dockerfile', 'gradle', 'jsonc', 'c++', 'ts', 'tsx', 'jsx', 'toml', 'KOTLIN'])
    assert.notEqual(highlightCode('x', name), null, `${name}에 문법이 없다`);
});

test('an unknown language or a Prism helper name is left alone', () => {
  for (const name of ['', 'nope', 'mermaid', 'extend', 'insertBefore', 'constructor', '__proto__', 'toString'])
    assert.equal(highlightCode('x', name), null, `${JSON.stringify(name)}는 문법으로 보면 안 된다`);
});

test('tags inside the code are escaped, never passed through', () => {
  const html = highlightCode('val s = "<script>alert(1)</script>"', 'kotlin');
  // Prism은 태그를 여는 `<`만 이스케이프하고 `>`는 그대로 둔다. `<`가 막혀 있으면 태그가 만들어지지 않는다.
  assert.doesNotMatch(html, /<script|<\/script/);
  assert.match(html, /&lt;script>/);
  assert.equal(html.replace(/<span class="token [^"]*">|<\/span>/g, ''), 'val s = "&lt;script>alert(1)&lt;/script>"');
});

test('the Lexical theme turns any token type into the shared CSS class', () => {
  assert.equal(codeHighlightTheme.keyword, 'token keyword');
  assert.equal(codeHighlightTheme['string-literal'], 'token string-literal');
});

test('the editor colours a Kotlin block without changing the saved Markdown', () => {
  const markdown = '```kotlin\nfun main() {\n  println("hi")\n}\n```';
  const { editor, unregister } = open(markdown);
  const kinds = editor.getEditorState().read(() =>
    $getRoot().getFirstChild().getChildren().filter($isCodeHighlightNode).map(node => [node.getTextContent(), node.getHighlightType()]));
  assert.ok(kinds.some(([text, type]) => text === 'fun' && type === 'keyword'), 'fun이 keyword로 칠해지지 않았다');
  assert.ok(kinds.some(([text, type]) => text === 'println' && type === 'function'), 'println이 function으로 칠해지지 않았다');
  assert.equal(editor.getEditorState().read($exportDocument), markdown, '색을 입혀도 저장되는 본문은 같아야 한다');
  unregister();
});

test('typing into a coloured block keeps the colours current and the Markdown exact', () => {
  const { editor, unregister } = open('```kotlin\nval a = 1\n```');
  editor.update(() => {
    const code = $getRoot().getFirstChild();
    const last = code.getLastChild();
    last.setTextContent(last.getTextContent() + '\nfun b() {}');
  }, { discrete: true });
  const kinds = editor.getEditorState().read(() =>
    $getRoot().getFirstChild().getChildren().filter($isCodeHighlightNode).map(node => [node.getTextContent(), node.getHighlightType()]));
  assert.ok(kinds.some(([text, type]) => text === 'fun' && type === 'keyword'), '새로 입력한 fun도 칠해져야 한다');
  assert.equal(editor.getEditorState().read($exportDocument), '```kotlin\nval a = 1\nfun b() {}\n```');
  unregister();
});

test('a block with no language or an unsupported one stays plain and exports unchanged', () => {
  for (const markdown of ['```\nfun main() {}\n```', '```mermaid\ngraph TD\nA-->B\n```', '```plaintext\nfun x\n```']) {
    const { editor, unregister } = open(markdown);
    const coloured = editor.getEditorState().read(() =>
      $getRoot().getFirstChild().getChildren().filter($isCodeHighlightNode).some(node => node.getHighlightType()));
    assert.equal(coloured, false, `${markdown.split('\n')[0]}는 색이 없어야 한다`);
    assert.equal(editor.getEditorState().read($exportDocument), markdown);
    unregister();
  }
});

test('every token name Prism colours through an alias has its own CSS rule', () => {
  // Lexical는 토큰의 종류 이름 하나만 클래스로 달고 별칭을 버린다. 그래서 별칭으로 색이 정해지는 이름에
  // 자기 이름의 규칙이 없으면 읽기 화면은 칠해지는데 편집기는 칠해지지 않는다.
  const coloured = new Set(['keyword', 'string', 'number', 'boolean', 'function', 'class-name', 'variable', 'property',
    'attr-name', 'attr-value', 'atrule', 'constant', 'symbol', 'builtin', 'comment', 'tag', 'selector', 'regex',
    'annotation', 'url', 'char']);
  const css = readFileSync(new URL('../src/styles.css', import.meta.url), 'utf8');
  const ruled = new Set([...css.matchAll(/\.token\.([a-z][a-z-]*)/g)].map(match => match[1]));
  const missing = new Set();
  const seen = new Set();
  const walk = grammar => {
    if (!grammar || typeof grammar !== 'object' || seen.has(grammar)) return;
    seen.add(grammar);
    for (const [name, definition] of Object.entries(grammar))
      for (const part of [].concat(definition)) {
        if (!part || typeof part !== 'object' || part instanceof RegExp) continue;
        const aliases = [].concat(part.alias ?? []);
        if (aliases.some(alias => coloured.has(alias)) && !ruled.has(name)) missing.add(name);
        walk(part.inside);
      }
  };
  for (const grammar of Object.values(Prism.languages)) walk(grammar);
  assert.deepEqual([...missing].sort(), [], 'styles.css에 규칙이 없는 토큰 이름');
});
