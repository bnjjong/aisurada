// 승인 대기함(#53). AI가 쓴 스킬·에이전트 변경이 정책에 따라 여기에 쌓이고, 사람이 이전 판과 나란히 견주어 보고 승인하거나 거절한다.
// 승인하기 전에는 목록·AI의 조회·실행 입력 어디에도 반영되지 않는다(엔진이 지킨다). 파일을 손으로 고친 것(승인 안 된 직접 수정)도 여기서 받아들이거나 되돌린다.
// 그리는 부분(`ApprovalsView`)과 엔진을 부르는 부분(`ApprovalsDialog`)을 나눠서, 그리는 부분은 서버 렌더로 시험한다.
import { useCallback, useEffect, useState } from "react";
import { Modal } from "./Modal.tsx";
import {
  FIXED_NOTE, POLICY_LABEL, POLICY_ROWS, STATUS_LABEL, approveChange, badgeText, fieldDiffs, getChange, loadApprovals, proposerLabel,
  rejectChange, resolveEdit, savePolicy, sideBySide, targetLabel,
} from "./approvalsClient.ts";
import type {
  ApprovalsOverview, ChangeDetail, ChangeRow, DiffRow, FieldDiff, Policy, PolicyKey, PolicyValue, ResolveAction, ResourceView, Unapproved,
} from "./approvalsClient.ts";
import { setApprovalCounts } from "./approvalsStore.ts";
import { formatRelativeTime } from "./dates.ts";
import { explainError } from "./resourcesClient.ts";
import type { EngineCall } from "./resourcesClient.ts";

export type ApprovalsTab = "pending" | "direct" | "decided" | "policy";

const TABS: readonly { id: ApprovalsTab; label: string }[] = [
  { id: "pending", label: "승인 대기" },
  { id: "direct", label: "직접 수정" },
  { id: "decided", label: "지난 결정" },
  { id: "policy", label: "정책" },
];

const when = (iso: string | undefined): string => (iso ? formatRelativeTime(Date.parse(iso) || iso) : "");

/** 자원 id를 이름으로 푼다. 모르면 id를 그대로 보인다. */
const nameOf = (names: Record<string, string>, id: string): string => names[id] ?? id;

// ---------------------------------------------------------------- 비교

function Lines({ rows }: { rows: DiffRow[] }) {
  return (
    <table className="apv-diff" aria-label="줄 단위 비교">
      <thead>
        <tr>
          <th scope="col">이전 판</th>
          <th scope="col">새 판</th>
        </tr>
      </thead>
      <tbody>
        {rows.map((r, i) => (
          <tr key={i} className={`apv-row apv-row-${r.kind}`}>
            <td>{r.left === null ? "" : r.left || " "}</td>
            <td>{r.right === null ? "" : r.right || " "}</td>
          </tr>
        ))}
      </tbody>
    </table>
  );
}

function Field({ diff }: { diff: FieldDiff }) {
  if (diff.kind === "line") {
    return (
      <div className="apv-field" data-field={diff.key}>
        <h4>{diff.label}</h4>
        <p className="apv-oneline">
          <del>{diff.before || "(비어 있음)"}</del>
          <span aria-hidden="true"> → </span>
          <ins>{diff.after || "(비어 있음)"}</ins>
        </p>
      </div>
    );
  }
  return (
    <div className="apv-field" data-field={diff.key}>
      <h4>{diff.label}</h4>
      <Lines rows={sideBySide(diff.before, diff.after)} />
    </div>
  );
}

/** 달라진 칸을 보인다. 만들기와 지우기는 그렇다고 먼저 말한다. */
export function Compare({ before, after, note }: { before?: ResourceView; after?: ResourceView; note?: string }) {
  const diffs = fieldDiffs(before, after);
  return (
    <div className="apv-compare">
      {note && <p className="apv-note">{note}</p>}
      {diffs.length === 0 ? <p className="apv-note">달라진 칸이 없어요.</p> : diffs.map((d) => <Field key={d.key} diff={d} />)}
    </div>
  );
}

function RefCompare({ added, removed, project, names }: { added: string[]; removed: string[]; project: string; names: Record<string, string> }) {
  return (
    <div className="apv-compare">
      <p className="apv-note">프로젝트 「{project}」의 글로벌 자원 참조를 바꿔요.</p>
      {added.length > 0 && (
        <div className="apv-field">
          <h4>켜는 것</h4>
          <ul>{added.map((id) => <li key={id}><ins>{nameOf(names, id)}</ins></li>)}</ul>
        </div>
      )}
      {removed.length > 0 && (
        <div className="apv-field">
          <h4>끄는 것</h4>
          <ul>{removed.map((id) => <li key={id}><del>{nameOf(names, id)}</del></li>)}</ul>
        </div>
      )}
    </div>
  );
}

function DetailBody({ detail, names }: { detail: ChangeDetail; names: Record<string, string> }) {
  if (detail.ref) return <RefCompare added={detail.ref.added} removed={detail.ref.removed} project={detail.ref.project} names={names} />;
  const note = !detail.before ? (detail.command === "restore-resource" ? "휴지통에서 이 내용으로 되살려요." : "새로 만들어요.") : !detail.after ? "지워요(휴지통으로 가요)." : undefined;
  return <Compare before={detail.before} after={detail.after} note={note} />;
}

// ---------------------------------------------------------------- 탭

function Reasons({ change }: { change: ChangeRow }) {
  return (
    <span className="apv-chips">
      {change.reasons.map((r) => (
        <span key={r.category} className={`apv-chip${r.fixed ? " apv-fixed" : ""}`} title={r.fixed ? "정책과 상관없이 늘 승인이 필요한 종류예요." : undefined}>
          {r.label}
        </span>
      ))}
    </span>
  );
}

export interface ViewProps {
  overview: ApprovalsOverview;
  tab: ApprovalsTab;
  onTab: (tab: ApprovalsTab) => void;
  /** 펼쳐서 보는 변경의 상세. 아직 못 읽었으면 "loading" */
  details: Record<string, ChangeDetail | "loading">;
  /** 비교를 펼친 항목의 키(변경 id 또는 직접 수정의 key) */
  open: ReadonlySet<string>;
  onToggle: (key: string) => void;
  names: Record<string, string>;
  /** 지금 처리하는 중인 항목의 키. 그동안 단추를 잠근다 */
  busy: string | null;
  rejecting: string | null;
  rejectText: string;
  onRejectText: (text: string) => void;
  onRejectOpen: (id: string | null) => void;
  onApprove: (id: string) => void;
  onReject: (id: string) => void;
  onResolve: (key: string, action: ResolveAction) => void;
  onPolicy: (key: PolicyKey, value: PolicyValue) => void;
  error: string;
}

function PendingTab(p: ViewProps) {
  const list = p.overview.pending;
  if (list.length === 0) return <p className="apv-empty">승인을 기다리는 변경이 없어요.</p>;
  return (
    <ul className="apv-list">
      {list.map((c) => {
        const detail = p.details[c.id];
        const isOpen = p.open.has(c.id);
        const working = p.busy === c.id;
        return (
          <li key={c.id} className="apv-card" data-change-id={c.id}>
            <header>
              <strong className="apv-summary">{c.summary}</strong>
              <Reasons change={c} />
            </header>
            <p className="apv-meta">
              {proposerLabel(c.proposer)}가 올림 · {when(c.createdAt)}
            </p>
            {isOpen && (detail && detail !== "loading" ? <DetailBody detail={detail} names={p.names} /> : <p className="apv-note">불러오는 중이에요…</p>)}
            {p.rejecting === c.id ? (
              <div className="apv-reject">
                <label>
                  <span>거절 이유(선택)</span>
                  <textarea rows={2} value={p.rejectText} onChange={(e) => p.onRejectText(e.target.value)} placeholder="AI가 다음에 참고하도록 남겨요." />
                </label>
                <div className="apv-actions">
                  <button type="button" onClick={() => p.onRejectOpen(null)}>취소</button>
                  <button type="button" className="button-primary" disabled={working} onClick={() => p.onReject(c.id)}>거절하기</button>
                </div>
              </div>
            ) : (
              <div className="apv-actions">
                <button type="button" aria-expanded={isOpen} onClick={() => p.onToggle(c.id)}>{isOpen ? "비교 접기" : "이전 판과 비교"}</button>
                <button type="button" disabled={working} onClick={() => p.onRejectOpen(c.id)}>거절</button>
                <button type="button" className="button-primary" disabled={working} onClick={() => p.onApprove(c.id)}>승인</button>
              </div>
            )}
          </li>
        );
      })}
    </ul>
  );
}

const STATE_TEXT: Record<Unapproved["state"], string> = {
  dirty: "승인한 뒤에 파일이 직접 고쳐졌어요. 지금 쓰이는 것은 승인해 둔 판이에요.",
  new: "승인 기록 없이 생긴 새 파일이에요. 승인하기 전에는 어디에서도 쓰이지 않아요.",
};

function UnapprovedCompare({ item, names }: { item: Unapproved; names: Record<string, string> }) {
  if (item.type === "refs") {
    const approved = Array.isArray(item.approved) ? item.approved : [];
    const live = Array.isArray(item.live) ? item.live : [];
    return <RefCompare added={live.filter((id) => !approved.includes(id))} removed={approved.filter((id) => !live.includes(id))} project={item.project ?? ""} names={names} />;
  }
  const approved = Array.isArray(item.approved) ? undefined : item.approved;
  const live = Array.isArray(item.live) ? undefined : item.live;
  return <Compare before={approved} after={live} note={item.state === "new" ? "새 파일의 내용이에요." : undefined} />;
}

function DirectTab(p: ViewProps) {
  const list = p.overview.unapproved;
  if (list.length === 0) return <p className="apv-empty">승인 안 된 직접 수정이 없어요.</p>;
  return (
    <ul className="apv-list">
      {list.map((u) => {
        const isOpen = p.open.has(u.key);
        const working = p.busy === u.key;
        const title = u.type === "refs" ? `프로젝트 「${u.project}」의 참조 목록` : [u.scope === "global" ? "글로벌" : `프로젝트 ${u.project ?? ""}`.trim(), u.kind === "skill" ? "스킬" : "에이전트", u.name].filter(Boolean).join(" · ");
        return (
          <li key={u.key} className="apv-card" data-unapproved-key={u.key}>
            <header>
              <strong className="apv-summary">{title}</strong>
              <span className="apv-chips"><span className="apv-chip apv-fixed">{u.state === "dirty" ? "직접 고쳐짐" : "새 파일"}</span></span>
            </header>
            <p className="apv-meta">{STATE_TEXT[u.state]}</p>
            {isOpen && <UnapprovedCompare item={u} names={p.names} />}
            <div className="apv-actions">
              <button type="button" aria-expanded={isOpen} onClick={() => p.onToggle(u.key)}>{isOpen ? "비교 접기" : "비교"}</button>
              <button type="button" disabled={working} onClick={() => p.onResolve(u.key, "revert")}>{u.state === "dirty" ? "승인한 판으로 되돌리기" : "지우기(휴지통으로)"}</button>
              <button type="button" className="button-primary" disabled={working} onClick={() => p.onResolve(u.key, "approve")}>이 내용을 승인</button>
            </div>
          </li>
        );
      })}
    </ul>
  );
}

function DecidedTab(p: ViewProps) {
  const list = p.overview.decided;
  if (list.length === 0) return <p className="apv-empty">아직 결정한 변경이 없어요.</p>;
  return (
    <ul className="apv-list">
      {list.map((c) => (
        <li key={c.id} className="apv-card apv-decided" data-change-id={c.id} data-status={c.status}>
          <header>
            <strong className="apv-summary">{c.summary}</strong>
            <span className={`apv-chip apv-status-${c.status}`}>{STATUS_LABEL[c.status]}</span>
          </header>
          <p className="apv-meta">
            {targetLabel(c.target) && `${targetLabel(c.target)} · `}
            {proposerLabel(c.proposer)}가 올림 · {when(c.createdAt)}
            {c.decidedAt && ` · ${when(c.decidedAt)} 결정`}
          </p>
          {c.reason && <p className="apv-note">{c.reason}</p>}
        </li>
      ))}
    </ul>
  );
}

function PolicyTab(p: ViewProps) {
  return (
    <section className="apv-policy" aria-label="승인 정책">
      <p className="apv-note">AI가 보낸 변경을 바로 적용할지, 사람의 승인을 거칠지 범위별로 정해요.</p>
      {POLICY_ROWS.map((row) => (
        <label key={row.key} className="apv-policy-row">
          <span>
            <strong>{row.label}</strong>
            <small>{row.hint}</small>
          </span>
          <select aria-label={row.label} value={p.overview.policy[row.key]} onChange={(e) => p.onPolicy(row.key, e.target.value as PolicyValue)}>
            {(["approve", "auto"] as const).map((v) => <option key={v} value={v}>{POLICY_LABEL[v]}</option>)}
          </select>
        </label>
      ))}
      <p className="apv-note apv-fixed-note">{FIXED_NOTE}</p>
    </section>
  );
}

/** 승인 대기함의 속(그리기만 한다). */
export function ApprovalsView(p: ViewProps) {
  const { counts } = p.overview;
  const tabCount: Record<ApprovalsTab, number> = { pending: counts.pending, direct: counts.unapproved, decided: 0, policy: 0 };
  return (
    <div className="apv">
      <div className="apv-tabs" role="tablist" aria-label="승인 대기함">
        {TABS.map((t) => (
          <button key={t.id} type="button" role="tab" aria-selected={p.tab === t.id} onClick={() => p.onTab(t.id)}>
            {t.label}
            {tabCount[t.id] > 0 && <span className="apv-count">{badgeText({ pending: tabCount[t.id], unapproved: 0 })}</span>}
          </button>
        ))}
      </div>
      {p.error && <p role="alert" className="apv-error">{p.error}</p>}
      {p.tab === "pending" && <PendingTab {...p} />}
      {p.tab === "direct" && <DirectTab {...p} />}
      {p.tab === "decided" && <DecidedTab {...p} />}
      {p.tab === "policy" && <PolicyTab {...p} />}
    </div>
  );
}

// ---------------------------------------------------------------- 대화상자

/**
 * 승인 대기함 창. 엔진에서 목록을 읽고, 승인·거절·직접 수정 처리·정책 변경을 엔진에 보낸 뒤 다시 읽는다.
 * `names`는 참조 변경에서 자원 id를 이름으로 보이려는 것이고, `onChanged`는 승인으로 자원이 바뀌었을 때 호출한 쪽이 목록을 다시 읽게 한다.
 */
export function ApprovalsDialog({ call, names, close, onChanged }: { call: EngineCall; names: Record<string, string>; close: () => void; onChanged?: () => void }) {
  const [overview, setOverview] = useState<ApprovalsOverview | null>(null);
  const [tab, setTab] = useState<ApprovalsTab>("pending");
  const [details, setDetails] = useState<Record<string, ChangeDetail | "loading">>({});
  const [open, setOpen] = useState<ReadonlySet<string>>(new Set());
  const [busy, setBusy] = useState<string | null>(null);
  const [rejecting, setRejecting] = useState<string | null>(null);
  const [rejectText, setRejectText] = useState("");
  const [error, setError] = useState("");

  const reload = useCallback(async () => {
    const o = await loadApprovals(call, 20);
    setOverview(o);
    setApprovalCounts(o.counts);
    return o;
  }, [call]);

  useEffect(() => {
    reload().then((o) => setTab(o.pending.length === 0 && o.unapproved.length > 0 ? "direct" : "pending"), (e) => setError(explainError(e)));
  }, [reload]);

  const act = async (key: string, job: () => Promise<void>) => {
    setBusy(key);
    try {
      await job();
      setError("");
    } catch (e) {
      setError(explainError(e));
    }
    try {
      await reload();
    } catch (e) {
      setError(explainError(e));
    }
    setOpen((s) => new Set([...s].filter((k) => k !== key)));
    setBusy(null);
    onChanged?.();
  };

  const toggle = (key: string) => {
    const next = new Set(open);
    if (next.has(key)) {
      next.delete(key);
    } else {
      next.add(key);
      if (key.startsWith("pc") && !details[key]) {
        setDetails((d) => ({ ...d, [key]: "loading" }));
        getChange(call, key).then(
          (detail) => setDetails((d) => ({ ...d, [key]: detail })),
          (e) => {
            setError(explainError(e));
            setDetails((d) => {
              const { [key]: _gone, ...rest } = d;
              return rest;
            });
          },
        );
      }
    }
    setOpen(next);
  };

  return (
    <Modal title="승인 대기함" close={close} wide className="apv-modal">
      {overview === null ? (
        <p className="apv-empty" role="status">{error || "불러오는 중이에요…"}</p>
      ) : (
        <ApprovalsView
          overview={overview}
          tab={tab}
          onTab={setTab}
          details={details}
          open={open}
          onToggle={toggle}
          names={names}
          busy={busy}
          rejecting={rejecting}
          rejectText={rejectText}
          onRejectText={setRejectText}
          onRejectOpen={(id) => {
            setRejecting(id);
            setRejectText("");
          }}
          onApprove={(id) => void act(id, () => approveChange(call, id))}
          onReject={(id) =>
            void act(id, async () => {
              await rejectChange(call, id, rejectText);
              setRejecting(null);
            })
          }
          onResolve={(key, action) => void act(key, () => resolveEdit(call, key, action))}
          onPolicy={(key, value) => {
            const next: Policy = { ...overview.policy, [key]: value };
            setOverview({ ...overview, policy: next });
            void savePolicy(call, next).then(
              (saved) => {
                setOverview((o) => (o ? { ...o, policy: saved } : o));
                setError("");
              },
              (e) => {
                setError(explainError(e));
                void reload();
              },
            );
          }}
          error={error}
        />
      )}
    </Modal>
  );
}
