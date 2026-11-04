// Markdown rendering for model answers with symbol chips and folding.
// Adheres to Chapter 4 (4.11, 4.12) of withlog Part 4 Claude integration specifications.
import React, { useEffect, useMemo, useRef, useState } from 'react';
import { message } from '@tauri-apps/plugin-dialog';
import { openUrl } from '@tauri-apps/plugin-opener';
import { sanitizeHtml } from '../../lib/sanitize.ts';
import { readingHTML } from '../../ui/ReadMarkdown.tsx';
import { openDocLink } from './docLinks.ts';

interface ModelProseProps {
  content: string;
  className?: string;
  foldable?: boolean;
}

// 4.11: a code span with no letter or digit in it is a symbol chip (`is-symbol`). A character reference
// such as `&lt;` counts as one symbol.
function tagSymbolCodeSpans(html: string): string {
  return html.replace(/<code>([^<]+)<\/code>/g, (match, codeText: string) => {
    const decoded = codeText
      .replace(/&lt;/g, '<')
      .replace(/&gt;/g, '>')
      .replace(/&quot;/g, '"')
      .replace(/&#39;/g, "'")
      .replace(/&amp;/g, '&')
      .trim();
    if (decoded.length > 0 && !/[\p{L}\p{N}]/u.test(decoded)) return `<code class="is-symbol">${codeText}</code>`;
    return match;
  });
}

/** What a click on a link in an answer does (4.11): outside addresses open in the browser, `#doc:` previews. */
export type ProseLink =
  | { kind: 'browser'; url: string }
  | { kind: 'doc'; id: string }
  | { kind: 'none' };

export function classifyProseLink(rawHref: string | null): ProseLink {
  const href = (rawHref ?? '').trim();
  if (/^(?:https?|mailto|tel):/i.test(href)) return { kind: 'browser', url: href };
  const doc = /^#doc:(.+)$/i.exec(href);
  if (doc) return { kind: 'doc', id: doc[1] };
  return { kind: 'none' };
}

const cache = new Map<string, string>();

/** The same text is not converted again: a card re-renders every time the pointer enters it (4.11). */
export function proseHtml(content: string): string {
  const hit = cache.get(content);
  if (hit !== undefined) return hit;
  const html = sanitizeHtml(tagSymbolCodeSpans(readingHTML(content)));
  if (cache.size > 200) cache.clear();
  cache.set(content, html);
  return html;
}

function cssPixels(element: Element, name: string, fallback: number): number {
  const value = parseFloat(getComputedStyle(element).getPropertyValue(name));
  return Number.isFinite(value) ? value : fallback;
}

export function ModelProse({ content, className = '', foldable = false }: ModelProseProps) {
  const [folded, setFolded] = useState(foldable);
  const [needsFold, setNeedsFold] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const html = useMemo(() => proseHtml(content), [content]);

  // 4.12: fold when the rendered height is over the fold height plus one line of slack. Both numbers live in
  // CSS variables (`--ask-fold-height`, `--ask-fold-slack`) and are read here, so there is one copy of them.
  useEffect(() => {
    if (!foldable) return;
    const element = containerRef.current;
    if (!element) return;
    const measure = () => {
      const limit = cssPixels(element, '--ask-fold-height', 126) + cssPixels(element, '--ask-fold-slack', 21);
      setNeedsFold(element.scrollHeight > limit);
    };
    measure();
    const observer = new ResizeObserver(measure);
    observer.observe(element);
    return () => observer.disconnect();
  }, [content, foldable]);

  const handleClick = (event: React.MouseEvent<HTMLDivElement>) => {
    const anchor = (event.target as HTMLElement).closest('a');
    if (!anchor) return;
    const link = classifyProseLink(anchor.getAttribute('href'));
    // Any other link does nothing: a relative `[file:line](path)` has no folder to resolve against here.
    event.preventDefault();
    if (link.kind === 'browser') {
      void openUrl(link.url).catch(() => {});
    } else if (link.kind === 'doc') {
      if (!openDocLink(link.id)) void message('노트를 찾을 수 없어요', { title: 'withlog', kind: 'warning' }).catch(() => {});
    }
  };

  return (
    <div className={`model-prose-container ${className}`}>
      <div
        ref={containerRef}
        className={`model-prose ${foldable && needsFold && folded ? 'ask-fold is-folded' : ''}`}
        dangerouslySetInnerHTML={{ __html: html }}
        onClick={handleClick}
      />
      {foldable && needsFold && (
        <button type="button" className="ask-fold-toggle" onClick={() => setFolded(!folded)}>
          {folded ? '펼치기' : '접기'}
        </button>
      )}
    </div>
  );
}
