// Implementation of @claude-plug/ask for Part 4 Claude-enabled builds.
// Adheres to Chapters 2, 3, 4 of withlog Part 4 Claude integration specifications.
import React, { createContext, useEffect, useRef, useState } from 'react';
import type { ReactNode } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Note } from '../../workspace/types.ts';
import { AskComposePopover } from './askPopover.tsx';
import type { AskComposePopoverProps, AskComposeState } from './askPopover.tsx';
import { AskHoverCard } from './askHoverCard.tsx';
import type { AskHoverCardProps } from './askHoverCard.tsx';
import { AskPanel } from './askPanel.tsx';
import type { AskThread, AskTurn } from './askPanel.tsx';
import {
  setPendingAsk,
  markNoteUnseen,
  incUnsavedAnswers,
  clearNoteUnseen,
} from './state.ts';

export type {
  AskComposePopoverProps,
  AskComposeState,
  AskHoverCardProps,
  AskThread,
  AskTurn,
};
export { AskComposePopover, AskHoverCard, AskPanel };

export interface NoteBodyPort {
  kind?: 'main' | 'sub';
  id?: string;
  parent?: string;
  write(body: string): Promise<void> | void;
  saveDraft?(body: string): void;
  read?(): string;
  autosaveBody?(body?: string): Promise<void> | void;
  currentBody?(): string;
  commitTitle?(title?: string): Promise<boolean | void> | void;
  protect?(enabled?: boolean): void;
  flush?(mode?: string): Promise<any> | void;
  discard?(): Promise<void> | void;
  keep?(): Promise<any> | void;
  restore?(body: string): Promise<any> | void;
  conflicted?(): boolean;
  editing?(): boolean;
}

export interface AskEditorProps {
  onAskAi?: (
    quote: string,
    question: string,
    turnId: string,
    images?: File[]
  ) => void;
  askDocId?: string;
  onAskFollowUp?: (
    question: string,
    underlineId: string,
    images?: File[]
  ) => void;
}

export interface NoteAskInput {
  doc: Note;
  port: NoteBodyPort;
  baseDir?: string;
  currentDraft: () => string;
  onFollowUp?: (
    question: string,
    underlineId: string,
    images?: File[]
  ) => void;
}

export interface NoteAskOutput {
  editor: AskEditorProps;
  panel: ReactNode;
}

// --------------------------------------------------------------------------
// Queue and Per-note Ask Execution
// --------------------------------------------------------------------------

// 3.5: 같은 노트의 물음은 순서대로 줄을 서서 앞 물음이 끝나야 뒤 물음이 claude에 감
const noteQueues = new Map<string, Promise<void>>();

function enqueueNoteTask(noteKey: string, task: () => Promise<void>) {
  const current = noteQueues.get(noteKey) || Promise.resolve();
  const next = current
    .then(task)
    .catch((err) => {
      console.error('Ask queue execution error on note', noteKey, err);
    })
    .finally(() => {
      if (noteQueues.get(noteKey) === next) {
        noteQueues.delete(noteKey);
      }
    });
  noteQueues.set(noteKey, next);
}

// 2.1: 밑줄 id 모양 ak<시각36진수>-<uuid8자>
export function generateUnderlineId(): string {
  const time36 = Date.now().toString(36);
  const uuid8 = Math.random().toString(36).slice(2, 10);
  return `ak${time36}-${uuid8}`;
}

interface SidecarCallResult {
  status: number;
  stdout: string;
  stderr: string;
}

export function useNoteAsk(props?: NoteAskInput): NoteAskOutput {
  const doc = props?.doc;
  const parent = props?.port.parent;
  const noteKey = doc ? (parent ? `${parent}:${doc.id}` : doc.id) : '';

  const [thread, setThread] = useState<AskThread | null>(null);
  const [loadError, setLoadError] = useState<string | null>(null);
  const threadRef = useRef<AskThread | null>(null);
  threadRef.current = thread;

  // 1. 초기 ask.json 로드
  useEffect(() => {
    if (!doc?.id) return;
    let cancelled = false;

    const loadThread = async () => {
      try {
        const argv = parent ? ['get-ask', parent, doc.id] : ['get-ask', doc.id];
        const res = await invoke<SidecarCallResult>('sidecar_call', { argv });
        if (cancelled) return;

        if (res.status === 0) {
          const stdoutTrimmed = res.stdout.trim();
          if (stdoutTrimmed === 'null' || !stdoutTrimmed) {
            setThread(null);
          } else {
            const parsed = JSON.parse(stdoutTrimmed);
            setThread(parsed);
          }
          setLoadError(null);
        } else {
          setLoadError(res.stderr.trim() || '문답 파일을 읽지 못했어요.');
        }
      } catch (err: any) {
        if (!cancelled) {
          setLoadError(String(err));
        }
      }
    };

    loadThread();
    clearNoteUnseen(noteKey);

    return () => {
      cancelled = true;
    };
  }, [doc?.id, parent, noteKey]);

  // 2. onAskAi 핸들러
  const handleAskAi = (
    quote: string,
    question: string,
    providedTurnId: string,
    images?: File[],
    overrideMarkId?: string
  ) => {
    if (!doc?.id) return;
    const turnId = providedTurnId || generateUnderlineId();
    const markId = overrideMarkId || turnId;

    // 3.11: 묻는 순간 브라우저 저장소 pending 에 등록
    setPendingAsk(noteKey, turnId, {
      queued: true,
      askedAt: Date.now(),
      markId,
      quote: quote || undefined,
    });

    enqueueNoteTask(noteKey, async () => {
      // 큐에서 작업 차례가 됨
      setPendingAsk(noteKey, turnId, {
        queued: false,
        askedAt: Date.now(),
        markId,
        quote: quote || undefined,
      });

      const currentThread = threadRef.current;
      const sessionId =
        currentThread?.sessionId ||
        'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, (c) => {
          const r = (Math.random() * 16) | 0;
          return (c === 'x' ? r : (r & 0x3) | 0x8).toString(16);
        });

      const model =
        (typeof localStorage !== 'undefined' &&
          localStorage.getItem('withlog.ask.pref.model')) ||
        'claude-sonnet-5-5';
      const effort =
        (typeof localStorage !== 'undefined' &&
          localStorage.getItem('withlog.ask.pref.effort')) ||
        'low';

      // 그림 첨부 파일 저장 (2.8)
      const imagePaths: string[] = [];
      if (images && images.length > 0) {
        for (const file of images) {
          try {
            const ext = file.name.split('.').pop()?.toLowerCase() || 'png';
            const imgFileName = `ask-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}.${ext}`;
            const arrayBuf = await file.arrayBuffer();
            const bytes = Array.from(new Uint8Array(arrayBuf));
            await invoke('write_attachment_bytes', {
              parent: parent ? `${parent}/${doc.id}` : doc.id,
              name: imgFileName,
              bytes,
            });
            imagePaths.push(`attach/${imgFileName}`);
          } catch (e) {
            console.error('Failed to write attachment image', e);
          }
        }
      }

      // 3.4 & 3.6 프롬프트 조립
      let promptBuilder = '';
      promptBuilder += `노트: ${parent ? `${parent}/${doc.id}` : doc.id}\n`;
      if (doc.agenda) {
        promptBuilder += `아젠다: ${doc.agenda}\n`;
      }

      // D4 고침: 실타래 회차 중 성공한 회차가 0개일 때만 작성 세션 대화(authorSessionBundle) 묶음 포함
      const successfulTurnsCount = (currentThread?.turns || []).filter(
        (t) => !t.error && t.answer
      ).length;
      let sessionLines: number | undefined;

      if (successfulTurnsCount === 0 && doc.sessionId) {
        // 작성 세션 대화 묶음 조회 시도
        try {
          const sessionRes = await invoke<SidecarCallResult>('sidecar_call', {
            argv: ['read-session', doc.sessionId, 'limit=50'],
          });
          if (sessionRes.status === 0 && sessionRes.stdout) {
            const sessData = JSON.parse(sessionRes.stdout);
            if (sessData.messages && sessData.messages.length > 0) {
              promptBuilder += '\n## 작성 세션 대화\n\n';
              for (const msg of sessData.messages) {
                const role = msg.author === 'user' ? '사용자' : 'Claude';
                promptBuilder += `### ${role}\n${msg.content}\n\n`;
              }
              sessionLines = sessData.messages.length;
            }
          }
        } catch {
          // ignore session reading error
        }
      }

      if (quote) {
        promptBuilder += `\n## 물은 대목\n\n> ${quote}\n\n`;
      }
      promptBuilder += `\n## 질문\n\n${question}\n`;

      const isResume = (currentThread?.turns?.length || 0) > 0;
      const vaultPath = props?.baseDir || '';

      // 5장 호스트 ask_claude 부름
      let answer = '';
      let error: string | undefined;
      let elapsedMs = 0;

      try {
        const askResp = await invoke<{
          ok: boolean;
          answer: string;
          error?: string;
          elapsed_ms: number;
        }>('ask_claude', {
          vault: vaultPath,
          sessionId,
          model,
          effort,
          prompt: promptBuilder,
          resume: isResume,
          timeoutSeconds: 3600,
          cwd: props?.baseDir,
          persona: `너는 사용자의 withlog 노트 안에서 질문을 받는다. 노트는 「${doc.title || '제목 없음'}」(${doc.id})이다.`,
        });

        elapsedMs = askResp.elapsed_ms || 0;
        if (askResp.ok) {
          answer = askResp.answer;
        } else {
          error = askResp.error || 'Claude 응답을 받지 못했어요.';
        }
      } catch (err: any) {
        error = String(err);
      }

      // 2장 엔진 append-ask-turn 호출로 ask.json 영속화
      const appendArgv = [
        'append-ask-turn',
        ...(parent ? [parent, doc.id] : [doc.id]),
        `session-id=${sessionId}`,
        `turn-id=${turnId}`,
        `mark-id=${markId}`,
        `asked-at=${new Date().toISOString()}`,
        `question=${question}`,
        `answer=${answer}`,
        `model=${model}`,
        `effort=${effort}`,
        `elapsed-ms=${elapsedMs}`,
      ];

      if (quote) {
        appendArgv.push(`quote=${quote}`);
      }
      if (error) {
        appendArgv.push(`error=${error}`);
      }
      if (sessionLines) {
        appendArgv.push(`session-lines=${sessionLines}`);
      }
      if (imagePaths.length > 0) {
        appendArgv.push(`images=${imagePaths.join(',')}`);
      }

      let appendSuccess = false;
      try {
        const appendRes = await invoke<SidecarCallResult>('sidecar_call', {
          argv: appendArgv,
        });
        if (appendRes.status === 0 && appendRes.stdout) {
          const updatedThread = JSON.parse(appendRes.stdout);
          setThread(updatedThread);
          appendSuccess = true;
        } else {
          console.error('append-ask-turn failed:', appendRes.stderr);
        }
      } catch (e) {
        console.error('append-ask-turn call error:', e);
      }

      // 저장이 실패한 경우 메모리에만 unsaved 플래그를 달아 회차 추가 (3.10)
      if (!appendSuccess) {
        incUnsavedAnswers();
        const fallbackTurn: AskTurn = {
          id: turnId,
          markId,
          askedAt: new Date().toISOString(),
          quote: quote || undefined,
          question,
          answer,
          error,
          model,
          effort,
          elapsedMs,
          sessionLines,
          images: imagePaths.length > 0 ? imagePaths : undefined,
          unsaved: true,
        };

        const existingTurns = currentThread?.turns ? [...currentThread.turns] : [];
        existingTurns.push(fallbackTurn);
        setThread({
          noteId: doc.id,
          sessionId,
          turns: existingTurns,
        });
      }

      // pending 해제 및 unseen 표시 (탭의 문답 점)
      setPendingAsk(noteKey, turnId, null);
      markNoteUnseen(noteKey, turnId);
    });
  };

  const handleAskFollowUp = (
    followUpQuestion: string,
    underlineId: string,
    images?: File[]
  ) => {
    handleAskAi('', followUpQuestion, generateUnderlineId(), images, underlineId);
  };

  return {
    editor: {
      askDocId: doc?.id,
      onAskAi: (q, ques, tid, imgs) => handleAskAi(q, ques, tid, imgs),
      onAskFollowUp: handleAskFollowUp,
    },
    panel: (
      <AskPanel
        noteKey={noteKey}
        thread={thread}
        loadError={loadError}
        onFollowUp={handleAskFollowUp}
      />
    ),
  };
}

// --------------------------------------------------------------------------
// UI Helper Frame & Editor Plugin
// --------------------------------------------------------------------------

const AskContext = createContext<{
  baseDir?: string;
  noteId?: string;
}>({});

export function NoteAskFrame({
  children,
  baseDir,
  noteId,
}: {
  baseDir?: string;
  noteId?: string;
  children?: ReactNode;
}): ReactNode {
  return (
    <AskContext.Provider value={{ baseDir, noteId }}>
      {children}
    </AskContext.Provider>
  );
}

export function EditorAskPlugins(props?: {
  docId?: string;
  onAsk?: (
    quote: string,
    question: string,
    turnId: string,
    images?: File[]
  ) => void;
  onFollowUp?: (
    question: string,
    underlineId: string,
    images?: File[]
  ) => void;
}): React.JSX.Element | null {
  const [hoveredMark, setHoveredMark] = useState<{
    turnId: string;
    left: number;
    top: number;
  } | null>(null);

  useEffect(() => {
    const handleMouseOver = (e: MouseEvent) => {
      const target = e.target as HTMLElement;
      const askMark = target.closest<HTMLElement>('.lex-ask, [data-ask-id]');
      if (askMark) {
        const askId = askMark.getAttribute('data-ask-id');
        if (askId) {
          const rect = askMark.getBoundingClientRect();
          setHoveredMark({
            turnId: askId,
            left: rect.left,
            top: rect.bottom + 4,
          });
        }
      }
    };

    document.addEventListener('mouseover', handleMouseOver);
    return () => {
      document.removeEventListener('mouseover', handleMouseOver);
    };
  }, []);

  if (!hoveredMark || !props?.docId) {
    return null;
  }

  return (
    <div
      style={{
        position: 'fixed',
        left: hoveredMark.left,
        top: hoveredMark.top,
        zIndex: 50,
      }}
    >
      <AskHoverCard
        docId={props.docId}
        turnId={hoveredMark.turnId}
        onFollowUp={props.onFollowUp}
        onClose={() => setHoveredMark(null)}
        pinned={false}
      />
    </div>
  );
}

// Backwards-compatible stub
export function NoteAskPanel(_props?: any): null {
  return null;
}
