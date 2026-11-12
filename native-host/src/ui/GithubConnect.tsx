// 설정 화면의 「GitHub 연결」. 사용자가 만든 GitHub App의 App ID와 개인 키(.pem) 파일로 앱을 연결한다.
// GitHub에 닿는 일과 키 보관(macOS 키체인)은 모두 호스트가 한다. 화면은 키 파일의 경로만 넘기고 키 내용은 보지 않는다.
import { useEffect, useRef, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import { connectGithub, disconnectGithub, refreshGithub, useGithub } from "./githubStore.ts";
import type { GithubStatus } from "./githubStore.ts";

export function GithubConnectView({
  status,
  error,
  busy,
  appId,
  pemName,
  native,
  onAppId,
  onPickKey,
  onConnect,
  onDisconnect,
}: {
  status: GithubStatus | null;
  error: string;
  busy: boolean;
  appId: string;
  pemName: string;
  native: boolean;
  onAppId: (value: string) => void;
  onPickKey: () => void;
  onConnect: () => void;
  onDisconnect: () => void;
}) {
  const connected = !!status?.connected;
  let line: string;
  if (!native) line = "데스크톱 앱에서만 연결할 수 있어요.";
  else if (!status) line = error ? "연결 상태를 확인하지 못했어요." : "확인하는 중이에요…";
  else if (connected) line = `연결되어 있어요 · ${status.appName || status.appSlug || "GitHub App"} (App ID ${status.appId ?? "?"})`;
  else line = "연결되어 있지 않아요.";
  const ready = appId.trim() !== "" && pemName !== "";
  return (
    <fieldset className="set-connect set-github">
      <legend>GitHub 연결</legend>
      <p className="set-connect-hint">
        내가 만든 GitHub App으로 앱이 저장소를 읽고 받게 연결해요. 프로젝트에 GitHub 저장소를 적으면 앱이 알아서 받아 둬요.
        개인 키는 이 Mac의 키체인에만 보관하고, 노트 폴더에는 남기지 않아요. 앱을 설치한 저장소만 보여요.
      </p>
      <p className="set-connect-status" role="status">
        {line}
      </p>
      {native && connected && status && (
        <p className="set-connect-hint">
          GitHub 전용 저장소를 받는 폴더: <code>{status.cloneRoot}</code>
          <br />
          개인 키는 키체인에 들어갔어요. 내려받은 .pem 파일은 지워도 돼요.
        </p>
      )}
      {native && !connected && (
        <>
          <label className="set-github-field">
            <span>App ID</span>
            <input
              inputMode="numeric"
              placeholder="예: 1234567"
              value={appId}
              disabled={busy}
              onChange={(e) => onAppId(e.target.value)}
            />
          </label>
          <div className="set-github-field">
            <span>개인 키(.pem)</span>
            <div className="set-github-key">
              <button type="button" disabled={busy} onClick={onPickKey}>
                {pemName ? "다른 파일 고르기" : "파일 고르기"}
              </button>
              <small>{pemName || "아직 고르지 않았어요"}</small>
            </div>
          </div>
        </>
      )}
      {native && (
        <div className="set-connect-actions">
          {!connected ? (
            <button type="button" className="button-primary" disabled={busy || !ready} onClick={onConnect}>
              {busy ? "연결하는 중…" : "GitHub에 연결"}
            </button>
          ) : (
            <button type="button" disabled={busy} onClick={onDisconnect}>
              연결 끊기
            </button>
          )}
        </div>
      )}
      {error && (
        <p className="set-connect-error" role="alert">
          {error}
        </p>
      )}
    </fieldset>
  );
}

export function GithubConnect({ controller }: { controller: WorkspaceController }) {
  const github = useGithub();
  const native = controller.isNative();
  const [appId, setAppId] = useState("");
  const [pemPath, setPemPath] = useState("");
  const [busy, setBusy] = useState(false);
  const alive = useRef(true);
  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);
  useEffect(() => {
    if (native) void refreshGithub(controller.callHost);
  }, [controller, native]);

  const run = async (job: () => Promise<unknown>) => {
    setBusy(true);
    try {
      await job();
    } finally {
      if (alive.current) setBusy(false);
    }
  };

  return (
    <GithubConnectView
      status={github.status}
      error={github.error}
      busy={busy}
      appId={appId}
      pemName={pemPath.split("/").pop() ?? ""}
      native={native}
      onAppId={setAppId}
      onPickKey={() => {
        void controller.chooseFile(["pem"]).then((path) => {
          if (path && alive.current) setPemPath(path);
        });
      }}
      onConnect={() =>
        void run(async () => {
          const status = await connectGithub(controller.callHost, appId, pemPath);
          // 연결되면 키가 키체인에 들어갔으니 입력은 비운다. 경로를 화면에 남기지 않는다.
          if (status?.connected && alive.current) {
            setAppId("");
            setPemPath("");
          }
        })
      }
      onDisconnect={() => void run(() => disconnectGithub(controller.callHost))}
    />
  );
}
