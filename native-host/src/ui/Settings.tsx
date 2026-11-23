// 설정 화면. 6개 묶음(모양, 글꼴, 노트, 알림, AI, 연결)으로 나누어 패널을 보인다.
import type { ReactNode } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { SettingsSection, WorkspaceState } from "../workspace/types.ts";
import { CLAUDE_ENABLED } from "@claude-plug/state";
import { Modal } from "./Modal.tsx";
import { SETTINGS_SECTIONS } from "./settingsSections.ts";
import { Icon } from "./Icon.tsx";
import { LookSettings } from "./LookSettings.tsx";
import { TypographySettings } from "./TypographySettings.tsx";
import { NoteSettings } from "./NoteSettings.tsx";
import { NotificationSettings } from "./NotificationSettings.tsx";
import { AiSettings } from "./AiSettings.tsx";
import { GithubConnect } from "./GithubConnect.tsx";
import { ArchylConnect } from "./archyl/ArchylConnect.tsx";

export interface SettingsLayoutViewProps {
  section: SettingsSection;
  onSelectSection: (section: SettingsSection) => void;
  claudeEnabled: boolean;
  panes: Record<SettingsSection, ReactNode>;
}

export function SettingsLayoutView({
  section,
  onSelectSection,
  claudeEnabled,
  panes,
}: SettingsLayoutViewProps) {
  const visibleSections = SETTINGS_SECTIONS.filter(
    (s) => claudeEnabled || s.id !== "ai",
  );

  return (
    <div className="set-frame">
      <div className="set-layout">
        <nav className="set-nav" aria-label="설정 묶음">
          <div className="set-tabs" role="tablist" aria-orientation="vertical">
            {visibleSections.map((s) => (
              <button
                type="button"
                role="tab"
                key={s.id}
                id={`set-tab-${s.id}`}
                aria-controls={`set-pane-${s.id}`}
                aria-selected={section === s.id}
                tabIndex={section === s.id ? 0 : -1}
                className={`set-nav-item ${section === s.id ? "active" : ""}`}
                onClick={() => onSelectSection(s.id)}
              >
                <Icon name={s.icon} size={16} />
                <span>{s.name}</span>
              </button>
            ))}
          </div>
          <p className="set-nav-foot">
            이 기기에만 적용돼요. 노트 파일은 바뀌지 않아요.
          </p>
        </nav>
        <div className="set-panes">
          {visibleSections.map((s) => (
            <section
              key={s.id}
              id={`set-pane-${s.id}`}
              role="tabpanel"
              aria-labelledby={`set-tab-${s.id}`}
              className="set-pane"
              hidden={section !== s.id}
            >
              {panes[s.id]}
            </section>
          ))}
        </div>
      </div>
    </div>
  );
}

export function Settings({
  controller,
  state,
}: {
  controller: WorkspaceController;
  state: WorkspaceState;
}) {
  const section = state.settingsSection || "look";

  const panes: Record<SettingsSection, ReactNode> = {
    look: <LookSettings controller={controller} state={state} />,
    fonts: <TypographySettings controller={controller} typography={state.typography} />,
    notes: <NoteSettings controller={controller} state={state} />,
    notifications: <NotificationSettings controller={controller} />,
    ai: <AiSettings controller={controller} state={state} />,
    connections: (
      <>
        <GithubConnect controller={controller} />
        <ArchylConnect controller={controller} />
      </>
    ),
  };

  return (
    <Modal
      title="설정"
      close={() => controller.settings(false)}
      className="settings-modal"
      defaultWidth={880}
      defaultHeight={620}
      storageKey="aisurada.modalSize.settings-v2"
    >
      <SettingsLayoutView
        section={section}
        onSelectSection={(s) => controller.settings(true, s)}
        claudeEnabled={CLAUDE_ENABLED}
        panes={panes}
      />
    </Modal>
  );
}
