import { useEffect, useState, type ReactNode } from "react";
import { dump } from "js-yaml";
import type { WorkspaceController } from "../workspace/controller.ts";
import { Confirm, Modal } from "./Modal.tsx";
import { SettingsSwitch } from "./SettingsParts.tsx";
import {
  belongsTo, eventName, eventTime, kindLabel, managerError, useManagers, webhookEvent, writeTools,
  type ManagerConfig, type ManagerDashboard, type ManagerItem, type ManagerKind, type ManagerProject,
} from "./managersStore.tsx";

const steps = ["매니저", "권한", "연결", "상태 알림"];
const writeHints: Record<string, string> = {
  create_task: "새 제보를 초안 태스크로 접수해요.",
  add_task_comment: "같은 제보는 기존 태스크의 댓글로 남겨요.",
  create_doc: "맥락을 모아 프로젝트 아젠다에 노트를 써요.",
};
const states = [
  ["ready-for-develop", "개발 준비"], ["in-review", "검토"], ["done", "완료"],
  ["failed", "실패"], ["in-progress", "진행 중"], ["waiting-answer", "답변 대기"],
];
const updateList = (list: string[], value: string) =>
  list.includes(value) ? list.filter(item => item !== value) : [...list, value];
const settings = (config: ManagerConfig) => [
  `label=${config.label.trim()}`, `kind=${config.kind}`, `tools=${config.tools.join(",")}`,
  `repos=${JSON.stringify(config.repos)}`, `agendas=${JSON.stringify(config.agendas)}`,
  `slackChannels=${JSON.stringify(config.slackChannels.map(value => value.trim()).filter(Boolean))}`,
  `webhook=${config.webhook ?? ""}`, `notify=${config.notify.join(",")}`, `notifyEnabled=${config.notifyEnabled}`,
];

type Update = (patch: Partial<ManagerConfig>) => void;

function Choices({ title, children }: { title: string; children: ReactNode }) {
  return <fieldset className="mgr-choices"><legend>{title}</legend>{children}</fieldset>;
}

function Check({ checked, onChange, children }: { checked: boolean; onChange(): void; children: ReactNode }) {
  return <label className="mgr-check"><input type="checkbox" checked={checked} onChange={onChange} />{children}</label>;
}

export function ManagerPermissions({ form, project, data, update }: {
  form: ManagerConfig; project: ManagerProject; data: ManagerDashboard; update: Update;
}) {
  return <div className="mgr-fields">
    <Choices title="쓸 수 있는 일">
      {writeTools.map(tool => <Check key={tool} checked={form.tools.includes(tool)} onChange={() => update({ tools: updateList(form.tools, tool) })}>
        <code>{tool}</code><small>{writeHints[tool]}</small>
      </Check>)}
    </Choices>
    <Choices title="읽을 수 있는 것">
      <div className="mgr-chips">
        {[...data.defaultTools.filter(tool => !writeTools.includes(tool)), ...data.optionalTools].map(tool => (
          <button key={tool} type="button" aria-pressed={form.tools.includes(tool)}
            onClick={() => update({ tools: updateList(form.tools, tool) })}>{tool}</button>
        ))}
      </div>
    </Choices>
    <div className="mgr-columns">
      <Choices title={`태스크를 만들 저장소 · ${project.title}`}>
        {project.repos.map(repo => <Check key={repo.name} checked={form.repos.includes(repo.github)} onChange={() => update({ repos: updateList(form.repos, repo.github) })}>
          {repo.name} · {repo.github}
        </Check>)}
        {!project.repos.length && <small>프로젝트 편집에서 GitHub 저장소를 추가해 주세요.</small>}
      </Choices>
      <Choices title={`노트를 쓸 아젠다 · ${project.title}`}>
        {project.agendas.map(agenda => <Check key={agenda.id} checked={form.agendas.includes(agenda.slug)} onChange={() => update({ agendas: updateList(form.agendas, agenda.slug) })}>
          {agenda.title}
        </Check>)}
        {!project.agendas.length && <small>이 프로젝트에 속한 아젠다가 없어요.</small>}
      </Choices>
    </div>
    <p className="mgr-hint">읽기도 {project.title} 안에서만 할 수 있어요.</p>
  </div>;
}

interface McpConfig {
  mcp_servers: { aisurada: { command: string; args: string[]; tools: { include: string[] } } };
}

export const configText = (config: McpConfig, kind: ManagerKind, name: string) => {
  const server = config.mcp_servers.aisurada;
  const key = `aisurada-${name}`;
  const { command, args } = server;
  return kind === "hermes" ? dump({ mcp_servers: { [key]: server } }, { lineWidth: 100 }) :
    JSON.stringify({ mcpServers: { [key]: { command, args } } }, null, 2);
};

// WebKit는 클릭을 처리하는 동안 복사를 시작해야 한다. 호스트에서 받을 값은 항목에 약속으로 넘긴다.
function copyFromHost(text: Promise<string>) {
  const content = text.then(value => new Blob([value], { type: "text/plain" }));
  return navigator.clipboard.write([new ClipboardItem({ "text/plain": content })]);
}

export function copyManagerConfig(controller: WorkspaceController, manager: ManagerConfig) {
  return copyFromHost(controller.callEngine(["manager", "config", manager.name])
    .then(value => configText(value as McpConfig, manager.kind, manager.name)));
}

export function ManagerConnection({ form, history, controller, update }: {
  form: ManagerConfig; history?: ManagerItem["history"]; controller: WorkspaceController; update: Update;
}) {
  const [format, setFormat] = useState<ManagerKind>(form.kind);
  const toolsKey = form.tools.join(",");
  const [config, setConfig] = useState<McpConfig | null>(null);
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const first = history?.firstCall;
  const last = history?.events.find(event => !webhookEvent(event) && event.result === "ok");
  const profile = form.name;

  useEffect(() => {
    let live = true;
    void controller.callEngine(["manager", "config", form.name]).then(value => {
      if (live) setConfig(value as McpConfig);
    }).catch(e => { if (live) setError(managerError(e)); });
    return () => { live = false; };
  }, [controller, form.name, toolsKey]);

  return <div className="mgr-fields">
    <div className="mgr-chips" aria-label="설정 형식">
      <button type="button" aria-pressed={format === "hermes"} onClick={() => setFormat("hermes")}>Hermes (YAML)</button>
      <button type="button" aria-pressed={format === "mcp"} onClick={() => setFormat("mcp")}>다른 MCP 에이전트 (JSON)</button>
    </div>
    {format === "hermes" ? (
      <ol className="mgr-instructions">
        <li>Hermes에서 <strong>{profile}</strong> 프로필을 만들고 아래 설정을 그 프로필의 <code>config.yaml</code>에 합치세요. 이미 연결한 프로필이 있으면 그 프로필을 사용하세요.</li>
        <li>Slack을 쓰면 기본 Hermes 프로필에 공용 Slack 봇을 설정하세요. <code>gateway.multiplex_profiles</code>를 켜고 <code>gateway.profile_routes</code>에서 Slack 채널 ID를 이 프로필에 연결하세요.</li>
        <li>텔레그램을 쓰면 이 프로필에 <code>aisurada-intake</code> Hermes 플러그인을 설치하세요.</li>
        <li>Hermes 게이트웨이를 다시 시작한 뒤 연결한 채팅에서 봇에게 태스크 목록을 물어보세요.</li>
      </ol>
    ) : <p>아래 설정을 에이전트의 MCP 설정에 합친 뒤 새 세션에서 태스크 목록을 요청하세요.</p>}
    {error && <p role="alert" className="mgr-error">{error}</p>}
    {config && <div className="mgr-code">
      <button type="button" onClick={async () => {
        try { await navigator.clipboard.writeText(configText(config, format, form.name)); setMessage("설정을 복사했어요."); }
        catch (e) { setError(managerError(e)); }
      }}>복사</button>
      <pre>{configText(config, format, form.name)}</pre>
    </div>}
    <label className="mgr-label">이 매니저가 듣는 Slack 채널
      <input value={form.slackChannels.join(", ")} placeholder="#project-qa" onChange={event =>
        update({ slackChannels: event.target.value.split(",").map(value => value.trimStart()) })} />
      <small>여러 채널은 쉼표로 나눠요. 채널 연결은 Hermes에서 설정해요.</small>
    </label>
    <div className={`mgr-feedback ${first ? "is-ok" : ""}`} role="status">
      <strong>{!form.enabled ? "매니저가 꺼져 있어요" : first ? `연결됐어요 · ${eventTime(first)}` : "첫 호출을 기다리는 중이에요"}</strong>
      <small>{!form.enabled ? "Hermes 설정을 마친 뒤 매니저 상세에서 켜고 첫 호출을 확인하세요." : first && last ? `최근 성공 · ${eventTime(last.at)} ${eventName(last.tool)}` : "연결을 기다리는 동안 다음 단계로 갈 수 있어요."}</small>
    </div>
    {message && <small role="status">{message}</small>}
  </div>;
}

const channelId = /^[CG][A-Z0-9]{1,63}$/;
export function ManagerNotifications({ form, controller, update }: {
  form: ManagerConfig; controller: WorkspaceController; update: Update;
}) {
  const [masked, setMasked] = useState("");
  const [channel, setChannel] = useState(() => form.slackChannels.find(value => channelId.test(value)) ?? "");
  const [text, setText] = useState("");
  const [error, setError] = useState("");
  const [working, setWorking] = useState(false);
  const [rotating, setRotating] = useState(false);
  const { refresh } = useManagers();
  const callSecret = (action: string) => controller.callHost<string>("manager_webhook_secret", { name: form.name, action });

  useEffect(() => {
    if (!form.notifyEnabled) return;
    let live = true;
    setError(""); setMasked("");
    void controller.callHost<string>("manager_webhook_secret", { name: form.name, action: "ensure" })
      .then(value => { if (live) setMasked(value); })
      .catch(e => { if (live) setError(managerError(e)); });
    return () => { live = false; };
  }, [controller, form.name, form.notifyEnabled]);

  const act = async (job: () => Promise<void>) => {
    setWorking(true); setError(""); setText("");
    try { await job(); } catch (e) { setError(managerError(e)); }
    finally { setWorking(false); }
  };
  return <div className="mgr-fields">
    <div className="mgr-between">
      <div><strong>태스크 상태가 바뀌면 알리기</strong><p>매니저가 만든 태스크의 상태를 제보가 온 Slack 스레드에 보내요.</p></div>
      <SettingsSwitch checked={form.notifyEnabled} ariaLabel="태스크 상태가 바뀌면 알리기" onChange={notifyEnabled => update({ notifyEnabled })} />
    </div>
    {form.notifyEnabled && <>
      <label className="mgr-label">웹훅 주소
        <input value={form.webhook ?? ""} placeholder="http://127.0.0.1:8644/webhooks/project"
          onChange={event => update({ webhook: event.target.value })} />
        <small>이 Mac의 주소만 받아요. Hermes에서 매니저마다 다른 경로를 설정해 주세요.</small>
      </label>
      <div className="mgr-label">
        서명 비밀
        <div className="mgr-actions">
          <code aria-label="가려진 서명 비밀">{masked || (error ? "확인 필요" : "읽는 중…")}</code>
          <button type="button" disabled={working || !masked} onClick={() => void act(async () => {
            await copyFromHost(callSecret("copy"));
            setText("서명 비밀을 복사했어요.");
          })}>복사</button>
          <button type="button" disabled={working || !masked} onClick={() => setRotating(true)}>다시 만들기</button>
        </div>
        <small>키체인에 저장해요. Hermes 웹훅의 서명 비밀에 같은 값을 넣어 주세요.</small>
      </div>
      <Choices title="알릴 상태">
        <div className="mgr-columns">
          {states.map(([id, label]) => <Check key={id} checked={form.notify.includes(id)} onChange={() => update({ notify: updateList(form.notify, id) })}>{label}</Check>)}
        </div>
      </Choices>
      <div className="mgr-actions mgr-test">
        <label className="mgr-label">시험 알림을 받을 Slack 채널 ID<input value={channel} placeholder="C0123456789" onChange={event => setChannel(event.target.value.trim())} /></label>
        <button type="button" disabled={working || !form.webhook || !masked || !channelId.test(channel)} onClick={() => void act(async () => {
          const answer = await controller.callHost<{status: number; elapsed: number}>("manager_test_webhook", { name: form.name, url: form.webhook?.trim(), channel });
          setText(`보냈어요 · HTTP ${answer.status} · ${answer.elapsed}ms`);
          await refresh();
        })}>시험 알림 보내기</button>
      </div>
    </>}
    {text && <small role="status">{text}</small>}
    {error && form.notifyEnabled && <p role="alert" className="mgr-error">{error}</p>}
    {rotating && <Confirm title="서명 비밀을 다시 만들까요?" label="다시 만들기"
      description="Hermes에 새 서명 비밀을 넣을 때까지 Slack 상태 알림 전송이 실패해요."
      close={() => setRotating(false)} accept={() => act(async () => {
        setMasked(await callSecret("rotate"));
        setText("Hermes 웹훅 설정에도 새 서명 비밀을 넣어 주세요.");
      })} />}
  </div>;
}

function suggestedName(kind: ManagerKind, slug: string, names: string[]) {
  const segment = slug.toLowerCase().replace(/[^a-z0-9-]/g, "-").replace(/-+/g, "-").replace(/^-|-$/g, "");
  const root = `${kind}-${segment || "project"}`.slice(0, 32);
  let name = root, suffix = 2;
  while (names.includes(name)) {
    const tail = `-${suffix++}`;
    name = root.slice(0, 32 - tail.length) + tail;
  }
  return name;
}

function webhookFor(name: string, source?: ManagerConfig) {
  const segment = encodeURIComponent(name);
  const existing = source?.webhook;
  const prefix = existing ? existing.slice(0, existing.lastIndexOf("/") + 1) : "http://127.0.0.1:8644/webhooks/";
  return prefix + segment;
}

const inputsValid = (form: ManagerConfig) => /^[a-z][a-z0-9-]{0,31}$/.test(form.name) && !!form.label.trim();
export const permissionsValid = (form: ManagerConfig) => !!form.tools.length
  && (!form.tools.some(tool => tool === "add_task_comment" || tool === "create_task") || form.repos.length > 0)
  && (!form.tools.includes("create_doc") || form.agendas.length > 0);

function ProjectChoice({ value, projects, onChange }: {
  value: string; projects: ManagerProject[]; onChange(project: string): void;
}) {
  return <label className="mgr-label">프로젝트
    <select value={value} onChange={event => onChange(event.target.value)} required>
      <option value="">프로젝트를 골라 주세요</option>
      {projects.map(project => <option key={project.slug} value={project.slug}>{project.title} · 저장소 {project.repos.length} · 아젠다 {project.agendas.length}</option>)}
    </select>
  </label>;
}

export function ManagerSetup({ controller, data, projects, initialProject, source, migrate, close, opened }: {
  controller: WorkspaceController;
  data: ManagerDashboard;
  projects: ManagerProject[];
  initialProject?: string;
  source?: ManagerConfig;
  migrate?: ManagerConfig;
  close(): void;
  opened(name: string): void;
}) {
  const [step, setStep] = useState(source ? 0 : 1);
  const [registered, setRegistered] = useState(false);
  const [customName, setCustomName] = useState(false);
  const [form, setForm] = useState<ManagerConfig>(() => {
    const template = source ?? migrate;
    const project = initialProject ?? (source ? projects.find(item => !belongsTo(source, item) && item.repos.length)?.slug : undefined) ?? "";
    const kind = template?.kind ?? "hermes";
    return {
      kind, project, enabled: migrate?.enabled ?? !source,
      label: template?.label ?? "Hermes",
      name: migrate?.name ?? suggestedName(kind, project, data.names),
      tools: template?.tools ?? data.defaultTools,
      repos: [], agendas: [], slackChannels: migrate?.slackChannels ?? [],
      notify: template?.notify ?? data.defaultNotify,
      notifyEnabled: template?.notifyEnabled ?? false,
      webhook: migrate?.webhook ?? null,
    };
  });
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const { data: live, refresh } = useManagers();
  const project = projects.find(item => item.slug === form.project);
  const update: Update = patch => setForm(value => ({ ...value, ...patch }));
  const selectProject = (slug: string) => {
    const target = projects.find(item => item.slug === slug);
    update({
      project: slug, name: customName || migrate ? form.name : suggestedName(form.kind, slug, data.names),
      repos: migrate?.repos.filter(repo => target?.repos.some(item => item.github === repo)) ?? [],
      agendas: migrate?.agendas.filter(agenda => target?.agendas.some(item => item.slug === agenda || item.title === agenda))
        .map(agenda => target!.agendas.find(item => item.slug === agenda || item.title === agenda)!.slug) ?? [],
    });
  };

  const work = async (job: () => Promise<void>) => {
    setBusy(true); setError("");
    try { await job(); } catch (e) { setError(managerError(e)); }
    finally { setBusy(false); }
  };

  const register = () => work(async () => {
    const args = registered || migrate ? ["update", form.name] : source ? ["copy", source.name, form.name] : ["add", form.name];
    const webhook = form.webhook ?? webhookFor(form.name, source);
    const newSettings = { ...form, webhook, notifyEnabled: false };
    await controller.callEngine(["manager", ...args, `project=${form.project}`, `enabled=${form.enabled}`, ...settings(newSettings)]);
    update({ webhook });
    setRegistered(true);
    await refresh();
    setStep(3);
  });
  const save = (skip = false) => work(async () => {
    const next = { ...form, webhook: form.webhook?.trim() || null };
    if (!skip && next.notifyEnabled && (!next.webhook || !next.notify.length)) {
      throw new Error("웹훅 주소와 알릴 상태를 골라 주세요.");
    }
    await controller.callEngine(["manager", "update", form.name, ...(skip ? ["notifyEnabled=false"] : settings(next))]);
    await refresh();
    opened(form.name);
  });

  return <Modal title={step === 0 ? `매니저 복사 · ${source?.label}` : `${migrate ? "매니저의 프로젝트 지정" : "매니저 추가"}${project ? ` · ${project.title}` : ""}`}
    className="mgr-modal" defaultWidth={680} close={() => { if (!busy) close(); }}>
    {step > 0 && <ol className="mgr-steps">{steps.map((label, index) => <li key={label}
      className={index + 1 < step ? "is-done" : ""}
      aria-current={index + 1 === step ? "step" : undefined}>{index + 1} {label}</li>)}</ol>}
    <div className="mgr-modal-body">
      {step === 0 && source && <>
        <p className="mgr-hint">원본 · {source.project} · {source.label} ({source.name})</p>
        <ProjectChoice value={form.project} projects={projects.filter(item => !belongsTo(source, item))} onChange={selectProject} />
        <div className="mgr-columns">
          <label className="mgr-label">새 이름<input maxLength={32} value={form.name} onChange={event => { setCustomName(true); update({ name: event.target.value }); }} /></label>
          <label className="mgr-label">보이는 이름<input maxLength={40} value={form.label} onChange={event => update({ label: event.target.value })} /></label>
        </div>
        <dl className="mgr-meta">
          <dt>가져오는 설정</dt><dd>{kindLabel(source.kind)} · 도구 {source.tools.length}개 · 알릴 상태 {source.notify.length}개 · 상태 알림 {source.notifyEnabled ? "켜짐" : "꺼짐"}</dd>
          <dt>새로 고를 설정</dt><dd>저장소, 아젠다, Slack 채널, 서명 비밀</dd>
          <dt>웹훅 주소</dt><dd>{project && inputsValid(form) ? webhookFor(form.name, source) : "프로젝트와 이름을 골라 주세요"}</dd>
        </dl>
        <p className="mgr-hint">복사본은 꺼져 있어요. 연결을 마친 뒤 켜 주세요.</p>
      </>}
      {step === 1 && <>
        <ProjectChoice value={form.project} projects={projects} onChange={selectProject} />
        <div className="mgr-choices">
          <strong>어떤 매니저인가요?</strong>
          <div className="mgr-kinds mgr-columns">
            {(["hermes", "mcp"] as const).map(kind => <button type="button" key={kind} aria-pressed={form.kind === kind} onClick={() =>
              update({ kind, label: kindLabel(kind), name: customName || migrate ? form.name : suggestedName(kind, form.project, data.names) })}>
              <strong>{kindLabel(kind)}</strong><small>{kind === "hermes" ? "프로젝트별 Hermes 프로필과 연결해요." : "MCP 서버를 연결할 수 있는 다른 에이전트예요."}</small>
            </button>)}
          </div>
        </div>
        <div className="mgr-columns">
          <label className="mgr-label">이름<input maxLength={32} readOnly={!!migrate} value={form.name} onChange={event => { setCustomName(true); update({ name: event.target.value }); }} />
            <small>태스크와 기록에 남아요. 영문 소문자로 시작하고 숫자와 하이픈을 쓸 수 있어요.</small>
          </label>
          <label className="mgr-label">보이는 이름<input maxLength={40} value={form.label} onChange={event => update({ label: event.target.value })} />
            <small>카드와 알림에서 이 이름으로 보여요.</small>
          </label>
        </div>
        <p className="mgr-hint">매니저는 고른 프로젝트 하나에 연결돼요.</p>
      </>}
      {step === 2 && project && <ManagerPermissions form={form} project={project} data={data} update={update} />}
      {step === 3 && <ManagerConnection form={form} history={live?.items.find(item => item.manager.name === form.name)?.history} controller={controller} update={update} />}
      {step === 4 && <ManagerNotifications form={form} controller={controller} update={update} />}
      {error && <p role="alert" className="mgr-error">{error}</p>}
    </div>
    <footer className="mgr-actions">
      {!registered && step === 2 && <button type="button" onClick={() => setStep(source ? 0 : 1)}>이전</button>}
      {step > 2 && <button type="button" disabled={busy} onClick={() => setStep(step - 1)}>이전</button>}
      <span className="mgr-space" />
      <button type="button" disabled={busy} onClick={close}>{registered ? "닫기" : "취소"}</button>
      {step === 0 && <button type="button" className="primary" disabled={!project || !inputsValid(form)} onClick={() => setStep(2)}>복사하고 이어서 설정</button>}
      {step === 1 && <button type="button" className="primary" disabled={!project || !inputsValid(form)} onClick={() => setStep(2)}>다음</button>}
      {step === 2 && <button type="button" className="primary" disabled={busy || !permissionsValid(form)} onClick={() => void register()}>{registered ? "권한 저장하고 계속" : "등록하고 연결 설정"}</button>}
      {step === 3 && <button type="button" className="primary" disabled={busy} onClick={() => void work(async () => {
        await controller.callEngine(["manager", "update", form.name, `slackChannels=${JSON.stringify(form.slackChannels.map(value => value.trim()).filter(Boolean))}`]);
        setStep(4);
      })}>다음</button>}
      {step === 4 && <>
        <button type="button" disabled={busy} onClick={() => void save(true)}>상태 알림 건너뛰기</button>
        <button type="button" className="primary" disabled={busy} onClick={() => void save()}>저장</button>
      </>}
    </footer>
  </Modal>;
}

export function ManagerEditor({ controller, item, project, data, tab }: {
  controller: WorkspaceController; item: ManagerItem; project: ManagerProject; data: ManagerDashboard;
  tab: "permissions" | "connection" | "notifications";
}) {
  const [form, setForm] = useState(() => ({
    ...item.manager,
    repos: item.manager.repos.filter(repo => project.repos.some(entry => entry.github === repo)),
    agendas: item.manager.agendas.filter(agenda => project.agendas.some(entry => entry.slug === agenda)),
  }));
  const [message, setMessage] = useState("");
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const { refresh } = useManagers();
  const update: Update = patch => { setMessage(""); setForm(value => ({ ...value, ...patch })); };
  const save = async () => {
    setBusy(true); setMessage(""); setError("");
    const patch = tab === "permissions" ? [`tools=${form.tools.join(",")}`, `repos=${JSON.stringify(form.repos)}`, `agendas=${JSON.stringify(form.agendas)}`] :
      tab === "connection" ? [`slackChannels=${JSON.stringify(form.slackChannels.map(value => value.trim()).filter(Boolean))}`] :
        [`webhook=${form.webhook?.trim() || ""}`, `notify=${form.notify.join(",")}`, `notifyEnabled=${form.notifyEnabled}`];
    try {
      if (tab === "notifications" && form.notifyEnabled && (!form.webhook?.trim() || !form.notify.length)) {
        throw new Error("웹훅 주소와 알릴 상태를 골라 주세요.");
      }
      await controller.callEngine(["manager", "update", form.name, ...patch]);
      await refresh();
      setMessage("저장했어요.");
    } catch (e) { setError(managerError(e)); }
    finally { setBusy(false); }
  };
  return <section className="mgr-panel">
    {tab === "permissions" && <ManagerPermissions form={form} project={project} data={data} update={update} />}
    {tab === "connection" && <ManagerConnection form={form} history={item.history} controller={controller} update={update} />}
    {tab === "notifications" && <ManagerNotifications form={form} controller={controller} update={update} />}
    {error && <p role="alert" className="mgr-error">{error}</p>}
    <div className="mgr-actions mgr-editor-footer">
      <button type="button" className="primary" disabled={busy || (tab === "permissions" && !permissionsValid(form))} onClick={() => void save()}>저장</button>
      {message && <small role="status">{message}</small>}
    </div>
  </section>;
}
