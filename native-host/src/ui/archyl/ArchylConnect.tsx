import type { WorkspaceController } from '../../workspace/controller.ts';
import { cancelLogin, disconnectAccount, loginAccount, reloadAccount, useArchylAccount } from './client.ts';

export function ArchylConnect({ controller }: { controller: WorkspaceController }) {
  const account = useArchylAccount(controller);
  const call = controller.callHost;
  const native = controller.isNative();
  const connected = account.connection;
  const busy = account.phase !== 'idle';

  return (
    <fieldset className="set-connect">
      <legend>Archyl</legend>
      {native && connected && (
        <>
          <p><strong>연결됨</strong> · {connected.writable ? '조회 및 변경' : '조회 전용'}</p>
          <p>접근할 수 있는 프로젝트 {connected.projects.length}개</p>
        </>
      )}
      {native && !connected && account.phase === 'checking' && <p role="status">MCP 연결을 확인하고 있어요.</p>}
      {native && !connected && account.phase !== 'checking' && <p>Archyl 계정으로 연결하면 C4 다이어그램을 열 수 있어요.</p>}
      {!native && <p>Archyl은 데스크톱 앱에서 연결할 수 있어요.</p>}
      {account.phase === 'login' && <p role="status">브라우저에서 Archyl 로그인을 완료해 주세요.</p>}
      {native && (
        <div className="set-connect-actions">
          {account.phase === 'login' ? (
            <button type="button" onClick={() => void cancelLogin(call)}>로그인 취소</button>
          ) : (
            <>
              <button type="button" className="button-primary" disabled={busy} onClick={() => void loginAccount(call, false)}>
                {connected ? 'Archyl에 다시 로그인' : 'Archyl에 로그인'}
              </button>
              {!connected && <button type="button" disabled={busy} onClick={() => void loginAccount(call, true)}>조회 전용 연결</button>}
              {(connected || account.error) && <button type="button" disabled={busy} onClick={() => void disconnectAccount(call)}>연결 해제</button>}
              {account.error && <button type="button" disabled={busy} onClick={() => void reloadAccount(call)}>연결 확인</button>}
            </>
          )}
        </div>
      )}
      {account.error && <p className="set-connect-error" role="alert">{account.error}</p>}
    </fieldset>
  );
}
