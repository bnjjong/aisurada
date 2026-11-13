// 태스크(티켓, 엔진 명령 9개)를 화면에서 부르는 얇은 층. 엔진 응답의 모양을 정리하고, 화면이 쓰는 순수한 계산
// (폼 값 → 인자, 거르기, 상태별 묶기, 한 줄 풀이)을 모아 시험으로 잰다.
// 규칙(완료 기준 필수, 상태 전이의 누가·조건, 막혀 있음)은 엔진이 지킨다. 이 층은 규칙을 다시 만들지 않고, 엔진이 알려 준
// `transitions`(사람이 바꿀 수 있는 곳과 막힌 까닭)를 그대로 보여 준다.
import { explainError as explainResourceError } from "./resourcesClient.ts";
import type { EngineCall } from "./resourcesClient.ts";
import { asText } from "../workspace/types.ts";
import type { Row } from "../workspace/types.ts";
import { parseRepos } from "./repoLinks.ts";

export type { EngineCall };

export type TaskStatus =
  | "draft"
  | "ready-for-develop"
  | "in-progress"
  | "waiting-answer"
  | "in-review"
  | "done"
  | "failed"
  | "cancelled";
export type TaskType = "feature" | "bug" | "chore";
export type TaskPriority = "high" | "medium" | "low";
/** 연결의 모양. 건 쪽(`blocks`, `duplicates`)과 받은 쪽(`blocked-by`, `duplicated-by`)이 따로 있고, `relates`는 한 가지다. */
export type RelationType = "blocks" | "blocked-by" | "relates" | "duplicates" | "duplicated-by";

export interface TaskRow {
  key: string;
  id: string;
  title: string;
  type: TaskType;
  priority: TaskPriority;
  status: TaskStatus;
  repo?: string;
  project?: string;
  baseBranch?: string;
  labels: string[];
  criteriaDone: number;
  criteriaTotal: number;
  /** 이 태스크를 막고 있으면서 아직 Done이 아닌 태스크 키 */
  blockedBy: string[];
  /** 이 태스크가 막고 있는 태스크 키 */
  blocks: string[];
  createdAt?: string;
  editedAt?: string;
  editedSession?: string;
  editedAgent?: string;
  path: string;
}

export interface TaskCriterion {
  text: string;
  done: boolean;
  checkedAt?: string;
  checkedBy?: string;
}

export interface TaskRef {
  kind: "note" | "github";
  target: string;
  title?: string;
}

export interface TaskRelated {
  type: RelationType;
  key: string;
  title: string;
  status: TaskStatus;
}

export interface TransitionOption {
  to: TaskStatus;
  title: string;
  ok: boolean;
  reason?: string;
}

export interface Readiness {
  item: string;
  ok: boolean;
  label: string;
}

export interface ActivityEvent {
  at: string;
  kind: string;
  by?: string;
  session?: string;
  agent?: string;
  body?: string;
  from?: string;
  to?: string;
  fields?: string[];
  type?: string;
  with?: string;
  direction?: string;
}

export interface TaskDetail extends TaskRow {
  body: string;
  criteria: TaskCriterion[];
  refs: TaskRef[];
  links: TaskRelated[];
  readiness: Readiness[];
  ready: boolean;
  transitions: TransitionOption[];
  activity: ActivityEvent[];
  /** 고칠 때 넘기는 쓰기 전 토큰 */
  precondition?: string;
}

export interface TaskTrashRow {
  key: string;
  id: string;
  title: string;
  deletedAt: string;
}

/** 화면의 폼이 들고 있는 값. 저장하기 전의 글이다. */
export interface TaskForm {
  title: string;
  type: TaskType;
  priority: TaskPriority;
  repo: string;
  /** 저장소를 고르면 따라오는 프로젝트 slug. 없으면 빈 글 */
  project: string;
  base: string;
  body: string;
  /** 쉼표로 이은 라벨 */
  labelsText: string;
  criteria: TaskCriterion[];
  refs: TaskRef[];
}

export const STATUSES: readonly { id: TaskStatus; title: string }[] = [
  { id: "draft", title: "Draft" },
  { id: "ready-for-develop", title: "Ready For Develop" },
  { id: "in-progress", title: "In Progress" },
  { id: "waiting-answer", title: "Waiting Answer" },
  { id: "in-review", title: "In Review" },
  { id: "done", title: "Done" },
  { id: "failed", title: "Failed" },
  { id: "cancelled", title: "Cancelled" },
];
/** 보드에 열로 늘어놓는 상태. 끝난 상태는 접어 둔다. */
export const BOARD_COLUMNS: readonly TaskStatus[] = ["draft", "ready-for-develop", "in-progress", "waiting-answer", "in-review"];
export const SHELF: readonly TaskStatus[] = ["done", "failed", "cancelled"];

export const TYPES: readonly { id: TaskType; label: string }[] = [
  { id: "feature", label: "기능" },
  { id: "bug", label: "버그" },
  { id: "chore", label: "잡무" },
];
export const PRIORITIES: readonly { id: TaskPriority; label: string }[] = [
  { id: "high", label: "높음" },
  { id: "medium", label: "보통" },
  { id: "low", label: "낮음" },
];

export const TITLE_MAX = 200;
export const CRITERION_MAX = 500;
export const CRITERIA_MAX = 50;
export const BODY_MAX = 100_000;
export const LABEL_MAX = 30;
export const LABELS_MAX = 20;

// ---------------------------------------------------------------- 읽기

const text = (v: unknown): string | undefined => (typeof v === "string" ? v : undefined);
const strings = (v: unknown): string[] => (Array.isArray(v) ? v.filter((s): s is string => typeof s === "string") : []);
const isRecord = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
const count = (v: unknown): number => (typeof v === "number" && Number.isFinite(v) ? v : 0);

const STATUS_IDS = new Set<string>(STATUSES.map((s) => s.id));
const isStatus = (v: unknown): v is TaskStatus => typeof v === "string" && STATUS_IDS.has(v);
const isType = (v: unknown): v is TaskType => v === "feature" || v === "bug" || v === "chore";
const isPriority = (v: unknown): v is TaskPriority => v === "high" || v === "medium" || v === "low";
const RELATIONS = new Set<string>(["blocks", "blocked-by", "relates", "duplicates", "duplicated-by"]);

/** 엔진 행을 태스크로 읽는다. 모양이 틀리면 null이다. */
export function toTask(v: unknown): TaskRow | null {
  if (!isRecord(v)) return null;
  const key = text(v.key), id = text(v.id), title = text(v.title);
  if (!key || !id || !title || !isStatus(v.status)) return null;
  return {
    key, id, title,
    type: isType(v.type) ? v.type : "feature",
    priority: isPriority(v.priority) ? v.priority : "medium",
    status: v.status,
    repo: text(v.repo),
    project: text(v.project),
    baseBranch: text(v.baseBranch),
    labels: strings(v.labels),
    criteriaDone: count(v.criteriaDone),
    criteriaTotal: count(v.criteriaTotal),
    blockedBy: strings(v.blockedBy),
    blocks: strings(v.blocks),
    createdAt: text(v.createdAt),
    editedAt: text(v.editedAt),
    editedSession: text(v.editedSession),
    editedAgent: text(v.editedAgent),
    path: text(v.path) ?? "",
  };
}

function toCriterion(v: unknown): TaskCriterion | null {
  if (!isRecord(v)) return null;
  const t = text(v.text);
  if (!t) return null;
  return { text: t, done: v.done === true, checkedAt: text(v.checkedAt), checkedBy: text(v.checkedBy) };
}

function toRef(v: unknown): TaskRef | null {
  if (!isRecord(v)) return null;
  const target = text(v.target);
  if ((v.kind !== "note" && v.kind !== "github") || !target) return null;
  return { kind: v.kind, target, title: text(v.title) };
}

function toRelated(v: unknown): TaskRelated | null {
  if (!isRecord(v)) return null;
  const type = text(v.type), key = text(v.key);
  if (!type || !RELATIONS.has(type) || !key || !isStatus(v.status)) return null;
  return { type: type as RelationType, key, title: text(v.title) ?? "", status: v.status };
}

function toEvent(v: unknown): ActivityEvent | null {
  if (!isRecord(v)) return null;
  const at = text(v.at), kind = text(v.kind);
  if (!at || !kind) return null;
  return {
    at, kind, by: text(v.by), session: text(v.session), agent: text(v.agent), body: text(v.body), from: text(v.from), to: text(v.to),
    fields: Array.isArray(v.fields) ? strings(v.fields) : undefined, type: text(v.type), with: text(v.with), direction: text(v.direction),
  };
}

const listOf = <T,>(v: unknown, read: (x: unknown) => T | null): T[] => (Array.isArray(v) ? v.map(read).filter((x): x is T => x !== null) : []);

/** `get-task`나 만들기·고치기·상태 바꾸기 응답을 상세로 읽는다. 모양이 틀리면 null이다. */
export function toDetail(v: unknown): TaskDetail | null {
  const row = toTask(v);
  if (!row || !isRecord(v)) return null;
  return {
    ...row,
    body: text(v.body) ?? "",
    criteria: listOf(v.criteria, toCriterion),
    refs: listOf(v.refs, toRef),
    links: listOf(v.links, toRelated),
    readiness: listOf(v.readiness, (x) => (isRecord(x) && text(x.item) ? { item: text(x.item)!, ok: x.ok === true, label: text(x.label) ?? "" } : null)),
    ready: v.ready === true,
    transitions: listOf(v.transitions, (x) =>
      isRecord(x) && isStatus(x.to) ? { to: x.to, title: text(x.title) ?? x.to, ok: x.ok === true, reason: text(x.reason) } : null),
    activity: listOf(v.activity, toEvent),
    precondition: text(v.precondition),
  };
}

export async function listTasks(call: EngineCall): Promise<{ tasks: TaskRow[]; skipped: string[] }> {
  const out = await call(["list-tasks"]);
  const o = isRecord(out) ? out : {};
  return { tasks: listOf(o.tasks, toTask), skipped: strings(o.skipped) };
}

export async function listTaskTrash(call: EngineCall): Promise<TaskTrashRow[]> {
  const out = await call(["list-tasks", "trash=true"]);
  return listOf(isRecord(out) ? out.trash : null, (x) => {
    if (!isRecord(x)) return null;
    const key = text(x.key), id = text(x.id);
    return key && id ? { key, id, title: text(x.title) ?? key, deletedAt: text(x.deletedAt) ?? "" } : null;
  });
}

export async function getTask(call: EngineCall, key: string, activity = 100): Promise<TaskDetail> {
  const d = toDetail(await call(["get-task", key, `activity=${activity}`]));
  if (!d) throw new Error("태스크 응답의 모양이 틀려요.");
  return d;
}

// ---------------------------------------------------------------- 쓰기

const detailOf = (out: unknown): TaskDetail => {
  const d = toDetail(out);
  if (!d) throw new Error("태스크 응답의 모양이 틀려요.");
  return d;
};

/** 만들기 인자. 완료 기준이 없으면 엔진이 거절한다(필수). 확인된 기준(done)은 만들 때 보내지 않는다. */
export function createArgv(form: TaskForm): string[] {
  const argv = ["create-task", `title=${form.title.trim()}`, `repo=${form.repo.trim()}`, `criteria=${JSON.stringify(form.criteria.map((c) => c.text.trim()).filter(Boolean))}`];
  argv.push(`type=${form.type}`, `priority=${form.priority}`);
  if (form.project) argv.push(`project=${form.project}`);
  if (form.base.trim()) argv.push(`base=${form.base.trim()}`);
  if (form.body.trim()) argv.push(`body=${form.body}`);
  const labels = parseLabels(form.labelsText);
  if (labels.length) argv.push(`labels=${JSON.stringify(labels)}`);
  if (form.refs.length) argv.push(`refs=${JSON.stringify(form.refs.map(refForEngine))}`);
  return argv;
}

const refForEngine = (r: TaskRef) => (r.title ? { kind: r.kind, target: r.target, title: r.title } : { kind: r.kind, target: r.target });

export async function createTask(call: EngineCall, form: TaskForm): Promise<TaskDetail> {
  return detailOf(await call(createArgv(form)));
}

const sameList = (a: string[], b: string[]) => a.length === b.length && a.every((v, i) => v === b[i]);
const sameCriteria = (a: TaskCriterion[], b: TaskCriterion[]) => a.length === b.length && a.every((c, i) => c.text === b[i].text && c.done === b[i].done);
const sameRefs = (a: TaskRef[], b: TaskRef[]) => a.length === b.length && a.every((r, i) => r.kind === b[i].kind && r.target === b[i].target && (r.title ?? "") === (b[i].title ?? ""));

/**
 * 고치기 인자. 바뀐 칸만 간다(그래서 같은 값이면 엔진이 파일을 안 바꾼다). 완료 기준은 확인 여부까지 통째로 보낸다.
 * 바뀐 칸이 없으면 null이다.
 */
export function updateArgv(existing: TaskDetail, form: TaskForm): string[] | null {
  const argv = ["update-task", existing.key, `f1=${existing.precondition ?? ""}`];
  const before = argv.length;
  if (form.title.trim() !== existing.title) argv.push(`title=${form.title.trim()}`);
  if (form.type !== existing.type) argv.push(`type=${form.type}`);
  if (form.priority !== existing.priority) argv.push(`priority=${form.priority}`);
  if (form.repo.trim() !== (existing.repo ?? "")) argv.push(`repo=${form.repo.trim()}`);
  if (form.project !== (existing.project ?? "")) argv.push(`project=${form.project}`);
  if (form.base.trim() !== (existing.baseBranch ?? "")) argv.push(`base=${form.base.trim()}`);
  if (form.body !== existing.body) argv.push(`body=${form.body}`);
  const labels = parseLabels(form.labelsText);
  if (!sameList(labels, existing.labels)) argv.push(`labels=${JSON.stringify(labels)}`);
  if (!sameCriteria(form.criteria, existing.criteria)) argv.push(`criteria=${JSON.stringify(form.criteria.map((c) => ({ text: c.text.trim(), done: c.done })))}`);
  if (!sameRefs(form.refs, existing.refs)) argv.push(`refs=${JSON.stringify(form.refs.map(refForEngine))}`);
  return argv.length === before ? null : argv;
}

export async function updateTask(call: EngineCall, existing: TaskDetail, form: TaskForm): Promise<TaskDetail> {
  const argv = updateArgv(existing, form);
  if (!argv) return existing;
  return detailOf(await call(argv));
}

/** 완료 기준 하나의 확인을 뒤집어 저장한다(상세 화면의 체크). */
export async function toggleCriterion(call: EngineCall, existing: TaskDetail, index: number): Promise<TaskDetail> {
  const next = existing.criteria.map((c, i) => (i === index ? { ...c, done: !c.done } : c));
  return detailOf(await call(["update-task", existing.key, `f1=${existing.precondition ?? ""}`, `criteria=${JSON.stringify(next.map((c) => ({ text: c.text, done: c.done })))}`]));
}

/** 상태를 바꾼다. `from`은 화면이 본 지금 상태다(그 사이에 러너나 다른 창이 먼저 바꿨으면 엔진이 거절한다). */
export async function transitionTask(call: EngineCall, task: Pick<TaskRow, "key" | "status">, to: TaskStatus): Promise<TaskDetail> {
  return detailOf(await call(["transition-task", task.key, `to=${to}`, `from=${task.status}`]));
}

/**
 * 연결을 건다. 화면은 이 태스크 기준으로 말한다(「이 태스크가 B를 막고 있다」, 「B에 막혀 있다」). 엔진은 건 쪽 하나에만 적으므로
 * 방향이 거꾸로인 말(`blocked-by`, `duplicated-by`)은 건 쪽을 바꿔서 보낸다.
 */
export function linkArgv(current: string, other: string, relation: RelationType, remove = false): string[] {
  const reversed = relation === "blocked-by" || relation === "duplicated-by";
  const type = relation === "blocked-by" ? "blocks" : relation === "duplicated-by" ? "duplicates" : relation;
  const argv = ["link-tasks", reversed ? other : current, reversed ? current : other, `type=${type}`];
  if (remove) argv.push("remove=true");
  return argv;
}

export async function linkTasks(call: EngineCall, current: string, other: string, relation: RelationType, remove = false): Promise<void> {
  await call(linkArgv(current, other, relation, remove));
}

export async function addComment(call: EngineCall, key: string, body: string): Promise<void> {
  await call(["add-task-comment", key, `body=${body.trim()}`]);
}

export async function deleteTask(call: EngineCall, key: string): Promise<void> {
  await call(["delete-task", key]);
}

export async function restoreTask(call: EngineCall, key: string): Promise<void> {
  await call(["restore-task", key]);
}

// ---------------------------------------------------------------- 폼

export function emptyTaskForm(): TaskForm {
  return { title: "", type: "feature", priority: "medium", repo: "", project: "", base: "", body: "", labelsText: "", criteria: [], refs: [] };
}

export function formFromTask(t: TaskDetail): TaskForm {
  return {
    title: t.title, type: t.type, priority: t.priority, repo: t.repo ?? "", project: t.project ?? "", base: t.baseBranch ?? "", body: t.body,
    labelsText: t.labels.join(", "), criteria: t.criteria.map((c) => ({ ...c })), refs: t.refs.map((r) => ({ ...r })),
  };
}

/** 쉼표로 이은 라벨 글을 목록으로. 빈 것과 겹친 것은 뺀다. */
export function parseLabels(textValue: string): string[] {
  const out: string[] = [];
  for (const raw of textValue.split(/[,\n]/)) {
    const label = raw.trim();
    if (label && !out.includes(label)) out.push(label);
  }
  return out;
}

const REPO = /^[A-Za-z0-9_.-]{1,100}\/[A-Za-z0-9_.-]{1,100}$/;
const BRANCH = /^[A-Za-z0-9][A-Za-z0-9._/-]{0,99}$/;

/** 저장하기 전에 알려 줄 만한 것만 먼저 막는다. 나머지는 엔진이 검사한다. 문제가 없으면 null이다. */
export function validateTaskForm(f: TaskForm): string | null {
  const title = f.title.trim();
  if (!title) return "요약이 필요해요.";
  if ([...title].length > TITLE_MAX) return `요약은 ${TITLE_MAX}자까지예요.`;
  if (/[\u0000-\u001f\u007f]/.test(title)) return "요약에 제어 문자를 쓸 수 없어요.";
  if (!f.repo.trim()) return "저장소를 골라 주세요.";
  if (!REPO.test(f.repo.trim())) return "저장소는 owner/name 모양이어야 해요.";
  if (f.base.trim() && (!BRANCH.test(f.base.trim()) || f.base.includes("..") || f.base.trim().endsWith("/"))) return "기준 브랜치 이름이 틀려요.";
  if (f.body.length > BODY_MAX) return `설명은 ${BODY_MAX.toLocaleString("ko-KR")}자까지예요.`;
  const items = f.criteria.map((c) => c.text.trim()).filter(Boolean);
  if (items.length === 0) return "완료 기준을 하나 이상 써 주세요. 일이 끝났는지 이 기준으로 확인해요.";
  if (items.length > CRITERIA_MAX) return `완료 기준은 ${CRITERIA_MAX}개까지예요.`;
  if (items.some((c) => [...c].length > CRITERION_MAX)) return `완료 기준 한 줄은 ${CRITERION_MAX}자까지예요.`;
  const labels = parseLabels(f.labelsText);
  if (labels.length > LABELS_MAX) return `라벨은 ${LABELS_MAX}개까지예요.`;
  if (labels.some((l) => [...l].length > LABEL_MAX)) return `라벨은 ${LABEL_MAX}자까지예요.`;
  return null;
}

/** 폼 값이 처음과 달라졌는지. 닫을 때 버릴지 묻는 데 쓴다. */
export function taskFormChanged(form: TaskForm, existing: TaskDetail | undefined): boolean {
  const base = existing ? formFromTask(existing) : emptyTaskForm();
  return (
    form.title !== base.title || form.type !== base.type || form.priority !== base.priority || form.repo !== base.repo || form.project !== base.project ||
    form.base !== base.base || form.body !== base.body || form.labelsText !== base.labelsText ||
    !sameCriteria(form.criteria.filter((c) => c.text.trim() !== ""), base.criteria) ||
    !sameRefs(form.refs, base.refs)
  );
}

/**
 * 받은 글에서 연결할 자원 하나를 읽는다. GitHub 이슈·PR 주소나 `owner/name#53`이면 github, 노트 id(`nd…`)면 note다.
 * 알아볼 수 없으면 null이다.
 */
export function parseRefInput(raw: string): TaskRef | null {
  const v = raw.trim();
  if (/^https:\/\/github\.com\/[A-Za-z0-9_.-]{1,100}\/[A-Za-z0-9_.-]{1,100}\/(issues|pull)\/[1-9][0-9]{0,8}$/.test(v)) return { kind: "github", target: v };
  if (/^[A-Za-z0-9_.-]{1,100}\/[A-Za-z0-9_.-]{1,100}#[1-9][0-9]{0,8}$/.test(v)) return { kind: "github", target: v };
  if (/^(nd|sn)[0-9a-z]+-[0-9a-f]{8}$/.test(v)) return { kind: "note", target: v };
  return null;
}

/** GitHub 자원의 열 수 있는 주소. `owner/name#53`은 이슈 주소로 바꾼다(PR이어도 GitHub가 알맞게 넘겨 준다). */
export function githubUrl(target: string): string {
  if (target.startsWith("https://")) return target;
  const m = /^([^#]+)#(\d+)$/.exec(target);
  return m ? `https://github.com/${m[1]}/issues/${m[2]}` : target;
}

/** 태스크를 만들 때 고를 수 있는 저장소. 프로젝트들의 저장소 가운데 GitHub 저장소가 있는 것이고, 고르면 그 프로젝트도 따라온다. */
export interface RepoChoice {
  repo: string;
  project: string;
  label: string;
}

export function repoChoices(projects: Row[]): RepoChoice[] {
  const out: RepoChoice[] = [];
  const seen = new Set<string>();
  for (const p of projects) {
    const slug = asText(p.slug);
    const title = asText(p.title) || slug;
    for (const entry of parseRepos(p.repos)) {
      if (!entry.github || seen.has(entry.github)) continue;
      seen.add(entry.github);
      out.push({ repo: entry.github, project: slug, label: `${entry.github} · ${title}` });
    }
  }
  return out;
}

// ---------------------------------------------------------------- 거르기·묶기

export interface TaskFilter {
  query: string;
  repo: string;
  type: TaskType | "";
  label: string;
  blockedOnly: boolean;
}

export const NO_FILTER: TaskFilter = { query: "", repo: "", type: "", label: "", blockedOnly: false };

/** 화면에서 바로 거른다(목록은 한 번에 다 받아 와서 입력할 때마다 엔진을 부르지 않는다). 키가 큰(새) 것이 앞이다. */
export function filterTasks(tasks: TaskRow[], f: TaskFilter): TaskRow[] {
  const q = f.query.trim().toLowerCase();
  const out = tasks.filter((t) => {
    if (q && !t.title.toLowerCase().includes(q) && !t.key.toLowerCase().includes(q)) return false;
    if (f.repo && t.repo !== f.repo) return false;
    if (f.type && t.type !== f.type) return false;
    if (f.label && !t.labels.includes(f.label)) return false;
    if (f.blockedOnly && t.blockedBy.length === 0) return false;
    return true;
  });
  return out.sort((a, b) => keyNumber(b.key) - keyNumber(a.key));
}

export const keyNumber = (key: string): number => Number(/^TSK-(\d+)$/.exec(key)?.[1] ?? 0);

export function groupByStatus(tasks: TaskRow[]): Record<TaskStatus, TaskRow[]> {
  const out = Object.fromEntries(STATUSES.map((s) => [s.id, [] as TaskRow[]])) as Record<TaskStatus, TaskRow[]>;
  for (const t of tasks) out[t.status].push(t);
  return out;
}

export const isActiveFilter = (f: TaskFilter): boolean => !!(f.query.trim() || f.repo || f.type || f.label || f.blockedOnly);

/** 거를 수 있는 값들: 쓰인 저장소와 라벨. */
export function filterChoices(tasks: TaskRow[]): { repos: string[]; labels: string[] } {
  const repos = new Set<string>(), labels = new Set<string>();
  for (const t of tasks) {
    if (t.repo) repos.add(t.repo);
    for (const l of t.labels) labels.add(l);
  }
  const sort = (s: Set<string>) => [...s].sort((a, b) => a.localeCompare(b, "ko"));
  return { repos: sort(repos), labels: sort(labels) };
}

// ---------------------------------------------------------------- 표시

export const statusTitle = (s: TaskStatus): string => STATUSES.find((x) => x.id === s)?.title ?? s;
export const typeLabel = (t: TaskType): string => TYPES.find((x) => x.id === t)?.label ?? t;
export const priorityLabel = (p: TaskPriority): string => PRIORITIES.find((x) => x.id === p)?.label ?? p;

export const RELATION_LABELS: Record<RelationType, string> = {
  blocks: "막고 있음",
  "blocked-by": "막혀 있음",
  relates: "관련",
  duplicates: "중복",
  "duplicated-by": "중복됨",
};
export const relationLabel = (t: RelationType): string => RELATION_LABELS[t];

/** 연결을 걸 때 고르는 종류(이 태스크 기준). */
export const LINK_CHOICES: readonly { id: RelationType; label: string }[] = [
  { id: "blocks", label: "이 태스크가 막고 있음" },
  { id: "blocked-by", label: "이 태스크가 막혀 있음" },
  { id: "relates", label: "관련" },
  { id: "duplicates", label: "이 태스크가 중복" },
];

/** 이 태스크에 연결할 수 있는 다른 태스크. 자기 자신과 이미 연결된 것은 뺀다. */
export function linkCandidates(tasks: TaskRow[], current: TaskDetail): TaskRow[] {
  const linked = new Set(current.links.map((l) => l.key));
  return tasks.filter((t) => t.key !== current.key && !linked.has(t.key)).sort((a, b) => keyNumber(b.key) - keyNumber(a.key));
}

const ACTORS: Record<string, string> = { human: "사람", runner: "러너" };
const CLI_NAMES: Record<string, string> = { claude: "Claude Code", codex: "Codex", gemini: "Gemini CLI" };

/** 활동 한 줄을 누가 했는지와 무엇을 했는지 글로 푼다. */
export function activityWho(e: ActivityEvent): string {
  if (e.session) return `AI(${CLI_NAMES[e.agent ?? "claude"] ?? e.agent ?? "Claude Code"})`;
  return ACTORS[e.by ?? "human"] ?? e.by ?? "사람";
}

const FIELD_NAMES: Record<string, string> = {
  title: "요약", type: "유형", priority: "우선순위", repo: "저장소", project: "프로젝트", baseBranch: "기준 브랜치", body: "설명", labels: "라벨",
  criteria: "완료 기준", refs: "연결한 자원",
};

export function activityText(e: ActivityEvent): string {
  switch (e.kind) {
    case "created": return "태스크를 만들었어요";
    case "status": return `상태를 ${statusTitle((e.from ?? "draft") as TaskStatus)}에서 ${statusTitle((e.to ?? "draft") as TaskStatus)}(으)로 바꿨어요`;
    case "edited": return `${(e.fields ?? []).map((f) => FIELD_NAMES[f] ?? f).join(", ") || "내용"}을(를) 고쳤어요`;
    case "linked":
    case "unlinked": {
      const rel = e.type ? RELATION_LABELS[relationOf(e.type, e.direction)] : "연결";
      return `${e.with ?? ""}와(과)의 연결(${rel})을 ${e.kind === "linked" ? "걸었어요" : "풀었어요"}`;
    }
    case "comment": return "댓글을 남겼어요";
    default: return e.kind;
  }
}

function relationOf(type: string, direction?: string): RelationType {
  const out = direction !== "in";
  if (type === "blocks") return out ? "blocks" : "blocked-by";
  if (type === "duplicates") return out ? "duplicates" : "duplicated-by";
  return "relates";
}

/** 완료 기준 확인 한 줄(확인한 시각과 쪽). 확인 전이면 빈 글이다. */
export function checkedLine(c: TaskCriterion): string {
  if (!c.done) return "";
  const who = c.checkedBy ? ACTORS[c.checkedBy] ?? c.checkedBy : "";
  return [who && `${who}가 확인`, c.checkedAt ? timeLabel(c.checkedAt) : ""].filter(Boolean).join(" · ");
}

/** `2026-11-13T14:05:07+09:00` → `11월 13일 14:05`. 읽을 수 없으면 받은 글 그대로다. */
export function timeLabel(iso: string): string {
  const m = /^\d{4}-(\d{2})-(\d{2})T(\d{2}):(\d{2})/.exec(iso);
  return m ? `${Number(m[1])}월 ${Number(m[2])}일 ${m[3]}:${m[4]}` : iso;
}

/** 한 줄 요약: 완료 기준 진행과 막힘. */
export function taskMeta(t: TaskRow): string {
  const parts: string[] = [];
  if (t.criteriaTotal) parts.push(`완료 기준 ${t.criteriaDone}/${t.criteriaTotal}`);
  if (t.blockedBy.length) parts.push(`${t.blockedBy.join(", ")}에 막혀 있어요`);
  return parts.join(" · ");
}

/** 엔진이 돌려준 오류를 사람이 읽을 글로 풀어 쓴다. */
export function explainError(e: unknown): string {
  const stderr = isRecord(e) && typeof e.stderr === "string" ? e.stderr : e instanceof Error ? e.message : String(e);
  if (/AIS-409/.test(stderr) && /이미 바뀌었다/.test(stderr)) return "다른 곳(러너나 다른 창)에서 먼저 상태를 바꿨어요. 새로 불러온 뒤 다시 해 주세요.";
  return explainResourceError(e);
}
