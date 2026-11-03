// Terminal Host component adhering to Chapter 14 of withlog Part 3 screen specifications.
import React, { useState, useEffect, useRef, useCallback } from 'react';
import { ptyStart, ptyWrite, ptyResize, ptyDetach, secretTableProbe } from './ptyBridge.ts';
import { extractLogicalLines } from './logicalLines.ts';
import { PinScrollbackModal } from './PinScrollbackModal.tsx';
import type { Agenda } from '../../workspace/types.ts';
import { planSessionShell } from '@claude-plug/terminal';

interface TerminalHostProps {
  tabKey: string; // e.g. "term:main" or "term:session-123"
  isVisible: boolean;
  agendas: Agenda[];
  onCreateDoc: (agendaSlug: string, title: string, body: string) => Promise<{ ok: boolean; id?: string; error?: string }>;
  onNavigate: (path: string) => void;
}

const ANSI_COLORS: Record<number, string> = {
  30: '#1e1e1e', 31: '#ef4444', 32: '#22c55e', 33: '#eab308',
  34: '#3b82f6', 35: '#a855f7', 36: '#06b6d4', 37: '#f8fafc',
  90: '#71717a', 91: '#f87171', 92: '#4ade80', 93: '#fde047',
  94: '#60a5fa', 95: '#c084fc', 96: '#22d3ee', 97: '#ffffff',
};

function renderAnsiLine(raw: string): React.ReactNode {
  if (!raw.includes('\x1b')) return raw || '\u00A0';
  const parts: React.ReactNode[] = [];
  const regex = /\x1b\[([0-9;]*)m/g;
  let lastIndex = 0;
  let currentColor: string | undefined;
  let isBold = false;
  let isDim = false;
  let match: RegExpExecArray | null;

  while ((match = regex.exec(raw)) !== null) {
    if (match.index > lastIndex) {
      const text = raw.substring(lastIndex, match.index).replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '');
      if (text) {
        parts.push(
          <span
            key={lastIndex}
            style={{
              color: currentColor,
              fontWeight: isBold ? 'bold' : undefined,
              opacity: isDim ? 0.6 : undefined,
            }}
          >
            {text}
          </span>
        );
      }
    }
    const codes = match[1] ? match[1].split(';').map(Number) : [0];
    for (const code of codes) {
      if (code === 0) {
        currentColor = undefined;
        isBold = false;
        isDim = false;
      } else if (code === 1) {
        isBold = true;
      } else if (code === 2) {
        isDim = true;
      } else if (ANSI_COLORS[code]) {
        currentColor = ANSI_COLORS[code];
      }
    }
    lastIndex = regex.lastIndex;
  }

  if (lastIndex < raw.length) {
    const text = raw.substring(lastIndex).replace(/\x1b\[[0-9;]*[a-zA-Z]/g, '');
    if (text) {
      parts.push(
        <span
          key={lastIndex}
          style={{
            color: currentColor,
            fontWeight: isBold ? 'bold' : undefined,
            opacity: isDim ? 0.6 : undefined,
          }}
        >
          {text}
        </span>
      );
    }
  }

  return parts.length ? parts : '\u00A0';
}

export function TerminalHost({
  tabKey,
  isVisible,
  agendas,
  onCreateDoc,
  onNavigate,
}: TerminalHostProps): React.JSX.Element {
  const [started, setStarted] = useState(false);
  const [fellBack, setFellBack] = useState(false);
  const [errorText, setErrorText] = useState<string | null>(null);
  const [rawLines, setRawLines] = useState<string[]>([]);
  const [pinModalOpen, setPinModalOpen] = useState(false);

  const screenRef = useRef<HTMLDivElement>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const containerRef = useRef<HTMLDivElement>(null);
  const startedRef = useRef(false);
  const decoderRef = useRef<TextDecoder | null>(null);

  // Buffer lines: cap to 10,000 lines (14.3)
  const appendOutput = useCallback((text: string) => {
    setRawLines(prev => {
      const split = text.split('\n');
      if (split.length === 0) return prev;
      let next: string[];
      if (prev.length === 0) {
        next = split;
      } else {
        const last = prev[prev.length - 1];
        next = [...prev.slice(0, -1), last + split[0], ...split.slice(1)];
      }
      if (next.length > 10000) {
        return next.slice(next.length - 10000);
      }
      return next;
    });
  }, []);

  // Launch PTY on mount, keep alive while tab exists in openIds
  useEffect(() => {
    if (startedRef.current) return;
    startedRef.current = true;
    if (!decoderRef.current) {
      decoderRef.current = new TextDecoder('utf-8');
    }

    let active = true;
    async function start() {
      // 14.2: Wait up to 2000ms for monospace fonts to load before starting PTY
      if (typeof document !== 'undefined' && 'fonts' in document) {
        try {
          await Promise.race([
            document.fonts.ready,
            new Promise(resolve => setTimeout(resolve, 2000)),
          ]);
        } catch {
          // ignore font loading race failure
        }
      }
      if (!active) return;
      void secretTableProbe().catch(() => {});

      try {
        const sessionId = tabKey.startsWith('term:') ? tabKey.slice(5) : undefined;
        const plan = await planSessionShell(tabKey, sessionId === 'main' ? undefined : sessionId);

        if (plan.kind === 'refuse') {
          if (active) setErrorText(plan.notice || '터미널을 시작할 수 없어요.');
          return;
        }

        const cols = 80;
        const rows = 24;

        const res = await ptyStart(tabKey, cols, rows, (b64) => {
          if (!active) return;
          try {
            const decoded = atob(b64);
            const bytes = new Uint8Array(decoded.length);
            for (let i = 0; i < decoded.length; i++) bytes[i] = decoded.charCodeAt(i);
            const str = decoderRef.current?.decode(bytes, { stream: true }) || '';
            if (str) appendOutput(str);
          } catch {
            // decode failure
          }
        });

        if (active) {
          setStarted(true);
          setFellBack(res.fellBack);
        }
      } catch (err: any) {
        if (active) setErrorText(err?.message || String(err));
      }
    }

    void start();

    return () => {
      active = false;
      // pty_detach on unmount (when tab is closed) (14.6)
      void ptyDetach(tabKey).catch(() => {});
    };
  }, [tabKey, appendOutput]);

  // Focus and keyboard input listener
  const handleKeyDown = useCallback((e: React.KeyboardEvent<HTMLElement>) => {
    if (!started) return;

    // Handle Cmd+V paste (macOS metaKey)
    if (e.metaKey && (e.key === 'v' || e.key === 'V')) {
      e.stopPropagation();
      e.preventDefault();
      if (typeof navigator !== 'undefined' && navigator.clipboard?.readText) {
        navigator.clipboard.readText().then(text => {
          if (text) void ptyWrite(tabKey, text).catch(() => {});
        }).catch(() => {});
      }
      return;
    }

    // Ignore other meta key combinations (Cmd+C, Cmd+W, etc.) so raw characters are not sent to shell
    if (e.metaKey) return;

    e.stopPropagation();

    let data = e.key;
    if (e.key === 'Enter') data = '\r';
    else if (e.key === 'Backspace') data = '\x7f';
    else if (e.key === 'Tab') {
      e.preventDefault();
      data = '\t';
    }
    else if (e.key === 'ArrowUp') data = '\x1b[A';
    else if (e.key === 'ArrowDown') data = '\x1b[B';
    else if (e.key === 'ArrowRight') data = '\x1b[C';
    else if (e.key === 'ArrowLeft') data = '\x1b[D';
    else if (e.key === 'Escape') data = '\x1b';
    else if (e.ctrlKey && e.key.length === 1) {
      const code = e.key.charCodeAt(0);
      if (code >= 97 && code <= 122) {
        data = String.fromCharCode(code - 96);
      }
    } else if (e.key.length > 1) {
      return; // Ignore other non-printable keys
    }

    void ptyWrite(tabKey, data).catch(() => {});
  }, [started, tabKey]);

  // Handle native paste event
  const handlePaste = useCallback((e: React.ClipboardEvent<HTMLElement>) => {
    if (!started) return;
    e.stopPropagation();
    e.preventDefault();
    const text = e.clipboardData.getData('text');
    if (text) {
      void ptyWrite(tabKey, text).catch(() => {});
    }
  }, [started, tabKey]);

  // Resize observer
  useEffect(() => {
    if (!started || !screenRef.current) return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        const cols = Math.max(20, Math.floor(width / 7.2));
        const rows = Math.max(5, Math.floor(height / 15));
        void ptyResize(tabKey, cols, rows).catch(() => {});
      }
    });
    observer.observe(screenRef.current);
    return () => observer.disconnect();
  }, [started, tabKey]);

  // Scroll to bottom on new output
  useEffect(() => {
    if (screenRef.current && isVisible) {
      screenRef.current.scrollTop = screenRef.current.scrollHeight;
    }
  }, [rawLines, isVisible]);

  // Focus only when tab becomes newly visible and user is not focused on an input/modal
  const prevVisible = useRef(isVisible);
  useEffect(() => {
    if (!prevVisible.current && isVisible) {
      const active = document.activeElement;
      const isInput = active && (active.tagName === 'INPUT' || active.tagName === 'TEXTAREA' || active.closest?.('.cmdk-panel, .modal-card, .ps-modal'));
      if (!isInput) {
        inputRef.current?.focus() || screenRef.current?.focus();
      }
    }
    prevVisible.current = isVisible;
  }, [isVisible]);

  const logicalLines = extractLogicalLines(rawLines);

  return (
    <div
      ref={containerRef}
      className={`term-host ${!isVisible ? 'is-hidden' : ''}`}
      data-terminal-key={tabKey}
      data-tab-key={tabKey}
      data-testid="terminal-host"
      hidden={!isVisible}
      style={{
        display: isVisible ? 'flex' : 'none',
        flexDirection: 'column',
        height: '100%',
        width: '100%',
        background: 'var(--bg-primary, #1e1e1e)',
        color: 'var(--text-primary, #f0f0f0)',
        fontFamily: 'D2Coding, Menlo, SF Mono, ui-monospace, monospace',
        fontSize: 12,
        outline: 'none',
      }}
    >
      <textarea
        ref={inputRef}
        className="term-input xterm-helper-textarea"
        aria-label="터미널 입력"
        autoCapitalize="off"
        autoComplete="off"
        spellCheck={false}
        tabIndex={0}
        onKeyDown={handleKeyDown}
        onInput={(e) => {
          const val = (e.target as HTMLTextAreaElement).value;
          if (val) {
            void ptyWrite(tabKey, val).catch(() => {});
            (e.target as HTMLTextAreaElement).value = '';
          }
        }}
        onPaste={handlePaste}
        style={{
          position: 'absolute',
          opacity: 0,
          left: -9999,
          top: 0,
          width: 1,
          height: 1,
          zIndex: -1,
        }}
      />
      {/* Terminal Screen Area */}
      <div
        ref={screenRef}
        className="term-screen"
        tabIndex={-1}
        onKeyDown={handleKeyDown}
        onPaste={handlePaste}
        onClick={() => inputRef.current?.focus() || screenRef.current?.focus()}
        style={{
          flex: 1,
          overflowY: 'auto',
          padding: '8px 12px',
          whiteSpace: 'pre-wrap',
          wordBreak: 'break-all',
          cursor: 'text',
          outline: 'none',
        }}
      >
        {errorText ? (
          <div className="term-empty" style={{ color: '#ef4444' }}>
            {errorText}
          </div>
        ) : rawLines.length === 0 ? (
          <div className="term-empty" style={{ color: '#888' }}>
            터미널을 시작하는 중…
          </div>
        ) : (
          rawLines.map((line, idx) => (
            <div key={idx}>{renderAnsiLine(line)}</div>
          ))
        )}
      </div>

      {/* Terminal Bar */}
      <div
        className="term-bar"
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          padding: '6px 12px',
          borderTop: '1px solid var(--border-color, #333)',
          background: 'var(--bg-secondary, #252525)',
          fontSize: 12,
        }}
      >
        <div>
          {fellBack && (
            <span className="term-bar-note" style={{ color: '#d97706' }}>
              원래 작업 폴더가 없어서 홈 폴더에서 이어 가요. 이 대화의 파일 명령은 홈을 기준으로 돌아요.
            </span>
          )}
        </div>
        <div>
          <button
            type="button"
            className="term-bar-btn btn btn-sm"
            disabled={!started}
            onClick={() => setPinModalOpen(true)}
          >
            굳히기
          </button>
        </div>
      </div>

      {/* Pin Modal */}
      {pinModalOpen && (
        <PinScrollbackModal
          logicalLines={logicalLines}
          hadTui={false}
          agendas={agendas}
          onClose={() => setPinModalOpen(false)}
          onCreateDoc={onCreateDoc}
          onNavigate={onNavigate}
        />
      )}
    </div>
  );
}
