import { useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import type { RefObject } from "react";
import { createPortal } from "react-dom";
import type { Agenda, Row } from "../workspace/types.ts";
import { asText } from "../workspace/types.ts";
import { HUE_COLORS } from "./model.ts";
import { Icon } from "./Icon.tsx";
import { SubAgendaChoices } from "./SubAgendas.tsx";

/** 고르기 목록의 한 묶음(프로젝트 하나, 또는 미분류) */
interface Group {
  key: string;
  title: string;
  agendas: Agenda[];
}

/** 찾는 글로 아젠다를 거른다. 이름과 slug에서 대소문자 없이 찾는다. */
export function matchAgendas(agendas: Agenda[], query: string): Agenda[] {
  const q = query.trim().toLowerCase();
  if (!q) return agendas;
  return agendas.filter((a) => a.title.toLowerCase().includes(q) || a.slug.toLowerCase().includes(q));
}

/** 찾는 글과 이름이나 slug가 똑같은 아젠다가 없을 때만 「새 아젠다 만들고 옮기기」를 보인다(결정 4-나). */
export function canCreateAgenda(agendas: Agenda[], query: string): boolean {
  const q = query.trim().toLowerCase();
  return !!q && !agendas.some((a) => a.title.toLowerCase() === q || a.slug.toLowerCase() === q);
}

/** 프로젝트 이름 차례로 묶고, 프로젝트가 없는 아젠다는 맨 뒤 「미분류」로 모은다. 레일의 묶음과 같은 기준이다. */
export function groupAgendas(agendas: Agenda[], projects: Row[]): Group[] {
  const groups: Group[] = [];
  for (const project of projects) {
    const slug = asText(project.slug);
    const members = agendas.filter((a) => asText(a.project) === slug);
    if (members.length) groups.push({ key: "p:" + slug, title: asText(project.title) || slug, agendas: members });
  }
  const known = new Set(projects.map((p) => asText(p.slug)));
  const rest = agendas.filter((a) => !known.has(asText(a.project)));
  if (rest.length) groups.push({ key: "none", title: "미분류", agendas: rest });
  return groups;
}

/**
 * 노트 머리의 아젠다 칩을 누르면 뜨는 아젠다 고르기 메뉴(TSK-2 결정 1-나).
 * 다른 아젠다를 누르면 [onPick]으로 바로 옮기고(확인 창 없음, 결정 5-가), 목록에 없는 이름을 찾으면 [onCreate]로 새 아젠다를 만들어 옮긴다(결정 4-나).
 * 칩을 누르면 아젠다 화면으로 가던 예전 동작은 맨 아래 「… 화면으로 가기」([onOpenAgenda])로 옮겼다.
 * 노트 머리의 단추 스타일이 번지지 않게 문서 맨 바깥(body)에 그리고, 칩 왼쪽 끝 바로 아래에 띄운다.
 * 바깥을 누르거나 Esc를 누르면 닫고, 화면이 스크롤되거나 창 크기가 바뀌면 자리가 어긋나지 않게 닫는다.
 */
export function AgendaPicker({
  anchor,
  agendas,
  projects,
  currentId,
  onPick,
  onCreate,
  onOpenAgenda,
  onClose,
}: {
  anchor: RefObject<HTMLElement | null>;
  agendas: Agenda[];
  projects: Row[];
  currentId: string;
  onPick(agendaId: string, subAgendaIds?: string[]): void;
  onCreate(title: string): void;
  onOpenAgenda(): void;
  onClose(): void;
}) {
  const box = useRef<HTMLDivElement>(null),
    input = useRef<HTMLInputElement>(null),
    [query, setQuery] = useState(""),
    [destinationId, setDestinationId] = useState<string>(),
    [selected, setSelected] = useState<string[]>([]),
    [place, setPlace] = useState<{ top: number; left: number; maxHeight: number } | null>(null);
  const current = agendas.find((a) => a.id === currentId);
  const destination = agendas.find((a) => a.id === destinationId);
  const choices = (destination?.subAgendas ?? []).filter((item) => !item.archived);
  const groups = useMemo(() => groupAgendas(matchAgendas(agendas, query), projects), [agendas, projects, query]);
  const creatable = canCreateAgenda(agendas, query);

  useLayoutEffect(() => {
    const rect = anchor.current?.getBoundingClientRect();
    if (!rect) return;
    const width = 280;
    setPlace({
      top: rect.bottom + 6,
      left: Math.max(8, Math.min(rect.left, window.innerWidth - width - 8)),
      maxHeight: Math.max(220, window.innerHeight - rect.bottom - 22),
    });
  }, [anchor]);

  useEffect(() => {
    if (!place) return;
    input.current?.focus();
    const outside = (e: PointerEvent) => {
      const target = e.target as Node;
      if (!box.current?.contains(target) && !anchor.current?.contains(target)) onClose();
    };
    const keys = (e: KeyboardEvent) => {
      if (e.key === "Escape") {
        e.preventDefault();
        e.stopPropagation();
        onClose();
        anchor.current?.focus();
        return;
      }
      if (e.key !== "ArrowDown" && e.key !== "ArrowUp") return;
      const buttons = [...(box.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") ?? [])];
      if (!buttons.length) return;
      e.preventDefault();
      const at = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const next = e.key === "ArrowDown" ? (at + 1) % buttons.length : at <= 0 ? buttons.length - 1 : at - 1;
      buttons[next].focus();
    };
    const close = (e: Event) => {
      // 목록 안의 스크롤은 닫을 까닭이 아니다
      if (e.type === "scroll" && box.current?.contains(e.target as Node)) return;
      onClose();
    };
    document.addEventListener("pointerdown", outside);
    window.addEventListener("keydown", keys, true);
    window.addEventListener("resize", close);
    window.addEventListener("scroll", close, true);
    return () => {
      document.removeEventListener("pointerdown", outside);
      window.removeEventListener("keydown", keys, true);
      window.removeEventListener("resize", close);
      window.removeEventListener("scroll", close, true);
    };
  }, [anchor, onClose, place]);

  // Enter: 지금 아젠다가 아닌 첫 줄을 고르고, 맞는 줄이 없으면 새 아젠다를 만든다
  const submit = () => {
    const first = groups.flatMap((g) => g.agendas).find((a) => a.id !== currentId);
    if (first) onPick(first.id);
    else if (creatable) onCreate(query.trim());
  };

  return createPortal(
    <div
      className="agenda-picker act-menu"
      role="dialog"
      aria-label="아젠다 바꾸기"
      ref={box}
      style={place ? { top: place.top, left: place.left, maxHeight: place.maxHeight } : { visibility: "hidden" }}
    >
      <div className="agenda-picker-title">아젠다 바꾸기</div>
      {destination ? (
        <div className="agenda-picker-assignment">
          <strong>{destination.title}</strong>
          <SubAgendaChoices items={choices} selected={selected} onChange={setSelected} />
          <div className="sub-agenda-line">
            <button type="button" onClick={() => setDestinationId(undefined)}>뒤로</button>
            <button type="button" className="button-primary"
              onClick={() => onPick(destination.id, selected.filter((id) => choices.some((item) => item.id === id)))}>
              이 아젠다로 옮기기
            </button>
          </div>
        </div>
      ) : <>
      <label className="agenda-picker-search">
        <Icon name="search" size={12} />
        <input
          ref={input}
          type="text"
          value={query}
          placeholder="아젠다 찾기"
          aria-label="아젠다 찾기"
          onChange={(e) => setQuery(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter" && !e.nativeEvent.isComposing) {
              e.preventDefault();
              submit();
            }
          }}
        />
      </label>
      <div className="agenda-picker-list" role="menu" aria-label="아젠다 목록">
        {groups.map((group) => (
          <div key={group.key} className="agenda-picker-group" role="group" aria-label={group.title}>
            <div className="agenda-picker-group-title">{group.title}</div>
            {group.agendas.map((agenda) => {
              const isCurrent = agenda.id === currentId;
              return (
                <div className="agenda-picker-option" key={agenda.id}>
                <button
                  type="button"
                  role="menuitem"
                  className={`act-item agenda-picker-item${isCurrent ? " is-current" : ""}`}
                  data-agenda-id={agenda.id}
                  disabled={isCurrent}
                  onClick={() => onPick(agenda.id)}
                >
                  <span
                    className="agenda-picker-dot"
                    style={{ color: HUE_COLORS[asText(agenda.hue)] || "var(--accent)" }}
                    aria-hidden="true"
                  >
                    ●
                  </span>
                  <span className="agenda-picker-name">{agenda.title}</span>
                  {isCurrent && (
                    <small>
                      <Icon name="check" size={12} /> 지금
                    </small>
                  )}
                </button>
                {!isCurrent && agenda.subAgendas?.some((item) => !item.archived) && (
                  <button type="button" role="menuitem" className="agenda-picker-choose-sub"
                    aria-label={`${agenda.title} 분류 선택 후 이동`}
                    onClick={() => { setSelected([]); setDestinationId(agenda.id); }}>
                    분류 선택
                  </button>
                )}
                </div>
              );
            })}
          </div>
        ))}
        {!groups.length && <p className="agenda-picker-empty">맞는 아젠다가 없어요.</p>}
        {creatable && (
          <button
            type="button"
            role="menuitem"
            className="act-item agenda-picker-create"
            onClick={() => onCreate(query.trim())}
          >
            <Icon name="plus" size={13} />
            <span>새 아젠다 「{query.trim()}」 만들고 옮기기</span>
          </button>
        )}
      </div>
      {current && (
        <>
          <hr role="separator" />
          <button type="button" role="menuitem" className="act-item agenda-picker-open" onClick={onOpenAgenda}>
            <Icon name="chevron" size={13} />
            <span>{current.title} 화면으로 가기</span>
          </button>
        </>
      )}
      </>}
    </div>,
    document.body,
  );
}
