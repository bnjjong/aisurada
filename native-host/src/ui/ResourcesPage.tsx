// 글로벌 자원 페이지: 어느 프로젝트든 참조할 수 있는 공용 스킬과 에이전트 정의를 만들고 고치고 지운다.
// 프로젝트는 여기 있는 것을 「참조」해야 쓸 수 있다(프로젝트 페이지에서 켠다). 참조하지 않은 것은 어느 프로젝트도 쓰지 않는다.
import { useCallback, useEffect, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import { ApprovalsDialog } from "./ApprovalPanel.tsx";
import { badgeTotal } from "./approvalsClient.ts";
import { refreshApprovalCounts, useApprovalCounts } from "./approvalsStore.ts";
import { Confirm } from "./Modal.tsx";
import { Page } from "./OtherPages.tsx";
import { ResourceFormModal } from "./ResourceFormModal.tsx";
import { ResourceItem } from "./ResourceItem.tsx";
import {
  deleteResource, explainError, getResource, kindLabel, listResources, listTrash, restoreResource, selectableSkills,
} from "./resourcesClient.ts";
import type { ResourceKind, ResourceRow, TrashRow } from "./resourcesClient.ts";

export function ResourcesPage({ controller }: { controller: WorkspaceController }) {
  const call = controller.callEngine;
  const [rows, setRows] = useState<ResourceRow[] | null>(null);
  const [trash, setTrash] = useState<TrashRow[]>([]);
  const [skipped, setSkipped] = useState<string[]>([]);
  const [error, setError] = useState("");
  const [form, setForm] = useState<{ kind: ResourceKind; existing?: ResourceRow } | null>(null);
  const [confirm, setConfirm] = useState<ResourceRow | null>(null);
  const [approvals, setApprovals] = useState(false);
  const counts = useApprovalCounts();

  const reload = useCallback(async () => {
    try {
      const [list, gone] = await Promise.all([listResources(call, { global: true }), listTrash(call)]);
      setRows(list.resources);
      setSkipped(list.skipped);
      setTrash(gone.filter((t) => t.scope === "global"));
      setError("");
      void refreshApprovalCounts(call);
    } catch (e) {
      setError(explainError(e));
      setRows((prev) => prev ?? []);
    }
  }, [call]);

  // AI나 다른 창이 고친 것을 따라가도록 창이 다시 눈에 띌 때 다시 읽는다
  useEffect(() => {
    void reload();
    const onFocus = () => void reload();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [reload]);

  const guard = async (job: () => Promise<void>) => {
    try {
      await job();
    } catch (e) {
      setError(explainError(e));
    }
  };

  const edit = (row: ResourceRow) =>
    guard(async () => {
      setForm({ kind: row.kind, existing: await getResource(call, row.id) });
    });
  const askDelete = (row: ResourceRow) =>
    guard(async () => {
      setConfirm(await getResource(call, row.id));
    });

  const globals = rows ?? [];
  const skills = globals.filter((r) => r.kind === "skill");
  const agents = globals.filter((r) => r.kind === "agent");
  const usedBy = confirm?.referencedBy ?? [];

  const section = (kind: ResourceKind, list: ResourceRow[]) => (
    <section className="res-section" aria-label={kindLabel(kind)}>
      <header className="res-section-head">
        <h2>{kindLabel(kind)}</h2>
        <button type="button" className="button-primary" onClick={() => setForm({ kind })}>
          새 {kindLabel(kind)}
        </button>
      </header>
      {rows === null ? (
        <p className="res-empty">불러오는 중이에요…</p>
      ) : list.length === 0 ? (
        <p className="res-empty">아직 글로벌 {kindLabel(kind)}이 없어요.</p>
      ) : (
        <ul className="res-list">
          {list.map((r) => (
            <ResourceItem
              key={r.id}
              row={r}
              badge={r.approval === "dirty" ? <span className="res-kind apv-chip apv-fixed" title="승인한 뒤에 파일이 직접 고쳐졌어요. 보이는 내용은 승인해 둔 판이에요.">직접 고쳐짐</span> : undefined}
              actions={
                <>
                  <button type="button" onClick={() => void edit(r)}>
                    편집
                  </button>
                  <button type="button" onClick={() => void askDelete(r)}>
                    삭제
                  </button>
                </>
              }
            />
          ))}
        </ul>
      )}
    </section>
  );

  return (
    <Page title="스킬·에이전트" className="res-page">
      <p className="res-intro">
        여러 프로젝트가 함께 쓰는 <b>글로벌</b> 스킬과 에이전트예요. 프로젝트가 <b>참조</b>해야 그 프로젝트에서 쓸 수 있고, 참조하지 않은 것은
        어느 프로젝트도 쓰지 않아요. 참조는 프로젝트 페이지에서 켜요. 스킬과 에이전트는 특정 AI 도구에 묶이지 않아요.
      </p>
      {badgeTotal(counts) > 0 && (
        <div className="apv-banner" role="status">
          <span>
            <strong>승인이 필요한 변경이 {badgeTotal(counts)}건 있어요.</strong> AI가 고친 스킬·에이전트는 사람이 승인해야 적용돼요. 그때까지는 이전 판이 쓰여요.
          </span>
          <button type="button" className="button-primary" onClick={() => setApprovals(true)}>
            승인 대기함 열기
          </button>
        </div>
      )}
      <div className="res-toolbar">
        <button type="button" onClick={() => void reload()}>
          새로고침
        </button>
        <button type="button" onClick={() => setApprovals(true)}>
          승인 대기함{badgeTotal(counts) > 0 ? ` ${badgeTotal(counts)}` : ""}
        </button>
      </div>
      {error && <p role="alert" className="res-error">{error}</p>}
      {skipped.length > 0 && (
        <details className="res-skipped">
          <summary>읽지 못한 파일 {skipped.length}개</summary>
          <ul>
            {skipped.map((s) => (
              <li key={s}>{s}</li>
            ))}
          </ul>
        </details>
      )}
      {section("skill", skills)}
      {section("agent", agents)}
      {trash.length > 0 && (
        <section className="res-section" aria-label="휴지통">
          <header className="res-section-head">
            <h2>지운 것 {trash.length}</h2>
          </header>
          <ul className="res-list">
            {trash.map((t) => (
              <li key={t.id} className="res-item res-trashed">
                <div className="res-main">
                  <p className="res-title">
                    <strong className="res-name">{t.name}</strong>
                    <span className="res-kind">{kindLabel(t.kind)}</span>
                  </p>
                </div>
                <div className="res-actions">
                  <button type="button" onClick={() => void guard(async () => { await restoreResource(call, t.id); await reload(); })}>
                    되살리기
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </section>
      )}
      {approvals && (
        <ApprovalsDialog
          call={call}
          names={Object.fromEntries(globals.map((r) => [r.id, r.name]))}
          close={() => setApprovals(false)}
          onChanged={() => void reload()}
        />
      )}
      {form && (
        <ResourceFormModal
          call={call}
          kind={form.kind}
          existing={form.existing}
          skillChoices={selectableSkills({ globals: skills, projectSkills: [] })}
          repoNames={[]}
          close={() => setForm(null)}
          saved={() => {
            setForm(null);
            void reload();
          }}
        />
      )}
      {confirm && (
        <Confirm
          title={`${confirm.name} ${kindLabel(confirm.kind)}을 지울까요?`}
          description={
            usedBy.length
              ? `${usedBy.length}개 프로젝트(${usedBy.join(", ")})가 이 ${kindLabel(confirm.kind)}을 참조하고 있어요. 지우면 그 프로젝트에서 「깨진 참조」로 보여요. 휴지통에서 되살릴 수 있어요.`
              : "휴지통으로 옮겨요. 되살릴 수 있어요."
          }
          label="휴지통으로"
          close={() => setConfirm(null)}
          accept={() =>
            guard(async () => {
              await deleteResource(call, confirm.id);
              await reload();
            })
          }
        />
      )}
    </Page>
  );
}
