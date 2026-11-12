// 스킬·에이전트 자원(엔진 명령 8개)을 화면에서 부르는 얇은 층. 엔진 응답의 모양을 정리하고,
// 화면이 쓰는 순수한 계산(폼 값 → 저장 인자, 오류 풀이, 참조 확인 목록)을 모아 시험으로 잰다.
// 규칙은 엔진이 지킨다(참조하지 않은 것은 쓸 수 없는 자원에 안 나온다). 이 층은 규칙을 다시 만들지 않는다.

export type ResourceKind = "skill" | "agent";
export type Permission = "read" | "edit" | "publish";
export type EngineCall = (argv: string[]) => Promise<unknown>;

export interface ResourceRow {
  id: string;
  kind: ResourceKind;
  scope: "global" | "project";
  project?: string;
  name: string;
  description: string;
  permission?: Permission;
  skills: string[];
  repo?: string;
  tools: Record<string, unknown>;
  editedAt?: string;
  editedSession?: string;
  editedAgent?: string;
  path: string;
  body?: string;
  /** 고칠 때 넘기는 쓰기 전 토큰. 목록에는 없고 `getResource`가 준다 */
  precondition?: string;
  /** 이 글로벌 자원을 참조한 프로젝트 slug들. `getResource`가 준다 */
  referencedBy?: string[];
  /** 쓸 수 있는 자원 목록에서만: `own`(프로젝트 자원) 또는 `ref`(참조한 글로벌 자원) */
  source?: "own" | "ref";
}

export interface TrashRow {
  id: string;
  kind: ResourceKind;
  scope: "global" | "project";
  project?: string;
  name: string;
  deletedAt: string;
}

export interface MissingSkill {
  agent: { id: string; name: string };
  skillId: string;
  skill?: { name: string; scope: string };
  canReference: boolean;
}

export interface UsableResources {
  project: string;
  skills: ResourceRow[];
  agents: ResourceRow[];
  broken: { id: string; hint: string }[];
  missingSkills: MissingSkill[];
}

/** 화면의 폼이 들고 있는 값. 저장하기 전의 글이다. */
export interface ResourceForm {
  kind: ResourceKind;
  name: string;
  description: string;
  body: string;
  permission: Permission;
  skills: string[];
  repo: string;
  /** 도구별 선택 칸을 JSON 글로 보인 것. 비어 있으면 없는 것이다 */
  toolsText: string;
}

/** 엔진에 보낼 저장 입력. 있는 칸만 인자로 간다. */
export interface SaveInput {
  id?: string;
  kind?: ResourceKind;
  scope?: "global";
  project?: string;
  name?: string;
  description?: string;
  body?: string;
  permission?: Permission;
  skills?: string[];
  repo?: string;
  /** JSON 객체 글. 빈 객체 글(`{}`)은 비운다 */
  tools?: string;
  f1?: string;
}

export const PERMISSIONS: readonly { id: Permission; label: string; hint: string }[] = [
  { id: "read", label: "읽기만", hint: "코드와 노트를 읽기만 해요." },
  { id: "edit", label: "수정", hint: "앱이 만든 격리 작업 폴더에서만 고쳐요." },
  { id: "publish", label: "내보내기", hint: "승인을 받아 push와 PR까지 해요." },
];

export const NAME_MAX = 60;
export const DESCRIPTION_MAX = 1000;
export const BODY_MAX = 100_000;

// ---------------------------------------------------------------- 읽기

const text = (v: unknown): string | undefined => (typeof v === "string" ? v : undefined);
const strings = (v: unknown): string[] => (Array.isArray(v) ? v.filter((s): s is string => typeof s === "string") : []);
const isRecord = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);

/** 엔진 행을 자원으로 읽는다. 모양이 틀리면 null이다. */
export function toResource(v: unknown): ResourceRow | null {
  if (!isRecord(v)) return null;
  const id = text(v.id);
  const kind = v.kind === "skill" || v.kind === "agent" ? v.kind : null;
  const scope = v.scope === "global" || v.scope === "project" ? v.scope : null;
  const name = text(v.name);
  if (!id || !kind || !scope || !name) return null;
  const permission = v.permission === "read" || v.permission === "edit" || v.permission === "publish" ? v.permission : undefined;
  const source = v.source === "own" || v.source === "ref" ? v.source : undefined;
  return {
    id, kind, scope, name,
    project: text(v.project),
    description: text(v.description) ?? "",
    permission,
    skills: strings(v.skills),
    repo: text(v.repo),
    tools: isRecord(v.tools) ? v.tools : {},
    editedAt: text(v.editedAt),
    editedSession: text(v.editedSession),
    editedAgent: text(v.editedAgent),
    path: text(v.path) ?? "",
    body: text(v.body),
    precondition: text(v.precondition),
    referencedBy: Array.isArray(v.referencedBy) ? strings(v.referencedBy) : undefined,
    source,
  };
}

const rowsOf = (v: unknown): ResourceRow[] => (Array.isArray(v) ? v.map(toResource).filter((r): r is ResourceRow => r !== null) : []);

export async function listResources(
  call: EngineCall,
  opts: { global?: boolean; project?: string; kind?: ResourceKind } = {},
): Promise<{ resources: ResourceRow[]; skipped: string[] }> {
  const argv = ["list-resources"];
  if (opts.global) argv.push("scope=global");
  if (opts.project) argv.push(`project=${opts.project}`);
  if (opts.kind) argv.push(`kind=${opts.kind}`);
  const out = await call(argv);
  const o = isRecord(out) ? out : {};
  return { resources: rowsOf(o.resources), skipped: strings(o.skipped) };
}

export async function listTrash(call: EngineCall): Promise<TrashRow[]> {
  const out = await call(["list-resources", "trash=true"]);
  const list = isRecord(out) && Array.isArray(out.trash) ? out.trash : [];
  const rows: TrashRow[] = [];
  for (const v of list) {
    if (!isRecord(v)) continue;
    const id = text(v.id), name = text(v.name);
    const kind = v.kind === "skill" || v.kind === "agent" ? v.kind : null;
    const scope = v.scope === "global" || v.scope === "project" ? v.scope : null;
    if (id && name && kind && scope) rows.push({ id, kind, scope, project: text(v.project), name, deletedAt: text(v.deletedAt) ?? "" });
  }
  return rows;
}

export async function getResource(call: EngineCall, id: string): Promise<ResourceRow> {
  const row = toResource(await call(["get-resource", id]));
  if (!row) throw new Error("자원 응답의 모양이 틀려요.");
  return row;
}

export async function usableResources(call: EngineCall, project: string): Promise<UsableResources> {
  const out = await call(["usable-resources", project]);
  const o = isRecord(out) ? out : {};
  const broken = (Array.isArray(o.broken) ? o.broken : []).filter(isRecord).map((b) => ({ id: text(b.id) ?? "", hint: text(b.hint) ?? "" })).filter((b) => b.id);
  const missingSkills: MissingSkill[] = [];
  for (const m of Array.isArray(o.missingSkills) ? o.missingSkills : []) {
    if (!isRecord(m) || !isRecord(m.agent)) continue;
    const skillId = text(m.skillId), agentId = text(m.agent.id);
    if (!skillId || !agentId) continue;
    missingSkills.push({
      agent: { id: agentId, name: text(m.agent.name) ?? "" },
      skillId,
      skill: isRecord(m.skill) ? { name: text(m.skill.name) ?? "", scope: text(m.skill.scope) ?? "" } : undefined,
      canReference: m.canReference === true,
    });
  }
  return { project: text(o.project) ?? project, skills: rowsOf(o.skills), agents: rowsOf(o.agents), broken, missingSkills };
}

// ---------------------------------------------------------------- 쓰기

/** 저장 인자를 만든다. 있는 칸만 간다. */
export function saveArgv(input: SaveInput): string[] {
  const argv = ["save-resource"];
  const add = (k: string, v: string | undefined) => {
    if (v !== undefined) argv.push(`${k}=${v}`);
  };
  add("id", input.id);
  add("kind", input.kind);
  add("scope", input.scope);
  add("project", input.project);
  add("name", input.name);
  add("description", input.description);
  add("body", input.body);
  add("permission", input.permission);
  if (input.skills !== undefined) argv.push(`skills=${JSON.stringify(input.skills)}`);
  add("repo", input.repo);
  add("tools", input.tools);
  add("f1", input.f1);
  return argv;
}

export async function saveResource(call: EngineCall, input: SaveInput): Promise<ResourceRow> {
  const row = toResource(await call(saveArgv(input)));
  if (!row) throw new Error("저장 응답의 모양이 틀려요.");
  return row;
}

export async function deleteResource(call: EngineCall, id: string): Promise<void> {
  await call(["delete-resource", id]);
}

export async function restoreResource(call: EngineCall, id: string): Promise<void> {
  await call(["restore-resource", id]);
}

/** 프로젝트가 글로벌 자원을 참조하는 것을 켠다(remove면 끈다). withSkills면 에이전트의 글로벌 스킬도 함께 켠다. */
export async function setReference(
  call: EngineCall,
  project: string,
  id: string,
  opts: { remove?: boolean; withSkills?: boolean } = {},
): Promise<void> {
  const argv = ["set-resource-ref", project, id];
  if (opts.remove) argv.push("remove=true");
  if (opts.withSkills) argv.push("with-skills=true");
  await call(argv);
}

// ---------------------------------------------------------------- 폼

/** 새 자원 폼의 처음 값. */
export function emptyForm(kind: ResourceKind): ResourceForm {
  return { kind, name: "", description: "", body: "", permission: "read", skills: [], repo: "", toolsText: "" };
}

/** 있는 자원으로 폼을 채운다. */
export function formFrom(r: ResourceRow): ResourceForm {
  return {
    kind: r.kind,
    name: r.name,
    description: r.description,
    body: r.body ?? "",
    permission: r.permission ?? "read",
    skills: [...r.skills],
    repo: r.repo ?? "",
    toolsText: toolsText(r.tools),
  };
}

export function toolsText(tools: Record<string, unknown>): string {
  return Object.keys(tools).length ? JSON.stringify(tools, null, 2) : "";
}

const TOOL_KEY = /^[a-z][a-z0-9_-]{0,31}$/;

/** 저장하기 전에 엔진이 거절할 값을 미리 알려 준다. 문제가 없으면 null이다. */
export function validateForm(f: ResourceForm): string | null {
  const name = f.name.trim();
  if (!name) return "이름이 필요해요.";
  if ([...name].length > NAME_MAX) return `이름은 ${NAME_MAX}자까지예요.`;
  if (/[\\/]/.test(name)) return "이름에는 / 와 \\ 를 쓸 수 없어요.";
  // eslint-disable-next-line no-control-regex
  if (/[\u0000-\u001f\u007f]/.test(name)) return "이름에 제어 문자를 쓸 수 없어요.";
  const description = f.description.trim();
  if (!description) return "설명이 필요해요. 이 자원이 무엇인지 한 문단으로 적어 주세요.";
  if ([...description].length > DESCRIPTION_MAX) return `설명은 ${DESCRIPTION_MAX}자까지예요.`;
  if (f.body.length > BODY_MAX) return `지침은 ${BODY_MAX.toLocaleString("ko-KR")}자까지예요.`;
  const tools = f.toolsText.trim();
  if (tools) {
    let parsed: unknown;
    try {
      parsed = JSON.parse(tools);
    } catch {
      return "도구별 칸이 JSON이 아니에요.";
    }
    if (!isRecord(parsed)) return "도구별 칸은 JSON 객체여야 해요. 예: {\"claude\": {}}";
    for (const key of Object.keys(parsed)) {
      if (!TOOL_KEY.test(key)) return `도구별 칸 이름은 소문자 도구 이름이어야 해요(예: claude, codex): ${key}`;
    }
  }
  return null;
}

const sameList = (a: string[], b: string[]) => a.length === b.length && a.every((v, i) => v === b[i]);

/**
 * 폼 값을 저장 입력으로 바꾼다. 새로 만들 때는 모든 칸을 보내고, 고칠 때는 바뀐 칸만 보낸다(안 바뀐 도구별 칸은 보내지 않아
 * 몰랐던 칸이 있어도 그대로 보존된다).
 */
export function formToInput(
  form: ResourceForm,
  ctx: { project?: string; existing?: ResourceRow },
): SaveInput {
  const e = ctx.existing;
  const isAgent = form.kind === "agent";
  if (!e) {
    const input: SaveInput = {
      kind: form.kind,
      ...(ctx.project ? { project: ctx.project } : { scope: "global" as const }),
      name: form.name.trim(),
      description: form.description.trim(),
      body: form.body,
    };
    if (isAgent) {
      input.permission = form.permission;
      if (form.skills.length) input.skills = form.skills;
      if (ctx.project && form.repo) input.repo = form.repo;
    }
    if (form.toolsText.trim()) input.tools = form.toolsText.trim();
    return input;
  }
  const input: SaveInput = { id: e.id, f1: e.precondition };
  if (form.name.trim() !== e.name) input.name = form.name.trim();
  if (form.description.trim() !== e.description) input.description = form.description.trim();
  if (form.body !== (e.body ?? "")) input.body = form.body;
  if (isAgent) {
    if (form.permission !== (e.permission ?? "read")) input.permission = form.permission;
    if (!sameList(form.skills, e.skills)) input.skills = form.skills;
    if (e.project && form.repo !== (e.repo ?? "")) input.repo = form.repo;
  }
  if (form.toolsText.trim() !== toolsText(e.tools)) input.tools = form.toolsText.trim() || "{}";
  return input;
}

/** 폼 값이 처음과 달라졌는지. 닫을 때 버릴지 묻는 데 쓴다. */
export function formChanged(form: ResourceForm, existing: ResourceRow | undefined, kind: ResourceKind): boolean {
  const base = existing ? formFrom(existing) : emptyForm(kind);
  return (
    form.name !== base.name || form.description !== base.description || form.body !== base.body || form.permission !== base.permission ||
    form.repo !== base.repo || form.toolsText !== base.toolsText || !sameList(form.skills, base.skills)
  );
}

// ---------------------------------------------------------------- 표시와 계산

export const kindLabel = (k: ResourceKind) => (k === "skill" ? "스킬" : "에이전트");

/** 이름이 같아도 구별되도록 범위를 붙인 이름(「글로벌 · 번역-검수」). */
export const scopeLabel = (r: Pick<ResourceRow, "scope" | "name">) => `${r.scope === "global" ? "글로벌" : "프로젝트"} · ${r.name}`;

export function permissionLabel(p: Permission | undefined): string {
  return PERMISSIONS.find((x) => x.id === p)?.label ?? "읽기만";
}

const AGENT_NAMES: Record<string, string> = { claude: "Claude Code", codex: "Codex", gemini: "Gemini CLI" };

/** 마지막으로 누가 고쳤는지 한 줄. 세션 ID가 있으면 AI가 고친 것이다. */
export function editedLine(r: Pick<ResourceRow, "editedAt" | "editedSession" | "editedAgent">): string {
  if (!r.editedAt && !r.editedSession) return "";
  if (r.editedSession) {
    const cli = AGENT_NAMES[r.editedAgent ?? "claude"] ?? r.editedAgent ?? "Claude Code";
    return `AI가 고쳤어요 · ${cli} · 세션 ${r.editedSession.slice(0, 8)}`;
  }
  return "사람이 고쳤어요";
}

/** 글로벌 자원 가운데 이 프로젝트가 아직 참조하지 않은 것. */
export function unreferencedGlobals(globals: ResourceRow[], usable: UsableResources): ResourceRow[] {
  const referenced = new Set([...usable.skills, ...usable.agents].filter((r) => r.source === "ref").map((r) => r.id));
  return globals.filter((g) => !referenced.has(g.id));
}

/**
 * 에이전트를 참조할 때 함께 참조할지 묻는 스킬 목록. 그 에이전트가 쓰는 글로벌 스킬 가운데 이 프로젝트가 아직 쓸 수 없는 것이다.
 * 확인하는 것이 곧 명시적인 참조이므로, 이 목록을 화면에 보여 주고 사용자가 고르게 한다.
 */
export function skillsToReference(agent: ResourceRow, globals: ResourceRow[], usable: UsableResources): ResourceRow[] {
  const usableIds = new Set(usable.skills.map((s) => s.id));
  const byId = new Map(globals.map((g) => [g.id, g]));
  const out: ResourceRow[] = [];
  for (const id of agent.skills) {
    const g = byId.get(id);
    if (g && g.kind === "skill" && !usableIds.has(id)) out.push(g);
  }
  return out;
}

/** 에이전트 폼에서 고를 수 있는 스킬: 글로벌 에이전트는 글로벌 스킬만, 프로젝트 에이전트는 글로벌 스킬과 그 프로젝트의 스킬이다. */
export function selectableSkills(opts: { project?: string; globals: ResourceRow[]; projectSkills: ResourceRow[] }): ResourceRow[] {
  const globals = opts.globals.filter((r) => r.kind === "skill");
  if (!opts.project) return globals;
  return [...opts.projectSkills.filter((r) => r.kind === "skill" && r.project === opts.project), ...globals];
}

/** 엔진이 돌려준 오류를 사람이 읽을 글로 풀어 쓴다. */
export function explainError(e: unknown): string {
  const stderr = isRecord(e) && typeof e.stderr === "string" ? e.stderr : e instanceof Error ? e.message : String(e);
  if (/AIS-412/.test(stderr)) return "다른 곳(AI나 다른 창)에서 먼저 고쳤어요. 목록을 새로 불러온 뒤 다시 고쳐 주세요.";
  const m = /AIS-\d+\s+(.*)/s.exec(stderr);
  return (m ? m[1] : stderr).trim() || "알 수 없는 오류가 났어요.";
}
