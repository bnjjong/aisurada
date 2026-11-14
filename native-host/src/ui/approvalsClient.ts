// 승인 워크플로(#53)를 화면에서 부르는 얇은 층. AI가 쓴 스킬·에이전트 변경이 정책에 따라 「대기 변경」으로 올라오고, 사람이 이전 판과 견주어 보고
// 승인하거나 거절한다. 규칙(무엇이 승인 대상이고 누가 AI인지, 승인한 판만 쓰이는 것)은 엔진이 지킨다. 이 층은 엔진 응답의 모양을 정리하고,
// 비교 화면이 쓰는 순수한 계산(필드별 비교, 줄 단위 비교, 사람이 읽을 글)을 모아 시험으로 잰다.
import type { EngineCall } from "./resourcesClient.ts";

export type ChangeStatus = "pending" | "approved" | "rejected" | "stale";
export type PolicyValue = "auto" | "approve";
export type PolicyKey = "project" | "global" | "refs" | "permission";

export interface Reason {
  category: string;
  label: string;
  /** 정책과 상관없이 늘 승인이 필요한 종류(권한 올림, 도구 칸 변경, 참조 켜기) */
  fixed: boolean;
}

/** 비교 화면이 쓰는 자원 한 판(엔진 `ResourceChanges.view`). */
export interface ResourceView {
  id: string;
  kind: "skill" | "agent";
  scope: "global" | "project";
  project?: string;
  name: string;
  description: string;
  permission?: string;
  skills?: string[];
  repo?: string;
  tools?: Record<string, unknown>;
  body: string;
}

export interface ChangeRow {
  id: string;
  status: ChangeStatus;
  command: string;
  createdAt: string;
  summary: string;
  target: { kind?: string; scope?: string; project?: string; id?: string; name?: string };
  proposer: { session?: string; agent?: string; channel?: string };
  fixed: boolean;
  /** 승인을 요구한 종류 */
  reasons: Reason[];
  decidedAt?: string;
  decidedBy?: string;
  reason?: string;
}

export interface ChangeDetail extends ChangeRow {
  before?: ResourceView;
  after?: ResourceView;
  ref?: { project: string; added: string[]; removed: string[] };
  changedFields: string[];
}

/** 승인 안 된 직접 수정. 파일을 승인한 뒤에 손으로 고쳤거나(dirty), 승인 기록 없이 생긴 새 파일(new)이다. */
export interface Unapproved {
  key: string;
  type: "resource" | "refs";
  state: "dirty" | "new";
  id?: string;
  kind?: string;
  scope?: string;
  project?: string;
  name?: string;
  path?: string;
  /** 자원이면 판, 참조 목록이면 자원 id들 */
  approved?: ResourceView | string[];
  live?: ResourceView | string[];
}

export type Policy = Record<PolicyKey, PolicyValue>;

export const DEFAULT_POLICY: Policy = { project: "auto", global: "approve", refs: "approve", permission: "approve" };

export interface ApprovalCounts {
  pending: number;
  unapproved: number;
}

export interface ApprovalsOverview {
  pending: ChangeRow[];
  unapproved: Unapproved[];
  decided: ChangeRow[];
  counts: ApprovalCounts;
  policy: Policy;
}

export const POLICY_ROWS: readonly { key: PolicyKey; label: string; hint: string }[] = [
  { key: "project", label: "프로젝트 자원의 변경", hint: "프로젝트 안의 스킬과 에이전트 정의(지침, 설명)를 AI가 고칠 때" },
  { key: "global", label: "글로벌 자원의 변경", hint: "여러 프로젝트가 함께 쓰는 자원을 AI가 고칠 때. 영향이 넓어요" },
  { key: "refs", label: "참조 끄기", hint: "프로젝트가 글로벌 자원을 쓰지 않게 AI가 바꿀 때" },
  { key: "permission", label: "권한 내림", hint: "에이전트의 권한을 AI가 낮출 때" },
];

export const FIXED_NOTE =
  "권한 올림, 도구 칸 변경, 참조 켜기는 이 설정과 상관없이 늘 사람의 승인이 필요해요. 사람이 앱에서 직접 고친 것은 늘 바로 적용돼요.";

export const POLICY_LABEL: Record<PolicyValue, string> = { auto: "바로 적용", approve: "승인 필요" };

export const STATUS_LABEL: Record<ChangeStatus, string> = {
  pending: "승인 대기",
  approved: "승인함",
  rejected: "거절함",
  stale: "적용 못 함",
};

// ---------------------------------------------------------------- 읽기

const isRecord = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
const text = (v: unknown): string | undefined => (typeof v === "string" && v !== "" ? v : undefined);
const strings = (v: unknown): string[] => (Array.isArray(v) ? v.filter((s): s is string => typeof s === "string") : []);
const STATUSES = new Set<string>(["pending", "approved", "rejected", "stale"]);

export function toView(v: unknown): ResourceView | undefined {
  if (!isRecord(v)) return undefined;
  const id = text(v.id), name = text(v.name);
  const kind = v.kind === "skill" || v.kind === "agent" ? v.kind : null;
  const scope = v.scope === "global" || v.scope === "project" ? v.scope : null;
  if (!id || !name || !kind || !scope) return undefined;
  return {
    id, kind, scope, name,
    project: text(v.project),
    description: typeof v.description === "string" ? v.description : "",
    permission: text(v.permission),
    skills: Array.isArray(v.skills) ? strings(v.skills) : undefined,
    repo: text(v.repo),
    tools: isRecord(v.tools) ? v.tools : undefined,
    body: typeof v.body === "string" ? v.body : "",
  };
}

const toReasons = (v: unknown): Reason[] =>
  (Array.isArray(v) ? v : []).filter(isRecord).map((r) => ({ category: text(r.category) ?? "", label: text(r.label) ?? "", fixed: r.fixed === true })).filter((r) => r.category);

/** 엔진의 변경 한 줄을 읽는다. 모양이 틀리면 null이다. */
export function toChange(v: unknown): ChangeRow | null {
  if (!isRecord(v)) return null;
  const id = text(v.id), status = text(v.status);
  if (!id || !status || !STATUSES.has(status)) return null;
  const target = isRecord(v.target) ? v.target : {};
  const proposer = isRecord(v.proposer) ? v.proposer : {};
  return {
    id,
    status: status as ChangeStatus,
    command: text(v.command) ?? "",
    createdAt: text(v.createdAt) ?? "",
    summary: text(v.summary) ?? "스킬·에이전트 변경",
    target: { kind: text(target.kind), scope: text(target.scope), project: text(target.project), id: text(target.id), name: text(target.name) },
    proposer: { session: text(proposer.session), agent: text(proposer.agent), channel: text(proposer.channel) },
    fixed: v.fixed === true,
    reasons: toReasons(v.reasons),
    decidedAt: text(v.decidedAt),
    decidedBy: text(v.decidedBy),
    reason: text(v.reason),
  };
}

export function toDetail(v: unknown): ChangeDetail | null {
  const row = toChange(v);
  if (!row || !isRecord(v)) return null;
  const ref = isRecord(v.ref) ? { project: text(v.ref.project) ?? "", added: strings(v.ref.added), removed: strings(v.ref.removed) } : undefined;
  return { ...row, before: toView(v.before), after: toView(v.after), ref, changedFields: strings(v.changedFields) };
}

export function toUnapproved(v: unknown): Unapproved | null {
  if (!isRecord(v)) return null;
  const key = text(v.key);
  const type = v.type === "resource" || v.type === "refs" ? v.type : null;
  const state = v.state === "dirty" || v.state === "new" ? v.state : null;
  if (!key || !type || !state) return null;
  const side = (x: unknown): ResourceView | string[] | undefined => (Array.isArray(x) ? strings(x) : toView(x));
  return {
    key, type, state,
    id: text(v.id), kind: text(v.kind), scope: text(v.scope), project: text(v.project), name: text(v.name), path: text(v.path),
    approved: side(v.approved), live: side(v.live),
  };
}

export function toPolicy(v: unknown): Policy {
  const out: Policy = { ...DEFAULT_POLICY };
  if (!isRecord(v)) return out;
  for (const k of Object.keys(out) as PolicyKey[]) {
    if (v[k] === "auto" || v[k] === "approve") out[k] = v[k];
  }
  return out;
}

const count = (v: unknown): number => (typeof v === "number" && Number.isFinite(v) && v > 0 ? Math.floor(v) : 0);

export function toCounts(v: unknown): ApprovalCounts {
  const o = isRecord(v) ? v : {};
  return { pending: count(o.pending), unapproved: count(o.unapproved) };
}

const rows = <T>(v: unknown, read: (x: unknown) => T | null): T[] => (Array.isArray(v) ? v.map(read).filter((r): r is T => r !== null) : []);

export function toOverview(out: unknown): ApprovalsOverview {
  const o = isRecord(out) ? out : {};
  const pending = rows(o.pending, toChange);
  const unapproved = rows(o.unapproved, toUnapproved);
  return {
    pending, unapproved,
    decided: rows(o.decided, toChange),
    counts: isRecord(o.counts) ? toCounts(o.counts) : { pending: pending.length, unapproved: unapproved.length },
    policy: toPolicy(o.policy),
  };
}

// ---------------------------------------------------------------- 엔진 호출

export async function loadApprovals(call: EngineCall, decided = 20): Promise<ApprovalsOverview> {
  return toOverview(await call(["list-resource-changes", `decided=${decided}`]));
}

/** 알림과 배지가 쓰는 수만 가볍게 읽는다. */
export async function loadCounts(call: EngineCall): Promise<ApprovalCounts> {
  const out = await call(["list-resource-changes", "decided=0"]);
  return isRecord(out) && isRecord(out.counts) ? toCounts(out.counts) : toOverview(out).counts;
}

export async function getChange(call: EngineCall, id: string): Promise<ChangeDetail> {
  const detail = toDetail(await call(["get-resource-change", id]));
  if (!detail) throw new Error("변경 응답의 모양이 틀려요.");
  return detail;
}

export async function approveChange(call: EngineCall, id: string): Promise<void> {
  await call(["approve-resource-change", id]);
}

export async function rejectChange(call: EngineCall, id: string, reason: string): Promise<void> {
  const argv = ["reject-resource-change", id];
  if (reason.trim()) argv.push(`reason=${reason.trim()}`);
  await call(argv);
}

export type ResolveAction = "approve" | "revert";

export async function resolveEdit(call: EngineCall, key: string, action: ResolveAction): Promise<void> {
  await call(["resolve-resource-edit", key, `action=${action}`]);
}

export async function savePolicy(call: EngineCall, policy: Policy): Promise<Policy> {
  const out = await call(["set-resource-policy", `project=${policy.project}`, `global=${policy.global}`, `refs=${policy.refs}`, `permission=${policy.permission}`]);
  return toPolicy(isRecord(out) ? out.policy : undefined);
}

// ---------------------------------------------------------------- 표시

/** 배지에 쓸 수: 승인을 기다리는 변경과 승인 안 된 직접 수정의 합. */
export const badgeTotal = (c: ApprovalCounts): number => c.pending + c.unapproved;

/** 배지 글. 0이면 빈 글, 99를 넘으면 99+다. */
export const badgeText = (c: ApprovalCounts): string => {
  const n = badgeTotal(c);
  return n === 0 ? "" : n > 99 ? "99+" : String(n);
};

/** 누가 올렸는지 사람이 읽을 글. 세션이 있으면 그 세션, 없으면 통로(MCP)로 말한다. */
export function proposerLabel(p: ChangeRow["proposer"]): string {
  const agent = p.agent ? p.agent[0].toUpperCase() + p.agent.slice(1) : "Claude";
  if (p.session) return `${agent} 세션 ${p.session.slice(0, 8)}`;
  return p.channel === "mcp" ? "MCP로 연결된 AI" : "AI";
}

const FIELD_LABELS: Record<string, string> = {
  name: "이름", description: "설명", permission: "권한", skills: "쓰는 스킬", repo: "저장소", tools: "도구별 칸", body: "지침",
};

export interface FieldDiff {
  key: string;
  label: string;
  before: string;
  after: string;
  /** body는 줄 단위 비교, 나머지는 한 줄 비교 */
  kind: "line" | "lines";
}

const show = (key: string, v: ResourceView | undefined): string => {
  if (!v) return "";
  switch (key) {
    case "skills": return (v.skills ?? []).join(", ");
    case "tools": return v.tools && Object.keys(v.tools).length ? JSON.stringify(v.tools, null, 2) : "";
    case "permission": return v.permission ?? "";
    case "repo": return v.repo ?? "";
    case "name": return v.name;
    case "description": return v.description;
    default: return v.body;
  }
};

/**
 * 이전 판과 새 판에서 달라진 칸을 낸다. 만들기(이전 판 없음)는 새 판의 채워진 칸 전부, 지우기(새 판 없음)는 이전 판의 채워진 칸 전부다.
 * 에이전트가 아니면 권한·스킬·저장소 칸은 비교하지 않는다.
 */
export function fieldDiffs(before: ResourceView | undefined, after: ResourceView | undefined): FieldDiff[] {
  const any = after ?? before;
  if (!any) return [];
  const keys = ["name", "description", ...(any.kind === "agent" ? ["permission", "skills", "repo"] : []), "tools", "body"];
  const out: FieldDiff[] = [];
  for (const key of keys) {
    const b = show(key, before), a = show(key, after);
    if (b === a) continue;
    out.push({ key, label: FIELD_LABELS[key], before: b, after: a, kind: key === "body" || key === "tools" ? "lines" : "line" });
  }
  return out;
}

export type DiffKind = "same" | "change" | "add" | "del";

export interface DiffRow {
  kind: DiffKind;
  left: string | null;
  right: string | null;
}

/** 줄 수의 곱이 이보다 크면 정확한 비교 대신 통째로 바뀐 것으로 본다(화면이 멈추지 않게). */
export const DIFF_LIMIT = 4_000_000;

/**
 * 두 글을 줄 단위로 나란히 견준다(가장 긴 공통 부분열). 같은 줄은 양쪽에, 바뀐 덩어리는 위아래로 짝지어 보이고 남는 쪽은 비운다.
 * 빈 글은 줄이 없는 것으로 본다.
 */
type Op = { op: "same" | "del" | "add"; t: string };

export function sideBySide(a: string, b: string): DiffRow[] {
  const x = a === "" ? [] : a.split("\n");
  const y = b === "" ? [] : b.split("\n");
  if (x.length * y.length > DIFF_LIMIT) return pair([...x.map((t): Op => ({ op: "del", t })), ...y.map((t): Op => ({ op: "add", t }))]);
  const n = x.length, m = y.length;
  const table: Uint32Array[] = Array.from({ length: n + 1 }, () => new Uint32Array(m + 1));
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      table[i][j] = x[i] === y[j] ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
    }
  }
  const ops: Op[] = [];
  let i = 0, j = 0;
  while (i < n && j < m) {
    if (x[i] === y[j]) {
      ops.push({ op: "same", t: x[i] });
      i++; j++;
    } else if (table[i + 1][j] >= table[i][j + 1]) {
      ops.push({ op: "del", t: x[i++] });
    } else {
      ops.push({ op: "add", t: y[j++] });
    }
  }
  while (i < n) ops.push({ op: "del", t: x[i++] });
  while (j < m) ops.push({ op: "add", t: y[j++] });
  return pair(ops);
}

function pair(ops: Op[]): DiffRow[] {
  const out: DiffRow[] = [];
  let k = 0;
  while (k < ops.length) {
    if (ops[k].op === "same") {
      out.push({ kind: "same", left: ops[k].t, right: ops[k].t });
      k++;
      continue;
    }
    const dels: string[] = [], adds: string[] = [];
    while (k < ops.length && ops[k].op !== "same") {
      (ops[k].op === "del" ? dels : adds).push(ops[k].t);
      k++;
    }
    for (let r = 0; r < Math.max(dels.length, adds.length); r++) {
      const left = r < dels.length ? dels[r] : null, right = r < adds.length ? adds[r] : null;
      out.push({ kind: left !== null && right !== null ? "change" : left !== null ? "del" : "add", left, right });
    }
  }
  return out;
}

/** 변경이 닿는 자원을 사람이 읽을 글로. */
export function targetLabel(t: ChangeRow["target"]): string {
  const scope = t.scope === "global" ? "글로벌" : t.scope === "project" ? `프로젝트${t.project ? ` ${t.project}` : ""}` : "";
  const kind = t.kind === "skill" ? "스킬" : t.kind === "agent" ? "에이전트" : "";
  return [scope, kind, t.name].filter(Boolean).join(" · ");
}
