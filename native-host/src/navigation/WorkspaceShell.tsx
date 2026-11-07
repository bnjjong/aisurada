import { Component } from "react";
import type { CSSProperties, ReactNode } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { WorkspaceState } from "../workspace/types.ts";
import { parseRoute, routeTab } from "./routes.ts";
import type { ScreenRoute } from "./routes.ts";
import { NoteScreen } from "./NoteReading.tsx";
import { TabBar } from "./TabBar.tsx";
import { InteractionOverlays } from "../interaction/Overlays.tsx";
import { VaultBanner } from "../data/VaultBanner.tsx";
import { Rail } from "../ui/Rail.tsx";
import { agendaOrder, clampRailWidth } from "../ui/model.ts";
import { RailResizer } from "../ui/RailResizer.tsx";
import { Preview } from "../ui/Preview.tsx";
import { Calendar, AgendaPage } from "../ui/CalendarAgenda.tsx";
import {
  Page,
  Favorites,
  Connections,
  Templates,
  Trash,
  Guide,
} from "../ui/OtherPages.tsx";
import { ExternalFile } from "../ui/ExternalFile.tsx";
import { TerminalHost } from "../ui/terminal/TerminalHost.tsx";
import { SessionRoute } from "@claude-plug/session";
import { useAskState } from "@claude-plug/state";
type Props = { controller: WorkspaceController; state: WorkspaceState };
class RouteBoundary extends Component<
  { children: ReactNode; controller: WorkspaceController },
  { failed: boolean }
> {
  state = { failed: false };
  static getDerivedStateFromError() {
    return { failed: true };
  }
  componentDidCatch(error: Error) {
    console.error("이 화면을 열지 못했어요", error);
  }
  render() {
    if (!this.state.failed) return this.props.children;
    return (
      <section className="empty-state">
        <h1>이 화면을 여는 중 문제가 생겼어요</h1>
        <div className="page-actions">
          <button onClick={() => location.reload()}>새로고침</button>
          <button onClick={() => this.props.controller.navigate("/calendar")}>
            캘린더
          </button>
          <button onClick={() => this.props.controller.navigate("/launcher")}>
            시작 화면
          </button>
        </div>
      </section>
    );
  }
}
export function WorkspaceShell({ controller, state }: Props) {
  useAskState();
  const route = parseRoute(state.route),
    ws = state.workspace!,
    rail = route.kind !== "doc" && route.kind !== "file",
    railWidth = clampRailWidth(state.prefs.railWidth);
  return (
    <div className="workspace-shell workspace-frame" aria-busy={state.busy}>
      <TabBar controller={controller} state={state} />
      {ws.noteReadFailed && (
        <div className="read-failure" role="alert">
          노트를 못 읽었어요 — 파일은 그대로 있어요. 엔진과 워크스페이스 경로를
          확인해 주세요.
        </div>
      )}
      <VaultBanner
        state={state}
        dismiss={() => controller.data.dismissFailures()}
      />
      <div
        className={`workspace-screen ${rail ? "" : "without-rail"}`}
        style={rail ? ({ "--rail-width": `${railWidth}px` } as CSSProperties) : undefined}
      >
        {rail && <Rail controller={controller} state={state} />}
        {rail && <RailResizer controller={controller} width={railWidth} />}
        <main className="screen-content" id="screen-content" role="tabpanel">
          <RouteBoundary
            controller={controller}
            key={route.kind === "templates" ? "templates" : state.route}
          >
            {route.kind !== "terminal" && (
              <RouteContent
                route={route}
                controller={controller}
                state={state}
              />
            )}
          </RouteBoundary>
          {state.tabs.openIds
            .filter((id) => id.startsWith("term:"))
            .map((id) => (
              <TerminalHost
                key={id}
                tabKey={id}
                isVisible={route.kind === "terminal" && routeTab(route) === id}
                // 14.9: the first agenda is the rail's first (12.1), not the engine's first
                agendas={agendaOrder(ws.agendas, ws.notes)}
                onCreateDoc={async (agendaSlug, title, body) => {
                  try {
                    const agendaObj = ws.agendas.find((a) => a.slug === agendaSlug);
                    const res = await controller.data.createDoc({
                      agendaId: agendaObj?.id || agendaSlug,
                      type: "free",
                      title,
                      body,
                      date: ws.today,
                      time: new Date().toTimeString().slice(0, 5),
                      tags: ["터미널기록"],
                      tier: "record",
                    });
                    if (res.ok && res.value?.id) {
                      return { ok: true, id: res.value.id as string };
                    }
                    return { ok: false, error: "노트 생성에 실패했어요." };
                  } catch (err: any) {
                    return { ok: false, error: err?.message || String(err) };
                  }
                }}
                onNavigate={(path) => controller.navigate(path)}
              />
            ))}
        </main>
      </div>
      {state.preview && (
        <Preview key={state.preview} controller={controller} state={state} />
      )}
      <InteractionOverlays controller={controller} state={state} />
    </div>
  );
}
function RouteContent({
  route,
  controller,
  state,
}: Props & { route: ScreenRoute }) {
  const ws = state.workspace!;
  switch (route.kind) {
    case "doc": {
      const note = ws.notes.find((n) => n.id === route.id);
      return note ? (
        <NoteScreen note={note} controller={controller} />
      ) : (
        <Page title="이 노트를 찾을 수 없어요" />
      );
    }
    case "file":
      return <ExternalFile path={route.path} controller={controller} />;
    case "calendar":
      return (
        <Calendar state={state} controller={controller} query={route.query} />
      );
    case "agenda":
      return (
        <AgendaPage
          state={state}
          controller={controller}
          id={route.id}
          open={route.open}
        />
      );
    case "favorites":
      return <Favorites state={state} controller={controller} />;
    case "connections":
      return <Connections state={state} controller={controller} />;
    case "templates":
      return (
        <Templates
          initialOpen={route.open}
          state={state}
          controller={controller}
        />
      );
    case "trash":
      return <Trash state={state} controller={controller} />;
    case "guide":
      return <Guide />;
    case "session":
      return (
        <SessionRoute
          sessionId={route.id}
          controller={controller}
          state={state}
        />
      );
    default:
      return null;
  }
}
