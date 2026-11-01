import type { WorkspaceState } from '../workspace/types.ts';
import { asText } from '../workspace/types.ts';

export function VaultBanner({ state, dismiss }: { state: WorkspaceState; dismiss?: () => void }) {
  const failures = state.vault.failures, workspace = state.workspace;
  if (!workspace) return null;
  const reason = failures.some(line => line.kind === 'conflict') ? '다른 쪽이 그 노트를 먼저 고쳐서 멈췄어요.'
    : failures.some(line => line.kind === 'reloaded') ? '디스크 글을 다시 읽었으니 한 번 더 해 주세요.' : '디스크 상태를 확인하고 다시 시도해 주세요.';
  return <>
    {!!workspace.skipped.length && <details className="vault-warning"><summary>읽지 못한 파일 {workspace.skipped.length}개</summary>
      {workspace.skipped.map((row, index) => <p key={index}>{asText(row.path)} · {asText(row.reason)}</p>)}</details>}
    {!!workspace.conflicts.length && <details className="vault-warning"><summary>충돌 사본 {workspace.conflicts.length}개</summary>
      {workspace.conflicts.map((row, index) => <p key={index}>{asText(row.path)}</p>)}</details>}
    {!!failures.length && <div className="vault-write-failures sync-banner warn" role="alert"><strong>저장 실패 {failures.length}건 — {reason}</strong>
      {dismiss && <button onClick={dismiss} aria-label="저장 실패 알림 닫기">닫기</button>}
      <ul className="sync-banner-files">{failures.map(line => <li key={line.text}>{line.text}</li>)}</ul></div>}
  </>;
}
