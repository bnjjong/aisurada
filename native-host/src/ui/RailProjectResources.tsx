// 레일에서 프로젝트 하나의 에이전트·스킬 줄. 노트 탭의 프로젝트 묶음과 글로벌 탭의 「프로젝트 리소스」가 같이 쓴다.
// 개수가 있으면 줄을 눌러 이름 목록을 펼치고, 이름을 누르면 편집 창을 연다. 「+등록」은 늘 새로 만들기다.
import { Fragment } from "react";
import { Icon } from "./Icon.tsx";
import { kindLabel } from "./resourcesClient.ts";
import type { ResourceKind, ResourceRow } from "./resourcesClient.ts";

/** 한 프로젝트가 가진 자원(승인된 판). 레일이 `list-resources` 한 번으로 모든 프로젝트 몫을 나눠 둔다. */
export interface ProjectRows {
  agents: ResourceRow[];
  skills: ResourceRow[];
}

export const EMPTY_PROJECT_ROWS: ProjectRows = { agents: [], skills: [] };

const KINDS: readonly ResourceKind[] = ["agent", "skill"];

/** 펼침 상태의 열쇠. 두 탭이 같은 상태를 쓰므로 한쪽에서 펼치면 다른 쪽도 펼쳐져 있다. */
export const expandKey = (slug: string, kind: ResourceKind) => `${slug}/${kind}`;

/** `list-resources`의 행을 프로젝트 slug별로 나눈다. 글로벌 자원은 뺀다. */
export function rowsByProject(resources: readonly ResourceRow[]): Record<string, ProjectRows> {
  const out: Record<string, ProjectRows> = {};
  for (const r of resources) {
    if (!r.project) continue;
    const rows = (out[r.project] ??= { agents: [], skills: [] });
    if (r.kind === "agent") rows.agents.push(r);
    else if (r.kind === "skill") rows.skills.push(r);
  }
  return out;
}

export function ProjectResourceRows({
  slug,
  name,
  rows,
  expanded,
  error,
  toggle,
  create,
  edit,
}: {
  slug: string;
  /** 프로젝트 이름(단추 설명에 쓴다) */
  name: string;
  rows: ProjectRows;
  expanded: ReadonlySet<string>;
  /** 이 프로젝트의 자원을 열다 난 오류 글 */
  error?: string;
  toggle: (key: string) => void;
  create: (kind: ResourceKind) => void;
  edit: (row: ResourceRow) => void;
}) {
  return (
    <>
      {KINDS.map((kind) => {
        const list = kind === "agent" ? rows.agents : rows.skills;
        const label = kindLabel(kind);
        const key = expandKey(slug, kind);
        const has = list.length > 0;
        const open = has && expanded.has(key);
        const createTitle = `${name}에 새 ${label} 등록`;
        return (
          <Fragment key={kind}>
            <div className="rail-agenda rail-sub-resource">
              <span className="agenda-dot sub-resource-icon" aria-hidden="true">
                <Icon name={kind === "agent" ? "bot" : "zap"} size={13} />
              </span>
              {/* 개수가 없으면 펼칠 것이 없으니 예전처럼 등록 창을 연다 */}
              <button
                type="button"
                className="agenda-name"
                aria-expanded={has ? open : undefined}
                onClick={() => (has ? toggle(key) : create(kind))}
                title={has ? `${name}의 ${label} ${open ? "접기" : "목록 보기"}` : createTitle}
              >
                <span className="sub-resource-label">
                  {label}
                  {has && (
                    <span className="sub-resource-chevron" aria-hidden="true">
                      <Icon name="chevron" size={10} />
                    </span>
                  )}
                </span>
                <small>{list.length}</small>
              </button>
              <button
                type="button"
                className="sub-resource-add-btn"
                title={createTitle}
                aria-label={createTitle}
                onClick={(e) => {
                  e.stopPropagation();
                  create(kind);
                }}
              >
                +등록
              </button>
            </div>
            {open && (
              <ul className="rail-sub-resource-list" aria-label={`${name}의 ${label}`}>
                {list.map((row) => (
                  <li key={row.id} className="rail-agenda rail-sub-resource-item">
                    <button
                      type="button"
                      className="agenda-name"
                      title={row.description ? `${row.name} — ${row.description}` : row.name}
                      onClick={() => edit(row)}
                    >
                      <span>{row.name}</span>
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </Fragment>
        );
      })}
      {error && (
        <p className="rail-hint" role="alert">
          {error}
        </p>
      )}
    </>
  );
}
