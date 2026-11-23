import { useEffect, useMemo, useState } from "react";
import { i18n } from "@pierre/diffs";
import type { FileDiffOptions, ThemeTypes } from "@pierre/diffs";
import { MultiFileDiff } from "@pierre/diffs/react";
import { INLINE_DIFF_MAX, comparisonTexts } from "./lineStats.ts";

i18n.set({
  locale: "ko",
  messages: {
    unmodifiedLines: "{count, plural, other {같은 줄 #개}}",
    unknownUnmodifiedLines: "같은 줄이 더 있을 수 있어요",
    expandAll: "모두 펼치기",
    expandLine: "줄 전체 보기",
    noNewlineAtEndOfFile: "파일 끝에 줄바꿈이 없어요",
  },
});

function appTheme(): ThemeTypes {
  const theme = document.documentElement.dataset.theme;
  return theme === "light" || theme === "dark" ? theme : "system";
}

export default function DiffBody({ before, after, style, lang, onRendered }: {
  before: string;
  after: string;
  style: "unified" | "split";
  lang: "markdown" | "json";
  onRendered(): void;
}) {
  const [themeType, setTheme] = useState(appTheme);
  useEffect(() => {
    const observer = new MutationObserver(() => setTheme(appTheme()));
    observer.observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });
    return () => observer.disconnect();
  }, []);
  const [oldFile, newFile] = useMemo(() => {
    const [oldText, newText] = comparisonTexts(before, after);
    const name = lang === "json" ? "diff.json" : "diff.md";
    return [{ name, contents: oldText, lang }, { name, contents: newText, lang }];
  }, [before, after, lang]);
  const options = useMemo<FileDiffOptions<undefined, undefined>>(() => ({
    diffStyle: style,
    overflow: "wrap",
    themeType,
    theme: { light: "pierre-light", dark: "pierre-dark" },
    lineDiffType: "word-alt",
    maxLineDiffLength: INLINE_DIFF_MAX,
    disableFileHeader: true,
    hunkSeparators: "line-info",
    onPostRender: (node, _instance, phase) => {
      if (phase !== "unmount" && (node.shadowRoot ?? node).querySelector("[data-line-type]")) {
        onRendered();
      }
    },
  }), [style, themeType, onRendered]);
  return <MultiFileDiff oldFile={oldFile} newFile={newFile} options={options} className="ais-diff-body" />;
}
