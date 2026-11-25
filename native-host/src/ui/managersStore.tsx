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
  work?: ManagerWork | null;
  workIssue?: string | null;
  history: {
    firstCall: number | null;
    events: ManagerEvent[];
    revision?: string;
    latestCall?: ManagerEvent | null;
    latestSuccess?: ManagerEvent | null;
    latestWebhook?: ManagerEvent | null;
    today?: DailyCalls;
  };
}
interface DailyCalls {
  calls: number;
  notes: number;
  rejected: number;
  sent: number;
}

interface ManagerWork { tasks: number; comments: number }
export interface ManagerDashboard {
  items: ManagerItem[];
  names: string[];
  registryIssue?: string | null;
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

export function ManagersProvider({ controller, workspace, showWork, children }: {
  controller: WorkspaceController;
  workspace: Workspace;
  showWork: boolean;
  children: ReactNode;
}) {
  const [data, setData] = useState<ManagerDashboard | null>(null);
  const [error, setError] = useState("");
  const request = useRef(0);
  const readingWork = useRef<number | undefined>(undefined);
  const lastWorkRead = useRef(0);
  const notified = useRef(new Set<string>());
  const sample = workspace.sample;
  const refresh = useCallback(async (includeWork = showWork) => {
    if (sample) return;
    const current = ++request.current;
    if (includeWork) {
      lastWorkRead.current = Date.now();
      readingWork.current = current;
    }
    try {
      const args = ["manager", "dashboard", "--compact", ...(includeWork ? [] : ["--summary"])];
      const value = await controller.callEngine(args) as ManagerDashboard;
      if (!Array.isArray(value?.items)) throw new Error("매니저 등록부 응답을 읽지 못했어요.");
      if (current === request.current) {
        setData(previous => {
          if (includeWork || !showWork || !previous) return value;
          const prior = new Map(previous.items.map(item => [item.manager.name, item]));
          return { ...value, items: value.items.map(item => {
            const saved = prior.get(item.manager.name);
            if (!saved || item.issue || saved.manager.projectId !== item.manager.projectId) return item;
            return { ...item, work: saved.work, workIssue: saved.workIssue };
          }) };
        });
        setError("");
      }
    } catch (e) {
      if (current === request.current) {
        setError(managerError(e));
      }
    } finally {
      if (readingWork.current === current) readingWork.current = undefined;
    }
  }, [controller, sample, showWork]);
  useEffect(() => {
    if (sample) return;
    void refresh();
    const interval = window.setInterval(() => {
      if (!document.hidden && readingWork.current === undefined) {
        void refresh(showWork && Date.now() - lastWorkRead.current >= 15_000);
      }
    }, 5000);
    const onFocus = () => void refresh();
    window.addEventListener("focus", onFocus);
    return () => {
      request.current++;
      readingWork.current = undefined;
      window.clearInterval(interval);
      window.removeEventListener("focus", onFocus);
    };
  }, [refresh, sample, showWork]);

  useEffect(() => {
    if (!controller.isNative() || !data) return;
    const earliest = Date.now() - 24 * 60 * 60 * 1000;
    // 한 번에 하나씩 넣어 알림 목록의 쓰기 순서를 지킨다.
    let live = true;
    const publish = async () => {
      for (const item of data.items) {
        if (item.issue || !item.intake) continue;
        const questions = item.manager.enabled ? item.intake.asked.filter(request => request.at >= earliest) : [];
        const attention = item.intake.attention ?? [];
        const entries = [
          ...questions.map(request => ({
            id: "intake-question-" + request.revision, kind: "intake-question",
            title: item.manager.label + "가 제보에 물어볼 내용을 남겼어요",
            body: intakeQuestionBody(request),
            intake: { manager: item.manager.name, entry: request.id, url: request.url },
          })),
          ...attention.map(request => ({
            id: "intake-review-" + item.manager.name + "-" + request.revision, kind: "intake-review",
            title: request.reason === "no_answer" ? "3일 동안 답이 없는 제보가 있어요" : "두 번 질문한 제보를 검토해 주세요",
            body: request.excerpt || item.manager.label,
            intake: { manager: item.manager.name, entry: request.id, url: request.id.startsWith("intake-") ? "" : request.url },
          })),
        ];
        for (const entry of entries) {
          if (!live || notified.current.has(entry.id)) continue;
          notified.current.add(entry.id);
          try {
            await controller.callHost("notifications_push", { expectedVault: workspace.path, item: entry });
          } catch {
            notified.current.delete(entry.id);
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
  !!item.manager.webhook && latestWebhook(item.history)?.result === "failed";
export const enabledManagers = (data: ManagerDashboard | null) =>
  data?.items.filter(({ manager, issue }) => manager.enabled && !!manager.project && !issue) ?? [];

type ManagerHistory = ManagerItem["history"];
export const latestCall = (history: ManagerHistory) =>
  history.latestCall ?? history.events.find(event => !webhookEvent(event));
export const latestSuccess = (history?: ManagerHistory) =>
  history?.latestSuccess ?? history?.events.find(event => !webhookEvent(event) && event.result === "ok");
export const latestWebhook = (history: ManagerHistory) =>
  history.latestWebhook ?? history.events.find(webhookEvent);

export function dailyCalls({ today, events }: ManagerHistory): DailyCalls {
  if (today) return today;
  const relevant = todayEvents(events);
  const ok = (tool: string) => relevant.filter(event => event.result === "ok" && event.tool === tool).length;
  return {
    calls: relevant.filter(event => !webhookEvent(event)).length,
    notes: ok("create_doc"),
    rejected: relevant.filter(event => event.result === "rejected").length,
    sent: ok("status_webhook"),
  };
}

const todayEvents = (events: ManagerEvent[]) => {
  const start = new Date().setHours(0, 0, 0, 0);
  return events.filter(event => event.at >= start);
};
export const workCount = (item: ManagerItem, kind: keyof ManagerWork) => item.work?.[kind] ?? "—";
export function totalCreatedTasks(items: ManagerItem[]) {
  const scoped = items.filter(item => !item.issue);
  return scoped.every(item => item.work) ? scoped.reduce((sum, item) => sum + (item.work?.tasks ?? 0), 0) : "—";
}
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
