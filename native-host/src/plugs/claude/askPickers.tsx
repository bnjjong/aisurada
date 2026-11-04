// The parts the first question box and every follow-up row share: the model and effort pickers (3.3) and the
// picture strip with its paste rules (3.2, 4.4). Adheres to Chapters 3 and 4 of withlog Part 4 specifications.
import { useCallback, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { ASK_EFFORTS, ASK_MODELS, IMAGE_MIME_EXTENSIONS, MAX_IMAGES, MAX_IMAGE_BYTES } from './askText.ts';
import { setAskEffort, setAskModel, useAskState } from './state.ts';

/** 3.3: two pickers that read and write the one model and effort of this window. */
export function AskOptionsRow() {
  const model = useAskState((state) => state.model);
  const effort = useAskState((state) => state.effort);
  return (
    <div className="ask-options-row">
      <select className="ask-option" aria-label="모델" value={model} onChange={(event) => setAskModel(event.target.value)}>
        {ASK_MODELS.map((item) => (
          <option key={item.id} value={item.id}>{item.label}</option>
        ))}
      </select>
      <select className="ask-option" aria-label="effort" value={effort} onChange={(event) => setAskEffort(event.target.value)}>
        {ASK_EFFORTS.map((item) => (
          <option key={item.id} value={item.id}>{item.label}</option>
        ))}
      </select>
    </div>
  );
}

export interface PickedImage {
  file: File;
  /** A data URL: the app's CSP does not open `blob:` for images (3.2). */
  preview: string;
}

export interface ImageRules {
  /** The note folder is known (the note is not an old single-file note). */
  hasNoteFolder: boolean;
  /** The way questions go is HTTP, which carries no pictures. */
  http: boolean;
}

/** 3.2: why a paste cannot be taken at all, or null when it can. */
export function imageRefusal(rules: ImageRules, native: boolean): string | null {
  if (!native) return '그림은 앱에서만 붙일 수 있어요.';
  if (!rules.hasNoteFolder) return '이 노트에는 그림을 둘 폴더가 없어요. 앱에서 한 번 열면 폴더로 바뀝니다.';
  if (rules.http) return '지금 묻기 길(HTTP)은 그림을 못 실어요.';
  return null;
}

/** 3.2: why one picture is not taken, or null. Type and size are seen when pasted, not when sent. */
export function pictureRefusal(file: { name: string; type: string; size: number }, taken: number): string | null {
  if (taken >= MAX_IMAGES) return '한 물음에 그림은 8장까지예요.';
  if (!(file.type in IMAGE_MIME_EXTENSIONS)) return `${file.name || '이 파일'} 은 못 붙여요.`;
  if (file.size > MAX_IMAGE_BYTES) return `그림 한 장은 5.0MB까지예요(이 그림은 ${(file.size / 1_000_000).toFixed(1)}MB).`;
  return null;
}

function readDataUrl(file: File): Promise<string> {
  return new Promise((resolve) => {
    const reader = new FileReader();
    reader.onload = () => resolve(typeof reader.result === 'string' ? reader.result : '');
    reader.onerror = () => resolve('');
    reader.readAsDataURL(file);
  });
}

/** The pictures of one box, the reason one was refused, and the paste handler. Empties when the box closes. */
export function useAskImages(rules: ImageRules) {
  const [images, setImages] = useState<PickedImage[]>([]);
  const [refusal, setRefusal] = useState<string | null>(null);

  const paste = useCallback(
    (event: React.ClipboardEvent) => {
      const files = [...(event.clipboardData?.files ?? [])].filter((file) => file.type.startsWith('image/'));
      if (files.length === 0) return; // a paste with no picture is left to the default
      event.preventDefault();
      const refused = imageRefusal(rules, isTauri());
      if (refused) {
        setRefusal(refused);
        return;
      }
      void (async () => {
        let count = images.length;
        const taken: PickedImage[] = [];
        let message: string | null = null;
        for (const file of files) {
          const why = pictureRefusal(file, count);
          if (why) {
            message = why;
            continue;
          }
          taken.push({ file, preview: await readDataUrl(file) });
          count += 1;
        }
        setRefusal(message);
        if (taken.length) setImages((prev) => [...prev, ...taken]);
      })();
    },
    [images.length, rules.hasNoteFolder, rules.http]
  );

  const remove = useCallback((index: number) => {
    setImages((prev) => prev.filter((_, at) => at !== index));
    setRefusal(null);
  }, []);
  const clear = useCallback(() => {
    setImages([]);
    setRefusal(null);
  }, []);
  return { images, refusal, paste, remove, clear };
}

/** The thumbnails with their remove buttons and one line for the reason a picture was refused. */
export function AskImageStrip({ images, refusal, onRemove }: { images: PickedImage[]; refusal: string | null; onRemove: (index: number) => void }) {
  if (images.length === 0 && !refusal) return null;
  return (
    <div className="ask-compose-images">
      {images.map((image, index) => (
        <span key={index} className="ask-image-thumb">
          <img src={image.preview} alt={image.file.name || '붙인 그림'} />
          <button type="button" aria-label="그림 빼기" onClick={() => onRemove(index)}>×</button>
        </span>
      ))}
      {refusal && <div className="ask-image-error">{refusal}</div>}
    </div>
  );
}
