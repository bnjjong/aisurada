import { useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { Workspace } from "../workspace/types.ts";
import { managerAddress, managersAddress, projectAddress } from "../navigation/routes.ts";
import { Confirm } from "./Modal.tsx";
import { Page } from "./OtherPages.tsx";
import { PROJECT_TYPE_NAMES, projectTypeOf } from "./model.ts";
import { SettingsSwitch } from "./SettingsParts.tsx";
import { copyManagerConfig, ManagerEditor, ManagerSetup } from "./ManagerSetup.tsx";
import {
  deliveryIssue, enabledManagers, eventName, eventResult, eventTime, kindLabel, managerError, managerProjects,
  todayEvents, useManagers, webhookEvent, type ManagerConfig, type ManagerItem, type ManagerProject,
} from "./managersStore.tsx";

type Setup = { project?: string; source?: ManagerConfig; migrate?: ManagerConfig };
const plural = (values: string[]) => !values.length ? "—" :
  values.length === 1 ? values[0] : `${values[0]} 외 ${values.length - 1}개`;
const metric = (item: ManagerItem, tool: string) =>
  todayEvents(item.history.events).filter(event => event.result === "ok" && event.tool === tool).length;

function ManagerMark({ manager }: { manager: ManagerConfig }) {
  return <span className="mgr-mark" aria-hidden="true">{manager.kind === "hermes" ? "H" : "M"}</span>;
}

export function ManagerStatePill({ item }: { item: ManagerItem }) {
  const on = item.manager.enabled && !!item.manager.project && !item.issue;
  return <span className={`mgr-badge ${on ? "is-ok" : ""}`}>{on ? "켜짐" : "꺼짐"}</span>;
}

function ManagerCard({ item, busy, canCopy, action, open, copy }: {
  item: ManagerItem; busy: boolean; canCopy: boolean;
  action(name: "config" | "enable" | "disable", manager: ManagerConfig): void;
  open(): void; copy(): void;
}) {
  const { manager } = item;
  const last = item.history.events.find(event => !webhookEvent(event));
  return <article className="mgr-panel mgr-card">
    <header className="mgr-between">
      <div className="mgr-identity">
        <ManagerMark manager={manager} />
        <div><h3>{manager.label} <ManagerStatePill item={item} /></h3><small>{manager.name}</small></div>
      </div>
      <SettingsSwitch checked={manager.enabled && !item.issue} disabled={busy || !!item.issue}
        ariaLabel={`${manager.name} 켜기`} onChange={on => action(on ? "enable" : "disable", manager)} />
    </header>
    {item.issue && <p className="mgr-error" role="status">{item.issue}</p>}
    <div className="mgr-columns">
      <dl className="mgr-meta">
        <dt>Slack 채널</dt><dd>{plural(manager.slackChannels)}</dd>
        <dt>저장소</dt><dd>{plural(manager.repos)}</dd>
        <dt>아젠다</dt><dd>{plural(manager.agendas)}</dd>
        <dt>도구</dt><dd>{manager.tools.length}개</dd>
      </dl>
      <dl className="mgr-meta">
        <dt>상태 알림</dt><dd>{manager.notifyEnabled && manager.webhook ? manager.webhook : "꺼짐"}</dd>
        <dt>마지막 호출</dt><dd>{last ? `${eventTime(last.at)} · ${eventName(last.tool)} ${eventResult(last.result)}` : "—"}</dd>
        <dt>오늘</dt><dd>태스크 {metric(item, "create_task")} · 댓글 {metric(item, "add_task_comment")} · 거절 {todayEvents(item.history.events).filter(event => event.result === "rejected").length}</dd>
      </dl>
    </div>
    {deliveryIssue(item) && <p className="mgr-error" role="status">최근 상태 알림을 보내지 못했어요.</p>}
    <footer className="mgr-actions">
      <button type="button" onClick={open}>열기</button>
      <button type="button" disabled={busy || !!item.issue} onClick={() => action("config", manager)}>MCP 설정 복사</button>
      <button type="button" disabled={!canCopy} onClick={copy}>다른 프로젝트로 복사</button>
    </footer>
  </article>;
}

export function ManagersPage({ project, name, workspace, controller }: {
  project?: string; name?: string; workspace: Workspace; controller: WorkspaceController;
}) {
  const { data, error, refresh } = useManagers();
  const [setup, setSetup] = useState<Setup | null>(null);
  const [working, setWorking] = useState("");
  const [feedback, setFeedback] = useState("");
  const [failure, setFailure] = useState("");
  const projects = managerProjects(workspace);
  const items = data?.items ?? [];
  const active = enabledManagers(data);
  const today = todayEvents(items.flatMap(item => item.history.events));
  const calls = today.filter(event => !webhookEvent(event));
  const shown = project ? projects.filter(item => item.slug === project) : projects;
  const selected = name ? items.find(item => item.manager.name === name) : undefined;

  const action = async (kind: "config" | "enable" | "disable" | "remove", manager: ManagerConfig) => {
    setWorking(manager.name); setFeedback(""); setFailure("");
    try {
      if (kind === "config") {
        await copyManagerConfig(controller, manager);
        setFeedback("MCP 설정을 복사했어요.");
      } else {
        await controller.callEngine(["manager", kind, manager.name]);
        await refresh();
        if (kind === "remove") controller.navigate(managersAddress(manager.project));
      }
    } catch (e) { setFailure(managerError(e)); }
    finally { setWorking(""); }
  };
  const open = (name: string) => controller.navigate(managerAddress(name));

  return <>
    <Page title={selected?.manager.label ?? "매니저"} className="mgr-page">
      {error && <p role="alert" className="mgr-error">{error}</p>}
      {failure && <p role="alert" className="mgr-error">{failure}</p>}
      {feedback && <p role="status" className="mgr-hint">{feedback}</p>}
      {workspace.sample && <p className="mgr-hint">앱에서 작업 폴더를 열면 매니저를 설정할 수 있어요.</p>}
      {!data && !error && !workspace.sample && <p role="status">매니저를 불러오는 중이에요.</p>}
      {!data && error && <button type="button" onClick={() => void refresh()}>다시 읽기</button>}
      {data && name && !selected && <p>매니저를 찾을 수 없어요. <button onClick={() => controller.navigate(managersAddress())}>매니저 목록</button></p>}
      {data && selected && <ManagerDetail item={selected} projects={projects} controller={controller}
        busy={working === selected.manager.name} action={action} setup={setSetup} />}
      {data && !name && <>
        <p className="mgr-intro">매니저는 Slack 등에서 받은 제보를 태스크로 접수해요. 매니저 하나는 프로젝트 하나에 연결돼요.</p>
        <div className="mgr-between mgr-overview">
          <div className="mgr-actions">
            <span className="mgr-badge is-ok">켜짐 {active.length}</span>
            <small>오늘 호출 {calls.length} · 만든 태스크 {calls.filter(event => event.tool === "create_task" && event.result === "ok").length} · 거절 {calls.filter(event => event.result === "rejected").length}</small>
          </div>
          <div className="mgr-actions">
            <button type="button" onClick={() => void refresh()}>새로고침</button>
            <button type="button" className="primary" disabled={!!error || !projects.length} onClick={() => setSetup({ project })}>＋ 매니저 추가</button>
          </div>
        </div>
        <nav className="mgr-chips" aria-label="프로젝트로 매니저 거르기">
          <button type="button" aria-pressed={!project} onClick={() => controller.navigate(managersAddress())}>모든 프로젝트</button>
          {projects.map(item => <button type="button" key={item.slug} aria-pressed={item.slug === project} onClick={() => controller.navigate(managersAddress(item.slug))}>{item.title}</button>)}
        </nav>
        {!project && items.some(item => item.issue && !projects.some(p => p.slug === item.manager.project)) && (
          <section className="mgr-project" aria-label="확인이 필요한 매니저">
            <h2>확인이 필요한 매니저</h2>
            {items.filter(item => item.issue && !projects.some(p => p.slug === item.manager.project)).map(item => (
              <div className="mgr-between mgr-panel" key={item.manager.name}>
                <div><strong>{item.manager.label}</strong><p>{item.manager.name} · {item.issue}</p></div>
                <button type="button" onClick={() => item.manager.project ? open(item.manager.name) : setSetup({ migrate: item.manager })}>
                  {item.manager.project ? "열기" : "프로젝트 지정"}
                </button>
              </div>
            ))}
          </section>
        )}
        {!shown.length && <p className="mgr-hint">{project ? "이 프로젝트를 찾을 수 없어요." : "프로젝트를 만들면 매니저를 추가할 수 있어요."}</p>}
        {shown.map(item => {
          const group = items.filter(entry => entry.manager.project === item.slug);
          const canAdd = item.repos.length > 0;
          return <section className="mgr-project" key={item.slug} aria-label={item.title}>
            <header className="mgr-between mgr-project-head">
              <div className="mgr-actions"><h2>{item.title}</h2>
                <span className="mgr-badge">{PROJECT_TYPE_NAMES[projectTypeOf(item.type)]}</span>
                <small>아젠다 {item.agendas.length} · 저장소 {item.repos.length}</small>
              </div>
              {!!group.length && canAdd && <button type="button" onClick={() => setSetup({ project: item.slug })}>＋ 이 프로젝트에 추가</button>}
            </header>
            {!canAdd && <div className="mgr-between mgr-warning">
              <span>저장소가 없어서 매니저가 태스크를 만들 수 없어요.</span>
              <button type="button" onClick={() => controller.navigate(projectAddress(item.slug))}>프로젝트 열기</button>
            </div>}
            {!group.length && canAdd && <div className="mgr-panel mgr-between">
              <span>이 프로젝트에 매니저가 없어요.</span>
              <button type="button" className="primary" onClick={() => setSetup({ project: item.slug })}>＋ 이 프로젝트에 매니저 추가</button>
            </div>}
            {group.map(entry => <ManagerCard key={entry.manager.name} item={entry} busy={working === entry.manager.name}
              canCopy={projects.some(p => p.slug !== item.slug)} action={(kind, manager) => void action(kind, manager)}
              open={() => open(entry.manager.name)} copy={() => setSetup({ source: entry.manager })} />)}
          </section>;
        })}
      </>}
    </Page>
    {setup && data && <ManagerSetup controller={controller} data={data} projects={projects}
      initialProject={setup.project} source={setup.source} migrate={setup.migrate}
      close={() => setSetup(null)} opened={name => { setSetup(null); open(name); }} />}
  </>;
}

const tabs = [
  ["history", "기록"], ["permissions", "권한"], ["connection", "연결"], ["notifications", "상태 알림"],
] as const;

function ManagerDetail({ item, projects, controller, busy, action, setup }: {
  item: ManagerItem; projects: ManagerProject[]; controller: WorkspaceController; busy: boolean;
  action(kind: "config" | "enable" | "disable" | "remove", manager: ManagerConfig): Promise<void>;
  setup(value: Setup): void;
}) {
  const [tab, setTab] = useState<(typeof tabs)[number][0]>("history");
  const [deleting, setDeleting] = useState(false);
  const { data } = useManagers();
  const { manager, history } = item;
  const project = projects.find(p => p.slug === manager.project);
  const last = history.events.find(event => !webhookEvent(event));
  const webhook = history.events.find(webhookEvent);
  return <>
    <button type="button" className="mgr-back" onClick={() => controller.navigate(managersAddress(manager.project))}>
      ← 매니저{project && ` / ${project.title}`} / {manager.label}
    </button>
    <div className="mgr-between mgr-detail-actions">
      <div className="mgr-actions">
        <ManagerMark manager={manager} />
        <span className="mgr-badge">{project?.title ?? manager.project ?? "프로젝트 미지정"}</span>
        <ManagerStatePill item={item} />
        {!manager.project ? <button type="button" onClick={() => setup({ migrate: manager })}>프로젝트 지정</button> :
          <button type="button" disabled={busy || (!manager.enabled && !!item.issue)} onClick={() => void action(manager.enabled ? "disable" : "enable", manager)}>{manager.enabled ? "끄기" : "켜기"}</button>}
        <button type="button" disabled={busy || !project || !!item.issue} onClick={() => void action("config", manager)}>MCP 설정 복사</button>
        <button type="button" disabled={!project || !projects.some(p => p.slug !== manager.project)} onClick={() => setup({ source: manager })}>다른 프로젝트로 복사</button>
      </div>
      <button type="button" className="mgr-danger" disabled={busy} onClick={() => setDeleting(true)}>지우기</button>
    </div>
    {item.issue && <p className="mgr-error" role="status">{item.issue}</p>}
    <nav className="mgr-tabs" aria-label="매니저 상세">
      {tabs.map(([key, label]) => <button type="button" key={key} aria-pressed={tab === key} disabled={key !== "history" && !project} onClick={() => setTab(key)}>{label}</button>)}
    </nav>
    <div className="mgr-detail">
      <div>
        {tab === "history" ? <ManagerHistory item={item} /> : project && data && <ManagerEditor key={tab}
          item={item} project={project} data={data} controller={controller} tab={tab} />}
      </div>
      <aside>
        <section className="mgr-panel"><h2>세부 정보</h2>
          <dl className="mgr-meta">
            <dt>프로젝트</dt><dd>{project?.title || manager.project || "—"}</dd>
            <dt>Slack 채널</dt><dd>{plural(manager.slackChannels)}</dd>
            <dt>이름</dt><dd><code>{manager.name}</code></dd>
            <dt>종류</dt><dd>{kindLabel(manager.kind)}</dd>
            <dt>도구</dt><dd>{manager.tools.length}개</dd>
            <dt>저장소</dt><dd>{plural(manager.repos)}</dd>
            <dt>아젠다</dt><dd>{plural(manager.agendas)}</dd>
            <dt>상태 알림</dt><dd>{manager.notifyEnabled && manager.webhook ? `켜짐 · ${manager.notify.length}개 상태` : "꺼짐"}</dd>
          </dl>
        </section>
        <section className="mgr-panel"><h2>연결 상태</h2>
          <dl className="mgr-meta">
            <dt>첫 호출</dt><dd>{history.firstCall ? new Date(history.firstCall).toLocaleString("ko-KR") : "대기 중"}</dd>
            <dt>마지막 호출</dt><dd>{eventTime(last?.at)}</dd>
            <dt>웹훅</dt><dd>{webhook ? `${eventResult(webhook.result)} · ${eventTime(webhook.at)}` : "—"}</dd>
          </dl>
        </section>
      </aside>
    </div>
    {deleting && <Confirm title="매니저를 지울까요?" close={() => setDeleting(false)}
      description={`${manager.label}의 연결 설정과 기기에 남긴 호출 기록을 지워요.`}
      accept={() => action("remove", manager)} />}
  </>;
}

function ManagerHistory({ item }: { item: ManagerItem }) {
  const today = todayEvents(item.history.events);
  const counts = [
    ["오늘 호출", today.filter(event => !webhookEvent(event)).length],
    ["만든 태스크", metric(item, "create_task")], ["댓글", metric(item, "add_task_comment")],
    ["노트", metric(item, "create_doc")], ["거절", today.filter(event => event.result === "rejected").length],
    ["보낸 알림", today.filter(event => event.tool === "status_webhook" && event.result === "ok").length],
  ];
  return <>
    <dl className="mgr-counts">{counts.map(([label, count]) => <div key={label}><dt>{label}</dt><dd>{count}</dd></div>)}</dl>
    <section className="mgr-panel">
      <h2>최근 호출</h2>
      {item.history.events.length ? <div className="mgr-table-wrap"><table className="mgr-table">
        <thead><tr><th>시각</th><th>도구</th><th>결과</th><th>대상</th></tr></thead>
        <tbody>{item.history.events.slice(0, 50).map((event, index) => <tr key={`${event.at}-${index}`}>
          <td title={new Date(event.at).toLocaleString("ko-KR")}>{eventTime(event.at)}</td>
          <td>{eventName(event.tool)}</td>
          <td><span className={`mgr-badge ${event.result === "ok" ? "is-ok" : "is-error"}`}>{eventResult(event.result)}</span></td>
          <td>{event.target ?? "—"}</td>
        </tr>)}</tbody>
      </table></div> : <p className="mgr-intro">아직 호출 기록이 없어요.</p>}
      <p className="mgr-intro">시각, 도구, 결과, 대상 식별자를 기록해요.</p>
    </section>
  </>;
}

export function ProjectManagers({ project, workspace, controller }: {
  project: string; workspace: Workspace; controller: WorkspaceController;
}) {
  const [adding, setAdding] = useState(false);
  const { data, error } = useManagers();
  const projects = managerProjects(workspace);
  const target = projects.find(item => item.slug === project);
  const items = data?.items.filter(item => item.manager.project === project) ?? [];
  return <section className="res-section mgr-project-section" aria-label="매니저">
    <header className="res-section-head"><h2>매니저</h2>
      <button type="button" disabled={!data || !target?.repos.length} onClick={() => setAdding(true)}>＋ 매니저 추가</button>
    </header>
    {error && <p role="alert" className="mgr-error">{error}</p>}
    {items.map(item => <div className="mgr-between mgr-project-row" key={item.manager.name}>
      <div className="mgr-actions"><ManagerMark manager={item.manager} /><strong>{item.manager.label}</strong><small>{item.manager.name}</small>
        <ManagerStatePill item={item} /><small>마지막 호출 {eventTime(item.history.events.find(event => !webhookEvent(event))?.at)} · 오늘 태스크 {metric(item, "create_task")}</small></div>
      <button type="button" onClick={() => controller.navigate(managerAddress(item.manager.name))}>열기</button>
    </div>)}
    {!items.length && <p className="res-empty">{target?.repos.length ? "이 프로젝트에 매니저가 없어요." : "프로젝트 편집에서 GitHub 저장소를 추가하면 매니저가 태스크를 만들 수 있어요."}</p>}
    {adding && data && <ManagerSetup controller={controller} data={data} projects={projects} initialProject={project}
      close={() => setAdding(false)} opened={name => { setAdding(false); controller.navigate(managerAddress(name)); }} />}
  </section>;
}
