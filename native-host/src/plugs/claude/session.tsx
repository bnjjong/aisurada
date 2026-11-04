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
// Chapter 9: Session Route Component (/session/:id)
// --------------------------------------------------------------------------

export interface SessionRouteProps {
  sessionId?: string;
  controller?: any;
  state?: any;
}

export function formatDuration(sec: number): string {
  if (sec < 60) {
    return `${sec.toFixed(1)}초`;
  }
  if (sec < 3600) {
    return `${Math.round(sec / 60)}분`;
  }
  return `${(sec / 3600).toFixed(1)}시간`;
}

export function formatBytes(bytes: number): string {
  if (bytes >= 1048576) {
    return `${(bytes / 1048576).toFixed(1)}MB`;
  }
  if (bytes >= 1024) {
    return `${Math.round(bytes / 1024)}KB`;
  }
  return `${bytes}B`;
}

export function buildPinTranscriptMarkdown(opts: {
  title: string;
  turns: any[];
  findingsCount: number;
}): string {
  const lines: string[] = [];
  lines.push(`# ${opts.title}`);
  lines.push('');

  // 콜아웃
  if (opts.findingsCount > 0) {
    lines.push(`> [!danger] 표에 없는데 비밀처럼 생긴 것 ${opts.findingsCount}건`);
    lines.push('> 이 검사는 도구 결과 본문은 보지 못해요.');
  } else {
    lines.push('> [!success] 모양 검사에 걸린 것이 0건입니다.');
    lines.push('> 이 검사는 도구 결과 본문은 보지 못해요.');
  }
  lines.push('');

  for (const t of opts.turns) {
    if (t.who === 'user' || t.user) {
      const text = t.text || t.user || '';
      lines.push('**나**');
      lines.push('');
      lines.push('```');
      lines.push(text);
      lines.push('```');
      lines.push('');
    } else if (t.who === 'assistant' || t.assistant) {
      const text = t.text || t.assistant || '';
      lines.push('**클로드**');
      lines.push('');
      lines.push('```');
      lines.push(text);
      lines.push('```');
      lines.push('');
    } else if (t.who === 'tool' || t.tools) {
      const tools = t.tools || (t.tool ? [t.tool] : []);
      for (const tool of tools) {
        const name = tool.name || String(tool);
        const arg = tool.arg ? ` — \`${tool.arg.replace(/\r?\n/g, ' ')}\`` : '';
        lines.push(`- \`${name}\`${arg}`);
      }
    }
  }

  return lines.join('\n');
}

export function SessionRoute({
  sessionId: rawSessionId,
  controller,
  state,
}: SessionRouteProps): React.JSX.Element {
  // 세션 id 추출 (props 우선, 없으면 URL hash/path 파싱)
  const sessionId = useMemo(() => {
    if (rawSessionId) return rawSessionId;
    if (typeof window !== 'undefined') {
      const path = window.location.pathname || window.location.hash;
      const m = path.match(/\/session\/([^/?#]+)/);
      if (m) return decodeURIComponent(m[1]);
    }
    return '';
  }, [rawSessionId]);

  // 세션 목록
  const projectSessions = useProjectsList();
  const sessionItem = useMemo(
    () => projectSessions.find((s) => s.sessionId === sessionId || s.id === sessionId),
    [projectSessions, sessionId]
  );

  // 전사 데이터 상태
  const [sessionData, setSessionData] = useState<any | null>(null);
  const [sessionLoading, setSessionLoading] = useState(true);
  const [sessionError, setSessionError] = useState<string | null>(null);

  // 오른쪽 레일 상태: 'notes' | 'links' | 'cli'
  const [tab, setTab] = useState<'notes' | 'links' | 'cli'>('notes');
  const [previewOpen, setPreviewOpen] = useState(true);

  // 세션 id 변경 시 오른쪽 탭을 'notes'로 리셋 (9.7)
  useEffect(() => {
    setTab('notes');
  }, [sessionId]);

  // 굳히기 모달 상태
  const [pinModalOpen, setPinModalOpen] = useState(false);
  const [pinCount, setPinCount] = useState<100 | 200 | 500>(100);
  const [pinTitle, setPinTitle] = useState('');
  const [pinAgenda, setPinAgenda] = useState('');
  const [pinError, setPinError] = useState<string | null>(null);
  const [pinLoading, setPinLoading] = useState(false);

  // 대화 읽기 (read-session)
  const loadTranscript = (limit = 100) => {
    if (!sessionId) return;
    setSessionLoading(true);
    setSessionError(null);

    callEngine(['read-session', sessionId, `limit=${limit}`, 'urls=true'])
      .then((data) => {
        setSessionData(data);
        setSessionLoading(false);
      })
      .catch((err) => {
        setSessionError(err?.message || '대화 기록을 못 읽었어요.');
        setSessionLoading(false);
      });
  };

  useEffect(() => {
    loadTranscript(100);
  }, [sessionId]);

  // 창 너비 감시 (1100px 이하 시 오른쪽 레일 자동 접힘, 9.7)
  const [windowNarrow, setWindowNarrow] = useState(false);
  useEffect(() => {
    const checkWidth = () => {
      setWindowNarrow(window.innerWidth <= 1100);
    };
    checkWidth();
    window.addEventListener('resize', checkWidth);
    return () => window.removeEventListener('resize', checkWidth);
  }, []);

  const isRailVisible = previewOpen && !windowNarrow;

  // 관련 노트 목록 (9.7)
  const relatedNotes = useMemo(() => {
    const notes: any[] = state?.workspace?.notes || [];
    return notes
      .filter((n) => n.sessionId === sessionId || n.editedSessionId === sessionId)
      .map((n) => {
        const isCreated = n.sessionId === sessionId;
        const why = isCreated ? '이 세션이 만들었어요.' : '이 세션이 고쳤어요.';
        return { ...n, why };
      })
      .sort((a, b) => {
        const d1 = String(a.created || '');
        const d2 = String(b.created || '');
        const c = d2.localeCompare(d1);
        if (c !== 0) return c;
        return String(a.id || '').localeCompare(String(b.id || ''));
      });
  }, [state?.workspace?.notes, sessionId]);

  // 링크 목록 (9.7)
  const urlsList = useMemo(() => {
    const urls: any[] = sessionData?.urls || [];
    return urls.map((u) => {
      if (typeof u === 'string') {
        try {
          const parsed = new URL(u);
          return { url: u, host: parsed.host, path: parsed.pathname + parsed.search, count: 1 };
        } catch {
          return { url: u, host: u, path: '', count: 1 };
        }
      }
      try {
        const parsed = new URL(u.url);
        return { url: u.url, host: parsed.host, path: parsed.pathname + parsed.search, count: u.count || 1 };
      } catch {
        return { url: u.url, host: u.url, path: '', count: u.count || 1 };
      }
    });
  }, [sessionData?.urls]);

  // 전사 굳히기 열기
  const handleOpenPinModal = () => {
    const now = new Date();
    const hh = String(now.getHours()).padStart(2, '0');
    const mm = String(now.getMinutes()).padStart(2, '0');
    setPinTitle(`세션대화 ${hh}:${mm}`);
    const agendas: any[] = state?.workspace?.agendas || [];
    if (agendas.length > 0) {
      setPinAgenda(agendas[0].slug || agendas[0].id);
    }
    setPinError(null);
    setPinModalOpen(true);
  };

  // 전사 굳히기 저장 (create-doc, D29)
  const handleSavePin = async () => {
    if (!pinTitle.trim() || !sessionData) return;
    setPinLoading(true);
    setPinError(null);

    const body = buildPinTranscriptMarkdown({
      title: pinTitle,
      turns: sessionData.turns || [],
      findingsCount: 0,
    });

    const utf8Bytes = new TextEncoder().encode(body).length;
    // D29 결함 고침: 1,000,000 바이트 초과 사전 검사
    if (utf8Bytes > 1000000) {
      setPinError(`본문이 1000000바이트를 넘어 저장할 수 없어요: ${utf8Bytes}바이트`);
      setPinLoading(false);
      return;
    }

    const now = new Date();
    const date = now.toISOString().slice(0, 10);
    const time = now.toISOString().slice(11, 16);

    try {
      const argv = [
        'create-doc',
        `agenda=${pinAgenda}`,
        `title=${pinTitle}`,
        `body=${body}`,
        `date=${date}`,
        `time=${time}`,
        'format=markdown',
        'tier=record',
      ];
      const res = await callEngine<{ id?: string }>(argv);
      setPinLoading(false);
      setPinModalOpen(false);

      if (res?.id && controller?.navigate) {
        controller.navigate(`/doc/${res.id}`);
      }
    } catch (err: any) {
      setPinError(err?.message || '노트를 만들지 못했어요.');
      setPinLoading(false);
    }
  };

  const isAlive = sessionItem?.status === 'running';

  return (
    <div className={`sess-screen ${!isRailVisible ? 'no-preview' : ''}`}>
      {/* 메인 영역 */}
      <div className="sess-main-area">
        {/* 머리 (9.3) */}
        <header className="sess-page-header">
          <div className="sess-page-head-row">
            <h1 className="sess-page-title">
              {sessionItem ? sessionItem.title || '(이름 없는 세션)' : '지금 안 돌고 있는 세션'}
              {sessionItem?.status === 'running' && <span className="sess-run" />}
            </h1>

            <div className="sess-page-actions">
              {!windowNarrow && (
                <button
                  type="button"
                  className="open-cli"
                  onClick={() => {
                    setPreviewOpen(true);
                    setTab('cli');
                  }}
                >
                  터미널 열기
                </button>
              )}
              <button
                type="button"
                className="sess-page-resume"
                onClick={handleOpenPinModal}
              >
                노트로 굳히기
              </button>
              {!windowNarrow && (
                <button
                  type="button"
                  className="prev-toggle"
                  onClick={() => setPreviewOpen(!previewOpen)}
                >
                  {previewOpen ? '레일 접기' : '레일 펴기'}
                </button>
              )}
            </div>
          </div>

          <div className="sess-page-meta">
            {sessionItem ? (
              <>
                <span className="sess-page-kind">
                  {sessionItem.kind === 'interactive'
                    ? '터미널'
                    : sessionItem.kind === 'background'
                      ? '백그라운드'
                      : '알 수 없음'}
                </span>
                <span className={`sess-page-status ${sessionItem.status === 'running' ? '' : 'sess-ended'}`}>
                  {sessionItem.status === 'running' ? '도는 중' : '끝남'}
                </span>
                {sessionItem.startedAt && (
                  <span className="sess-page-started">
                    {formatTurnTime(new Date(sessionItem.startedAt).toISOString())}에 시작
                  </span>
                )}
                {sessionItem.pid && <span className="sess-page-pid">pid {sessionItem.pid}</span>}
                {sessionItem.branch && <span className="sess-page-branch">{sessionItem.branch}</span>}
              </>
            ) : (
              <span>
                {sessionLoading
                  ? '세션 목록을 읽는 중이에요.'
                  : '끝났거나 다른 기기의 세션이에요. 대화는 디스크에 남아 있어서 아래 명령으로 이어 갈 수 있어요.'}
              </span>
            )}
          </div>

          {sessionItem?.cwd && <div className="sess-page-cwd">{sessionItem.cwd}</div>}

          {/* 마지막 회차 줄 (runState) */}
          {sessionData?.runState && (
            <div className="sess-page-runstate">
              <span>마지막 회차</span>
              {sessionData.runState.model && <span>{sessionData.runState.model}</span>}
              {sessionData.runState.effort && <span>{sessionData.runState.effort}</span>}
              {sessionData.runState.contextTokens && (
                <span>컨텍스트 {sessionData.runState.contextTokens.toLocaleString()}토큰</span>
              )}
              {sessionData.runState.quotaBlocked && (
                <span
                  className="sess-page-runstate-quota"
                  title={
                    sessionData.runState.quotaResetsAt
                      ? `재설정 예정: ${formatTurnTime(sessionData.runState.quotaResetsAt)}`
                      : undefined
                  }
                >
                  사용량 한도에 걸린 적 있음
                </span>
              )}
            </div>
          )}
        </header>

        {/* 대화 칸 (.sess-page-talk, 9.4) */}
        <section className="sess-page-talk">
          <div className="sess-page-talk-head">
            <h2>대화</h2>
            {sessionData?.linesTotal != null && (
              <span className="sess-count">{sessionData.linesTotal}</span>
            )}
          </div>

          {sessionLoading && (
            <div className="sess-page-empty">대화 기록을 읽는 중이에요.</div>
          )}
          {sessionError && !sessionLoading && (
            <div className="sess-page-empty" title={sessionError}>
              대화 기록을 못 읽었어요. 사이드카가 안 떠 있을 수 있어요.
            </div>
          )}
          {!sessionLoading && !sessionError && sessionData && (
            <>
              {sessionData.file === null ? (
                <div className="sess-page-empty">
                  이 기계에 그 세션의 기록이 없어요. 다른 기기에서 돌던 세션일 수 있어요.
                </div>
              ) : sessionData.turns?.length === 0 ? (
                // D20 결함 고침: 말이 없을 때 정직한 명시
                <div className="sess-page-empty">기록은 있는데 주고받은 말이 없어요.</div>
              ) : (
                <>
                  <div className="sess-page-source">
                    {formatBytes(sessionData.sizeBytes || 0)} 중에서 골랐어요 ·{' '}
                    {sessionData.linesTotal || 0}줄에서 {sessionData.turns?.length || 0}건
                    {Number(sessionData.linesDropped || 0) > 0 && (
                      <strong> · 못 읽은 줄 {sessionData.linesDropped}개</strong>
                    )}
                  </div>

                  {sessionData.hasMore && (
                    <div className="sess-page-hasmore">
                      앞의 대화가 더 있어요. 지금은 최근 건만 보여요.
                    </div>
                  )}

                  {/* 회차 목록 (.sess-page-turns) */}
                  <div className="sess-page-turns">
                    {sessionData.turns.map((t: any, idx: number) => {
                      const atStr = formatTurnTime(t.askedAt || t.at || '');
                      if (t.who === 'user' || t.user) {
                        return (
                          <div key={idx} className="sess-turn sess-turn-user">
                            <div className="sess-turn-head">
                              <span className="sess-turn-author">나</span>
                              <span className="sess-turn-at">{atStr}</span>
                            </div>
                            <div className="sess-turn-text">{t.user || t.text}</div>
                          </div>
                        );
                      }
                      if (t.who === 'assistant' || t.assistant) {
                        return (
                          <div key={idx} className="sess-turn sess-turn-assistant">
                            <div className="sess-turn-head">
                              <span className="sess-turn-author">클로드</span>
                              <span className="sess-turn-at">{atStr}</span>
                              {t.truncated && <span className="sess-turn-cut">2,000자 중 앞부분</span>}
                            </div>
                            <div className="sess-turn-text">
                              <ModelProse content={t.assistant || t.text || ''} />
                            </div>
                          </div>
                        );
                      }
                      if (t.who === 'tool' || t.tools) {
                        const tools = t.tools || (t.tool ? [t.tool] : []);
                        return tools.map((tl: any, ti: number) => {
                          const failed = tl.failed || t.failed;
                          const sec = tl.seconds != null ? tl.seconds : t.seconds;
                          return (
                            <div key={`${idx}-${ti}`} className={`toolrow ${failed ? 'is-failed' : ''}`}>
                              <span
                                className="tr-ic"
                                data-state={failed ? 'failed' : sec != null ? 'done' : 'running'}
                              >
                                {failed ? '✕' : sec != null ? '✓' : '●'}
                              </span>
                              <span className="tr-name">{tl.name}</span>
                              {tl.arg && <span className="tr-arg" title={tl.arg}>{tl.arg}</span>}
                              {sec != null && <span className="tr-ms">{formatDuration(sec)}</span>}
                            </div>
                          );
                        });
                      }
                      return null;
                    })}
                  </div>
                </>
              )}
            </>
          )}

          {/* 하단 독 (.sess-dock, 9.5) */}
          <footer className="sess-dock">
            {/* 서브에이전트 바 (D26 결함 고침) */}
            {sessionData?.runningAgents && sessionData.runningAgents.length > 0 && (
              <div className="subagent-bar sess-dock-row">
                <div className="sess-dock-head">
                  {isAlive && <span className="sess-run" />}
                  <span>
                    {isAlive
                      ? `서브에이전트 ${sessionData.runningAgents.length}개가 도는 중`
                      : `결과가 안 온 서브에이전트 ${sessionData.runningAgents.length}개, 지금 도는지는 몰라요`}
                  </span>
                </div>
                <div className="sess-dock-list">
                  {sessionData.runningAgents.map((ag: any, ai: number) => (
                    <div key={ai} className="sess-dock-item">
                      <span className="sess-dock-what">{ag.name || ag.task || '에이전트'}</span>
                      {ag.startedAt && (
                        <span className="sess-dock-at">{formatTurnTime(ag.startedAt)}</span>
                      )}
                    </div>
                  ))}
                </div>
              </div>
            )}

            {/* 할 일 바 (9.5) */}
            {sessionData?.todos && sessionData.todos.length > 0 && (
              <div className="todo-bar sess-dock-row">
                <div className="sess-dock-head">
                  <span>할 일 ({sessionData.todos.length})</span>
                </div>
                <div className="sess-dock-list">
                  {sessionData.todos.map((td: any, ti: number) => (
                    <div key={ti} className="sess-dock-item">
                      <span className="todo-status" data-status={td.status}>
                        {td.status === 'completed' ? '완료' : '진행 중'}
                      </span>
                      <span className="sess-dock-what">{td.content}</span>
                    </div>
                  ))}
                </div>
              </div>
            )}
          </footer>
        </section>
      </div>

      {/* 오른쪽 레일 (.prevcol, 9.7) */}
      {isRailVisible && (
        <aside className="prevcol">
          {/* 탭 헤더 */}
          <div className="prev-tabs">
            <button
              type="button"
              className={`prev-tab ${tab === 'notes' ? 'on' : ''}`}
              onClick={() => setTab('notes')}
            >
              관련 노트 <span className="prev-tab-n">{relatedNotes.length}</span>
            </button>
            <button
              type="button"
              className={`prev-tab ${tab === 'links' ? 'on' : ''}`}
              onClick={() => setTab('links')}
            >
              링크 <span className="prev-tab-n">{urlsList.length}</span>
            </button>
            <button
              type="button"
              className={`prev-tab ${tab === 'cli' ? 'on' : ''}`}
              onClick={() => setTab('cli')}
            >
              CLI
            </button>
          </div>

          {/* 관련 노트 패널 */}
          <div className="prev-pane" style={{ display: tab === 'notes' ? 'block' : 'none' }}>
            {relatedNotes.length === 0 ? (
              <div className="prev-empty">
                이 세션이 만들거나 고친 노트가 없어요. 노트에 세션 도장이 찍히기 시작한 것이 2026-09-12 이라, 그보다 이른 세션은 만들었어도 안 적혀 있어요.
              </div>
            ) : (
              <div className="relnotes-list">
                {relatedNotes.map((n) => (
                  <div
                    key={n.id}
                    className="relnote"
                    onClick={() => controller?.navigate?.(`/doc/${n.id}`)}
                  >
                    <div className="relnote-title">{n.title || '(제목 없는 노트)'}</div>
                    <div className="relnote-why">{n.why}</div>
                    <div className="relnote-date">{n.date || String(n.created || '').slice(0, 10)}</div>
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* 링크 패널 */}
          <div className="prev-pane" style={{ display: tab === 'links' ? 'block' : 'none' }}>
            {urlsList.length === 0 ? (
              <div className="prev-empty">그 세션에서 오간 링크가 없어요.</div>
            ) : (
              <div className="sess-links-list">
                {urlsList.map((u, ui) => (
                  <div
                    key={ui}
                    className="sess-page-link"
                    title={u.url}
                    onClick={() => {
                      safeInvoke('open_external', { url: u.url }).catch(() => {
                        window.open(u.url, '_blank');
                      });
                    }}
                  >
                    <strong>{u.host}</strong>
                    <span>{u.path}</span>
                    {u.count > 1 && (
                      <span className="sess-page-link-count">({u.count})</span>
                    )}
                  </div>
                ))}
              </div>
            )}
          </div>

          {/* CLI 패널 */}
          <div className="prev-pane is-cli" style={{ display: tab === 'cli' ? 'block' : 'none' }}>
            <div className="cli-placeholder">
              터미널 뷰어 (`claude --resume {sessionId}`)
            </div>
          </div>
        </aside>
      )}

      {/* 9.8 전사 굳히기 모달 (.ndf) */}
      {pinModalOpen && (
        <div className="modal-backdrop">
          <div className="ndf modal-content">
            <header className="modal-header">
              <h3>노트로 굳히기</h3>
              <button type="button" onClick={() => setPinModalOpen(false)}>✕</button>
            </header>

            <div className="ndf-body">
              {/* 회차 수 선택 */}
              <div className="pin-ranges">
                <button
                  type="button"
                  className={`btn-range ${pinCount === 100 ? 'active' : ''}`}
                  onClick={() => {
                    setPinCount(100);
                    loadTranscript(100);
                  }}
                >
                  최근 100건
                </button>
                <button
                  type="button"
                  className={`btn-range ${pinCount === 200 ? 'active' : ''}`}
                  onClick={() => {
                    setPinCount(200);
                    loadTranscript(200);
                  }}
                >
                  최근 200건
                </button>
                <button
                  type="button"
                  className={`btn-range ${pinCount === 500 ? 'active' : ''}`}
                  onClick={() => {
                    setPinCount(500);
                    loadTranscript(500);
                  }}
                >
                  최근 500건
                </button>
              </div>

              <div className="pin-hint">
                실제로 담을 회차는 {sessionData?.turns?.length || 0}건이에요.
              </div>

              {/* 제목 입력 */}
              <div className="form-group">
                <label>노트 제목</label>
                <input
                  type="text"
                  value={pinTitle}
                  onChange={(e) => setPinTitle(e.target.value)}
                />
              </div>

              {/* 아젠다 선택 */}
              <div className="form-group">
                <label>아젠다</label>
                <select
                  value={pinAgenda}
                  onChange={(e) => setPinAgenda(e.target.value)}
                >
                  {(state?.workspace?.agendas || []).map((ag: any) => (
                    <option key={ag.id} value={ag.slug || ag.id}>
                      {ag.title || ag.slug}
                    </option>
                  ))}
                </select>
              </div>

              {pinError && <div className="pin-warn">{pinError}</div>}
            </div>

            <footer className="ndf-foot modal-footer">
              <button
                type="button"
                className="btn-secondary"
                onClick={() => setPinModalOpen(false)}
              >
                취소
              </button>
              <button
                type="button"
                className="btn-primary"
                disabled={pinLoading || !pinTitle.trim()}
                onClick={() => void handleSavePin()}
              >
                {pinLoading ? '저장 중…' : '기록 노트로 만들기'}
              </button>
            </footer>
          </div>
        </div>
      )}
    </div>
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

