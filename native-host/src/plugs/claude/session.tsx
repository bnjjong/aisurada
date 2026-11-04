// Implementation of @claude-plug/session for Part 4 Claude-enabled builds.
// Adheres to Chapter 8, 9, 17 of withlog Part 4 Claude integration specifications.
import React, { useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { ModelProse } from './modelProse.tsx';
import { useProjectsList } from './projects.tsx';

// --------------------------------------------------------------------------
// Safe Tauri Invoke
// --------------------------------------------------------------------------

async function safeInvoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  try {
    return await invoke<T>(cmd, args);
  } catch (err) {
    if (typeof window !== 'undefined' && (window as any).__TAURI_INTERNALS__) {
      throw err;
    }
    return undefined as unknown as T;
  }
}

async function callEngine<T = any>(argv: string[]): Promise<T> {
  const res = await safeInvoke<{ status: number; stdout: string; stderr: string }>('sidecar_call', { argv });
  if (res && res.status === 0 && res.stdout) {
    return JSON.parse(res.stdout) as T;
  }
  throw new Error(res?.stderr || '사이드카 명령 실패');
}

// --------------------------------------------------------------------------
// D22 결함 고침: 시각은 기기 시간대 YYYY-MM-DD HH:mm으로 변환
// --------------------------------------------------------------------------

export function formatTurnTime(iso: string): string {
  if (!iso) return '';
  try {
    const d = new Date(iso);
    if (isNaN(d.getTime())) {
      return iso.slice(0, 16).replace('T', ' ');
    }
    const yyyy = d.getFullYear();
    const mm = String(d.getMonth() + 1).padStart(2, '0');
    const dd = String(d.getDate()).padStart(2, '0');
    const hh = String(d.getHours()).padStart(2, '0');
    const min = String(d.getMinutes()).padStart(2, '0');
    return `${yyyy}-${mm}-${dd} ${hh}:${min}`;
  } catch {
    return iso.slice(0, 16).replace('T', ' ');
  }
}

// --------------------------------------------------------------------------
// Model & Effort Choices (Chapter 3.3, 8.9)
// --------------------------------------------------------------------------

export const MODEL_OPTIONS = [
  { id: 'claude-sonnet-5-5', label: 'Sonnet 3.5' },
  { id: 'claude-opus-5-5', label: 'Opus 3.5' },
  { id: 'claude-haiku-5-5', label: 'Haiku 3.5' },
  { id: 'claude-sonnet-4-6', label: 'Sonnet 3.0' },
  { id: 'claude-opus-4-6', label: 'Opus 3.0' },
  { id: 'claude-haiku-4-6', label: 'Haiku 3.0' },
];

export const EFFORT_OPTIONS = [
  { id: 'low', label: 'low' },
  { id: 'med', label: 'med' },
  { id: 'high', label: 'high' },
];

export function modelLabel(id: string): string {
  const found = MODEL_OPTIONS.find((m) => m.id === id);
  return found ? found.label : id;
}

// --------------------------------------------------------------------------
// Types
// --------------------------------------------------------------------------

export interface ChatTurn {
  id: string;
  askedAt: string;
  question: string;
  answer: string;
  error?: string;
  model: string;
  effort: string;
  elapsedMs: number;
  images?: string[];
  unsaved?: boolean;
}

export interface ChatRecord {
  noteId: string;
  chatId: string;
  sourceSessionId: string;
  createdAt: string;
  turns: ChatTurn[];
}

export interface SessionTurn {
  turnId?: string;
  user?: string;
  assistant?: string;
  tools?: Array<{ name: string; args?: string }>;
  failed?: boolean;
  seconds?: number;
}

export interface SessionData {
  file: string | null;
  sizeBytes?: number;
  linesTotal?: number;
  linesDropped?: number;
  turns: SessionTurn[];
  hasMore?: boolean;
  title?: string;
  branch?: string;
  cwd?: string;
  runState?: {
    model?: string;
    effort?: string;
    contextTokens?: number;
    quotaBlocked?: boolean;
    quotaResetsAt?: string;
  };
}

export interface NoteSessionRailProps {
  sessionId: string;
  onClose?: () => void;
  onLeave?: () => void;
  chat?: {
    target: { noteId: string; subId?: string };
    noteMarkdown?: string;
    noteDir?: string;
  };
}

// --------------------------------------------------------------------------
// 8.7 Transcript Bundle & Persona Generation
// --------------------------------------------------------------------------

export function buildTranscriptBundle(
  sessionId: string,
  turns: SessionTurn[]
): { bundle: string; truncated: boolean } {
  if (!turns || turns.length === 0) return { bundle: '', truncated: false };

  const lines: string[] = [];
  for (const t of turns) {
    if (t.user) {
      lines.push(`사용자: ${t.user}`);
    }
    if (t.assistant) {
      lines.push(`클로드: ${t.assistant}`);
    }
    if (t.tools && Array.isArray(t.tools) && t.tools.length > 0) {
      const counts: Record<string, number> = {};
      const order: string[] = [];
      for (const tool of t.tools) {
        const name = tool.name || String(tool);
        if (!counts[name]) {
          counts[name] = 0;
          order.push(name);
        }
        counts[name]++;
      }
      const toolStr = order.map((n) => (counts[n] > 1 ? `${n}×${counts[n]}` : n)).join(', ');
      lines.push(`(도구: ${toolStr})`);
    }
  }

  if (lines.length === 0) return { bundle: '', truncated: false };

  // 뒤에서부터 24,000자까지 담는다
  let totalLen = 0;
  const selected: string[] = [];
  let truncated = false;
  for (let i = lines.length - 1; i >= 0; i--) {
    const lineLen = lines[i].length + 1;
    if (totalLen + lineLen > 24000) {
      truncated = true;
      break;
    }
    selected.unshift(lines[i]);
    totalLen += lineLen;
  }

  if (selected.length === 0) return { bundle: '', truncated: false };

  const header = [
    `사용자가 노트 옆에서 연 Claude Code 세션 \`${sessionId}\`의 대화다. 최신 쪽부터 24,000자까지 실었다. 도구 호출은 이름만 남겼다.`,
    `이것은 읽은 순간의 스냅샷이라 가장 최근 활동이 빠졌을 수 있다.`,
    `아래 대화만으로 답이 모자라면 짐작하지 말고 모자라다고 말하라.`,
  ];
  if (truncated) {
    header[0] = header[0].replace('실었다.', '실었다 (앞쪽은 뺐다).');
  }

  const bundle = [
    header.join('\n'),
    '<session-transcript>',
    selected.join('\n'),
    '</session-transcript>',
  ].join('\n\n');

  return { bundle, truncated };
}

export function extractNoteHeading(markdown?: string): string {
  if (!markdown) return '';
  const clean = markdown.replace(/<!--[\s\S]*?-->/g, '');
  const m = clean.match(/^#\s+(.+)$/m);
  return m ? m[1].trim() : '';
}

export function buildRailPersona(opts: {
  noteMarkdown?: string;
  noteId: string;
  subId?: string;
  sessionId: string;
  hasTranscript: boolean;
}): string {
  const heading = extractNoteHeading(opts.noteMarkdown);
  const noteName = heading ? `「${heading}」` : '';
  const noteTargetText = opts.subId
    ? `노트 ${opts.noteId}의 하위 노트 ${opts.subId}`
    : opts.noteId;
  const noteDesc = `노트는 ${noteName}(${noteTargetText})이다.`;
  const readCmd = opts.subId
    ? `wl-cli get_sub_note '{"parent":"${opts.noteId}","id":"${opts.subId}"}'`
    : `wl-cli get_doc '{"id":"${opts.noteId}"}'`;
  const updateCmd = opts.subId ? 'wl-cli update_sub_note' : 'wl-cli update_doc';
  
  // D20 고침 (c5b929ea): 둘째 줄 끝 가르기
  const transcriptNotice = opts.hasTranscript
    ? '첫 물음 앞에 그 세션의 최근 대화가 실려 있다.'
    : '그 세션의 대화는 실리지 않았다.';

  return [
    `너는 사용자의 withlog 앱에서, 노트 옆 세션 레일로 질문을 받는다. ${noteDesc}`,
    `이 대화는 Claude Code 세션 ${opts.sessionId}와 그 세션이 쓴 이 노트를 두고 묻는다. ${transcriptNotice}`,
    `노트 내용이 필요하면 \`${readCmd}\`로 읽는다. 노트를 고쳐 달라고 하면 그 응답의 precondition 값을 f1으로 실어 \`${updateCmd}\`로 고친다.`,
    `작업 폴더는 그 세션이 돌던 폴더다. 코드나 다른 파일을 고쳐 달라는 말이 없으면 노트 밖 파일은 고치지 않는다.`,
  ].join('\n');
}

// --------------------------------------------------------------------------
// NoteSessionRail Component
// --------------------------------------------------------------------------

export function NoteSessionRail({
  sessionId,
  onClose,
  onLeave: _onLeave,
  chat,
}: NoteSessionRailProps): React.JSX.Element {
  // 8.3 폭 조절: 392px ~ 960px, 더블클릭 392px
  const [width, setWidth] = useState(392);
  const isResizing = useRef(false);
  const startX = useRef(0);
  const startW = useRef(width);

  // 6장 세션 프로젝트 목록에서 해당 세션 정보 조회
  const projectSessions = useProjectsList();
  const sessionItem = useMemo(
    () => projectSessions.find((s) => s.sessionId === sessionId || s.id === sessionId),
    [projectSessions, sessionId]
  );

  // 세션 대화 읽기 상태
  const [sessionData, setSessionData] = useState<SessionData | null>(null);
  const [sessionLoading, setSessionLoading] = useState(true);
  const [sessionError, setSessionError] = useState<string | null>(null);

  // 레일 대화 목록 상태
  const [chats, setChats] = useState<ChatRecord[]>([]);
  const [unreadableChats, setUnreadableChats] = useState<string[]>([]);
  const [chatsError, setChatsError] = useState<string | null>(null);

  // 지금 보는 대화 id (없으면 새 대화)
  const [currentChatId, setCurrentChatId] = useState<string | null>(null);
  const [pendingTurn, setPendingTurn] = useState<ChatTurn | null>(null);
  const [composerNotice, setComposerNotice] = useState<string | null>(null);

  // 입력 및 옵션
  const [inputText, setInputText] = useState('');
  const [attachedImages, setAttachedImages] = useState<File[]>([]);
  const [pickedModel, setPickedModel] = useState<string | null>(null);
  const [pickedEffort, setPickedEffort] = useState<string | null>(null);

  const scrollRef = useRef<HTMLDivElement>(null);

  const targetNoteId = chat?.target?.noteId || '';
  const targetSubId = chat?.target?.subId;

  // 1. 세션 전사 1회 읽기
  useEffect(() => {
    let active = true;
    setSessionLoading(true);
    setSessionError(null);

    callEngine<SessionData>(['read-session', sessionId, 'limit=100', 'urls=true'])
      .then((data) => {
        if (!active) return;
        setSessionData(data);
        setSessionLoading(false);
      })
      .catch((err) => {
        if (!active) return;
        setSessionError(err?.message || '대화 기록을 못 읽었어요.');
        setSessionLoading(false);
      });

    return () => {
      active = false;
    };
  }, [sessionId]);

  // 2. 레일 대화 목록 읽기 (list-chats)
  const reloadChats = () => {
    if (!targetNoteId) return;
    const argv = ['list-chats', targetNoteId];
    if (targetSubId) argv.push(targetSubId);

    callEngine<{ chats: ChatRecord[]; unreadable: string[] }>(argv)
      .then((res) => {
        setChats(res.chats || []);
        setUnreadableChats(res.unreadable || []);
        setChatsError(null);
      })
      .catch((err) => {
        setChatsError(err?.message || '대화 목록을 읽지 못했어요.');
      });
  };

  useEffect(() => {
    reloadChats();
  }, [targetNoteId, targetSubId]);

  // 지금 보고 있는 대화 객체 결정 (8.5)
  const activeChat = useMemo(() => {
    if (pendingTurn && currentChatId) {
      const found = chats.find((c) => c.chatId === currentChatId);
      if (found) return found;
    }
    if (currentChatId) {
      const found = chats.find((c) => c.chatId === currentChatId);
      if (found) return found;
    }
    // 밑감 세션이 이 레일 세션인 대화 중 마지막 것
    const match = chats.filter((c) => c.sourceSessionId === sessionId);
    if (match.length > 0) {
      return match[match.length - 1];
    }
    return null;
  }, [chats, currentChatId, pendingTurn, sessionId]);

  // 대화의 모든 턴 (activeChat의 턴들 + 화면에만 있는 pendingTurn)
  const currentTurns = useMemo(() => {
    const list: ChatTurn[] = activeChat ? [...activeChat.turns] : [];
    return list;
  }, [activeChat]);

  // 첫 물음 여부 (성공한 회차가 없으면 첫 물음)
  const isFirstQuestion = useMemo(() => {
    if (!activeChat || activeChat.turns.length === 0) return true;
    return !activeChat.turns.some((t) => !t.error && t.answer);
  }, [activeChat]);

  // 8.9 모델 및 effort 우선순위 계산
  const { chosenModel, chosenEffort, hint } = useMemo(() => {
    let m = 'claude-sonnet-5-5';
    let e = 'low';
    let mSource = 'default';
    let eSource = 'default';

    // 1. picked
    if (pickedModel) {
      m = pickedModel;
      mSource = 'picked';
    }
    if (pickedEffort) {
      e = pickedEffort;
      eSource = 'picked';
    }

    // 2. chat (마지막 회차 값)
    const lastTurn = currentTurns.length > 0 ? currentTurns[currentTurns.length - 1] : null;
    if (mSource === 'default' && lastTurn?.model) {
      m = lastTurn.model;
      mSource = 'chat';
    }
    if (eSource === 'default' && lastTurn?.effort) {
      e = lastTurn.effort;
      eSource = 'chat';
    }

    // 3. session (runState)
    const rs = sessionData?.runState;
    if (mSource === 'default' && rs?.model) {
      const cleanM = rs.model.replace(/\s*\[.*\]$/, '');
      if (MODEL_OPTIONS.some((opt) => opt.id === cleanM)) {
        m = cleanM;
        mSource = 'session';
      }
    }
    if (eSource === 'default' && rs?.effort) {
      if (EFFORT_OPTIONS.some((opt) => opt.id === rs.effort)) {
        e = rs.effort;
        eSource = 'session';
      }
    }

    let h = '';
    if (mSource === eSource) {
      if (mSource === 'chat') h = '이 대화의 값';
      else if (mSource === 'session') h = '세션 값';
    }

    return { chosenModel: m, chosenEffort: e, hint: h };
  }, [pickedModel, pickedEffort, currentTurns, sessionData?.runState]);

  // 폭 리사이징 핸들러
  const handleMouseDown = (e: React.MouseEvent) => {
    isResizing.current = true;
    startX.current = e.clientX;
    startW.current = width;

    const onMouseMove = (ev: MouseEvent) => {
      if (!isResizing.current) return;
      const delta = startX.current - ev.clientX; // 왼쪽으로 끌면 커짐
      const newW = Math.max(392, Math.min(960, startW.current + delta));
      setWidth(newW);
    };

    const onMouseUp = () => {
      isResizing.current = false;
      window.removeEventListener('mousemove', onMouseMove);
      window.removeEventListener('mouseup', onMouseUp);
    };

    window.addEventListener('mousemove', onMouseMove);
    window.addEventListener('mouseup', onMouseUp);
  };

  // 새 대화 열기 (8.5)
  const handleNewChat = () => {
    setCurrentChatId(null);
    setComposerNotice(null);
  };

  // 물음 전송 (8.6)
  const handleSend = async () => {
    const q = inputText.trim();
    if (!q || pendingTurn) return;
    if (sessionLoading) return; // 첫 물음인데 읽는 중이면 무시

    setInputText('');
    const imgsToSend = [...attachedImages];
    setAttachedImages([]);

    // 대화 id 결정 (없으면 새 UUID)
    const chatId = currentChatId || activeChat?.chatId || crypto.randomUUID();
    setCurrentChatId(chatId);

    // 앞 회차 확인 (첫 물음 여부)
    const existingTurns = activeChat ? activeChat.turns : [];

    // 묶음 글 생성
    const { bundle } = buildTranscriptBundle(sessionId, sessionData?.turns || []);
    const hasTranscript = bundle.length > 0;

    // 페르소나 생성
    const persona = buildRailPersona({
      noteMarkdown: chat?.noteMarkdown,
      noteId: targetNoteId,
      subId: targetSubId,
      sessionId,
      hasTranscript,
    });

    // 프롬프트 생성
    let prompt = q;
    if (isFirstQuestion && hasTranscript) {
      prompt = `## 이 세션의 최근 대화\n\n${bundle}\n\n## 물음\n\n${q}`;
    }

    const askedAt = new Date().toISOString();
    const turnId = `rt${Date.now().toString(36)}-${crypto.randomUUID().slice(0, 8)}`;

    // 임시 도는 물음 등록
    setPendingTurn({
      id: turnId,
      askedAt,
      question: q,
      answer: '',
      model: chosenModel,
      effort: chosenEffort,
      elapsedMs: 0,
      images: imgsToSend.length > 0 ? imgsToSend.map((f) => `attach/${f.name}`) : undefined,
    });

    // 이미지 첨부 저장 (attach/ 폴더)
    const savedImagePaths: string[] = [];
    for (const f of imgsToSend) {
      try {
        const ext = f.name.split('.').pop()?.toLowerCase() || 'png';
        const imgName = `ask-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}.${ext}`;
        const arrayBuf = await f.arrayBuffer();
        await safeInvoke('write_attachment_bytes', {
          parent: targetSubId ? `${targetNoteId}/${targetSubId}` : targetNoteId,
          name: imgName,
          bytes: Array.from(new Uint8Array(arrayBuf)),
        });
        savedImagePaths.push(`attach/${imgName}`);
      } catch {
        // 이미지 저장 실패
      }
    }

    // ask_claude 호출
    const req = {
      vault: '',
      sessionId: chatId,
      model: chosenModel,
      effort: chosenEffort,
      prompt,
      resume: existingTurns.length > 0,
      timeoutSeconds: 120,
      persona,
      cwd: sessionData?.cwd || sessionItem?.cwd,
    };

    let answer = '';
    let error: string | undefined;
    let elapsedMs = 0;

    try {
      const askRes = await safeInvoke<{
        answer?: string;
        error?: string;
        elapsed_ms?: number;
        missing_cwd?: boolean;
      }>('ask_claude', { request: req });

      if (askRes) {
        answer = askRes.answer || '';
        error = askRes.error;
        elapsedMs = askRes.elapsed_ms || 0;
        if (askRes.missing_cwd && sessionData?.cwd) {
          setComposerNotice(`이 세션이 처음 돌던 폴더(${sessionData.cwd})가 없어서 vault 에서 답했어요.`);
        }
      } else {
        error = '호스트 응답이 비어 있습니다.';
      }
    } catch (err: any) {
      error = err?.message || '요청 실패';
    }

    // append-chat-turn 인자 구성
    const appendArgv = ['append-chat-turn', targetNoteId];
    if (targetSubId) appendArgv.push(targetSubId);
    appendArgv.push(
      `chat-id=${chatId}`,
      `source-session=${sessionId}`,
      `turn-id=${turnId}`,
      `asked-at=${askedAt}`,
      `question=${q}`,
      `answer=${answer}`,
      `model=${chosenModel}`,
      `effort=${chosenEffort}`,
      `elapsed-ms=${elapsedMs}`
    );
    if (error) appendArgv.push(`error=${error}`);
    if (savedImagePaths.length > 0) appendArgv.push(`images=${savedImagePaths.join(',')}`);

    try {
      await callEngine(appendArgv);
      reloadChats();
    } catch {
      // 실패 시 화면에 unsaved로 남김 (8.10)
      setChats((prev) => {
        const found = prev.find((c) => c.chatId === chatId);
        const newTurn: ChatTurn = {
          id: turnId,
          askedAt,
          question: q,
          answer,
          error,
          model: chosenModel,
          effort: chosenEffort,
          elapsedMs,
          images: savedImagePaths.length > 0 ? savedImagePaths : undefined,
          unsaved: true,
        };
        if (found) {
          return prev.map((c) =>
            c.chatId === chatId ? { ...c, turns: [...c.turns, newTurn] } : c
          );
        }
        return [
          ...prev,
          {
            noteId: targetNoteId,
            chatId,
            sourceSessionId: sessionId,
            createdAt: askedAt,
            turns: [newTurn],
          },
        ];
      });
    }

    setPendingTurn(null);

    // 스크롤 맨 아래로 이동
    setTimeout(() => {
      if (scrollRef.current) {
        scrollRef.current.scrollTop = scrollRef.current.scrollHeight;
      }
    }, 50);
  };

  const isQuerying = pendingTurn !== null;

  return (
    <aside
      className="nsrail"
      data-at-min={width <= 392 ? true : undefined}
      style={{ width: `${width}px` }}
    >
      {/* 8.3 리사이저 손잡이 */}
      <div
        className="nsrail-resizer"
        role="separator"
        aria-orientation="vertical"
        onMouseDown={handleMouseDown}
        onDoubleClick={() => setWidth(392)}
      />

      {/* 8.4 레일 머리 */}
      <header className="nsrail-header">
        <div className="nsrail-title-row">
          <span className="nsrail-title">
            {sessionItem ? sessionItem.title || '(제목 없는 세션)' : '지금 안 돌고 있는 세션'}
          </span>
          {sessionItem?.status === 'running' && <span className="sess-run" />}
          <button
            type="button"
            className="nsrail-close"
            aria-label="세션 레일 닫기"
            onClick={onClose}
          >
            ✕
          </button>
        </div>

        {sessionItem && (
          <div className="nsrail-meta-row">
            <span className="nsrail-kind">
              {sessionItem.kind === 'interactive'
                ? '터미널'
                : sessionItem.kind === 'background'
                  ? '백그라운드'
                  : '알 수 없음'}
            </span>
            {sessionItem.cwd && <span className="nsrail-cwd">{sessionItem.cwd}</span>}
          </div>
        )}

        <div className="nsrail-actions">
          <button
            type="button"
            className="nsrail-open-session"
            onClick={() => {
              window.location.hash = `/session/${encodeURIComponent(sessionId)}`;
            }}
          >
            세션 화면에서 보기
          </button>
        </div>
      </header>

      {/* 스크롤 본문 칸 (.nsrail-body) */}
      <div className="nsrail-body" ref={scrollRef}>
        {/* 1. 세션 대화 칸 */}
        <section className="nsrail-session-section">
          {sessionLoading && (
            <div className="nsrail-msg">대화 기록을 읽는 중이에요.</div>
          )}
          {sessionError && !sessionLoading && (
            <div className="nsrail-msg">대화 기록을 못 읽었어요. 사이드카가 안 떠 있을 수 있어요.</div>
          )}
          {!sessionLoading && !sessionError && sessionData && (
            <>
              {sessionData.file === null ? (
                <div className="nsrail-msg">이 기계에 그 세션의 기록이 없어요. 다른 기기에서 돌던 세션일 수 있어요.</div>
              ) : sessionData.turns.length === 0 ? (
                // D20 결함 고침: 기록은 있는데 주고받은 말이 없어요
                <div className="nsrail-msg">기록은 있는데 주고받은 말이 없어요.</div>
              ) : (
                <>
                  <div className="nsrail-source">
                    {sessionData.sizeBytes ?? 0}B · {sessionData.linesTotal ?? 0}줄 · {sessionData.turns.length}회차
                    {Number(sessionData.linesDropped || 0) > 0 ? ` · 못 읽은 줄 ${sessionData.linesDropped}개` : ''}
                  </div>
                  {sessionData.hasMore && (
                    <div className="nsrail-has-more">앞의 대화가 더 있어요</div>
                  )}
                  <div className="sess-turns">
                    {sessionData.turns.map((t, idx) => (
                      <div key={t.turnId || idx} className="sess-turn-group">
                        {t.user && (
                          <div className="sess-turn sess-turn-user">
                            <span className="sess-turn-author">사용자</span>
                            <div className="sess-turn-body">{t.user}</div>
                          </div>
                        )}
                        {t.assistant && (
                          <div className="sess-turn sess-turn-assistant">
                            <span className="sess-turn-author">클로드</span>
                            <div className="sess-turn-body">
                              <ModelProse content={t.assistant} />
                            </div>
                          </div>
                        )}
                        {t.tools && t.tools.length > 0 && (
                          <div className="sess-tool-line">
                            (도구: {t.tools.map((tl) => tl.name || tl).join(', ')})
                          </div>
                        )}
                      </div>
                    ))}
                  </div>
                </>
              )}
            </>
          )}
        </section>

        {/* 2. 레일 대화 칸 (.rchat) */}
        <section className="rchat">
          <div className="rchat-header">
            <h4>새 대화</h4>
            {(currentTurns.length > 0 || isQuerying) && (
              <button
                type="button"
                className="rchat-new"
                disabled={isQuerying}
                onClick={handleNewChat}
              >
                새 대화 열기
              </button>
            )}
          </div>

          {chatsError && <div className="rchat-warn">{chatsError}</div>}
          {unreadableChats.length > 0 && (
            <div className="rchat-warn">
              못 읽은 대화 파일이 있어요: {unreadableChats.join(', ')}
            </div>
          )}

          {currentTurns.length === 0 && !isQuerying && (
            <div className="rchat-empty">
              이 세션에는 보내지 않아요. 새 대화를 열고, 이 세션의 최근 대화와 노트를 함께 실어 물어요.
            </div>
          )}

          {/* 회차 목록 */}
          <div className="rchat-turns">
            {currentTurns.map((turn) => (
              <div key={turn.id} className="rchat-turn-pair">
                {/* 물음 줄 */}
                <div className="sess-turn sess-turn-user">
                  <div className="sess-turn-head">
                    <span className="sess-turn-author">나</span>
                    <span className="sess-turn-time">{formatTurnTime(turn.askedAt)}</span>
                  </div>
                  <div className="sess-turn-body">{turn.question}</div>
                  {turn.images && turn.images.length > 0 && (
                    <div className="sess-turn-images">
                      {turn.images.map((img) => (
                        <span key={img} className="sess-image-chip">{img}</span>
                      ))}
                    </div>
                  )}
                </div>

                {/* 답 줄 */}
                <div className={`sess-turn sess-turn-assistant ${turn.error ? 'rchat-failed' : ''}`}>
                  <div className="sess-turn-head">
                    <span className="sess-turn-author">클로드</span>
                    <span className="sess-turn-model">{modelLabel(turn.model)}</span>
                    <span className="sess-turn-effort">{turn.effort}</span>
                    <span className="sess-turn-time">
                      {(turn.elapsedMs / 1000).toFixed(1)}초
                    </span>
                  </div>
                  <div className="sess-turn-body">
                    {turn.error ? (
                      <div className="rchat-error">못 받았어요: {turn.error}</div>
                    ) : (
                      <ModelProse content={turn.answer} />
                    )}
                  </div>
                  {turn.unsaved && (
                    <div className="rchat-unsaved">
                      이 답은 vault 에 못 적었어요. 앱을 끄면 사라져요.
                    </div>
                  )}
                </div>
              </div>
            ))}

            {/* 도는 물음 (.rchat-pending) */}
            {pendingTurn && (
              <div className="rchat-turn-pair rchat-pending">
                <div className="sess-turn sess-turn-user">
                  <div className="sess-turn-head">
                    <span className="sess-turn-author">나</span>
                    <span className="sess-turn-time">{formatTurnTime(pendingTurn.askedAt)}</span>
                  </div>
                  <div className="sess-turn-body">{pendingTurn.question}</div>
                </div>
                <div className="sess-turn sess-turn-assistant">
                  <div className="sess-turn-head">
                    <span className="sess-turn-author">클로드</span>
                    <span className="sess-turn-model">{modelLabel(pendingTurn.model)}</span>
                    <span className="sess-turn-effort">{pendingTurn.effort}</span>
                  </div>
                  <div className="sess-turn-body">
                    요청 중…
                    {attachedImages.length > 0 && ` (그림 ${attachedImages.length}장 붙임)`}
                  </div>
                </div>
              </div>
            )}
          </div>
        </section>
      </div>

      {/* 3. 하단 입력창 (.sess-composer.nsrail-composer) */}
      <footer className="sess-composer nsrail-composer">
        {/* 고르개 줄 */}
        <div className="rchat-options">
          <select
            value={chosenModel}
            onChange={(e) => setPickedModel(e.target.value)}
          >
            {MODEL_OPTIONS.map((opt) => (
              <option key={opt.id} value={opt.id}>
                {opt.label}
              </option>
            ))}
          </select>

          <select
            value={chosenEffort}
            onChange={(e) => setPickedEffort(e.target.value)}
          >
            {EFFORT_OPTIONS.map((opt) => (
              <option key={opt.id} value={opt.id}>
                {opt.label}
              </option>
            ))}
          </select>

          {hint && <span className="rchat-options-hint">{hint}</span>}
        </div>

        {/* 안내 줄 (missing_cwd 알림 등) */}
        {composerNotice && (
          <div className="sess-composer-notice">{composerNotice}</div>
        )}

        {/* 첨부 그림 썸네일 줄 */}
        {attachedImages.length > 0 && (
          <div className="sess-composer-images">
            {attachedImages.map((f, idx) => (
              <span key={idx} className="sess-composer-img">
                {f.name}
                <button
                  type="button"
                  onClick={() =>
                    setAttachedImages((prev) => prev.filter((_, i) => i !== idx))
                  }
                >
                  ✕
                </button>
              </span>
            ))}
          </div>
        )}

        {/* 입력 textarea */}
        <textarea
          className="sess-composer-input"
          aria-label="새 대화로 묻기"
          placeholder="이 세션과 노트에 대해 물어요… (⌘V로 그림 붙이기, ⌘/Ctrl+Enter로 보내기)"
          value={inputText}
          rows={3}
          onChange={(e) => setInputText(e.target.value)}
          onPaste={(e) => {
            const items = e.clipboardData?.items;
            if (!items) return;
            for (let i = 0; i < items.length; i++) {
              if (items[i].type.startsWith('image/')) {
                const file = items[i].getAsFile();
                if (file) {
                  setAttachedImages((prev) => [...prev, file].slice(0, 8));
                }
              }
            }
          }}
          onKeyDown={(e) => {
            if ((e.metaKey || e.ctrlKey) && e.key === 'Enter') {
              e.preventDefault();
              void handleSend();
            }
          }}
        />

        {/* 전송 버튼 */}
        <div className="sess-composer-footer">
          <button
            type="button"
            className="sess-composer-send"
            disabled={isQuerying || (!inputText.trim() && attachedImages.length === 0) || (isFirstQuestion && sessionLoading)}
            onClick={() => void handleSend()}
          >
            {isQuerying
              ? '요청 중…'
              : sessionLoading && isFirstQuestion
                ? '세션 대화를 읽는 중…'
                : '묻기'}
          </button>
        </div>
      </footer>
    </aside>
  );
}

// --------------------------------------------------------------------------
// Chapter 9 Stubs
// --------------------------------------------------------------------------

export function SessionRoute(_props?: any): React.JSX.Element {
  return (
    <section className="session-route">
      <h1>세션 대화</h1>
    </section>
  );
}

export function SessionView({ sessionId }: { sessionId?: string }): React.JSX.Element {
  return <SessionRoute sessionId={sessionId} />;
}

export function SessionRail(_props?: any): null {
  return null;
}

export function useSessionBadge(_docId?: string): null {
  return null;
}
