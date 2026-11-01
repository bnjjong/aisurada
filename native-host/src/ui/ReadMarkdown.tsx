import { useEffect, useMemo, useRef, useState } from "react";
import { marked, Renderer } from "marked";
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
  const html = marked.parse(body, {
    renderer,
    gfm: true,
    async: false,
  }) as string;
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
