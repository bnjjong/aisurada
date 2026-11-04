// Markdown rendering for model answers with symbol chips and folding.
// Adheres to Chapter 4 (4.11, 4.12) of withlog Part 4 Claude integration specifications.
import React, { useEffect, useRef, useState } from 'react';
import { sanitizeHtml } from '../../lib/sanitize.ts';
import { readingHTML } from '../../ui/ReadMarkdown.tsx';

interface ModelProseProps {
  content: string;
  className?: string;
  foldable?: boolean;
}

// 4.11: 기호만 든 인라인 코드(*, \ 따위)는 is-symbol 클래스를 부여해 굵게 그림
function tagSymbolCodeSpans(html: string): string {
  return html.replace(/<code>([^<]+)<\/code>/g, (match, codeText) => {
    // 디코딩 후 영문/숫자/한글이 전혀 없고 기호만 있는지 판별
    const clean = codeText
      .replace(/&amp;/g, '&')
      .replace(/&lt;/g, '<')
      .replace(/&gt;/g, '>')
      .replace(/&quot;/g, '"')
      .replace(/&#39;/g, "'")
      .trim();
    const hasAlphaNumeric = /[a-zA-Z0-9\u3131-\uD79D]/.test(clean);
    if (!hasAlphaNumeric && clean.length > 0) {
      return `<code class="is-symbol">${codeText}</code>`;
    }
    return match;
  });
}

export function ModelProse({ content, className = '', foldable = false }: ModelProseProps) {
  const [folded, setFolded] = useState(foldable);
  const [needsFold, setNeedsFold] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);

  const rawHtml = React.useMemo(() => {
    const rendered = readingHTML(content);
    const tagged = tagSymbolCodeSpans(rendered);
    return sanitizeHtml(tagged);
  }, [content]);

  // 4.12: 접는 높이 126px + 여유 21px = 147px 초과 시 접기 버튼 제공
  useEffect(() => {
    if (!foldable) return;
    const elem = containerRef.current;
    if (!elem) return;

    const checkHeight = () => {
      const scrollHeight = elem.scrollHeight;
      if (scrollHeight > 147) {
        setNeedsFold(true);
      } else {
        setNeedsFold(false);
      }
    };

    checkHeight();
    const observer = new ResizeObserver(checkHeight);
    observer.observe(elem);
    return () => observer.disconnect();
  }, [content, foldable]);

  // 4.11: 바깥 링크는 기본 브라우저로 열기
  const handleClick = (e: React.MouseEvent<HTMLDivElement>) => {
    const target = e.target as HTMLElement;
    const anchor = target.closest('a');
    if (!anchor) return;

    const href = anchor.getAttribute('href');
    if (!href) return;

    if (href.startsWith('http://') || href.startsWith('https://')) {
      e.preventDefault();
      // open in default browser
      import('@tauri-apps/api/core')
        .then(({ invoke }) => invoke('open_target', { path: href }))
        .catch(() => {});
    } else if (href.startsWith('#doc:')) {
      // #doc:<노트id> 링크는 기본 처리
    } else {
      // 세션 대화의 [파일:줄] 링크 등 상대경로는 무시
      e.preventDefault();
    }
  };

  return (
    <div className={`model-prose-container ${className}`}>
      <div
        ref={containerRef}
        className={`model-prose ${foldable && needsFold && folded ? 'ask-fold is-folded' : ''}`}
        style={foldable && needsFold && folded ? { maxHeight: '126px', overflow: 'hidden' } : undefined}
        dangerouslySetInnerHTML={{ __html: rawHtml }}
        onClick={handleClick}
      />
      {foldable && needsFold && (
        <button
          type="button"
          className="ask-fold-toggle"
          onClick={() => setFolded(!folded)}
        >
          {folded ? '펼치기' : '접기'}
        </button>
      )}
    </div>
  );
}
