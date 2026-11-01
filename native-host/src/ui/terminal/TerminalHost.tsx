// Terminal Host component adhering to Chapter 14 of withlog Part 3 screen specifications.
import React, { useState, useEffect, useRef, useCallback } from 'react';
import { ptyStart, ptyWrite, ptyResize, ptyDetach } from './ptyBridge.ts';
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
  const containerRef = useRef<HTMLDivElement>(null);
  const startedRef = useRef(false);

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

  // Launch or reattach PTY on first view
  useEffect(() => {
    if (!isVisible || startedRef.current) return;
    startedRef.current = true;

    let active = true;
    async function start() {
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
            const str = new TextDecoder().decode(bytes);
            appendOutput(str);
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

    start();

    return () => {
      active = false;
      // pty_detach on unmount (14.6)
      ptyDetach(tabKey);
    };
  }, [isVisible, tabKey, appendOutput]);

  // Focus and keyboard input listener
  const handleKeyDown = useCallback((e: React.KeyboardEvent<HTMLDivElement>) => {
    if (!started) return;
    e.stopPropagation();

    let data = e.key;
    if (e.key === 'Enter') data = '\r';
    else if (e.key === 'Backspace') data = '\x7f';
    else if (e.key === 'Tab') data = '\t';
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

    ptyWrite(tabKey, data).catch(() => {});
  }, [started, tabKey]);

  // Resize observer
  useEffect(() => {
    if (!started || !screenRef.current) return;
    const observer = new ResizeObserver((entries) => {
      for (const entry of entries) {
        const { width, height } = entry.contentRect;
        const cols = Math.max(20, Math.floor(width / 7.2));
        const rows = Math.max(5, Math.floor(height / 15));
        ptyResize(tabKey, cols, rows).catch(() => {});
      }
    });
    observer.observe(screenRef.current);
    return () => observer.disconnect();
  }, [started, tabKey]);

  // Auto scroll to bottom
  useEffect(() => {
    if (screenRef.current && isVisible) {
      screenRef.current.scrollTop = screenRef.current.scrollHeight;
    }
  }, [rawLines, isVisible]);

  const logicalLines = extractLogicalLines(rawLines);

  return (
    <div
      ref={containerRef}
      className={`term-host ${!isVisible ? 'is-hidden' : ''}`}
      data-tab-key={tabKey}
      data-testid="terminal-host"
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
      {/* Terminal Screen Area */}
      <div
        ref={screenRef}
        className="term-screen"
        tabIndex={0}
        onKeyDown={handleKeyDown}
        onClick={() => screenRef.current?.focus()}
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
            <div key={idx}>{line || '\u00A0'}</div>
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
