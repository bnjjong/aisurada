// 프로젝트 페이지: 이 프로젝트가 쓸 수 있는 스킬과 에이전트(프로젝트 자원 + 참조한 글로벌 자원)를 보고, 참조를 켜고 끄고,
// 프로젝트 자원을 만들고 고친다. 「쓸 수 있는 자원」은 엔진이 한 곳에서 계산한다(참조하지 않은 것은 여기에 안 나온다).
import { useCallback, useEffect, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { WorkspaceState } from "../workspace/types.ts";
import { asText } from "../workspace/types.ts";
import { Confirm, Modal } from "./Modal.tsx";
import { Page } from "./OtherPages.tsx";
import { ResourceFormModal } from "./ResourceFormModal.tsx";
import { ResourceItem } from "./ResourceItem.tsx";
import { parseRepos, repoLabel } from "./repoLinks.ts";
import {
  deleteResource, explainError, getResource, kindLabel, listResources, listTrash, restoreResource, scopeLabel, selectableSkills,
  setReference, skillsToReference, unreferencedGlobals, usableResources,
} from "./resourcesClient.ts";
import type { ResourceKind, ResourceRow, TrashRow, UsableResources } from "./resourcesClient.ts";

const EMPTY: UsableResources = { project: "", skills: [], agents: [], broken: [], missingSkills: [] };

export function ProjectPage({
  slug,
  state,
  controller,
}: {
  slug: string;
  state: WorkspaceState;
  controller: WorkspaceController;
}) {
  const call = controller.callEngine;
  const ws = state.workspace!;
  const project = ws.projects.find((p) => p.slug === slug || p.id === slug);
  const projectSlug = project ? asText(project.slug) : slug;
  const [usable, setUsable] = useState<UsableResources | null>(null);
  const [globals, setGlobals] = useState<ResourceRow[]>([]);
  const [trash, setTrash] = useState<TrashRow[]>([]);
  const [error, setError] = useState("");
  const [form, setForm] = useState<{ kind: ResourceKind; existing?: ResourceRow } | null>(null);
  const [confirm, setConfirm] = useState<ResourceRow | null>(null);
  const [refAsk, setRefAsk] = useState<{ agent: ResourceRow; skills: ResourceRow[] } | null>(null);

  const reload = useCallback(async () => {
    if (!project) return;
    try {
      const [u, g, gone] = await Promise.all([
        usableResources(call, projectSlug),
        listResources(call, { global: true }),
        listTrash(call),
      ]);
      setUsable(u);
      setGlobals(g.resources);
      setTrash(gone.filter((t) => t.scope === "project" && t.project === projectSlug));
      setError("");
    } catch (e) {
      setError(explainError(e));
      setUsable((prev) => prev ?? EMPTY);
    }
  }, [call, project, projectSlug]);

  useEffect(() => {
    void reload();
    const onFocus = () => void reload();
    window.addEventListener("focus", onFocus);
    return () => window.removeEventListener("focus", onFocus);
  }, [reload]);

  if (!project) return <Page title="이 프로젝트를 찾을 수 없어요" />;

  const guard = async (job: () => Promise<void>) => {
    try {
      await job();
    } catch (e) {
      setError(explainError(e));
    }
  };
  const refresh = async () => {
    await reload();
  };

  const u = usable ?? EMPTY;
  const repos = parseRepos(project.repos);
  const agendas = ws.agendas.filter((a) => a.project === projectSlug);
  const ownSkills = u.skills.filter((r) => r.source === "own");
  const globalSkills = globals.filter((r) => r.kind === "skill");
  const candidates = usable ? unreferencedGlobals(globals, u) : [];

  const edit = (row: ResourceRow) =>
    guard(async () => {
      setForm({ kind: row.kind, existing: await getResource(call, row.id) });
    });

  // 에이전트를 참조할 때는 그 에이전트가 쓰는 글로벌 스킬을 보여 주고 함께 참조할지 묻는다(확인이 곧 명시적인 참조다)
  const reference = (row: ResourceRow) =>
    guard(async () => {
      if (row.kind === "agent") {
        const full = await getResource(call, row.id);
        const need = skillsToReference(full, globals, u);
        if (need.length) {
          setRefAsk({ agent: full, skills: need });
          return;
        }
      }
      await setReference(call, projectSlug, row.id);
      await refresh();
    });

  const itemActions = (r: ResourceRow) =>
    r.source === "ref" ? (
      <button type="button" onClick={() => void guard(async () => { await setReference(call, projectSlug, r.id, { remove: true }); await refresh(); })}>
        참조 끄기
      </button>
    ) : (
      <>
        <button type="button" onClick={() => void edit(r)}>
          편집
        </button>
        <button type="button" onClick={() => setConfirm(r)}>
          삭제
        </button>
      </>
    );

  const badge = (r: ResourceRow) => (
    <span className={r.source === "ref" ? "res-badge res-badge-ref" : "res-badge"}>
      {r.source === "ref" ? "글로벌에서 참조" : "프로젝트 자원"}
    </span>
  );

  // 이 에이전트가 필요로 하지만 이 프로젝트가 쓸 수 없는 스킬
  const missingNote = (agent: ResourceRow) => {
    const list = u.missingSkills.filter((m) => m.agent.id === agent.id);
    if (!list.length) return null;
    return (
      <ul className="res-warns">
        {list.map((m) => (
          <li key={m.skillId} className="res-warn">
            <span>
              필요한 스킬 <b>{m.skill?.name ?? "찾을 수 없는 스킬"}</b>을 아직 쓸 수 없어요.
              {m.canReference ? " 참조하면 쓸 수 있어요." : " 지웠거나 다른 프로젝트의 것이에요."}
            </span>
            {m.canReference && (
              <button type="button" onClick={() => void guard(async () => { await setReference(call, projectSlug, m.skillId); await refresh(); })}>
                참조 추가
              </button>
            )}
          </li>
        ))}
      </ul>
    );
  };

  const list = (kind: ResourceKind, rows: ResourceRow[]) => (
    <section className="res-section" aria-label={`쓸 수 있는 ${kindLabel(kind)}`}>
      <header className="res-section-head">
        <h2>쓸 수 있는 {kindLabel(kind)}</h2>
        <button type="button" onClick={() => setForm({ kind })}>
          새 {kindLabel(kind)} 만들기
        </button>
      </header>
      {usable === null ? (
        <p className="res-empty">불러오는 중이에요…</p>
      ) : rows.length === 0 ? (
        <p className="res-empty">
          아직 없어요. 이 프로젝트만의 {kindLabel(kind)}을 만들거나, 아래에서 글로벌 {kindLabel(kind)}을 참조해 보세요.
        </p>
      ) : (
        <ul className="res-list">
          {rows.map((r) => (
            <ResourceItem key={r.id} row={r} badge={badge(r)} actions={itemActions(r)} notes={r.kind === "agent" ? missingNote(r) : null} />
          ))}
        </ul>
      )}
    </section>
  );

  const confirmSkills = refAsk?.skills ?? [];

  return (
    <Page title={asText(project.title)} className="res-page res-project">
      {asText(project.summary) && <p className="res-intro">{asText(project.summary)}</p>}
      <div className="res-toolbar">
        <button type="button" onClick={() => controller.form({ kind: "project", id: asText(project.id) })}>
          프로젝트 편집
        </button>
        <button type="button" onClick={() => void reload()}>
          새로고침
        </button>
      </div>
      {error && <p role="alert" className="res-error">{error}</p>}

      <section className="res-section" aria-label="저장소">
        <header className="res-section-head">
          <h2>저장소</h2>
        </header>
        {repos.length ? (
          <ul className="res-plain">
            {repos.map((r) => (
              <li key={r.name}>{repoLabel(r)}</li>
            ))}
          </ul>
        ) : (
          <p className="res-empty">등록한 저장소가 없어요. 프로젝트 편집에서 추가할 수 있어요.</p>
        )}
      </section>

      <section className="res-section" aria-label="아젠다">
        <header className="res-section-head">
          <h2>아젠다</h2>
        </header>
        {agendas.length ? (
          <ul className="res-plain">
            {agendas.map((a) => (
              <li key={a.id}>
                <button type="button" className="res-link" onClick={() => controller.navigate(`/agenda/${encodeURIComponent(a.id)}`)}>
                  {a.title}
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <p className="res-empty">이 프로젝트에 속한 아젠다가 없어요.</p>
        )}
      </section>

      <p className="res-rule">
        이 프로젝트가 쓸 수 있는 건 <b>프로젝트 자원</b>과 <b>참조한 글로벌 자원</b>뿐이에요. 참조하지 않은 것은 쓰지 않아요.
      </p>
      {list("skill", u.skills)}
      {list("agent", u.agents)}

      {u.broken.length > 0 && (
        <section className="res-section" aria-label="깨진 참조">
          <header className="res-section-head">
            <h2>깨진 참조</h2>
          </header>
          <ul className="res-warns">
            {u.broken.map((b) => (
              <li key={b.id} className="res-warn">
                <span>
                  참조하던 <b>{b.hint || b.id}</b>이 지워졌어요. 글로벌 페이지의 휴지통에서 되살리면 다시 이어져요.
                </span>
                <button type="button" onClick={() => void guard(async () => { await setReference(call, projectSlug, b.id, { remove: true }); await refresh(); })}>
                  참조 끄기
                </button>
              </li>
            ))}
          </ul>
        </section>
      )}

      {trash.length > 0 && (
        <section className="res-section" aria-label="지운 것">
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
                  <button type="button" onClick={() => void guard(async () => { await restoreResource(call, t.id); await refresh(); })}>
                    되살리기
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </section>
      )}

      <section className="res-section" aria-label="글로벌 자원 참조하기">
        <header className="res-section-head">
          <h2>글로벌 자원 참조하기</h2>
          <button type="button" onClick={() => controller.navigate("/resources")}>
            글로벌 페이지 열기
          </button>
        </header>
        {usable === null ? null : candidates.length === 0 ? (
          <p className="res-empty">
            {globals.length === 0 ? "글로벌 스킬과 에이전트가 아직 없어요." : "글로벌 자원을 모두 참조하고 있어요."}
          </p>
        ) : (
          <ul className="res-list">
            {candidates.map((g) => (
              <ResourceItem
                key={g.id}
                row={g}
                badge={<span className="res-badge res-badge-off">참조 안 함</span>}
                actions={
                  <button type="button" className="button-primary" onClick={() => void reference(g)}>
                    참조하기
                  </button>
                }
              />
            ))}
          </ul>
        )}
      </section>

      {form && (
        <ResourceFormModal
          call={call}
          kind={form.kind}
          project={projectSlug}
          existing={form.existing}
          skillChoices={selectableSkills({ project: projectSlug, globals: globalSkills, projectSkills: ownSkills })}
          repoNames={repos.map((r) => r.name)}
          close={() => setForm(null)}
          saved={() => {
            setForm(null);
            void refresh();
          }}
        />
      )}
      {confirm && (
        <Confirm
          title={`${scopeLabel(confirm)} ${kindLabel(confirm.kind)}을 지울까요?`}
          description="휴지통으로 옮겨요. 아래 「지운 것」에서 되살릴 수 있어요."
          label="휴지통으로"
          close={() => setConfirm(null)}
          accept={() =>
            guard(async () => {
              await deleteResource(call, confirm.id);
              await refresh();
            })
          }
        />
      )}
      {refAsk && (
        <Modal title={`${refAsk.agent.name} 참조`} close={() => setRefAsk(null)} className="res-ref-ask">
          <div className="res-ref-body">
            <p>
              이 에이전트는 아래 글로벌 스킬을 써요. 스킬도 함께 참조해야 이 프로젝트에서 에이전트가 제대로 일할 수 있어요.
            </p>
            <ul className="res-plain">
              {confirmSkills.map((s) => (
                <li key={s.id}>{scopeLabel(s)}</li>
              ))}
            </ul>
            <footer>
              <button type="button" onClick={() => setRefAsk(null)}>
                취소
              </button>
              <button
                type="button"
                onClick={() =>
                  void guard(async () => {
                    const agent = refAsk.agent;
                    setRefAsk(null);
                    await setReference(call, projectSlug, agent.id);
                    await refresh();
                  })
                }
              >
                에이전트만 참조
              </button>
              <button
                type="button"
                className="button-primary"
                onClick={() =>
                  void guard(async () => {
                    const agent = refAsk.agent;
                    setRefAsk(null);
                    await setReference(call, projectSlug, agent.id, { withSkills: true });
                    await refresh();
                  })
                }
              >
                스킬도 함께 참조
              </button>
            </footer>
          </div>
        </Modal>
      )}
    </Page>
  );
}
