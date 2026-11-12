// 스킬이나 에이전트 정의 한 줄. 글로벌 페이지와 프로젝트 페이지가 같이 쓴다.
import type { ReactNode } from "react";
import { editedLine, kindLabel, permissionLabel } from "./resourcesClient.ts";
import type { ResourceRow } from "./resourcesClient.ts";

/** 한 줄에 보일 요약: 에이전트는 권한과 쓰는 스킬 수와 저장소, 그리고 누가 고쳤는지. */
export function resourceMeta(row: ResourceRow): string {
  const parts: string[] = [];
  if (row.kind === "agent") {
    parts.push(`권한 ${permissionLabel(row.permission)}`);
    if (row.skills.length) parts.push(`스킬 ${row.skills.length}개`);
    if (row.repo) parts.push(`저장소 ${row.repo}`);
  }
  const edited = editedLine(row);
  if (edited) parts.push(edited);
  return parts.join(" · ");
}

export function ResourceItem({
  row,
  badge,
  actions,
  notes,
}: {
  row: ResourceRow;
  /** 이름 옆 꼬리표(예: 「프로젝트 자원」, 「글로벌에서 참조」) */
  badge?: ReactNode;
  actions?: ReactNode;
  /** 줄 아래에 붙는 안내(빠진 스킬 같은 경고) */
  notes?: ReactNode;
}) {
  const meta = resourceMeta(row);
  return (
    <li className="res-item" data-resource-id={row.id} data-resource-kind={row.kind}>
      <div className="res-main">
        <p className="res-title">
          <strong className="res-name">{row.name}</strong>
          {badge}
          <span className="res-kind">{kindLabel(row.kind)}</span>
        </p>
        {row.description && <p className="res-desc">{row.description}</p>}
        {meta && <p className="res-meta">{meta}</p>}
        {notes}
      </div>
      {actions && <div className="res-actions">{actions}</div>}
    </li>
  );
}
