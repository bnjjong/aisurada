import { asRow, asText } from "../workspace/types.ts";
import type { Note, Agenda, Row, StoragePort } from "../workspace/types.ts";
// C28: an emptied repository clears the agenda's old one only when the form
// opened with a repository; otherwise an empty field is not sent at all.
export const agendaRepoField = (opened: string, repo: string) =>
  repo.trim() || opened.trim() ? { repo } : {};
export const HUE_COLORS: Record<string, string> = {
  blue: "#5f8fbd",
  violet: "#9380bf",
  teal: "#4b9992",
  orange: "#cc8b55",
  pink: "#bf83a0",
  green: "#6c9878",
  red: "#bf7069",
  amber: "#b4984d",
  lime: "#8d9e59",
  cyan: "#5f9ea9",
  fuchsia: "#b175b0",
  brown: "#997c66",
};
export const TYPES = [
  "plan",
  "research",
  "build",
  "meeting",
  "report",
  "free",
] as const;
export const TYPE_NAMES: Record<string, string> = {
  plan: "계획",
  research: "리서치",
  build: "구현",
  meeting: "회의",
  report: "보고",
  free: "자유",
};
export interface Preferences {
  titleDatePrefix?: boolean;
  showRecordTier?: boolean;
  mermaidShowSource?: boolean;
  titleDate: boolean;
  showRecords: boolean;
  diagrams: boolean;
  weekLayout: "agendas" | "dates" | "swimlane" | "stack";
  agendaSort: "structure" | "time";
  agendaDesc: boolean;
  docSortMode?: "structure" | "time";
  docSortDir?: "desc" | "asc";
  collapsedProjects: string[];
  previewSize: { width: number; height: number } | null;
  [key: string]: unknown;
}
export const defaultPrefs: Preferences = {
  titleDatePrefix: true,
  showRecordTier: false,
  mermaidShowSource: false,
  titleDate: true,
  showRecords: false,
  diagrams: false,
  weekLayout: "agendas",
  agendaSort: "structure",
  agendaDesc: true,
  docSortMode: "structure",
  docSortDir: "desc",
  collapsedProjects: [],
  previewSize: null,
};
export function preferences(storage: StoragePort): Preferences {
  try {
    const state =
      asRow(
        asRow(JSON.parse(storage.getItem("withlog.prefs.v1") || "null"))?.state,
      ) || {};
    return {
      ...defaultPrefs,
      ...state,
      titleDate: state.titleDate !== false,
      showRecords: state.showRecords === true,
      diagrams: state.diagrams === true,
      collapsedProjects: Array.isArray(state.collapsedProjects)
        ? state.collapsedProjects.filter(
            (x): x is string => typeof x === "string",
          )
        : [],
    };
  } catch {
    return { ...defaultPrefs };
  }
}
export const tags = (note: Row) =>
  Array.isArray(note.tags)
    ? note.tags.filter((t): t is string => typeof t === "string")
    : [];
export const visibleNotes = (notes: Note[], show = false) =>
  notes.filter((n) => show || n.tier !== "record");
export function displayTitle(note: Row, enabled = true) {
  const title = asText(note.title),
    date = asText(note.date) || asText(note.created).slice(0, 10),
    digits = date.replace(/-/g, "");
  if (!enabled || digits.length !== 8) return title || "제목 없는 노트";
  const y = digits.slice(0, 4),
    m = digits.slice(4, 6),
    d = digits.slice(6),
    pattern = new RegExp(
      `^(?:${digits}|${y}[-./]${m}[-./]${d})(?=$|[\\s_.,:/—–-])(?:[\\s_.,:/—–-]*)`,
    );
  return (
    digits +
    (title.replace(pattern, "") ? "-" + title.replace(pattern, "") : "")
  );
}
export function agendaOrder(agendas: Agenda[], notes: Note[]) {
  const last = (id: string) =>
    notes
      .filter((n) => n.agendaId === id)
      .reduce((v, n) => (n.created > v ? n.created : v), "");
  return [...agendas].sort((a, b) => {
    const x = last(a.id),
      y = last(b.id);
    return !x !== !y
      ? !x
        ? -1
        : 1
      : y.localeCompare(x) || a.slug.localeCompare(b.slug);
  });
}
export function railAgendaOrder(
  agendas: Agenda[],
  notes: Note[],
  projects: Row[],
) {
  const sorted = agendaOrder(agendas, notes);
  return [
    ...projects.flatMap((project) =>
      sorted.filter((agenda) => agenda.project === project.slug),
    ),
    ...sorted.filter(
      (agenda) => !projects.some((project) => project.slug === agenda.project),
    ),
  ];
}
export function tagCandidates(
  notes: Note[],
  own: string[],
  query = "",
  limit = 8,
) {
  const counts = new Map<string, number>();
  for (const n of notes)
    for (const tag of tags(n)) counts.set(tag, (counts.get(tag) || 0) + 1);
  return [...counts]
    .filter(
      ([t]) =>
        !own.includes(t) && t.toLowerCase().includes(query.toLowerCase()),
    )
    .sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0]))
    .slice(0, limit)
    .map(([t]) => t);
}
const days = (a: Note, b: Note) =>
  Math.min(
    90,
    Math.abs(Date.parse(a.date) - Date.parse(b.date)) / 86400000 || 0,
  );
export function recommendations(
  note: Note,
  notes: Note[],
  relations: { from: string; to: string }[] = [],
) {
  const same = notes
      .filter((n) => n.agendaId === note.agendaId)
      .sort((a, b) => a.created.localeCompare(b.created)),
    i = same.findIndex((n) => n.id === note.id),
    near = [same[i - 1]?.id, same[i + 1]?.id],
    linked = new Set(
      (Array.isArray(note.links) ? note.links : []).map((l: Row) => l.to),
    );
  for (const relation of relations) {
    if (relation.from === note.id) linked.add(relation.to);
    if (relation.to === note.id) linked.add(relation.from);
  }
  return notes
    .filter((n) => n.id !== note.id && !linked.has(n.id))
    .map((n) => {
      const shared = tags(n).filter((t) => tags(note).includes(t)).length,
        adjacent = near.includes(n.id);
      return {
        note: n,
        kind: adjacent || n.agendaId === note.agendaId ? "direct" : "attach",
        score: (adjacent ? 100 : 0) + shared * 10 - days(note, n) * 0.1,
        reason: adjacent
          ? "같은 아젠다의 앞뒤 기록"
          : `함께 쓴 태그 ${shared}개`,
        eligible: adjacent || shared > 0,
      };
    })
    .filter((r) => r.eligible)
    .sort(
      (a, b) =>
        b.score - a.score || b.note.created.localeCompare(a.note.created),
    )
    .slice(0, 5);
}
export function linkCandidates(
  note: Note,
  notes: Note[],
  kind: string,
  query: string,
  tag: string,
) {
  const connected = new Set(
    (Array.isArray(note.links) ? note.links : [])
      .filter((l: Row) => l.kind === kind)
      .map((l: Row) => l.to),
  );
  return notes
    .filter(
      (n) =>
        n.id !== note.id &&
        !connected.has(n.id) &&
        n.title.toLowerCase().includes(query.toLowerCase()) &&
        (!tag || tags(n).includes(tag)),
    )
    .sort(
      (a, b) =>
        tags(b).filter((t) => tags(note).includes(t)).length -
          tags(a).filter((t) => tags(note).includes(t)).length ||
        days(note, a) - days(note, b) ||
        b.created.localeCompare(a.created),
    );
}
export const shortDate = (value: unknown) => {
  const s = asText(value),
    d = s.slice(0, 16);
  return d.length >= 16
    ? `${Number(d.slice(5, 7))}/${Number(d.slice(8, 10))} · ${d.slice(11, 16)}`
    : s;
};
export function relativeFile(base: string, raw: string) {
  try {
    if (!raw || /^(?:[a-z][a-z\d+.-]*:|\/|#)/i.test(raw)) return null;
    const path = decodeURIComponent(raw.split(/[?#]/)[0]);
    if (!/\.(md|markdown)$/i.test(path)) return null;
    const parts = base.split("/").slice(0, -1);
    for (const part of path.split("/")) {
      if (part === "..") parts.pop();
      else if (part && part !== ".") parts.push(part);
    }
    return parts.join("/");
  } catch {
    return null;
  }
}

// schedule is a CLI response, not an engine note. Only its documented keys
// participate in the review banner.
export function scheduleNotice(value: unknown): string {
  const row = asRow(value);
  if (!row) return "";
  const messages: string[] = [];
  if (row.staleWrapper === true)
    messages.push("리포트 실행 파일을 다시 설치해 주세요.");
  for (const item of Array.isArray(row.kinds) ? row.kinds : []) {
    const kind = asRow(item);
    if (!kind) continue;
    const label = asText(kind.kind),
      run = asRow(kind.lastRun);
    if (kind.stalePlist === true)
      messages.push(`${label} 작업 설정을 다시 설치해 주세요.`);
    if (!run) continue;
    const failure = asText(run.lastFailureDisplay) || asText(run.lastFailure);
    if (failure || Number(run.failureStreak) > 0)
      messages.push(
        `${label}: ${failure || "리포트 작업이 연속으로 실패했어요."}`,
      );
    const nudge = asText(run.nudge);
    if (nudge) messages.push(nudge);
  }
  return [...new Set(messages)].join(" ");
}

const LOOSE_GROUP_KEY = "(미분류)";
// 12.1: with no project there is no group header and the agendas are simply listed, so a
// saved fold of 「미분류」 must not hide them where nothing can open it again.
export const railGroupCollapsed = (collapsed: readonly string[], projectCount: number, slug: string) => {
  if (projectCount === 0) return false;
  const loose = slug === LOOSE_GROUP_KEY || slug === "" || slug === "__none__";
  return loose ? [LOOSE_GROUP_KEY, "", "__none__"].some((key) => collapsed.includes(key)) : collapsed.includes(slug);
};
