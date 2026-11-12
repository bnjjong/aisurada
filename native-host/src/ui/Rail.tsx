import { useRef, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { WorkspaceState, Agenda } from "../workspace/types.ts";
import { asText } from "../workspace/types.ts";
import { agendaOrder, HUE_COLORS, railGroupCollapsed } from "./model.ts";
import { ActionMenu } from "./ActionMenu.tsx";
import { LiveBrandMark } from "./BrandMark.tsx";
import { Icon } from "./Icon.tsx";
import { useSessionBadge } from "@claude-plug/session";
import { parseRoute } from "../navigation/routes.ts";
export function Rail({
  controller,
  state,
}: {
  controller: WorkspaceController;
  state: WorkspaceState;
}) {
  useSessionBadge();
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
      <div className="brand" role="img" aria-label="aisurada">
        <LiveBrandMark />
      </div>
      <div className="workspace-identity" title={ws.path || ws.name}>
        <strong>{ws.name}</strong>
        {ws.sample && <span className="sample-badge">샘플</span>}
        {ws.path && <span className="workspace-path" title={ws.path}>{ws.path}</span>}
      </div>
      <nav data-rail-section="nav" aria-label="워크스페이스 메뉴">
        {[
          ["notes", "전체 노트"],
          ["calendar", "캘린더"],
          ["favorites", "즐겨찾기"],
          ["connections", "연결"],
          ["templates", "템플릿"],
          ["trash", "휴지통"],
          ["guide", "연동 가이드"],
        ].map(([path, label]) => (
          <button
            className={`nav-item ${state.route.startsWith("/" + path) ? "active" : ""}`}
            key={path}
            onClick={() => controller.navigate("/" + path)}
          >
            {label}
            {path === "trash" && !!ws.trash.length && ` ${ws.trash.length}`}
          </button>
        ))}
      </nav>
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
