// The question box that opens over a selection, a table row or an HTML box.
// Adheres to Chapter 3 (3.2, 3.3) of aisurada Part 4 Claude integration specifications.
// It is the part 3 comment compose box with three additions: the pickers, a picture strip and picture paste.
import { useContext, useEffect, useRef, useState } from 'react';
import { AskContext } from './askContext.ts';
import { AskImageStrip, AskOptionsRow, useAskImages } from './askPickers.tsx';
import { useAskState } from './state.ts';

export { ASK_EFFORTS, ASK_MODELS } from './askText.ts';

export interface AskComposeState {
  left: number;
  top: number;
  quote: string;
  /** An HTML box asks with its own question id (`html:<path>`) and plants no underline. */
  markId?: string;
}

export interface AskComposePopoverProps {
  state: AskComposeState;
  onSubmit: (question: string, images?: File[]) => void;
  onCancel: () => void;
}

const MOUSE = 'comment-compose-popover ask-compose-popover';

export function AskComposePopover({ state, onSubmit, onCancel }: AskComposePopoverProps) {
  const context = useContext(AskContext);
  const runtime = useAskState((store) => store.runtime);
  const [question, setQuestion] = useState('');
  const pictures = useAskImages({ hasNoteFolder: Boolean(context.baseDir) && !/\.(?:md|markdown|html|json)$/i.test(context.baseDir ?? ''), http: runtime === 'http' });
  const box = useRef<HTMLDivElement>(null);
  const textarea = useRef<HTMLTextAreaElement>(null);
  const previousFocus = useRef<HTMLElement | null>(null);

  const cancelAndRestore = () => {
    onCancel();
    if (previousFocus.current?.isConnected) {
      previousFocus.current.focus();
    }
  };

  useEffect(() => {
    if (!previousFocus.current) {
      previousFocus.current = document.activeElement as HTMLElement | null;
    }
    textarea.current?.focus();
  }, []);

  // 3.2: a press outside the box closes it unless writing a question or pictures are held.
  useEffect(() => {
    const outside = (event: PointerEvent) => {
      if (box.current && !box.current.contains(event.target as Node)) {
        if (!question.trim() && pictures.images.length === 0) cancelAndRestore();
      }
    };
    document.addEventListener('pointerdown', outside);
    return () => document.removeEventListener('pointerdown', outside);
  }, [onCancel, question, pictures.images.length]);

  const submit = () => {
    const trimmed = question.trim();
    if (!trimmed) return;
    onSubmit(trimmed, pictures.images.length > 0 ? pictures.images.map((image) => image.file) : undefined);
  };

  return (
    <div
      ref={box}
      className={MOUSE}
      style={{
        position: 'fixed',
        left: typeof window !== 'undefined' ? Math.max(10, Math.min(state.left, window.innerWidth - 380)) : Math.max(10, state.left),
        top: typeof window !== 'undefined' ? Math.max(10, Math.min(state.top, window.innerHeight - 280)) : Math.max(10, state.top),
        zIndex: 60,
      }}
      onKeyDown={(event) => {
        if (event.key === 'Escape') {
          event.preventDefault();
          event.stopPropagation();
          cancelAndRestore();
        }
        // 3.2: Cmd or Ctrl with Enter asks; a plain Enter is a new line.
        if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
          event.preventDefault();
          event.stopPropagation();
          submit();
        }
      }}
    >
      {state.quote && <blockquote>{state.quote}</blockquote>}
      <AskOptionsRow />
      <AskImageStrip images={pictures.images} refusal={pictures.refusal} onRemove={pictures.remove} />
      <textarea
        ref={textarea}
        className="comment-input"
        aria-label="묻는 내용"
        value={question}
        placeholder="이 대목에 대해 물어볼 것… (⌘V로 그림 붙이기, ⌘/Ctrl+Enter로 묻기)"
        onChange={(event) => setQuestion(event.target.value)}
        onPaste={pictures.paste}
      />
      <div>
        <button type="button" className="link-popover-btn" onClick={onCancel}>취소</button>
        <button type="button" className="link-popover-btn primary" disabled={!question.trim()} onClick={submit}>묻기</button>
      </div>
    </div>
  );
}
