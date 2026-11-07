import type { WorkspaceController } from "../workspace/controller.ts";
import type { WorkspaceState, Agenda } from "../workspace/types.ts";
import { asText } from "../workspace/types.ts";
import { agendaOrder, HUE_COLORS, railGroupCollapsed } from "./model.ts";
import { LiveBrandMark } from "./BrandMark.tsx";
import { RailProjects, useProjectsList } from "@claude-plug/projects";
import { useSessionBadge } from "@claude-plug/session";
export function Rail({
  controller,
  state,
}: {
  controller: WorkspaceController;
  state: WorkspaceState;
}) {
  useProjectsList();
  useSessionBadge();
  const LOOSE_GROUP_KEY = "(미분류)";
  const ws = state.workspace!,
    sorted = agendaOrder(ws.agendas, ws.notes);
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
          <header>
            <button
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
              {collapsed ? "▸" : "▾"} {name}
            </button>
            {id && (
              <>
                <button
                  aria-label={`${name} 프로젝트 편집`}
                  onClick={() => controller.form({ kind: "project", id })}
                >
                  편집
                </button>
                <button
                  aria-label={`${name}에 아젠다 추가`}
                  onClick={() =>
                    controller.form({ kind: "agenda", project: slug })
                  }
                >
                  ＋
                </button>
              </>
            )}
          </header>
        )}
        {!collapsed &&
          agendas.map((a) => (
            <div
              className={`rail-agenda ${state.hiddenAgendas.includes(a.id) ? "hidden-agenda" : ""}`}
              key={a.id}
            >
              <button
                className="agenda-dot"
                title={`${a.title} 표시 전환`}
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
                aria-label={`${a.title} 아젠다 편집`}
                onClick={() => controller.form({ kind: "agenda", id: a.id })}
              >
                ⋯
              </button>
            </div>
          ))}
      </section>
    );
  };
  return (
    <aside className="workspace-rail">
      <div className="brand" role="img" aria-label="aisurada">
        <LiveBrandMark />
      </div>
      <div className="workspace-identity">
        <strong>{ws.name}</strong>
        {ws.sample && <span className="sample-badge">샘플</span>}
      </div>
      <nav data-rail-section="nav" aria-label="워크스페이스 메뉴">
        {[
          ["calendar", "캘린더"],
          ["favorites", "즐겨찾기"],
          ["connections", "연결"],
          ["templates", "템플릿"],
          ["trash", "휴지통"],
          ["guide", "연동 가이드"],
        ].map(([path, label]) => (
          <button
            className={`nav-item ${state.route.startsWith("/" + path) || (path === "calendar" && state.route.startsWith("/agenda")) ? "active" : ""}`}
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
          <h2>아젠다</h2>
          <button
            aria-label="프로젝트 추가"
            onClick={() => controller.form({ kind: "project" })}
          >
            프로젝트 ＋
          </button>
          <button
            aria-label="아젠다 추가"
            onClick={() => controller.form({ kind: "agenda" })}
          >
            ＋
          </button>
        </header>
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
        <RailProjects controller={controller} state={state} />
      </section>
      <footer className="rail-footer">
        <button
          className="iconbtn icon-btn"
          title="테마 전환"
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
