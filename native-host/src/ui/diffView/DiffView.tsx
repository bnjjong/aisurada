import { Suspense, lazy, useCallback, useEffect, useMemo, useState } from "react";
import { lineStats } from "./lineStats.ts";

const DiffBody = lazy(() => import("./DiffBody.tsx"));
type Style = "unified" | "split";
type Tone = "change" | "neutral";
const STYLE_KEY = "ais.diffStyle.";
const COLORS: Record<Tone, [string, string]> = {
  change: ["빨강", "초록"],
  neutral: ["회색", "보라"],
};

function useComparisonStyle(name: string, defaultStyle: Style) {
  const [style, setStyle] = useState<Style>(() => {
    try {
      const value = localStorage.getItem(STYLE_KEY + name);
      if (value === "unified" || value === "split") return value;
    } catch {
      // 저장 공간이 막힌 창에서도 보기를 바꿀 수 있다.
    }
    return defaultStyle;
  });

  function changeStyle(next: Style) {
    setStyle(next);
    try {
      localStorage.setItem(STYLE_KEY + name, next);
    } catch {
      // 현재 창에서는 고른 보기를 유지한다.
    }
  }

  return [style, changeStyle] as const;
}

export function DiffView({ before, after, beforeLabel, afterLabel, label, rememberAs, defaultStyle = "unified", tone = "change", lang = "markdown", formal = false, sameMessage }: {
  before: string;
  after: string;
  beforeLabel: string;
  afterLabel: string;
  label: string;
  rememberAs: string;
  defaultStyle?: Style;
  tone?: Tone;
  lang?: "markdown" | "json";
  formal?: boolean;
  sameMessage?: string;
}) {
  const [style, changeStyle] = useComparisonStyle(rememberAs, defaultStyle);
  const [mounted, setMounted] = useState(false);
  useEffect(() => setMounted(true), []);
  const stats = useMemo(() => lineStats(before, after), [before, after]);
  const inputs = useMemo(() => ({ before, after, style, lang }), [before, after, style, lang]);
  const [rendered, setRendered] = useState<typeof inputs | null>(null);
  const onRendered = useCallback(() => setRendered(inputs), [inputs]);
  const [beforeColor, afterColor] = COLORS[tone];
  const count = `${beforeLabel}에만 ${stats.deleted}줄(${beforeColor}), ${afterLabel}에만 ${stats.added}줄(${afterColor})`;
  const long = stats.long ? ` 줄 ${stats.long}개는 너무 길어서 줄 안까지 갈라 보지 ${formal ? "않았습니다." : "않았어요."}` : "";
  const waiting = <p role="status">{formal ? "비교를 그리는 중입니다." : "비교를 그리는 중이에요."}</p>;

  return (
    <section className="ais-diff" aria-label={label} data-diff-style={style} data-tone={tone} data-diff-ready={rendered === inputs || undefined}>
      {stats.same ? (
        <p>{sameMessage ?? (formal ? "두 글이 글자까지 같습니다." : "두 글이 글자까지 같아요.")}</p>
      ) : (
        <>
          <div className="ais-diff-bar">
            <p>{count + (formal ? "입니다." : " 있어요.") + long}</p>
            <div className="ais-diff-styles" role="group" aria-label="보기 전환">
              <button type="button" aria-pressed={style === "unified"} onClick={() => changeStyle("unified")}>한 줄 보기</button>
              <button type="button" aria-pressed={style === "split"} onClick={() => changeStyle("split")}>나란히 보기</button>
            </div>
          </div>
          {mounted ? (
            <Suspense fallback={waiting}>
              <DiffBody {...inputs} onRendered={onRendered} />
            </Suspense>
          ) : waiting}
        </>
      )}
    </section>
  );
}
