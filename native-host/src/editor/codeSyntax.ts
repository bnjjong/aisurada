import Prism from "prismjs";
// 문법은 서로 기대어 지어진다. Kotlin·Groovy는 clike, JSX는 markup·javascript, TSX는 JSX·typescript 위에
// 세워지므로 기대는 쪽을 먼저 불러온다. Lexical에 든 Prism에는 Kotlin, Bash, YAML, JSON, Groovy,
// Dockerfile, properties가 없어서 편집기의 언어 선택 목록에 있는데도 색이 입혀지지 않았다.
import "prismjs/components/prism-clike.js";
import "prismjs/components/prism-markup.js";
import "prismjs/components/prism-javascript.js";
import "prismjs/components/prism-typescript.js";
import "prismjs/components/prism-jsx.js";
import "prismjs/components/prism-tsx.js";
import "prismjs/components/prism-kotlin.js";
import "prismjs/components/prism-groovy.js";
import "prismjs/components/prism-bash.js";
import "prismjs/components/prism-yaml.js";
import "prismjs/components/prism-json.js";
import "prismjs/components/prism-docker.js";
import "prismjs/components/prism-properties.js";
import "prismjs/components/prism-toml.js";
import { PrismTokenizer, registerCodeHighlighting } from "@lexical/code";
import type { LexicalEditor } from "lexical";

// 읽기 화면은 우리가 직접 칠한다. Prism이 페이지가 열릴 때 코드 블록을 한 번 더 건드리지 않게 막는다.
Prism.manual = true;

/** 펜스에 흔히 쓰는 이름 가운데 Prism이 다른 이름으로 가진 것. */
const ALIASES: Record<string, string> = {
  gradle: "groovy",
  zsh: "bash",
  jsonc: "json",
  "c++": "cpp",
  "objective-c": "objectivec",
};
for (const [alias, name] of Object.entries(ALIASES)) {
  if (!Object.hasOwn(Prism.languages, alias)) Prism.languages[alias] = Prism.languages[name];
}

function grammarOf(language: string): unknown {
  const name = language.toLowerCase();
  // languages에는 extend, insertBefore 같은 함수도 들어 있어서 이름만으로 문법이라 보지 않는다.
  if (!Object.hasOwn(Prism.languages, name)) return null;
  const grammar = Prism.languages[name];
  return grammar !== null && typeof grammar === "object" ? grammar : null;
}

/**
 * [code]를 [language] 문법으로 나눠 `<span class="token keyword">` 꼴의 HTML로 돌려줍니다.
 * 모르는 언어면 null이라 호출하는 쪽이 원래 글자를 그대로 둡니다. 글자는 Prism이 이스케이프하므로
 * 돌려준 HTML에는 span 말고 코드의 태그가 섞이지 않습니다.
 */
export function highlightCode(code: string, language: string): string | null {
  const grammar = grammarOf(language);
  return grammar ? Prism.highlight(code, grammar, language.toLowerCase()) : null;
}

/**
 * 읽기 화면에 그려진 `<pre><code class="language-…">`마다 문법 색을 입힙니다. 언어를 모르거나 적지
 * 않은 블록은 그대로 둡니다. 이미 칠한 블록에 다시 불러도 같은 결과라서 여러 번 불러도 안전합니다.
 */
export function highlightCodeBlocks(root: ParentNode): void {
  root.querySelectorAll<HTMLElement>("pre > code").forEach((code) => {
    const language = /(?:^|\s)language-(\S+)/.exec(code.className)?.[1];
    if (!language) return;
    const html = highlightCode(code.textContent ?? "", language);
    if (html !== null) code.innerHTML = html;
  });
}

/**
 * Lexical의 `theme.codeHighlight`. 토큰 종류가 곧 CSS 클래스(`token keyword`)가 되어 편집기와 읽기 화면이
 * 같은 CSS를 쓴다. 토큰 종류는 언어마다 달라서(Kotlin의 string-literal 등) 목록 대신 이름을 그대로 받는다.
 */
export const codeHighlightTheme: Record<string, string> = new Proxy(
  {},
  { get: (_target, type) => (typeof type === "string" ? `token ${type}` : undefined) },
);

// Lexical은 언어를 적지 않은 코드 블록을 기본값인 javascript로 바꿔 JS 색을 입힌다. 그러면 노트를
// 저장할 때 펜스가 ```에서 ```javascript로 바뀌어 본문이 달라지므로, 기본 언어를 꺼서 그대로 둔다.
const tokenizer = { ...PrismTokenizer, defaultLanguage: null };

/** 편집기의 코드 블록을 입력할 때마다 다시 나눠 색을 입히고, 코드 안의 Tab·줄 이동 같은 편집 기능도 켠다. */
export function registerCodeSyntax(editor: LexicalEditor): () => void {
  return registerCodeHighlighting(editor, tokenizer);
}
