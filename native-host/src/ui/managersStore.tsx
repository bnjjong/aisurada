import { createContext, useCallback, useContext, useEffect, useRef, useState, type ReactNode } from "react";
import { SidecarCommandError } from "../workspace/engine.ts";
import type { WorkspaceController } from "../workspace/controller.ts";
import { asText, type Workspace } from "../workspace/types.ts";
import type { IntakeData } from "./intakeModel.ts";
import { parseRepos } from "./repoLinks.ts";

export type ManagerKind = "hermes" | "mcp";
export interface ManagerConfig {
  name: string;
  label: string;
  kind: ManagerKind;
  project: string;
  projectId?: string;
  enabled: boolean;
  tools: string[];
  repos: string[];
  agendas: string[];
  webhook: string | null;
  slackChannels: string[];
  notify: string[];
  notifyEnabled: boolean;
}
export interface ManagerEvent {
  at: number;
  tool: string;
  result: "ok" | "rejected" | "failed";
  target: string | null;
  reason?: string;
}
export interface ManagerItem {
  manager: ManagerConfig;
  issue: string | null;
  historyIssue?: string | null;
  intake?: IntakeData | null;
  intakeIssue?: string | null;
  history: { firstCall: number | null; events: ManagerEvent[] };
}
export interface ManagerDashboard {
  items: ManagerItem[];
  names: string[];
  defaultTools: string[];
  optionalTools: string[];
  defaultNotify: string[];
}
interface ManagersState {
  data: ManagerDashboard | null;
  error: string;
  refresh(): Promise<void>;
}
const empty = { data: null, error: "", refresh: async () => {} };
const ManagerContext = createContext<ManagersState>(empty);
export const useManagers = () => useContext(ManagerContext);

export const managerError = (error: unknown) =>
  error instanceof SidecarCommandError ? error.stderr : error instanceof Error ? error.message : String(error);

function intakeQuestionBody({ excerpt, question }: IntakeData["asked"][number]) {
  const original = excerpt?.replace(/\s+/g, " ").trim();
  const draft = question?.replace(/\s+/g, " ").trim();
  return [original && `「${original}」`, draft, "원장에서 답글 초안을 확인해 주세요."].filter(Boolean).join(" · ");
}

export function ManagersProvider({ controller, workspace, children }: {
  controller: WorkspaceController;
  workspace: Workspace;
  children: ReactNode;
}) {
  const [data, setData] = useState<ManagerDashboard | null>(null);
  const [error, setError] = useState("");
  const request = useRef(0);
  const notified = useRef(new Set<string>());
  const sample = workspace.sample;
  const refresh = useCallback(async () => {
    if (sample) return;
    const current = ++request.current;
    try {
      const value = await controller.callEngine(["manager", "dashboard"]) as ManagerDashboard;
      if (!Array.isArray(value?.items)) throw new Error("매니저 등록부 응답을 읽지 못했어요.");
      if (current === request.current) {
        setData(value);
        setError("");
      }
    } catch (e) {
      if (current === request.current) {
        setError(managerError(e));
      }
    }
  }, [controller, sample]);
  useEffect(() => {
    if (sample) return;
    void refresh();
    const interval = window.setInterval(() => { if (!document.hidden) void refresh(); }, 5000);
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => {
      request.current++;
      window.clearInterval(interval);
      window.removeEventListener("focus", onFocus);
    };
  }, [refresh, sample]);

  useEffect(() => {
    if (!controller.isNative() || !data) return;
    const earliest = Date.now() - 24 * 60 * 60 * 1000;
    // 한 번에 하나씩 넣어 알림 목록의 쓰기 순서를 지킨다.
    let live = true;
    const publish = async () => {
      for (const item of data.items) {
        if (!item.manager.enabled || item.issue) continue;
        for (const request of item.intake?.asked ?? []) {
          if (!live || request.at < earliest) continue;
          const id = "intake-question-" + request.revision;
          if (notified.current.has(id)) continue;
          notified.current.add(id);
          try {
            await controller.callHost("notifications_push", { expectedVault: workspace.path, item: {
              id, kind: "intake-question", title: `${item.manager.label}가 제보에 물어볼 내용을 남겼어요`,
              body: intakeQuestionBody(request),
              intake: { manager: item.manager.name, entry: request.id, url: request.url },
            } });
          } catch {
            notified.current.delete(id);
          }
        }
      }
    };
    void publish();
    return () => { live = false; };
  }, [controller, data, workspace.path]);

  return <ManagerContext.Provider value={{ data, error, refresh }}>{children}</ManagerContext.Provider>;
}

export const writeTools = ["create_task", "add_task_comment", "create_doc"];
export const kindLabel = (kind: ManagerKind) => kind === "hermes" ? "Hermes" : "다른 MCP 에이전트";
export const webhookEvent = (event: ManagerEvent) => event.tool === "status_webhook" || event.tool === "test_webhook";
export const deliveryIssue = (item: ManagerItem) => item.manager.enabled && item.manager.notifyEnabled &&
  !!item.manager.webhook && item.history.events.find(webhookEvent)?.result === "failed";
export const enabledManagers = (data: ManagerDashboard | null) =>
  data?.items.filter(({ manager, issue }) => manager.enabled && !!manager.project && !issue) ?? [];

export const todayEvents = (events: ManagerEvent[]) => {
  const start = new Date().setHours(0, 0, 0, 0);
  return events.filter(event => event.at >= start);
};
export const eventTime = (time?: number | null) => time
  ? new Date(time).toLocaleTimeString("ko-KR", { hour: "2-digit", minute: "2-digit", hour12: false })
  : "—";
export const eventName = (tool: string) =>
  tool === "status_webhook" ? "상태 알림" : tool === "test_webhook" ? "시험 알림" : tool === "unknown" ? "알 수 없는 도구" : tool;
export const eventResult = (result: ManagerEvent["result"]) =>
  result === "ok" ? "성공" : result === "rejected" ? "거절" : "실패";

export function managerProjects(workspace: Workspace) {
  return workspace.projects.map(project => ({
    id: asText(project.id),
    slug: asText(project.slug),
    title: asText(project.title) || asText(project.slug),
    type: project.type,
    repos: parseRepos(project.repos).flatMap(repo => repo.github ? [{ name: repo.name, github: repo.github }] : []),
    agendas: workspace.agendas.filter(agenda => agenda.project === project.slug),
  })).filter(project => project.slug);
}

export type ManagerProject = ReturnType<typeof managerProjects>[number];
export const belongsTo = (manager: ManagerConfig, project: ManagerProject) =>
  manager.project === project.slug && (!manager.projectId || manager.projectId === project.id);
