// The parts the note panel and the underline cards draw the same way: one turn (4.2), the "요청 중" line (4.9)
// and the follow-up row (4.4). Adheres to Chapter 4 of aisurada Part 4 Claude integration specifications.
import { useContext, useEffect, useRef, useState } from 'react';
import { convertFileSrc, invoke, isTauri } from '@tauri-apps/api/core';
import { AskContext } from './askContext.ts';
import { AskImageStrip, AskOptionsRow, useAskImages } from './askPickers.tsx';
import { modelLabel } from './askText.ts';
import type { AskTurn } from './askText.ts';
import { ModelProse } from './modelProse.tsx';
import type { PendingAsk } from './state.ts';
import { useAskState } from './state.ts';

/** 4.2: the elapsed seconds with one decimal. */
export function elapsedSeconds(elapsedMs: number): string {
  return (elapsedMs / 1000).toFixed(1);
}

/** 4.2: the meta line of a turn. */
export function metaLine(turn: AskTurn): string {
  const base = `${modelLabel(turn.model)} · ${turn.effort} · ${elapsedSeconds(turn.elapsedMs)}초`;
  return turn.sessionLines ? `${base} · 작성 세션 대화 ${turn.sessionLines}줄 함께 보냄` : base;
}

function pictureSource(baseDir: string, relative: string): string {
  return convertFileSrc(`${baseDir}/${relative}`);
}

/** 4.2: one turn from top to bottom. `fold` folds a long answer; only the note panel does that (4.12). */
export function AskTurnView({ turn, fold }: { turn: AskTurn; fold: boolean }) {
  const { baseDir } = useContext(AskContext);
  const pictures = turn.images ?? [];
  return (
    <div className={`ask-turn${turn.error ? ' is-error' : ''}`} data-turn-id={turn.id}>
      {turn.quote && <div className="ask-quote" title={turn.quote}>{turn.quote}</div>}
      <div className="ask-question">{turn.question}</div>
      {pictures.length > 0 &&
        (baseDir && isTauri() ? (
          <div className="ask-images">
            {pictures.map((relative) => (
              <button
                type="button"
                key={relative}
                className="ask-image-thumb"
                aria-label="붙인 그림 열기"
                onClick={() => void invoke('open_in_default_app', { path: `${baseDir}/${relative}` }).catch(() => {})}
              >
                <img src={pictureSource(baseDir, relative)} alt="붙인 그림" />
              </button>
            ))}
          </div>
        ) : (
          <div className="ask-images">그림 {pictures.length}장 붙임</div>
        ))}
      {turn.error ? <div className="ask-error">{turn.error}</div> : <div className="ask-answer"><ModelProse content={turn.answer} foldable={fold} /></div>}
      <div className="ask-meta">{metaLine(turn)}</div>
      {turn.unsaved && <div className="ask-unsaved">이 답은 vault 에 저장되지 않았어요. 앱을 닫으면 사라져요.</div>}
    </div>
  );
}

/** 4.9: the text of the "요청 중" line from the record and the clock. */
export function pendingText(record: Pick<PendingAsk, 'at' | 'queued' | 'timeoutSeconds'>, now: number): string {
  const seconds = Math.max(0, Math.floor((now - record.at) / 1000));
  if (record.queued) return `앞 물음을 기다리는 중이에요… ${seconds}초`;
  const limit = record.timeoutSeconds ?? 3600;
  return seconds >= limit / 2 ? `요청 중이에요… ${seconds}초 · ${limit}초까지 기다려요` : `요청 중이에요… ${seconds}초`;
}

/** 4.9: the line of a question in flight. The second count is made from the clock every second, not counted. */
export function PendingLine({ record }: { record: PendingAsk }) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, []);
  return (
    <div className={`ask-pending${record.queued ? ' is-queued' : ''}`}>
      <span className="ask-pending-label">{pendingText(record, now)}</span>
      <span className="ask-pending-bar" aria-hidden="true" />
    </div>
  );
}

/**
 * 4.4: the follow-up row. The pickers, the picture strip, a multi-line box and the button. It sends with Cmd or Ctrl
 * and Enter, clears itself after sending, and asks nothing when the question is blank.
 */
export function FollowUpRow({
  onSend,
  onClose,
  autoFocus = false,
}: {
  onSend: (question: string, images?: File[]) => void;
  /** Called by Esc in an empty box. */
  onClose?: () => void;
  autoFocus?: boolean;
}) {
  const context = useContext(AskContext);
  const runtime = useAskState((state) => state.runtime);
  const pictures = useAskImages({ hasNoteFolder: Boolean(context.baseDir) && !/\.(?:md|markdown|html|json)$/i.test(context.baseDir ?? ''), http: runtime === 'http' });
  const [text, setText] = useState('');
  const box = useRef<HTMLTextAreaElement>(null);
  useEffect(() => {
    if (autoFocus) box.current?.focus();
  }, [autoFocus]);

  const send = () => {
    const question = text.trim();
    if (!question) return;
    onSend(question, pictures.images.length > 0 ? pictures.images.map((image) => image.file) : undefined);
    setText('');
    pictures.clear();
  };

  return (
    <div className="ask-followup">
      <AskOptionsRow />
      <AskImageStrip images={pictures.images} refusal={pictures.refusal} onRemove={pictures.remove} />
      <textarea
        ref={box}
        className="ask-followup-input"
        aria-label="이어 묻는 내용"
        value={text}
        placeholder="이어서 물어볼 것… (⌘V로 그림 붙이기, ⌘/Ctrl+Enter로 묻기)"
        onChange={(event) => setText(event.target.value)}
        onPaste={pictures.paste}
        onKeyDown={(event) => {
          if (event.key === 'Enter' && (event.metaKey || event.ctrlKey)) {
            event.preventDefault();
            send();
          } else if (event.key === 'Escape') {
            // 4.3, 4.4: this box took the Esc. An empty box closes; a box with a question stays. Either way the
            // default is stopped so an outer modal does not close with it. Propagation is left alone: a sub page
            // reads the mark and steps back.
            event.preventDefault();
            if (!text.trim()) onClose?.();
          }
        }}
      />
      <button type="button" className="ask-followup-send" disabled={!text.trim()} onClick={send}>이어 묻기</button>
    </div>
  );
}
