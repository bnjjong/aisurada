// 설정을 묶음별 패널로 보여 주고 항목을 검색한다.
import { useState, type ReactNode } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { SettingsSection, WorkspaceState } from "../workspace/types.ts";
import { CLAUDE_ENABLED } from "@claude-plug/state";
import { Modal } from "./Modal.tsx";
import { enabledManagers, useManagers } from "./managersStore.tsx";
import { SettingsGroup, SettingsPane, SettingsRow } from "./SettingsParts.tsx";
import {
  SETTINGS_SECTIONS,
  searchSettings,
  type SearchIndexItem,
} from "./settingsSections.ts";
import { Icon } from "./Icon.tsx";
import { LookSettings } from "./LookSettings.tsx";
import { TypographySettings } from "./TypographySettings.tsx";
import { NoteSettings } from "./NoteSettings.tsx";
import { NotificationSettings } from "./NotificationSettings.tsx";
import { WorkspaceGitSettings } from "./WorkspaceGitSettings.tsx";
import { AiSettings } from "./AiSettings.tsx";
import { GithubConnect } from "./GithubConnect.tsx";

export interface SettingsLayoutViewProps {
  section: SettingsSection;
  onSelectSection: (section: SettingsSection) => void;
  claudeEnabled: boolean;
  panes: Record<SettingsSection, ReactNode>;
}

/**
 * 탭 목록에서 화살표 및 Home/End 키로 다음 묶음을 계산하는 순수 함수
 */
export function nextSection(
  current: SettingsSection,
  key: string,
  visible: readonly SettingsSection[],
): SettingsSection | null {
  const idx = visible.indexOf(current);
  if (idx < 0) return null;
  const len = visible.length;
  if (key === "ArrowDown" || key === "ArrowRight") {
    return visible[(idx + 1) % len];
  }
  if (key === "ArrowUp" || key === "ArrowLeft") {
    return visible[(idx - 1 + len) % len];
  }
  if (key === "Home") {
    return visible[0];
  }
  if (key === "End") {
    return visible[len - 1];
  }
  return null;
}

export function SettingsLayoutView({
  section,
  onSelectSection,
  claudeEnabled,
  panes,
}: SettingsLayoutViewProps) {
  const [query, setQuery] = useState("");
  const [activeIndex, setActiveIndex] = useState(0);

  const visibleSections = SETTINGS_SECTIONS.filter(
    (s) => claudeEnabled || s.id !== "ai",
  );

  const isSearching = query.trim().length > 0;
  const results = isSearching
    ? searchSettings(query, { claude: claudeEnabled })
    : [];

  const handleTabKeyDown = (
    e: React.KeyboardEvent<HTMLButtonElement>,
    current: SettingsSection,
  ) => {
    const next = nextSection(
      current,
      e.key,
      visibleSections.map((v) => v.id),
    );
    if (next) {
      e.preventDefault();
      onSelectSection(next);
      if (typeof document !== "undefined") {
        document.getElementById(`set-tab-${next}`)?.focus();
      }
    }
  };

  const jumpToSetting = (item: SearchIndexItem) => {
    setQuery("");
    setActiveIndex(0);
    onSelectSection(item.section);
    if (typeof requestAnimationFrame !== "undefined") {
      requestAnimationFrame(() => {
        setTimeout(() => {
          if (typeof document !== "undefined") {
            const el = document.querySelector(`[data-setting="${item.key}"]`);
            if (el) {
              const prefersReducedMotion =
                typeof window !== "undefined" &&
                window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;
              el.scrollIntoView({
                behavior: prefersReducedMotion ? "auto" : "smooth",
                block: "center",
              });
              el.classList.add("set-flash");
              setTimeout(() => el.classList.remove("set-flash"), 1200);
              const focusable = el.querySelector<HTMLElement>(
                "input, button, select, textarea",
              );
              if (focusable) focusable.focus();
            }
          }
        }, 50);
      });
    }
  };

  const handleKeyDown = (e: React.KeyboardEvent<HTMLInputElement>) => {
    if (e.key === "Escape") {
      if (query.trim().length > 0) {
        e.preventDefault();
        e.stopPropagation();
        setQuery("");
        setActiveIndex(0);
      }
      return;
    }
    if (e.key === "ArrowDown") {
      e.preventDefault();
      if (results.length > 0) {
        setActiveIndex((prev) => (prev + 1) % results.length);
      }
      return;
    }
    if (e.key === "ArrowUp") {
      e.preventDefault();
      if (results.length > 0) {
        setActiveIndex((prev) => (prev - 1 + results.length) % results.length);
      }
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      if (results.length > 0) {
        jumpToSetting(results[activeIndex] || results[0]);
      }
    }
  };

  return (
    <div className="set-frame">
      <div className="set-layout">
        <nav className="set-nav" aria-label="설정 묶음">
          <div className="set-search-wrap">
            <input
              type="search"
              role="combobox"
              aria-autocomplete="list"
              aria-expanded={isSearching}
              aria-controls="set-search-results"
              aria-activedescendant={
                isSearching && results[activeIndex]
                  ? `set-result-${activeIndex}`
                  : undefined
              }
              className="set-search-input"
              placeholder="설정 찾기..."
              value={query}
              onChange={(e) => {
                setQuery(e.target.value);
                setActiveIndex(0);
              }}
              onKeyDown={handleKeyDown}
              aria-label="설정 찾기"
            />
          </div>

          <div
            className="set-tabs"
            role="tablist"
            aria-orientation="vertical"
            hidden={isSearching}
          >
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
                onKeyDown={(e) => handleTabKeyDown(e, s.id)}
              >
                <Icon name={s.icon} size={16} />
                <span>{s.name}</span>
              </button>
            ))}
          </div>

          {isSearching && (
            <div
              id="set-search-results"
              className="set-results"
              role="listbox"
              aria-label="설정 검색 결과"
            >
              {results.length === 0 ? (
                <p className="set-no-results">찾는 설정이 없어요</p>
              ) : (
                results.map((item, idx) => {
                  const meta = SETTINGS_SECTIONS.find((s) => s.id === item.section);
                  const isSelected = idx === activeIndex;
                  return (
                    <button
                      type="button"
                      key={`${item.section}-${item.key}`}
                      id={`set-result-${idx}`}
                      role="option"
                      aria-selected={isSelected}
                      className={`set-result ${isSelected ? "active" : ""}`}
                      onClick={() => jumpToSetting(item)}
                    >
                      <Icon name={meta?.icon || "note"} size={16} />
                      <div className="set-result-text">
                        <span className="set-result-label">{item.label}</span>
                        <span className="set-result-section">{meta?.name}</span>
                      </div>
                    </button>
                  );
                })
              )}
            </div>
          )}
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
  const { data } = useManagers();
  const active = enabledManagers(data);
  const projectNames = [...new Set(active.map(item =>
    state.workspace?.projects.find(project => project.slug === item.manager.project)?.title || item.manager.project,
  ))];

  const panes: Record<SettingsSection, ReactNode> = {
    look: <LookSettings controller={controller} state={state} />,
    fonts: <TypographySettings controller={controller} typography={state.typography} />,
    notes: <NoteSettings controller={controller} state={state} />,
    workspace: section === "workspace" && <WorkspaceGitSettings
      key={state.workspace?.path}
      call={controller.callHost}
      path={controller.isNative() && !state.workspace?.sample ? state.workspace?.path : undefined}
    />,
    notifications: <NotificationSettings controller={controller} />,
    ai: <AiSettings controller={controller} state={state} />,
    connections: (
      <SettingsPane title="연결" desc="앱이 바깥 서비스에 닿는 연결이에요. 키는 이 Mac의 키체인에만 둬요.">
        <GithubConnect controller={controller} />
        <SettingsGroup>
          <SettingsRow as="div" title="매니저 연결" settingKey="managers"
            desc={`에이전트 탭의 「매니저」에서 연결을 관리해요.${active.length ? ` 지금 ${active.length}개가 켜져 있어요(${projectNames.join(", ")}).` : ""}`}
            control={<button type="button" onClick={() => {
              controller.settings(false);
              controller.navigate("/managers");
            }}>매니저 화면 열기 →</button>}
          />
        </SettingsGroup>
      </SettingsPane>
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
