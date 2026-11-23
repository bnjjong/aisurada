// 설정 화면의 「Claude 연결」. Claude Code가 이 워크스페이스의 노트를 MCP 도구로 읽고 쓰게 연결한다.
// 일은 모두 `ais-cli claude-mcp`가 한다(Rust `ais-cli`의 register.rs). 화면은 `sidecar_call`로 부르기만 하므로
// 호스트 커맨드를 더하지 않는다.
import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useRef, useState } from "react";
import { StatusLine } from "../../ui/SettingsParts.tsx";

export interface ConnectStatus {
  claudeFound: boolean;
  registered: boolean;
  write: boolean;
  detail: string;
  /** Java 시절의 jar를 가리키는 낡은 등록이다. 앱을 새로 받으면 그 자리가 없어서 다시 연결해야 한다. 없으면(true가 아니면) 낡지 않다. */
  stale?: boolean;
}
interface Reply {
  status: number;
  stdout: string;
  stderr: string;
}
export type ConnectCall = (argv: string[]) => Promise<Reply>;

const callHost: ConnectCall = (argv) => invoke<Reply>("sidecar_call", { argv });

/** `claude-mcp status`가 낸 글을 읽는다. 모양이 틀리면 null이다. */
export function parseConnectStatus(stdout: string): ConnectStatus | null {
  try {
    const row = JSON.parse(stdout) as Partial<ConnectStatus>;
    if (typeof row.claudeFound !== "boolean" || typeof row.registered !== "boolean") return null;
    return {
      claudeFound: row.claudeFound,
      registered: row.registered,
      write: row.write === true,
      detail: typeof row.detail === "string" ? row.detail : "",
      ...(row.stale === true ? { stale: true } : {}),
    };
  } catch {
    return null;
  }
}

/** 실패한 호출의 사람이 읽을 이유. 엔진은 오류 글을 표준 오류에 낸다. */
export function connectFailure(reply: Reply): string {
  const text = reply.stderr.trim() || reply.stdout.trim();
  return text || `연결 명령이 실패했어요(종료 코드 ${reply.status}).`;
}

export function ConnectView({
  status,
  busy,
  error,
  write,
  desktop,
  onWrite,
  onConnect,
  onDisconnect,
  onRefresh,
}: {
  status: ConnectStatus | null;
  busy: boolean;
  error: string;
  write: boolean;
  desktop: string;
  onWrite: (write: boolean) => void;
  onConnect: () => void;
  onDisconnect: () => void;
  onRefresh?: () => void;
}) {
  let line: string;
  let dotStatus: "ok" | "off" | "warn";
  if (!status && error) {
    line = "연결 상태를 확인하지 못했어요.";
    dotStatus = "warn";
  } else if (!status) {
    line = "확인하는 중이에요…";
    dotStatus = "off";
  } else if (!status.claudeFound) {
    line = "Claude Code를 찾지 못했어요. 설치한 뒤 설정을 다시 열어 주세요.";
    dotStatus = "warn";
  } else if (!status.registered) {
    line = "연결되어 있지 않아요.";
    dotStatus = "off";
  } else if (status.stale) {
    line = "예전 앱의 경로로 연결되어 있어요. 아래에서 다시 연결해 주세요.";
    dotStatus = "warn";
  } else {
    line = status.write ? "연결되어 있어요 · 노트를 읽고 쓸 수 있어요" : "연결되어 있어요 · 노트를 읽기만 해요";
    dotStatus = "ok";
  }

  return (
    <div className="set-card set-connect" data-setting="claude">
      <StatusLine
        status={dotStatus}
        text={line}
        desc="Claude Code가 이 워크스페이스의 노트를 도구로 찾아 읽을 수 있게 연결해요. 연결은 지금 연 워크스페이스 하나에 묶여요. 다른 워크스페이스에서 다시 연결하면 그쪽으로 바뀌어요."
        className="set-connect-status"
      />
      {!status && error && onRefresh && (
        <div className="set-connect-actions">
          <button type="button" disabled={busy} onClick={onRefresh}>
            다시 확인
          </button>
        </div>
      )}
      {status?.claudeFound && (
        <>
          <label className="set-row set-switch set-connect-write" data-setting="claude-write">
            <span className="set-row-label">
              <strong>노트를 만들고 고치는 것도 허용</strong>
              <small>끄면 Claude는 읽기만 해요. 켜도 영구 삭제는 못 해요.</small>
            </span>
            <input
              type="checkbox"
              role="switch"
              className="set-toggle"
              checked={write}
              disabled={busy}
              onChange={(e) => onWrite(e.target.checked)}
            />
          </label>
          <div className="set-connect-actions">
            <button type="button" className="button-primary" disabled={busy} onClick={onConnect}>
              {status.registered ? "이 설정으로 다시 연결" : "Claude Code에 연결"}
            </button>
            {status.registered && (
              <button type="button" disabled={busy} onClick={onDisconnect}>
                연결 끊기
              </button>
            )}
          </div>
        </>
      )}
      {error && (
        <p className="set-connect-error" role="alert">
          {error}
        </p>
      )}
      {desktop && (
        <details className="set-connect-desktop" style={{ padding: "0 18px 14px" }}>
          <summary>Claude Desktop에 연결하려면</summary>
          <p>
            Claude Desktop 설정 파일(<code>claude_desktop_config.json</code>)의 <code>mcpServers</code>에 아래를 합쳐
            넣고 Claude Desktop을 다시 시작해요.
          </p>
          <pre>{desktop}</pre>
          <button type="button" onClick={() => void navigator.clipboard?.writeText(desktop)}>
            복사
          </button>
        </details>
      )}
    </div>
  );
}

export function ClaudeConnect({
  call = callHost,
  readStatus = parseConnectStatus,
}: {
  call?: ConnectCall;
  readStatus?: (stdout: string) => ConnectStatus | null;
}) {
  const [status, setStatus] = useState<ConnectStatus | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const [write, setWrite] = useState(false);
  const [desktop, setDesktop] = useState("");
  const alive = useRef(true);

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  const refresh = useCallback(async () => {
    try {
      const reply = await call(["claude-mcp", "status"]);
      if (!alive.current) return;
      const parsed = readStatus(reply.stdout);
      if (parsed) {
        setStatus(parsed);
        setWrite(parsed.write);
      } else {
        setStatus(null);
        setError(connectFailure(reply));
      }
    } catch (e) {
      if (alive.current) {
        setStatus(null);
        setError(String(e));
      }
    }
  }, [call, readStatus]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    let cancelled = false;
    (async () => {
      try {
        const reply = await call(["claude-mcp", "desktop-snippet", ...(write ? ["--write"] : [])]);
        if (!cancelled && reply.status === 0 && reply.stdout.trim()) {
          setDesktop(reply.stdout.trim());
        }
      } catch {
        /* Desktop 조각 읽기는 실패해도 본체 동작을 막지 않는다 */
      }
    })();
    return () => {
      cancelled = true;
    };
  }, [call, write]);

  const run = async (argv: string[]) => {
    setBusy(true);
    setError("");
    try {
      const reply = await call(argv);
      if (!alive.current) return;
      if (reply.status !== 0) setError(connectFailure(reply));
      await refresh();
    } catch (e) {
      if (alive.current) setError(String(e));
    } finally {
      if (alive.current) setBusy(false);
    }
  };

  return (
    <ConnectView
      status={status}
      busy={busy}
      error={error}
      write={write}
      desktop={desktop}
      onWrite={setWrite}
      onConnect={() => void run(["claude-mcp", "add", ...(write ? ["--write"] : [])])}
      onDisconnect={() => void run(["claude-mcp", "remove"])}
      onRefresh={() => void refresh()}
    />
  );
}
