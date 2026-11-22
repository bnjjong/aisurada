import { invoke } from "@tauri-apps/api/core";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { getArchylProjectTeam } from "../editor/archyl.ts";

type Bounds = { left: number; top: number; right: number; bottom: number };
type ViewAction =
  | { kind: "show"; projectId: string; teamId: string; bounds: Bounds; reload: boolean }
  | { kind: "hide" | "close" };

let pendingViewAction = Promise.resolve();

function updateView(action: ViewAction) {
  const result = pendingViewAction.then(() => invoke<void>("archyl_view", { action }));
  pendingViewAction = result.catch(() => {});
  return result;
}

function viewBounds(element: HTMLElement): Bounds | undefined {
  const { left, top, right, bottom } = element.getBoundingClientRect();
  const ratio = (value: number, extent: number) => Math.min(1, Math.max(0, value / extent));
  const bounds = {
    left: ratio(left, window.innerWidth),
    top: ratio(top, window.innerHeight),
    right: ratio(right, window.innerWidth),
    bottom: ratio(bottom, window.innerHeight),
  };
  if (bounds.left < bounds.right && bounds.top < bounds.bottom) return bounds;
}

const dialogOpen = () => document.querySelector('[aria-modal="true"]') !== null;

function NativeView({
  projectId, teamId, revision, hidden,
}: {
  projectId: string;
  teamId: string;
  revision: number;
  hidden: boolean;
}) {
  const placeholder = useRef<HTMLDivElement>(null);
  const requested = useRef<{ key: string; revision?: number }>({ key: "" });
  const [error, setError] = useState("");

  useLayoutEffect(() => () => {
    requested.current = { key: "" };
    void updateView({ kind: "close" }).catch(() => {});
  }, []);

  useLayoutEffect(() => {
    const element = placeholder.current;
    if (!element) return;
    let hasDialog = dialogOpen();
    const sync = () => {
      // 네이티브 웹 화면은 HTML 대화 상자 뒤에 놓을 수 없어 잠시 숨긴다.
      const bounds = hidden || hasDialog ? undefined : viewBounds(element);
      const key = bounds ? JSON.stringify({ bounds, revision }) : "hide";
      const previous = requested.current;
      if (previous.key === key) return;
      const request = { key, revision: bounds ? revision : previous.revision };
      requested.current = request;
      const action: ViewAction = bounds
        ? { kind: "show", projectId, teamId, bounds, reload: previous.revision !== revision }
        : { kind: "hide" };
      void updateView(action).then(() => {
        if (bounds && requested.current === request) setError("");
      }, (reason) => {
        if (requested.current === request) setError(String(reason));
      });
    };

    sync();
    if (hidden) return;
    const resize = new ResizeObserver(sync);
    resize.observe(element);
    const dialogs = new MutationObserver(() => {
      const open = dialogOpen();
      if (open === hasDialog) return;
      hasDialog = open;
      sync();
    });
    dialogs.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ["aria-modal"] });
    window.addEventListener("resize", sync);
    window.addEventListener("scroll", sync, true);
    return () => {
      resize.disconnect();
      dialogs.disconnect();
      window.removeEventListener("resize", sync);
      window.removeEventListener("scroll", sync, true);
    };
  }, [hidden, projectId, revision, teamId]);

  return (
    <div className="archyl-original-view" ref={placeholder} hidden={hidden} role="region" aria-label="Archyl 원본 다이어그램">
      {error && <p className="archyl-view-message" role="alert">{error}</p>}
    </div>
  );
}

export function ArchylProjectView({ projectId, revision, hidden }: {
  projectId: string;
  revision: number;
  hidden: boolean;
}) {
  const [teamId, setTeamId] = useState("");
  const [error, setError] = useState("");

  useEffect(() => {
    if (teamId) return;
    let active = true;
    setError("");
    void getArchylProjectTeam(projectId).then((id) => {
      if (active) setTeamId(id);
    }, (reason) => {
      if (active) setError(String(reason));
    });
    return () => { active = false; };
  }, [projectId, revision, teamId]);

  if (teamId) return <NativeView projectId={projectId} teamId={teamId} revision={revision} hidden={hidden} />;
  if (hidden) return null;
  return <p className="archyl-view-message" role={error ? "alert" : "status"}>{error || "Archyl 프로젝트를 여는 중…"}</p>;
}
