// Floating popover for asking Claude on highlighted text or table rows.
// Adheres to Chapter 3 (3.2, 3.3) of withlog Part 4 Claude integration specifications.
import React, { useEffect, useRef, useState } from 'react';

export interface AskComposeState {
  left: number;
  top: number;
  quote: string;
}

export interface AskComposePopoverProps {
  state: AskComposeState;
  onSubmit: (question: string, images?: File[]) => void;
  onCancel: () => void;
}

const PREF_KEY_MODEL = 'withlog.ask.pref.model';
const PREF_KEY_EFFORT = 'withlog.ask.pref.effort';

export const ASK_MODELS = [
  { id: 'claude-sonnet-5-5', label: 'Sonnet 3.5 / 5.5' },
  { id: 'claude-haiku-5-5', label: 'Haiku 3.5 / 5.5' },
  { id: 'claude-opus-5-5', label: 'Opus 3.5 / 5.5' },
];

export const ASK_EFFORTS = [
  { id: 'low', label: '낮음' },
  { id: 'medium', label: '보통' },
  { id: 'high', label: '높음' },
];

const MAX_IMAGES = 8;
const MAX_IMAGE_BYTES = 5_000_000;

export function AskComposePopover({
  state,
  onSubmit,
  onCancel,
}: AskComposePopoverProps) {
  const [question, setQuestion] = useState('');
  const [model, setModel] = useState(() => {
    return (
      (typeof localStorage !== 'undefined' &&
        localStorage.getItem(PREF_KEY_MODEL)) ||
      'claude-sonnet-5-5'
    );
  });
  const [effort, setEffort] = useState(() => {
    return (
      (typeof localStorage !== 'undefined' &&
        localStorage.getItem(PREF_KEY_EFFORT)) ||
      'low'
    );
  });
  const [images, setImages] = useState<File[]>([]);
  const [imageError, setImageError] = useState<string | null>(null);

  const textareaRef = useRef<HTMLTextAreaElement>(null);
  const fileInputRef = useRef<HTMLInputElement>(null);
  const popoverRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    textareaRef.current?.focus();
  }, []);

  const handleModelChange = (newModel: string) => {
    setModel(newModel);
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(PREF_KEY_MODEL, newModel);
    }
  };

  const handleEffortChange = (newEffort: string) => {
    setEffort(newEffort);
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(PREF_KEY_EFFORT, newEffort);
    }
  };

  const handleAddImages = (files: FileList | null) => {
    if (!files || files.length === 0) return;
    setImageError(null);

    const validNewFiles: File[] = [];
    for (let i = 0; i < files.length; i++) {
      const file = files[i];
      if (file.size > MAX_IMAGE_BYTES) {
        setImageError(`그림 크기가 5MB를 넘어요: ${file.name}`);
        continue;
      }
      validNewFiles.push(file);
    }

    if (images.length + validNewFiles.length > MAX_IMAGES) {
      setImageError(`그림은 한 물음에 최대 ${MAX_IMAGES}장까지 첨부할 수 있어요.`);
      return;
    }

    setImages((prev) => [...prev, ...validNewFiles]);
  };

  const handleRemoveImage = (index: number) => {
    setImages((prev) => prev.filter((_, i) => i !== index));
    setImageError(null);
  };

  const handlePaste = (e: React.ClipboardEvent<HTMLTextAreaElement>) => {
    if (e.clipboardData.files && e.clipboardData.files.length > 0) {
      const imgFiles = Array.from(e.clipboardData.files).filter((f) =>
        f.type.startsWith('image/')
      );
      if (imgFiles.length > 0) {
        e.preventDefault();
        const dt = new DataTransfer();
        imgFiles.forEach((f) => dt.items.add(f));
        handleAddImages(dt.files);
      }
    }
  };

  const handleSubmit = (e?: React.FormEvent) => {
    if (e) e.preventDefault();
    const trimmed = question.trim();
    if (!trimmed) return;
    onSubmit(trimmed, images.length > 0 ? images : undefined);
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLTextAreaElement>) => {
    if (e.key === 'Escape') {
      e.preventDefault();
      onCancel();
    } else if (e.key === 'Enter' && (e.metaKey || e.ctrlKey || !e.shiftKey)) {
      e.preventDefault();
      handleSubmit();
    }
  };

  return (
    <div
      ref={popoverRef}
      className="comment-compose-popover ask-compose-popover"
      style={{
        left: Math.max(10, state.left),
        top: Math.max(10, state.top),
        zIndex: 100,
      }}
    >
      {state.quote && (
        <div className="ask-quote-preview" title={state.quote}>
          “{state.quote.length > 80 ? state.quote.slice(0, 80) + '…' : state.quote}”
        </div>
      )}

      <div className="ask-options-row">
        <select
          value={model}
          onChange={(e) => handleModelChange(e.target.value)}
          className="ask-model-select"
          aria-label="Claude 모델 선택"
        >
          {ASK_MODELS.map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
        </select>

        <select
          value={effort}
          onChange={(e) => handleEffortChange(e.target.value)}
          className="ask-effort-select"
          aria-label="Effort 선택"
        >
          {ASK_EFFORTS.map((eff) => (
            <option key={eff.id} value={eff.id}>
              {eff.label}
            </option>
          ))}
        </select>

        <button
          type="button"
          className="ask-attach-btn"
          title="그림 첨부 (최대 8장)"
          onClick={() => fileInputRef.current?.click()}
        >
          📎
        </button>
        <input
          ref={fileInputRef}
          type="file"
          accept="image/png,image/jpeg,image/gif,image/webp,image/avif,image/bmp,image/svg+xml"
          multiple
          style={{ display: 'none' }}
          onChange={(e) => handleAddImages(e.target.files)}
        />
      </div>

      {images.length > 0 && (
        <div className="ask-compose-images">
          {images.map((file, idx) => (
            <span key={idx} className="ask-image-tag">
              {file.name}
              <button
                type="button"
                onClick={() => handleRemoveImage(idx)}
                aria-label="그림 삭제"
              >
                ×
              </button>
            </span>
          ))}
        </div>
      )}

      {imageError && <div className="ask-image-error">{imageError}</div>}

      <textarea
        ref={textareaRef}
        value={question}
        onChange={(e) => setQuestion(e.target.value)}
        onKeyDown={handleKeyDown}
        onPaste={handlePaste}
        placeholder="이 대목에 대해 Claude에게 물어보세요 (Enter로 전송)"
        rows={3}
      />

      <div className="ask-compose-actions">
        <button type="button" onClick={onCancel}>
          취소
        </button>
        <button
          type="button"
          className="button-primary"
          disabled={!question.trim()}
          onClick={() => handleSubmit()}
        >
          AI에게 묻기
        </button>
      </div>
    </div>
  );
}
