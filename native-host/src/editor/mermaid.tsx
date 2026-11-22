import { useEffect, useRef, useState } from "react";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { $getRoot, $isElementNode } from "lexical";
import type { LexicalNode } from "lexical";
import { $isCodeNode } from "@lexical/code";
import DOMPurify from "dompurify";

let rendering: Promise<unknown> = Promise.resolve();
export function renderDiagram(source: string, theme: string): Promise<string> {
  const task = rendering
    .catch(() => {})
    .then(async () => {
      const mermaid = (await import("mermaid")).default;
      mermaid.initialize({
        startOnLoad: false,
        securityLevel: "strict",
        suppressErrorRendering: true,
        htmlLabels: false,
        theme:
          theme === "dark" ||
          (theme === "system" &&
            matchMedia("(prefers-color-scheme: dark)").matches)
            ? "dark"
            : "default",
        flowchart: { htmlLabels: false },
      });
      const result = await mermaid.render(
        "ais-mermaid-" + crypto.randomUUID(),
        source,
      );
      return DOMPurify.sanitize(result.svg, {
        USE_PROFILES: { svg: true, svgFilters: true },
        FORBID_TAGS: ["foreignObject", "image", "a"],
      });
    });
  rendering = task;
  return task;
}
export function MermaidLayer({ open }: { open: boolean }) {
  const [editor] = useLexicalComposerContext(),
    [blocks, setBlocks] = useState<{ key: string; source: string }[]>([]);
  useEffect(() => {
    const collect = () =>
      editor.getEditorState().read(() => {
        const next: typeof blocks = [];
        const walk = (node: LexicalNode) => {
          if ($isCodeNode(node) && node.getLanguage() === "mermaid")
            next.push({ key: node.getKey(), source: node.getTextContent() });
          else if ($isElementNode(node)) node.getChildren().forEach(walk);
        };
        walk($getRoot());
        setBlocks((previous) =>
          JSON.stringify(previous) === JSON.stringify(next) ? previous : next,
        );
      });
    collect();
    return editor.registerUpdateListener(collect);
  }, [editor]);
  return (
    <>
      {blocks.map((block) => (
        <Preview
          key={block.key}
          nodeKey={block.key}
          source={block.source}
          open={open}
        />
      ))}
    </>
  );
}
function Preview({
  nodeKey,
  source,
  open,
}: {
  nodeKey: string;
  source: string;
  open: boolean;
}) {
  const [editor] = useLexicalComposerContext(),
    [svg, setSVG] = useState(""),
    [error, setError] = useState(""),
    [expanded, setExpanded] = useState(open);
  const [position, setPosition] = useState({ left: 0, top: 0, width: 0 }),
    surface = useRef<HTMLDivElement>(null);
  const [theme, setTheme] = useState(
    document.documentElement.dataset.theme || "system",
  );
  useEffect(() => setExpanded(open), [open]);
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
    let current = true;
    const timer = setTimeout(() => {
      void (async () => {
        if (!current) return;
        try {
          const clean = await renderDiagram(source, theme);
          if (current) {
            setSVG(clean);
            setError("");
          }
        } catch (reason) {
          if (current) {
            setError(`다이어그램을 그릴 수 없습니다: ${String(reason)}`);
            setExpanded(true);
          }
        }
      })();
    }, 250);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [source, theme]);
  useEffect(() => {
    const code = editor.getElementByKey(nodeKey),
      root = editor.getRootElement(),
      outer = root?.closest<HTMLElement>(".lex-editor");
    if (!code || !outer) return;
    const layout = () => {
      code.classList.toggle("ais-mermaid-source-folded", !expanded);
      const height = surface.current?.getBoundingClientRect().height || 180;
      code.style.paddingBottom = height + 16 + "px";
      const rect = code.getBoundingClientRect(),
        base = outer.getBoundingClientRect();
      setPosition({
        left: rect.left - base.left,
        top: rect.bottom - base.top - height - 10,
        width: rect.width,
      });
    };
    layout();
    const observer = new ResizeObserver(layout);
    observer.observe(outer);
    if (surface.current) observer.observe(surface.current);
    const focus = () => {
      const anchor = window.getSelection()?.anchorNode;
      if (anchor && code.contains(anchor)) setExpanded(true);
    };
    document.addEventListener("selectionchange", focus);
    return () => {
      observer.disconnect();
      document.removeEventListener("selectionchange", focus);
      code.classList.remove("ais-mermaid-source-folded");
      code.style.paddingBottom = "";
    };
  }, [editor, nodeKey, expanded, svg, error]);
  return (
    <div
      className="mermaid-preview"
      ref={surface}
      style={{ left: position.left, top: position.top, width: position.width }}
    >
      <button onClick={() => setExpanded(!expanded)}>
        {expanded ? "코드 숨기기" : "코드"}
      </button>
      {error && !svg ? (
        <p role="status" className="mermaid-error">{error}</p>
      ) : (
        <>
          {error && <div className="mermaid-error-bar" role="status" title={error}>{error.split('\n')[0]}</div>}
          <div
            className="mermaid-svg"
            dangerouslySetInnerHTML={{ __html: svg }}
          />
        </>
      )}
    </div>
  );
}
