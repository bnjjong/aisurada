import { useCallback, useEffect, useRef, useState } from "react";
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
import { decisionAddress, usePendingDecisions } from "./decisionsStore.ts";
import { runnerSummary, runnerTone, useRunner } from "./runnerStore.ts";
import { RailRuns } from "./RailRuns.tsx";
import { useSessionBadge } from "@claude-plug/session";
import { architectureAddress, parseRoute } from "../navigation/routes.ts";
import { activeRailTab, RAIL_TABS, railTabTarget, rememberRailTab } from "../navigation/railTabs.ts";
import type { RailTab } from "../navigation/railTabs.ts";
import { ResourceFormModal } from "./ResourceFormModal.tsx";
import { explainError, getResource, listResources, selectableSkills } from "./resourcesClient.ts";
import type { ResourceKind, ResourceRow } from "./resourcesClient.ts";
import { parseRepos } from "./repoLinks.ts";
import { useGlobalResources } from "./globalResources/globalResourcesStore.ts";
import { openApprovalsDialog } from "./approvalsDialogStore.ts";
import { EMPTY_PROJECT_ROWS, ProjectResourceRows, rowsByProject } from "./RailProjectResources.tsx";
import type { ProjectRows } from "./RailProjectResources.tsx";

const TAB_LABELS: Record<RailTab, string> = { notes: "노트", agents: "에이전트", global: "글로벌 리소스" };
const TAB_ICONS: Record<RailTab, "note" | "bot" | "globe"> = { notes: "note", agents: "bot", global: "globe" };
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
  ],
  global: [
    ["global-resources", "글로벌 리소스 정의"],
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
  // 글로벌 리소스 화면이 엔진에서 읽어 둔 개수. 글로벌 탭의 요약에 쓴다(화면에 들어가기 전에는 기본 시드 값이다)
  const globalResources = useGlobalResources();
  // 승인이 필요한 스킬·에이전트 변경의 수(#53). 스킬·에이전트 메뉴에 배지로 붙는다
  const approvalCounts = useApprovalCounts();
  const approvalBadge = badgeText(approvalCounts);
  // 러너가 사람의 답을 기다리는 태스크의 수. 태스크 메뉴에 배지로 붙는다
  const runner = useRunner();
  const waitingAnswers = runner.waiting.length;
  // AI가 노트에 쓴 「정할 것」 가운데 사람의 답을 기다리는 노트(후속 노트 포함)
  const decisions = usePendingDecisions();
  // 에이전트 탭 이름 옆 배지는 「내 차례」(질문 대기 + 답을 기다리는 노트 + 승인 대기)의 합이다. 사람이 병목이라 다른 탭에서도 보이게 한다
  const mineBadge = countText(waitingAnswers + decisions.length + badgeTotal(approvalCounts));
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

  // 프로젝트별 에이전트/스킬 등록 모달 상태
  const [resForm, setResForm] = useState<{ kind: ResourceKind; project: string; existing?: ResourceRow } | null>(null);
  // 프로젝트 헤더 + 버튼 액션 메뉴 상태
  const [projectMenuAnchor, setProjectMenuAnchor] = useState<{ element: HTMLElement; slug: string; name: string } | null>(null);
  // 프로젝트별 에이전트·스킬(이름 목록과 개수)과 편집 창이 고를 전체 스킬 목록
  const [projectRows, setProjectRows] = useState<Record<string, ProjectRows>>({});
  const [allSkills, setAllSkills] = useState<ResourceRow[]>([]);
  // 레일에서 펼친 에이전트·스킬 줄(`slug/kind`)과, 자원을 열다 난 오류(프로젝트 하나에만 보인다)
  const [expandedRes, setExpandedRes] = useState<ReadonlySet<string>>(() => new Set());
  const [resError, setResError] = useState<{ slug: string; text: string } | null>(null);

  const loadProjectResources = useCallback(async () => {
    try {
      const list = await listResources(controller.callEngine);
      setProjectRows(rowsByProject(list.resources));
      setAllSkills(list.resources.filter((r) => r.kind === "skill"));
    } catch {
      // 무시
    }
  }, [controller.callEngine]);

  // 처음 뜰 때와 승인 대기 수가 바뀔 때(승인·거절로 자원이 바뀌었을 수 있다) 다시 읽는다
  useEffect(() => {
    void loadProjectResources();
  }, [loadProjectResources, approvalCounts.pending, approvalCounts.unapproved]);
  // 창이 다시 눈에 띌 때도 읽는다(다른 창이나 AI가 고쳤을 수 있다)
  useEffect(() => {
    const onFocus = () => void loadProjectResources();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [loadProjectResources]);

  const toggleRes = (key: string) =>
    setExpandedRes((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  // 목록의 행에는 본문과 쓰기 전 토큰이 없어서 편집 창을 열기 전에 한 건을 다시 읽는다
  const editRes = async (slug: string, row: ResourceRow) => {
    try {
      setResError(null);
      setResForm({ kind: row.kind, project: slug, existing: await getResource(controller.callEngine, row.id) });
    } catch (e) {
      setResError({ slug, text: explainError(e) });
    }
  };
  const resourceRows = (slug: string, name: string) => (
    <ProjectResourceRows
      slug={slug}
      name={name}
      rows={projectRows[slug] ?? EMPTY_PROJECT_ROWS}
      expanded={expandedRes}
      error={resError?.slug === slug ? resError.text : undefined}
      toggle={toggleRes}
      create={(kind) => setResForm({ kind, project: slug })}
      edit={(row) => void editRes(slug, row)}
    />
  );

  const group = (
    name: string,
    slug: string,
    agendas: Agenda[],
    id?: string,
  ) => {
    const isNone = slug === LOOSE_GROUP_KEY || slug === "" || slug === "__none__";
    const projectObj = ws.projects.find((p) => p.slug === slug);
    const isArchylTarget = !isNone && (asText(projectObj?.type) === "development" || Boolean(asText(projectObj?.archylProjectId)));
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
              <span
                className="rail-group-name"
                onClick={(e) => {
                  if (!isNone && agendas.length > 0) {
                    e.stopPropagation();
                    controller.navigate("/agenda/" + encodeURIComponent(agendas[0].id));
                  }
                }}
                title={isNone ? undefined : agendas.length > 0 ? `${name} 아젠다 보기` : undefined}
              >
                {name}
              </span>
              <small className="rail-group-count">{agendas.length}</small>
            </button>
            {id && (
              <span className="rail-acts">
                <button
                  title={`${name}에 추가`}
                  aria-label={`${name}에 추가`}
                  onClick={(e) => {
                    e.stopPropagation();
                    setProjectMenuAnchor({ element: e.currentTarget, slug, name });
                  }}
                >
                  <Icon name="plus" />
                </button>
                <button
                  title="프로젝트 편집"
                  aria-label={`${name} 프로젝트 편집`}
                  onClick={(e) => {
                    e.stopPropagation();
                    controller.form({ kind: "project", id });
                  }}
                >
                  <Icon name="edit" />
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
            {isArchylTarget && (
              <div
                className={`rail-agenda rail-sub-resource ${state.route === architectureAddress(slug) ? "agenda-selected" : ""}`}
              >
                <span className="agenda-dot sub-resource-icon" aria-hidden="true">
                  <Icon name="architecture" size={13} />
                </span>
                <button
                  type="button"
                  className="agenda-name"
                  onClick={() => controller.navigate(architectureAddress(slug))}
                  title={`${name}의 C4 아키텍처 보기`}
                >
                  <span className="sub-resource-label">C4 아키텍처</span>
                </button>
              </div>
            )}
            {!isNone && resourceRows(slug, name)}
          </div>
        )}
      </section>
    );
  };
  return (
    <aside className="workspace-rail intellij-rail">
      <div className="rail-activity-bar" aria-label="도구 막대">
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
                title={`${TAB_LABELS[id]} (${id === "notes" ? "⌥1" : id === "global" ? "⌥2" : "⌥3"})`}
                onClick={() => {
                  // 이미 그 탭의 화면이면 그대로 두고, 아니면 그 탭에서 마지막으로 보던 화면으로 간다
                  const to = railTabTarget(id, state.route);
                  if (to) controller.navigate(to);
                }}
              >
                {TAB_LABELS[id]}
                <span className="rail-tab-icon">
                  <Icon name={TAB_ICONS[id]} size={18} />
                </span>
                {badge && <span className="rail-tab-badge" role="img" aria-label={`내 차례 ${badge}건`}>{badge}</span>}
              </button>
            );
          })}
        </div>
        <div className="rail-activity-bottom">
          <button
            type="button"
            className="rail-activity-btn"
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
            type="button"
            className="rail-activity-btn"
            title="설정"
            aria-label="설정"
            onClick={() => controller.settings(true)}
          >
            <Icon name="settings" size={20} />
          </button>
          <button
            className={`iconbtn icon-btn ${route.kind === "guide" ? "active" : ""}`}
            title="연동 가이드"
            aria-label="연동 가이드"
            aria-current={route.kind === "guide" ? "page" : undefined}
            onClick={() => controller.navigate("/guide")}
          >
            ?
          </button>
        </div>
      </div>
      <div className="rail-panel" id="rail-panel" role="tabpanel" aria-labelledby={`rail-tab-${tab}`}>
        <div className="rail-companion-card">
          <div className="rail-companion-top">
            <div className="rail-companion-avatar" role="img" aria-label="aisurada">
              <LiveBrandMark />
            </div>
            <div className="rail-companion-meta">
              <div className="rail-companion-name" title={ws.path || ws.name}>
                <strong>{ws.name}</strong>
                {ws.sample && <span className="sample-badge">샘플</span>}
              </div>
              {ws.path && <span className="rail-companion-sub" title={ws.path}>{ws.path}</span>}
            </div>
            <div className="rail-companion-bell">
              <NotificationBell controller={controller} />
            </div>
          </div>
          <div className="rail-companion-speech" role="status" aria-live="polite">
            <div className="rail-companion-bubble">
              <span className="rail-companion-bubble-tail" aria-hidden="true" />
              <p className="rail-companion-msg">
                {waitingAnswers > 0 ? (
                  `💬 답변 대기 태스크가 ${waitingAnswers}건 있어요.`
                ) : decisions.length > 0 ? (
                  // 답을 기다리는 노트는 누르면 첫 노트를 연다
                  <button type="button" className="rail-companion-action" title={`${decisions[0].title} 열기`} onClick={() => controller.navigate(decisionAddress(decisions[0]))}>
                    {`📝 AI가 답을 기다리는 노트가 ${decisions.length}건 있어요.`} <span>열기 ›</span>
                  </button>
                ) : approvalBadge ? (
                  // 승인을 기다리는 변경은 글로만 알리지 않고, 누르면 승인 대기함이 열리게 한다
                  <button type="button" className="rail-companion-action" title="승인 대기함 열기" onClick={openApprovalsDialog}>
                    {`⚠️ 승인이 필요한 변경이 ${approvalBadge}건 있어요.`} <span>열기 ›</span>
                  </button>
                ) : (
                  "⚡️ 오늘 어떤 작업을 진행할까요?"
                )}
              </p>
            </div>
          </div>
        </div>
        <nav data-rail-section="nav" aria-label={tab === "notes" ? "노트 메뉴" : tab === "agents" ? "에이전트 메뉴" : "글로벌 리소스 메뉴"}>
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
      {tab === "agents" && runner.supported && <RailRuns controller={controller} taskKey={route.kind === "task" ? route.key : undefined} />}
      {tab === "global" ? (
      <section data-rail-section="global-resources" className="agenda-summary rail-resources">
        <header>
          <h2>공통 리소스 관리</h2>
        </header>
        <p className="rail-hint">
          공통 개발 프로세스 에이전트, 글로벌 스킬, 인프라 연동 리소스를 설정합니다.
        </p>

        <div className="rail-global-summary" style={{ margin: "10px 0 16px" }}>
          <header>
            <h3>공통 자원 관리</h3>
          </header>
          <ul className="rail-global-list">
            <li><span>프로세스 에이전트</span><strong>{globalResources.agents.length}개</strong></li>
            <li><span>글로벌 스킬</span><strong>{globalResources.skills.length}개</strong></li>
            <li><span>연동 인프라 (Mock)</span><strong>{globalResources.resources.length}개</strong></li>
          </ul>
        </div>

        <header style={{ marginTop: "16px" }}>
          <h2>프로젝트 리소스</h2>
        </header>
        {ws.projects.length === 0 ? (
          <p className="rail-hint">프로젝트를 만들면 프로젝트별 에이전트와 스킬이 여기에 나와요.</p>
        ) : (
          <div className="rail-project-resources-list">
            {ws.projects.map((p) => {
              const slug = asText(p.slug);
              const name = asText(p.title);
              const projectAgendas = ws.agendas.filter((a) => a.project === slug);
              return (
                <div key={asText(p.id) || slug} className="rail-group" style={{ marginBottom: "10px" }}>
                  <header>
                    <button
                      type="button"
                      className="rail-group-toggle"
                      style={{ cursor: projectAgendas.length > 0 ? "pointer" : "default" }}
                      onClick={() => {
                        if (projectAgendas.length > 0) {
                          controller.navigate("/agenda/" + encodeURIComponent(projectAgendas[0].id));
                        }
                      }}
                      title={projectAgendas.length > 0 ? `${name} 아젠다 보기` : undefined}
                    >
                      <Icon name="folder" size={16} />
                      <span className="rail-group-name">
                        {name}
                      </span>
                    </button>
                    <span className="rail-acts">
                      <button
                        title={`${name}에 추가`}
                        aria-label={`${name}에 추가`}
                        onClick={(e) => {
                          e.stopPropagation();
                          setProjectMenuAnchor({ element: e.currentTarget, slug, name });
                        }}
                      >
                        <Icon name="plus" />
                      </button>
                    </span>
                  </header>
                  <div className="rail-kids" style={{ marginTop: "4px" }}>
                    {resourceRows(slug, name)}
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </section>
      ) : tab === "notes" ? (
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
      ) : null}
      <button
        className="rail-back"
        onClick={() => void controller.launcher()}
      >
        시작 화면
      </button>
      {projectMenuAnchor && (
        <ActionMenu
          label={`${projectMenuAnchor.name} 추가 메뉴`}
          anchor={{ current: projectMenuAnchor.element }}
          onClose={() => setProjectMenuAnchor(null)}
          items={[
            {
              key: "agenda",
              label: "새 아젠다 추가",
              icon: "dotPlus",
              onSelect: () => {
                const slug = projectMenuAnchor.slug;
                setProjectMenuAnchor(null);
                controller.form({ kind: "agenda", project: slug });
              },
            },
            {
              key: "agent",
              label: "새 에이전트 등록 (이 프로젝트용)",
              icon: "bot",
              onSelect: () => {
                const slug = projectMenuAnchor.slug;
                setProjectMenuAnchor(null);
                setResForm({ kind: "agent", project: slug });
              },
            },
            {
              key: "skill",
              label: "새 스킬 등록 (이 프로젝트용)",
              icon: "zap",
              onSelect: () => {
                const slug = projectMenuAnchor.slug;
                setProjectMenuAnchor(null);
                setResForm({ kind: "skill", project: slug });
              },
            },
          ]}
        />
      )}
      {resForm && (
        <ResourceFormModal
          call={controller.callEngine}
          kind={resForm.kind}
          project={resForm.project}
          skillChoices={selectableSkills({
            project: resForm.project,
            globals: allSkills.filter((s) => s.scope === "global"),
            projectSkills: allSkills.filter((s) => s.project === resForm.project),
          })}
          repoNames={parseRepos(ws.projects.find((p) => p.slug === resForm.project || p.id === resForm.project)?.repos).map((r) => r.name)}
          existing={resForm.existing}
          close={() => setResForm(null)}
          saved={() => {
            setResForm(null);
            void loadProjectResources();
          }}
        />
      )}
      </div>
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
