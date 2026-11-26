import { useEffect, useState } from "react";
import type { TypedHostCall, VaultGitStatus } from "../workspace/hostCommands.ts";
import { SettingsGroup, SettingsPane, SettingsRow, SettingsSwitch, StatusLine } from "./SettingsParts.tsx";

const messages: Record<VaultGitStatus["phase"], string> = {
  off: "자동 커밋이 꺼져 있어요.",
  ready: "변경 이력을 이 컴퓨터의 Git 저장소에 남겨요.",
  "needs-setup": "첫 Git 커밋을 아직 만들지 못했어요.",
  "git-unavailable": "Git을 실행할 수 없어요. Git을 설치하거나 실행 경로를 확인한 뒤 다시 시도해 주세요.",
  unsupported: "이 워크스페이스의 .git을 사용할 수 없어요. Git 저장소 상태를 확인해 주세요.",
  "other-branch": "자동 커밋은 main 브랜치에서 실행돼요.",
  paused: "실행 환경 설정에서 자동 커밋이 꺼져 있어요.",
};

export function WorkspaceGitSettings({ call, path }: { call: TypedHostCall; path?: string }) {
  const [status, setStatus] = useState<VaultGitStatus>();
  const [error, setError] = useState("");
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    if (!path) return;
    let active = true;
    void call("vault_git_status", { expectedVault: path }).then(
      value => { if (active) setStatus(value); },
      failure => { if (active) setError(String(failure)); },
    );
    return () => { active = false; };
  }, [call, path]);

  const save = async (enabled: boolean) => {
    if (!path || saving) return;
    setSaving(true);
    setError("");
    try {
      setStatus(await call("set_vault_git", { expectedVault: path, enabled }));
    } catch (failure) {
      setError(String(failure));
      // 첫 커밋에 실패했어도 켜짐 값은 저장됐을 수 있어요.
      try { setStatus(await call("vault_git_status", { expectedVault: path })); }
      catch { /* 설정을 읽지 못하면 마지막으로 확인한 값을 표시한다. */ }
    } finally {
      setSaving(false);
    }
  };

  const retry = status?.phase === "needs-setup" || status?.phase === "git-unavailable";
  return <SettingsPane title="워크스페이스" desc="지금 열린 워크스페이스에 적용돼요.">
    <SettingsGroup>
      <SettingsRow title="Git으로 변경 이력 저장" settingKey="vault-git"
        desc="Git 저장소가 없으면 현재 파일을 첫 커밋으로 남겨요. 꺼도 쌓인 이력은 볼 수 있어요."
        control={<SettingsSwitch checked={status?.enabled ?? false} disabled={!status || saving}
          onChange={enabled => void save(enabled)} />}
      />
      <StatusLine
        status={error ? "warn" : status?.phase === "ready" ? "ok" : status?.phase === "off" || !status ? "off" : "warn"}
        role={error ? "alert" : "status"}
        text={error || (saving ? "Git 설정을 적용하고 있어요…" : status ? messages[status.phase] : path ? "Git 상태를 확인하고 있어요…" : "앱에서 워크스페이스를 열면 설정할 수 있어요.")}
        desc={retry && <button type="button" className="button-subtle" disabled={saving} onClick={() => void save(true)}>다시 시도</button>}
      />
    </SettingsGroup>
  </SettingsPane>;
}
