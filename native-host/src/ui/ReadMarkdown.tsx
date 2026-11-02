import { useEffect, useMemo, useRef, useState } from "react";
import { Marked, Renderer } from "marked";
import type { Tokens } from "marked";
import { SENTINEL, sentinelInline } from "../data/mdInline.ts";
import { fenceTracker, foldParagraphs } from "../editor/grammar.ts";
import DOMPurify from "dompurify";
import { convertFileSrc } from "@tauri-apps/api/core";
import { renderDiagram } from "../editor/mermaid.tsx";
const escape = (s: string) =>
  s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
const allowed = new Set(
  "a abbr b br code del em i img ins kbd mark picture s small source span strong sub sup u blockquote center details div figcaption figure h1 h2 h3 h4 h5 h6 hr li ol p summary table tbody td th thead tr ul".split(
    " ",
  ),
);
export type SanitizedHTML = string & { readonly __sanitizedHTML: unique symbol };
// C178, C197, C199, C200: the reading view draws the editor's pairs. Paired marks
// become sentinels before parsing and the parser's own emphasis is off, so a pair the
// editor does not see is never drawn here either. Notes fold lines like the editor.
const TAGS: [string, string, string][] = [
  [...SENTINEL.bold, "strong"], [...SENTINEL.italic, "em"], [...SENTINEL.strikethrough, "del"], [...SENTINEL.highlight, "mark"],
];
const OPENERS = new RegExp(`[${TAGS.map(([open]) => open).join("")}]`);
const reader = new Marked({
  tokenizer: { emStrong: () => undefined, del: () => undefined } as never,
  extensions: [{
    name: "wlFormat",
    level: "inline",
    start: (src: string) => { const at = src.search(OPENERS); return at < 0 ? undefined : at; },
    tokenizer(src: string) {
      const kind = TAGS.find(([open]) => src[0] === open);
      if (!kind) return undefined;
      let depth = 0;
      for (let i = 0; i < src.length; i++) {
        if (src[i] === kind[0]) depth++;
        else if (src[i] === kind[1] && --depth === 0)
          return { type: "wlFormat", raw: src.slice(0, i + 1), tag: kind[2], tokens: this.lexer.inlineTokens(src.slice(1, i)) };
      }
      return undefined;
    },
    renderer(token: Tokens.Generic) {
      return `<${token.tag}>${this.parser.parseInline(token.tokens || [])}</${token.tag}>`;
    },
  }],
});
function readingSource(body: string, external: boolean) {
  const fenced = fenceTracker();
  return (external ? body : foldParagraphs(body)).split("\n").map((line) => {
    if (fenced(line) || /^\s*([-*_])(\s*\1){2,}\s*$/.test(line)) return line;
    if (/^\s*\|/.test(line)) return line.split(/(?<!\\)\|/).map((cell, n, cells) => n && n < cells.length - 1 ? sentinelInline(cell) : cell).join("|");
    const prefix = /^\s*(?:>\s?)*\s*(?:(?:[-*+]|\d+\.)\s+(?:\[[ xX/-]\]\s+)?)?(?:#{1,6}\s+)?/.exec(line)![0];
    return prefix + sentinelInline(line.slice(prefix.length));
  }).join("\n");
}
export function readingHTML(body: string, external = false, renderer?: Renderer) {
  return (reader.parse(readingSource(body, external), { renderer, gfm: true, async: false }) as string).replace(/[\ue020-\ue027]/g, "");
}
export function markdownHTML(body: string, external = false) {
  const renderer = new Renderer();
  renderer.html = ({ text, block }) =>
    external
      ? block &&
        /^ {0,3}<(?:blockquote|center|details|div|figure|h[1-6]|hr|ol|p|picture|summary|table|ul)\b/i.test(
          text,
        )
        ? text
        : text.replace(/<\/?([a-z][\w-]*)\b[^>]*>/gi, (tag, name: string) =>
            allowed.has(name.toLowerCase()) ? tag : escape(tag),
          )
      : escape(text);
  const html = readingHTML(body, external, renderer);
  const template = document.createElement("template");
  template.innerHTML = html;
  if (external) {
    template.content.querySelectorAll("img").forEach((img) => {
      if (/^(https?:|\/\/)/i.test(img.getAttribute("src") || "")) {
        const chip = document.createElement("span");
        chip.className = "ext-md-img-alt";
        chip.textContent = img.alt || "그림";
        img.replaceWith(chip);
      }
    });
    template.content.querySelectorAll("source").forEach((source) => {
      if (/https?:|\/\//i.test(source.getAttribute("srcset") || ""))
        source.remove();
    });
  }
  return DOMPurify.sanitize(template.innerHTML) as SanitizedHTML;
}
export function imageSources(html: string) {
  const t = document.createElement("template");
  t.innerHTML = html;
  const values = [...t.content.querySelectorAll("img")]
    .map((i) => i.getAttribute("src") || "")
    .filter((s) => s && !/^(?:\/|[a-z][a-z\d+.-]*:)/i.test(s));
  return [
    ...new Set(
      values.flatMap((s) => {
        try {
          return [decodeURIComponent(s)];
        } catch {
          return [];
        }
      }),
    ),
  ];
}
export function ReadMarkdown({
  body,
  html,
  preparedHTML,
  external = false,
  directory,
  native = false,
  onLink,
}: {
  body?: string;
  html?: string;
  // Only HTML produced by markdownHTML, before native asset resolution.
  preparedHTML?: SanitizedHTML;
  external?: boolean;
  directory?: string;
  native?: boolean;
  onLink?(url: string): void | boolean;
}) {
  const root = useRef<HTMLDivElement>(null);
  const rendered = useMemo(() => {
    const safe = preparedHTML ?? (
      html === undefined
        ? markdownHTML(body || "", external)
        : DOMPurify.sanitize(html));
    if (!directory || !native) return safe;
    // Sanitize before resolving trusted native assets. A later sanitizer
    // would discard the asset: URI; resolving in an effect would issue a
    // relative request before the final image URL is ready.
    const template = document.createElement("template");
    template.innerHTML = safe;
    template.content.querySelectorAll("img").forEach((img) => {
      const src = img.getAttribute("src") || "";
      if (src && !/^(?:\/|[a-z][a-z\d+.-]*:)/i.test(src)) {
        try {
          img.src = convertFileSrc(directory + "/" + decodeURIComponent(src));
        } catch {}
      }
    });
    return template.innerHTML;
  }, [body, html, preparedHTML, external, directory, native]);
  const [theme, setTheme] = useState(
    document.documentElement.dataset.theme || "system",
  );
  useEffect(() => {
    const observer = new MutationObserver(() =>
      setTheme(document.documentElement.dataset.theme || "system"),
    );
    observer.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ["data-theme"],
    });
    return () => observer.disconnect();
  }, []);
  useEffect(() => {
    const element = root.current;
    if (!element) return;
    let live = true;
    element
      .querySelectorAll<HTMLElement>("pre > code.language-mermaid")
      .forEach((code) => {
        const pre = code.parentElement!,
          preview = document.createElement("div");
        preview.className = "ext-md-mermaid";
        pre.before(preview);
        pre.hidden = true;
        void renderDiagram(code.textContent || "", theme).then(
          (svg) => {
            if (live) preview.innerHTML = svg;
          },
          (reason) => {
            if (live) {
              pre.hidden = false;
              preview.textContent =
                "다이어그램을 그릴 수 없어서 코드로 보여 줘요: " +
                String(reason);
            }
          },
        );
      });
    return () => {
      live = false;
      element.querySelectorAll(".ext-md-mermaid").forEach((n) => n.remove());
    };
  }, [rendered, theme, directory, native]);
  return (
    <div
      ref={root}
      className={`read-markdown ${external ? "ext-md" : ""}`}
      onClickCapture={(e) => {
        const a = (e.target as Element).closest("a");
        if (a && onLink) {
          if (onLink(a.getAttribute("href") || "") === false) return;
          e.preventDefault();
          e.stopPropagation();
        }
      }}
      dangerouslySetInnerHTML={{ __html: rendered }}
    />
  );
}
