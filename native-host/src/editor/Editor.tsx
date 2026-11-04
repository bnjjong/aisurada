import { CLAUDE_ENABLED } from "@claude-plug/state";
import { EditorAskPlugins } from "@claude-plug/ask";
import { useEffect, useRef, useState } from "react";
import { LexicalComposer } from "@lexical/react/LexicalComposer";
import { RichTextPlugin } from "@lexical/react/LexicalRichTextPlugin";
import { ContentEditable } from "@lexical/react/LexicalContentEditable";
import { LexicalErrorBoundary } from "@lexical/react/LexicalErrorBoundary";
import { HistoryPlugin } from "@lexical/react/LexicalHistoryPlugin";
import { ListPlugin } from "@lexical/react/LexicalListPlugin";
import { CheckListPlugin } from "@lexical/react/LexicalCheckListPlugin";
import { TablePlugin } from "@lexical/react/LexicalTablePlugin";
import { TabIndentationPlugin } from "@lexical/react/LexicalTabIndentationPlugin";
import { LinkPlugin } from "@lexical/react/LexicalLinkPlugin";
import { MarkdownShortcutPlugin } from "@lexical/react/LexicalMarkdownShortcutPlugin";
import { useLexicalComposerContext } from "@lexical/react/LexicalComposerContext";
import { $isHeadingNode } from "@lexical/rich-text";
import { $isCodeNode } from "@lexical/code";
import { ListItemNode } from "@lexical/list";
import { $isTableSelection } from "@lexical/table";
import {
  $getRoot,
  $getSelection,
  $isRangeSelection,
  $isElementNode,
  $setSelection,
  $createParagraphNode,
  CLEAR_HISTORY_COMMAND,
  KEY_DOWN_COMMAND,
  FORMAT_TEXT_COMMAND,
  COMMAND_PRIORITY_HIGH,
  $addUpdateTag,
  $getState,
  $setState,
} from "lexical";
import type { LexicalEditor } from "lexical";
import { EditorContext } from "./context.ts";
import type { EditorFeatures } from "./context.ts";
import {
  EDITOR_NODES,
  $importDocument,
  $exportDocument,
  $repairDocument,
  transformers,
  pendingState,
} from "./markdown.ts";
import { EditorTools } from "./tools.tsx";
import { TableTools } from "./tableTools.tsx";
import { MermaidLayer } from "./mermaid.tsx";
import { headingSlug } from "./grammar.ts";
import { openEditorLink } from "./media.tsx";

export interface EditorHeading {
  key: string;
  level: number;
  text: string;
  slug: string;
}
export interface EditorHandle {
  markdown(): string;
  comment(id: string): void;
  removeComment(id: string): void;
  removeChild(id: string): void;
  heading(keyOrSlug: string): boolean;
}
export interface EditorProps {
  body: string;
  format?: string;
  label?: string;
  features?: EditorFeatures;
  onChange?(body: string, cursor: 1 | 2): void;
  mirror?: { revision: number; body: string; external?: boolean };
  onMirrorPending?(pending: boolean): void;
  onMirrorConflict?(): void;
  onHeadings?(headings: EditorHeading[]): void;
  onSection?(key: string): void;
  onReady?(handle: EditorHandle | null): void;
  diagramsOpen?: boolean;
  askDocId?: string;
  onAskAi?(quote: string, question: string, turnId: string, images?: File[]): void;
  onAskFollowUp?(question: string, underlineId: string, images?: File[]): void;
  onOpenAskCompose?(quote: string, rect: { left: number; top: number }, wrapMark?: (turnId: string) => void, markId?: string): void;
}
const theme = {
  paragraph: "lex-paragraph",
  quote: "lex-quote",
  link: "lex-link",
  code: "lex-code",
  heading: {
    h1: "lex-h1",
    h2: "lex-h2",
    h3: "lex-h3",
    h4: "lex-h4",
    h5: "lex-h5",
    h6: "lex-h6",
  },
  text: {
    bold: "lex-bold",
    italic: "lex-italic",
    strikethrough: "lex-strike",
    underline: "lex-underline",
    code: "lex-inline-code",
    highlight: "lex-highlight",
  },
  list: {
    ul: "lex-ul",
    ol: "lex-ol",
    listitem: "lex-li",
    listitemChecked: "lex-li-checked",
    listitemUnchecked: "lex-li-unchecked",
    nested: { listitem: "lex-nested" },
  },
  table: "lex-table",
  tableCell: "lex-cell",
  tableCellHeader: "lex-cell-header",
  tableSelection: "lex-table-selection",
};
export const stableJSON = (value: unknown): string =>
  JSON.stringify(
    value,
    (_key, item) =>
      item && typeof item === "object" && !Array.isArray(item)
        ? Object.fromEntries(
            Object.keys(item)
              .sort()
              .map((key) => [key, item[key]]),
          )
        : item,
    2,
  );

export function Editor(props: EditorProps) {
  const [error, setError] = useState("");
  const initial = useRef(props);
  const config = useRef({
    namespace: "withlog-editor",
    theme,
    nodes: EDITOR_NODES,
    editorState: (editor: LexicalEditor) => {
      if (initial.current.format === "json" && initial.current.body.trim())
        editor.setEditorState(editor.parseEditorState(initial.current.body));
      else $importDocument(initial.current.body);
    },
    onError: (reason: Error) => {
      console.error("편집기 오류", reason);
      setError("본문을 표시하는 중 문제가 생겼어요.");
    },
  }).current;
  return (
    <EditorContext.Provider value={props.features || {}}>
      <div className={`lex-editor ${CLAUDE_ENABLED ? "" : "lex-no-ask"}`}>
        {error && (
          <p className="lex-error" role="alert">
            {error}
          </p>
        )}
        <LexicalComposer initialConfig={config}>
          <RichTextPlugin
            contentEditable={
              <ContentEditable
                className="lex-content"
                aria-label={props.label || "노트 본문 편집"}
                aria-multiline="true"
                spellCheck={false}
              />
            }
            ErrorBoundary={LexicalErrorBoundary}
          />
          <HistoryPlugin />
          <ListPlugin />
          <CheckListPlugin />
          <TablePlugin
            hasCellMerge={false}
            hasCellBackgroundColor={false}
            hasNestedTables={false}
          />
          <TabIndentationPlugin />
          <LinkPlugin />
          <MarkdownShortcutPlugin transformers={transformers()} />
          <EditorBridge {...props} />
          <EditorTools {...props} />
          <TableTools {...props} />
          <MermaidLayer open={props.diagramsOpen || false} />
          <EditorAskPlugins docId={props.askDocId} onAsk={props.onAskAi} onFollowUp={props.onAskFollowUp} onOpenCompose={(quote, rect, markId) => props.onOpenAskCompose?.(quote, rect, undefined, markId)} />
        </LexicalComposer>
      </div>
    </EditorContext.Provider>
  );
}

function EditorBridge(props: EditorProps) {
  const [editor] = useLexicalComposerContext(),
    callbacks = useRef(props);
  callbacks.current = props;
  const revision = useRef(props.mirror?.revision),
    composing = useRef(false);
  const pending = useRef<{
    body: string;
    baseline: string;
    external?: boolean;
  } | null>(null);
  const last = useRef(""),
    heads = useRef("");
  const initialized = useRef(false);
  const exportBody = () =>
    props.format === "json"
      ? stableJSON(editor.getEditorState().toJSON())
      : $exportDocument();
  function applyMirror(body: string, external?: boolean) {
    let parsed;
    if (props.format === "json") {
      if (!body.trim()) return;
      try {
        parsed = editor.parseEditorState(body, () => {});
      } catch {
        return;
      }
      if (parsed.isEmpty()) return;
    }
    if (external) editor.dispatchCommand(CLEAR_HISTORY_COMMAND, undefined);
    if (parsed) editor.setEditorState(parsed, { tag: "withlog-mirror" });
    else
      editor.update(
        () => {
          $addUpdateTag("withlog-mirror");
          $importDocument(body);
          $repairDocument();
        },
        { discrete: true },
      );
  }
  function goHeading(key: string): boolean {
    let target: string | undefined;
    editor.getEditorState().read(() => {
      target = $getRoot()
        .getChildren()
        .filter($isHeadingNode)
        .find(
          (node) =>
            node.getKey() === key || headingSlug(node.getTextContent()) === key,
        )
        ?.getKey();
    });
    if (!target) return false;
    editor
      .getElementByKey(target)
      ?.scrollIntoView({ block: "start", behavior: "smooth" });
    return true;
  }
  useEffect(() => {
    editor.update(
      () => {
        $addUpdateTag("wl:repair");
        $repairDocument();
        $setSelection(null);
      },
      { discrete: true },
    );
    editor.getEditorState().read(() => {
      last.current = exportBody();
      const headings = $getRoot()
        .getChildren()
        .filter($isHeadingNode)
        .map((node) => ({
          key: node.getKey(),
          level: Number(node.getTag().slice(1)),
          text: node.getTextContent(),
          slug: headingSlug(node.getTextContent()),
        }));
      heads.current = JSON.stringify(headings);
      callbacks.current.onHeadings?.(headings);
    });
    let secondFrame = 0;
    const initialFrame = requestAnimationFrame(() => {
      secondFrame = requestAnimationFrame(() => {
        initialized.current = true;
        editor.getEditorState().read(() => {
          last.current = exportBody();
        });
        editor.getEditorState().read(() => {
          const headings = $getRoot()
            .getChildren()
            .filter($isHeadingNode)
            .map((node) => ({
              key: node.getKey(),
              level: Number(node.getTag().slice(1)),
              text: node.getTextContent(),
              slug: headingSlug(node.getTextContent()),
            }));
          heads.current = JSON.stringify(headings);
          callbacks.current.onHeadings?.(headings);
        });
        const groups = new Map<string, HTMLElement[]>();
        editor
          .getRootElement()
          ?.querySelectorAll<HTMLElement>("[data-comment-id]")
          .forEach((mark) => {
            const id = mark.dataset.commentId!;
            groups.set(id, [...(groups.get(id) || []), mark]);
          });
        for (const group of groups.values())
          group.at(-1)?.classList.add("lex-comment-end");
      });
    });
    const tasks = editor.registerNodeTransform(ListItemNode, (node) => {
      if (node.getChecked() && $getState(node, pendingState))
        $setState(node, pendingState, "");
    });
    const root = editor.getRootElement();
    if (!root) return;
    const start = () => {
      composing.current = true;
    };
    const end = () => {
      composing.current = false;
      setTimeout(() => {
        const deferred = pending.current;
        pending.current = null;
        if (!deferred) return;
        if (deferred.baseline !== editor.getEditorState().read(exportBody))
          callbacks.current.onMirrorConflict?.();
        else applyMirror(deferred.body, deferred.external);
        callbacks.current.onMirrorPending?.(false);
      }, 0);
    };
    const click = (event: MouseEvent) => {
      const link = (event.target as Element).closest("a");
      if (!link || !root.contains(link)) return;
      event.preventDefault();
      if (event.metaKey || event.ctrlKey)
        void openEditorLink(
          link.getAttribute("href") || "",
          callbacks.current.features || {},
          goHeading,
        );
    };
    root.addEventListener("compositionstart", start);
    root.addEventListener("compositionend", end);
    root.addEventListener("click", click);
    const remove = editor.registerUpdateListener(
      ({ editorState, dirtyElements, dirtyLeaves, tags }) => {
        editorState.read(() => {
          const headings = $getRoot()
            .getChildren()
            .filter($isHeadingNode)
            .map((node) => ({
              key: node.getKey(),
              level: Number(node.getTag().slice(1)),
              text: node.getTextContent(),
              slug: headingSlug(node.getTextContent()),
            }));
          const signature = JSON.stringify(headings);
          if (signature !== heads.current) {
            heads.current = signature;
            callbacks.current.onHeadings?.(headings);
          }
          if (!dirtyElements.size && !dirtyLeaves.size) return;
          const body = exportBody();
          const markers = new Map<string, HTMLElement[]>();
          root
            .querySelectorAll<HTMLElement>("[data-comment-id]")
            .forEach((mark) => {
              mark.classList.remove("lex-comment-end");
              const id = mark.dataset.commentId!;
              markers.set(id, [...(markers.get(id) || []), mark]);
            });
          for (const group of markers.values())
            group.at(-1)?.classList.add("lex-comment-end");
          if (
            !initialized.current ||
            tags.has("wl:import") ||
            tags.has("wl:repair") ||
            tags.has("wl:mirror") ||
            tags.has("withlog-mirror")
          ) {
            last.current = body;
            return;
          }
          if (body === last.current) return;
          last.current = body;
          const selection = $getSelection();
          const firstHeading = $getRoot().getChildren().find($isHeadingNode);
          const top = $isRangeSelection(selection)
            ? selection.anchor.getNode().getTopLevelElement()
            : null;
          callbacks.current.onChange?.(
            body,
            firstHeading && top?.is(firstHeading) ? 1 : 2,
          );
        });
      },
    );
    const keys = editor.registerCommand(
      KEY_DOWN_COMMAND,
      (event) => {
        if (
          !event ||
          event.altKey ||
          event.shiftKey ||
          !event.metaKey ||
          event.ctrlKey ||
          event.code !== "KeyE"
        )
          return false;
        const selection = $getSelection();
        if (
          $isTableSelection(selection) ||
          !$isRangeSelection(selection) ||
          selection.anchor.getNode().getParents().some($isCodeNode)
        )
          return false;
        event.preventDefault();
        editor.dispatchCommand(FORMAT_TEXT_COMMAND, "code");
        return true;
      },
      COMMAND_PRIORITY_HIGH,
    );
    const section = () => {
      const currentRoot = editor.getRootElement();
      if (!currentRoot) return;
      const scroller =
        currentRoot.closest(".doc-scroll, .page-scroll, .subnote-page") ||
        document.scrollingElement;
      const top = scroller?.getBoundingClientRect().top || 0;
      let key = "";
      editor.getEditorState().read(() => {
        for (const node of $getRoot().getChildren().filter($isHeadingNode))
          if (
            (editor.getElementByKey(node.getKey())?.getBoundingClientRect()
              .top || 0) <=
            top + 100
          )
            key = node.getKey();
      });
      callbacks.current.onSection?.(key);
    };
    document.addEventListener("scroll", section, true);
    callbacks.current.onReady?.({
      markdown: () => editor.getEditorState().read(exportBody),
      heading: goHeading,
      comment: (id) => {
        const currentRoot = editor.getRootElement();
        if (!currentRoot) return;
        const target = currentRoot.querySelector(
          `[data-comment-id="${CSS.escape(id)}"]`,
        );
        target?.scrollIntoView({ block: "center", behavior: "smooth" });
        target?.classList.add("lex-comment-focus");
        setTimeout(() => target?.classList.remove("lex-comment-focus"), 1200);
      },
      removeComment: (id) =>
        editor.update(() => {
          for (const node of $getRoot().getAllTextNodes())
            for (const mark of node.getParents())
              if (mark.getType() === "wl-annotation" && "deleteID" in mark) {
                const annotation = mark as import("./nodes.tsx").AnnotationNode;
                annotation.deleteID(id);
                if (!annotation.getIDs().length) {
                  annotation
                    .getChildren()
                    .forEach((child) => annotation.insertBefore(child));
                  annotation.remove();
                }
              }
        }),
      removeChild: (id) =>
        editor.update(() => {
          const walk = (element: import("lexical").ElementNode) => {
            for (const node of element.getChildren()) {
              if (
                node.getType() === "wl-reference" &&
                (node as import("./nodes.tsx").ReferenceNode).isCard() &&
                (node as import("./nodes.tsx").ReferenceNode).getTarget() === id
              )
                node.remove();
              else if ($isElementNode(node)) walk(node);
            }
          };
          walk($getRoot());
          if (!$getRoot().getChildrenSize())
            $getRoot().append($createParagraphNode());
        }),
    });
    return () => {
      cancelAnimationFrame(initialFrame);
      cancelAnimationFrame(secondFrame);
      tasks();
      remove();
      keys();
      root.removeEventListener("compositionstart", start);
      root.removeEventListener("compositionend", end);
      root.removeEventListener("click", click);
      document.removeEventListener("scroll", section, true);
      callbacks.current.onReady?.(null);
    };
  }, [editor]);
  useEffect(() => {
    if (!props.mirror || props.mirror.revision === revision.current) return;
    revision.current = props.mirror.revision;
    const body = editor.getEditorState().read(exportBody);
    if (props.mirror.body === body) return;
    if (composing.current || editor.isComposing()) {
      pending.current = {
        body: props.mirror.body,
        baseline: pending.current?.baseline ?? body,
        external: pending.current?.external || props.mirror.external,
      };
      props.onMirrorPending?.(true);
    } else applyMirror(props.mirror.body, props.mirror.external);
  }, [props.mirror?.revision]);
  return null;
}
