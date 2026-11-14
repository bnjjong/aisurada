import { useEffect, useRef, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { WorkspaceState, Agenda } from "../workspace/types.ts";
import { asText } from "../workspace/types.ts";
import { agendaOrder, HUE_COLORS, railGroupCollapsed } from "./model.ts";
import { ActionMenu } from "./ActionMenu.tsx";
import { LiveBrandMark } from "./BrandMark.tsx";
import { Icon } from "./Icon.tsx";
import { NotificationBell } from "./NotificationCenter.tsx";
import { badgeText, badgeTotal } from "./approvalsClient.ts";
import { useApprovalCounts } from "./approvalsStore.ts";
import { runnerSummary, runnerTone, useRunner } from "./runnerStore.ts";
import { useSessionBadge } from "@claude-plug/session";
import { parseRoute, projectAddress } from "../navigation/routes.ts";
import { activeRailTab, RAIL_TABS, railTabTarget, rememberRailTab } from "../navigation/railTabs.ts";
import type { RailTab } from "../navigation/railTabs.ts";

const TAB_LABELS: Record<RailTab, string> = { notes: "노트", agents: "에이전트" };
/** 탭마다 보이는 메뉴(주소 조각, 글). 연동 가이드는 두 탭 공통이라 아래 줄에 있다 */
const NAV_ITEMS: Record<RailTab, ReadonlyArray<readonly [string, string]>> = {
  notes: [
    ["notes", "전체 노트"],
    ["calendar", "캘린더"],
    ["favorites", "즐겨찾기"],
    ["connections", "연결"],
    ["templates", "템플릿"],
    ["trash", "휴지통"],
  ],
  agents: [
    ["tasks", "태스크"],
    ["resources", "스킬·에이전트"],
  ],
};
/** 숫자를 배지 글로. 0이면 빈 글, 99를 넘으면 99+ */
const countText = (n: number) => (n <= 0 ? "" : n > 99 ? "99+" : String(n));

export function Rail({
  controller,
  state,
}: {
  controller: WorkspaceController;
  state: WorkspaceState;
}) {
  useSessionBadge();
  // 승인이 필요한 스킬·에이전트 변경의 수(#53). 스킬·에이전트 메뉴에 배지로 붙는다
  const approvalCounts = useApprovalCounts();
  const approvalBadge = badgeText(approvalCounts);
  // 러너가 사람의 답을 기다리는 태스크의 수. 태스크 메뉴에 배지로 붙는다
  const runner = useRunner();
  const waitingAnswers = runner.waiting.length;
  // 에이전트 탭 이름 옆 배지는 「내 차례」(질문 대기 + 승인 대기)의 합이다. 사람이 병목이라 다른 탭에서도 보이게 한다
  const mineBadge = countText(waitingAnswers + badgeTotal(approvalCounts));
  // 켜질 탭은 화면 주소가 정한다. 주소가 탭을 정하지 않는 화면이면 직전에 켜 둔 탭이다
  const tab = activeRailTab(state.route);
  useEffect(() => rememberRailTab(state.route), [state.route]);
  // 「＋」 하나로 새 프로젝트와 새 아젠다 중 고르게 하는 메뉴.
  const addButton = useRef<HTMLButtonElement>(null);
  const [adding, setAdding] = useState(false);
  const LOOSE_GROUP_KEY = "(미분류)";
  const ws = state.workspace!,
    sorted = agendaOrder(ws.agendas, ws.notes);
  const route = parseRoute(state.route);
  const activeAgendaId = route.kind === "agenda" ? route.id : null;
  const group = (
    name: string,
    slug: string,
    agendas: Agenda[],
    id?: string,
  ) => {
    const isNone = slug === LOOSE_GROUP_KEY || slug === "" || slug === "__none__";
    const collapsed = railGroupCollapsed(state.prefs.collapsedProjects, ws.projects.length, slug);
    return (
      <section key={slug || LOOSE_GROUP_KEY} className="rail-group">
        {ws.projects.length > 0 && (
          <header className={isNone ? "rail-group-none" : undefined}>
            <button
              className="rail-group-toggle"
              aria-expanded={!collapsed}
              onClick={() =>
                controller.preference({
                  collapsedProjects: collapsed
                    ? state.prefs.collapsedProjects.filter(
                        (p) =>
                          p !== slug &&
                          (!isNone ||
                            (p !== LOOSE_GROUP_KEY &&
                              p !== "" &&
                              p !== "__none__")),
                      )
                    : [
                        ...state.prefs.collapsedProjects,
                        isNone ? LOOSE_GROUP_KEY : slug,
                      ],
                })
              }
            >
              <Icon name="chevron" size={12} />
              <Icon name={isNone ? "folderDashed" : "folder"} size={16} />
              <span className="rail-group-name">{name}</span>
              <small className="rail-group-count">{agendas.length}</small>
            </button>
            {id && (
              <span className="rail-acts">
                <button
                  title="프로젝트 편집"
                  aria-label={`${name} 프로젝트 편집`}
                  onClick={() => controller.form({ kind: "project", id })}
                >
                  <Icon name="edit" />
                </button>
                <button
                  title="이 프로젝트에 아젠다 추가"
                  aria-label={`${name}에 아젠다 추가`}
                  onClick={() =>
                    controller.form({ kind: "agenda", project: slug })
                  }
                >
                  <Icon name="plus" />
                </button>
              </span>
            )}
          </header>
        )}
        {!collapsed && (
          <div className={ws.projects.length > 0 ? "rail-kids" : undefined}>
            {agendas.map((a) => {
            const isActive =
              activeAgendaId !== null &&
              (a.id === activeAgendaId || a.slug === activeAgendaId);
            return (
              <div
                className={`rail-agenda ${isActive ? "active" : ""} ${state.hiddenAgendas.includes(a.id) ? "hidden-agenda" : ""}`}
                key={a.id}
              >
                <button
                  className="agenda-dot"
                  title={`${a.title} 표시 전환`}
                  aria-label={`${a.title} 표시 전환`}
                  aria-pressed={!state.hiddenAgendas.includes(a.id)}
                  style={{ color: HUE_COLORS[asText(a.hue)] || "var(--accent)" }}
                  onClick={() =>
                    controller.filterAgendas(
                      state.hiddenAgendas.includes(a.id)
                        ? state.hiddenAgendas.filter((id) => id !== a.id)
                        : [...state.hiddenAgendas, a.id],
                    )
                  }
                >
                  ●
                </button>
                <button
                  className="agenda-name"
                  onClick={() =>
                    controller.navigate("/agenda/" + encodeURIComponent(a.id))
                  }
                >
                  {a.title}
                  <small>
                    {ws.notes.filter((n) => n.agendaId === a.id).length}
                  </small>
                </button>
                <button
                  className="agenda-edit"
                  title="아젠다 편집"
                  aria-label={`${a.title} 아젠다 편집`}
                  onClick={() => controller.form({ kind: "agenda", id: a.id })}
                >
                  <Icon name="edit" />
                </button>
              </div>
            );
          })}
          </div>
        )}
      </section>
    );
  };
  return (
    <aside className="workspace-rail">
      <div className="rail-top">
        <div className="brand" role="img" aria-label="aisurada">
          <LiveBrandMark />
        </div>
        <NotificationBell controller={controller} />
      </div>
      <div className="workspace-identity" title={ws.path || ws.name}>
        <strong>{ws.name}</strong>
        {ws.sample && <span className="sample-badge">샘플</span>}
        {ws.path && <span className="workspace-path" title={ws.path}>{ws.path}</span>}
      </div>
      <div className="rail-tabs" role="tablist" aria-label="레일 구역" data-rail-section="tabs">
        {RAIL_TABS.map((id) => {
          const selected = id === tab;
          const badge = id === "agents" ? mineBadge : "";
          return (
            <button
              key={id}
              type="button"
              role="tab"
              id={`rail-tab-${id}`}
              aria-selected={selected}
              aria-controls="rail-panel"
              className={`rail-tab ${selected ? "active" : ""}`}
              onClick={() => {
                // 이미 그 탭의 화면이면 그대로 두고, 아니면 그 탭에서 마지막으로 보던 화면으로 간다
                const to = railTabTarget(id, state.route);
                if (to) controller.navigate(to);
              }}
            >
              {TAB_LABELS[id]}
              {badge && <span className="rail-tab-badge" role="img" aria-label={`내 차례 ${badge}건`}>{badge}</span>}
            </button>
          );
        })}
      </div>
      <div className="rail-panel" id="rail-panel" role="tabpanel" aria-labelledby={`rail-tab-${tab}`}>
      <nav data-rail-section="nav" aria-label={tab === "notes" ? "노트 메뉴" : "에이전트 메뉴"}>
        {NAV_ITEMS[tab].map(([path, label]) => (
          <button
            className={`nav-item ${(path === "tasks" ? state.route.startsWith("/task") : state.route.startsWith("/" + path)) ? "active" : ""}`}
            key={path}
            onClick={() => controller.navigate("/" + path)}
          >
            {label}
            {path === "trash" && !!ws.trash.length && ` ${ws.trash.length}`}
            {path === "tasks" && waitingAnswers > 0 && <span className="nav-badge" role="img" aria-label={`사람의 답을 기다리는 태스크 ${waitingAnswers}개`}>{countText(waitingAnswers)}</span>}
            {path === "resources" && approvalBadge && <span className="nav-badge" role="img" aria-label={`승인이 필요한 변경 ${approvalBadge}개`}>{approvalBadge}</span>}
          </button>
        ))}
      </nav>
      {tab === "agents" && runner.supported && (
        <button
          type="button"
          className={`rail-runner ${runnerTone(runner)}`}
          title="러너 설정은 태스크 화면 맨 위에서 바꿔요"
          onClick={() => controller.navigate("/tasks")}
        >
          <i aria-hidden="true" />
          러너 · {runnerSummary(runner)}
        </button>
      )}
      {tab === "agents" ? (
      <section data-rail-section="resources" className="agenda-summary rail-resources">
        <header>
          <h2>프로젝트 자원</h2>
        </header>
        {ws.projects.length === 0 && (
          <p className="rail-hint">프로젝트를 만들면 프로젝트별 스킬·에이전트가 여기에 나와요.</p>
        )}
        {ws.projects.map((p) => {
          const slug = asText(p.slug),
            name = asText(p.title),
            isActive = route.kind === "project" && route.slug === slug;
          return (
            <div className={`rail-agenda ${isActive ? "active" : ""}`} key={asText(p.id) || slug}>
              <button
                type="button"
                className="agenda-name"
                title="프로젝트 페이지(스킬·에이전트)"
                aria-label={`${name} 프로젝트 페이지 열기`}
                aria-current={isActive ? "page" : undefined}
                onClick={() => controller.navigate(projectAddress(slug))}
              >
                <Icon name="folder" size={16} />
                {name}
              </button>
            </div>
          );
        })}
      </section>
      ) : (
      <section data-rail-section="agendas" className="agenda-summary">
        <header>
          <h2>{ws.projects.length > 0 ? "프로젝트" : "아젠다"}</h2>
          <button
            ref={addButton}
            className="rail-add"
            title="새로 만들기"
            aria-label="새로 만들기"
            aria-haspopup="menu"
            aria-expanded={adding}
            onClick={() => setAdding((open) => !open)}
          >
            <Icon name="plus" />
          </button>
        </header>
        {adding && (
          <ActionMenu
            label="새로 만들기"
            anchor={addButton}
            onClose={() => setAdding(false)}
            items={[
              {
                key: "project",
                label: "새 프로젝트",
                icon: "folder",
                onSelect: () => controller.form({ kind: "project" }),
              },
              {
                key: "agenda",
                label: "새 아젠다",
                icon: "dotPlus",
                onSelect: () => controller.form({ kind: "agenda" }),
              },
            ]}
          />
        )}
        {ws.projects.map((p) =>
          group(
            asText(p.title),
            asText(p.slug),
            sorted.filter((a) => a.project === p.slug),
            asText(p.id),
          ),
        )}
        {group(
          "미분류",
          LOOSE_GROUP_KEY,
          sorted.filter((a) => !ws.projects.some((p) => p.slug === a.project)),
        )}
      </section>
      )}
      </div>
      <footer className="rail-footer">
        <button
          className="iconbtn icon-btn"
          title="테마 전환"
          aria-label="테마 전환"
          onClick={() =>
            controller.preference({
              theme: state.theme === "dark" ? "light" : "dark",
            })
          }
        >
          ◐
        </button>
        <button
          className="iconbtn icon-btn"
          title="설정"
          aria-label="설정"
          onClick={() => controller.settings(true)}
        >
          ⚙
        </button>
        {/* 연동 가이드는 두 탭이 같이 쓰므로 탭 메뉴가 아니라 이 줄에 둔다 */}
        <button
          className={`iconbtn icon-btn ${route.kind === "guide" ? "active" : ""}`}
          title="연동 가이드"
          aria-label="연동 가이드"
          aria-current={route.kind === "guide" ? "page" : undefined}
          onClick={() => controller.navigate("/guide")}
        >
          ?
        </button>
        <button
          className="rail-back"
          onClick={() => void controller.launcher()}
        >
          시작 화면
        </button>
      </footer>
    </aside>
  );
}
export function AgendaFilter({
  state,
  controller,
}: {
  state: WorkspaceState;
  controller: WorkspaceController;
}) {
  const ws = state.workspace!;
  return (
    <details className="agenda-filter">
      <summary>아젠다 필터</summary>
      <button onClick={() => controller.filterAgendas([])}>모두 표시</button>
      {[
        ...ws.projects.map((p) => ({
          slug: asText(p.slug),
          title: asText(p.title),
        })),
        { slug: "", title: "미분류" },
      ].map((p) => {
        const list = ws.agendas.filter((a) =>
          p.slug
            ? a.project === p.slug
            : !ws.projects.some((x) => x.slug === a.project),
        );
        return (
          <section key={p.slug}>
            <strong>{p.title}</strong>
            <button
              onClick={() =>
                controller.filterAgendas([
                  ...state.hiddenAgendas,
                  ...list.map((a) => a.id),
                ])
              }
            >
              모두 숨기기
            </button>
            <button
              onClick={() =>
                controller.filterAgendas(
                  state.hiddenAgendas.filter(
                    (id) => !list.some((a) => a.id === id),
                  ),
                )
              }
            >
              보이기
            </button>
            <button
              onClick={() =>
                controller.filterAgendas(
                  ws.agendas.filter((a) => !list.includes(a)).map((a) => a.id),
                )
              }
            >
              이 프로젝트만 보기
            </button>
            {list.map((a) => (
              <div key={a.id}>
                <label>
                  <input
                    type="checkbox"
                    checked={!state.hiddenAgendas.includes(a.id)}
                    onChange={() =>
                      controller.filterAgendas(
                        state.hiddenAgendas.includes(a.id)
                          ? state.hiddenAgendas.filter((id) => id !== a.id)
                          : [...state.hiddenAgendas, a.id],
                      )
                    }
                  />
                  {a.title}
                </label>
                <button
                  onClick={() =>
                    controller.filterAgendas(
                      ws.agendas.filter((x) => x.id !== a.id).map((x) => x.id),
                    )
                  }
                >
                  이 아젠다만 보기
                </button>
              </div>
            ))}
          </section>
        );
      })}
    </details>
  );
}
