// Terminal Host component adhering to Chapter 14 of aisurada Part 3 screen specifications.
// The terminal itself is xterm 6.0.0 (14.13): it keeps the screen buffer, the 10,000 line
// scrollback, Unicode 11 widths (Hangul is two cells) and draws with WebGL when it can.
import React, { useState, useEffect, useRef } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import '@xterm/xterm/css/xterm.css';
import type { Terminal } from '@xterm/xterm';
import type { FitAddon } from '@xterm/addon-fit';
import { ptyStart, ptyWrite, ptyResize, ptyDetach, secretTableProbe } from './ptyBridge.ts';
import { extractLogicalLines } from './logicalLines.ts';
import { readLogicalLines, selectionToLines } from './terminalBuffer.ts';
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

// 14.8: the CSS token --font-mono is the font list, read here because the terminal library
// does not read CSS. D2Coding goes first: macOS's monospace fonts have no Hangul.
const FALLBACK_FONTS = "'D2Coding', ui-monospace, Menlo, monospace";
const terminalFontFamily = () => {
  try { return getComputedStyle(document.documentElement).getPropertyValue('--font-mono').trim() || FALLBACK_FONTS; }
  catch { return FALLBACK_FONTS; }
};
// 14.3 (2): the normal and the bold weight are loaded at 12px, and 2,000ms is the longest it waits.
const loadFonts = (family: string) => Promise.race([
  Promise.all([document.fonts.load(`12px ${family}`), document.fonts.load(`bold 12px ${family}`)]).catch(() => {}),
  new Promise(resolve => setTimeout(resolve, 2000)),
]);

const decodeChunk = (base64: string) => {
  const raw = atob(base64), bytes = new Uint8Array(raw.length);
  for (let i = 0; i < raw.length; i++) bytes[i] = raw.charCodeAt(i);
  return bytes;
};

interface PinSnapshot { lines: string[]; hadTui: boolean; selectionIndices?: [number, number]; selectionNotice?: string; }

export function TerminalHost({ tabKey, isVisible, agendas, onCreateDoc, onNavigate }: TerminalHostProps): React.JSX.Element {
  const [started, setStarted] = useState(false);
  const [fellBack, setFellBack] = useState(false);
  const [errorText, setErrorText] = useState<string | null>(null);
  const [pin, setPin] = useState<PinSnapshot | null>(null);

  const hostRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const launched = useRef(false);
  const alive = useRef(true);
  const visibleRef = useRef(isVisible);
  visibleRef.current = isVisible;

  // 14.3: the shell starts once, the first time the cell is shown. A cell taken down while
  // starting does not cancel the start; only the terminal built at step 7 is cleared.
  useEffect(() => {
    if (!isVisible || launched.current) return;
    launched.current = true;
    void (async () => {
      if (!isTauri()) { setErrorText('내장 터미널은 데스크톱 앱에서만 열립니다.'); return; }
      try {
        const sessionId = tabKey.startsWith('term:') ? tabKey.slice(5) : undefined;
        const plan = await planSessionShell(tabKey, sessionId === 'main' ? undefined : sessionId);
        if (plan.kind === 'refuse') { if (alive.current) setErrorText(plan.notice || '터미널을 시작할 수 없어요.'); return; }
        const family = terminalFontFamily();
        const [xterm, fit, unicode] = await Promise.all([
          import('@xterm/xterm'), import('@xterm/addon-fit'), import('@xterm/addon-unicode11'), loadFonts(family),
        ]);
        if (!alive.current || !hostRef.current) return;
        const term = new xterm.Terminal({ fontSize: 12, fontFamily: family, scrollback: 10000, cursorBlink: true, allowProposedApi: true });
        const fitAddon = new fit.FitAddon();
        term.loadAddon(fitAddon);
        term.loadAddon(new unicode.Unicode11Addon());
        term.unicode.activeVersion = '11';
        term.open(hostRef.current);
        try {
          const { WebglAddon } = await import('@xterm/addon-webgl');
          const webgl = new WebglAddon();
          webgl.onContextLoss(() => webgl.dispose());
          term.loadAddon(webgl);
        } catch { /* WebGL is not available: the DOM renderer draws */ }
        fitAddon.fit();
        void secretTableProbe().catch(() => {}); // 14.3 (5): not waited for, a failure is dropped
        let sent = { cols: term.cols, rows: term.rows };
        // 10.11: a plain shell starts with pty_start; a session plan starts its own shell (pty_start_session).
        const result = plan.kind === 'session'
          ? await plan.start(term.cols, term.rows, bytes => { try { term.write(bytes); } catch { /* a bad chunk is dropped */ } })
          : await ptyStart(tabKey, term.cols, term.rows, base64 => {
            try { term.write(decodeChunk(base64)); } catch { /* a chunk that is not base64 is dropped */ }
          });
        if (!alive.current) { term.dispose(); return; } // 14.3 (7)
        termRef.current = term; fitRef.current = fitAddon;
        setFellBack(result.fellBack);
        // 14.5: what the terminal writes goes to the shell as it is. onBinary (the default
        // form of mouse reports) is not sent. A size is sent only when it differs.
        term.onData(data => { void ptyWrite(tabKey, data).catch(() => {}); });
        term.onResize(({ cols, rows }) => {
          if (cols === sent.cols && rows === sent.rows) return;
          sent = { cols, rows };
          void ptyResize(tabKey, cols, rows).catch(() => {});
        });
        setStarted(true);
        term.focus();
      } catch (error) {
        if (alive.current) setErrorText(`터미널을 띄우지 못했습니다. ${error instanceof Error ? error.message : String(error)}`);
      }
    })();
  }, [isVisible, tabKey]);

  // 14.6: taking the cell down detaches the shell (only if it was started) and clears the terminal.
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
      if (launched.current) void ptyDetach(tabKey).catch(() => {});
      termRef.current?.dispose(); termRef.current = null; fitRef.current = null;
    };
  }, [tabKey]);

  // 14.5: the size is measured when the cell is shown again and while it is shown and its size
  // changes (window size, rail width). Not while hidden: a hidden box has no cell size.
  useEffect(() => {
    if (!started || !hostRef.current) return;
    if (isVisible) fitRef.current?.fit();
    const observer = new ResizeObserver(() => { if (visibleRef.current) fitRef.current?.fit(); });
    observer.observe(hostRef.current);
    return () => observer.disconnect();
  }, [started, isVisible]);

  // 14.9: opening the dialog reads the scrollback once, as logical lines of the normal buffer.
  const openPin = () => {
    const term = termRef.current;
    if (!term) return;
    const hadTui = term.buffer.active.type === 'alternate';
    const { lines, rowToLine } = readLogicalLines(term.buffer.normal);
    const picked = selectionToLines(term.getSelectionPosition(), rowToLine, hadTui);
    setPin({ lines: extractLogicalLines(lines), hadTui,
      ...('range' in picked ? { selectionIndices: picked.range } : { selectionNotice: picked.reason }) });
  };

  return (
    <div
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
        outline: 'none',
      }}
    >
      <div ref={hostRef} className="term-screen" style={{ flex: 1, minHeight: 0, padding: '8px 12px', overflow: 'hidden' }}>
        {errorText && <div className="term-empty" style={{ color: '#ef4444' }}>{errorText}</div>}
      </div>

      <div
        className="term-bar"
        style={{
          display: 'flex', alignItems: 'center', justifyContent: 'space-between', padding: '6px 12px',
          borderTop: '1px solid var(--border-color, #333)', background: 'var(--bg-secondary, #252525)', fontSize: 12,
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
          <button type="button" className="term-bar-btn btn btn-sm" disabled={!started} onClick={openPin}>굳히기</button>
        </div>
      </div>

      {pin && (
        <PinScrollbackModal
          logicalLines={pin.lines}
          selectionIndices={pin.selectionIndices}
          selectionNotice={pin.selectionNotice}
          hadTui={pin.hadTui}
          agendas={agendas}
          onClose={() => setPin(null)}
          onCreateDoc={onCreateDoc}
          onNavigate={onNavigate}
        />
      )}
    </div>
  );
}
