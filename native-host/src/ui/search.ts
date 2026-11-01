import type { Note, Agenda } from "../workspace/types.ts";
import { asText } from "../workspace/types.ts";
import { tags } from "./model.ts";
export const searchTrim = (s: string) =>
  s.replace(/^[\s\u001c-\u001f]+|[\s\u001c-\u001f]+$/g, "");
const entities = (s: string) =>
  s
    .replace(/&amp;/g, "&")
    .replace(/&lt;/g, "<")
    .replace(/&gt;/g, ">")
    .replace(/&quot;/g, '"')
    .replace(/&#0*39;|&apos;/g, "'")
    .replace(/&nbsp;/g, " ");
export function plainBody(body: string, format = "markdown"): string {
  let s = body;
  if (format === "json") {
    try {
      const tree = JSON.parse(body),
        text = (n: { text?: string; children?: unknown[] }): string =>
          typeof n.text === "string"
            ? n.text
            : (n.children || []).map((c) => text(c as typeof n)).join("");
      s = (tree.root?.children || []).map(text).join(" ");
    } catch {
      s = "";
    }
  } else if (format === "html")
    s = entities(
      s
        .replace(/<(script|style)\b[^>]*>[\s\S]*?<\/\1>/gi, " ")
        .replace(/<[^>]*>/g, " "),
    );
  else
    s = s
      .replace(/<!--[\s\S]*?-->/g, " ")
      .replace(/```[\s\S]*?```/g, " ")
      .replace(/`([^`]*)`/g, "$1")
      .replace(/!\[[^\]]*\]\([^)]*\)/g, " ")
      .replace(/\[([^\]]*)\]\([^)]*\)/g, "$1")
      .replace(/^\s{0,3}#{1,6}\s+/gm, "")
      .replace(/^\s*>\s?(\[![a-z]+\])?\s*/gim, "")
      .replace(/^\s*(?:[-*+]|\d+\.)\s+(?:\[[ x/\-]\]\s*)?/gm, "")
      .replace(/^\s*\|[\s:|\-]*\|\s*$/gm, " ")
      .replace(/^\s*\|.*\|\s*$/gm, (line) => line.replace(/\|/g, " "))
      .replace(/^\s*[-*_]{3,}\s*$/gm, " ")
      .replace(
        /\*\*([^*]+)\*\*|\*([^*]+)\*|__([^_]+)__|~~([^~]+)~~/g,
        (_, ...m) => m.slice(0, 4).find((v) => v !== undefined) || "",
      );
  return searchTrim(s.replace(/\s+/g, " "));
}
const cache = new Map<
  string,
  { body: string; format: string; attach: string; plain: string; lower: string }
>();
export interface Hit {
  note: Note;
  rank: number;
  snippet: string;
  group?: string;
}
export function findNotes(
  notes: Note[],
  agendas: Agenda[],
  query: string,
): Hit[] {
  const needle = searchTrim(query).toLowerCase();
  if (!needle) return [];
  const idNeedle = needle.replace(/^#doc:/, "").split("/")[0];
  return notes
    .flatMap((note) => {
      if (!searchTrim(note.title)) return [];
      const agenda = agendas.find((a) => a.id === note.agendaId)?.title || "";
      let rank = -1,
        snippet = "";
      if (note.id.toLowerCase() === idNeedle) rank = 0;
      else if (note.title.toLowerCase().includes(needle)) rank = 1;
      else if (
        agenda.toLowerCase().includes(needle) ||
        tags(note).some((t) => t.toLowerCase().includes(needle))
      )
        rank = 2;
      else {
        const attach = asText(note.attachText);
        let value = cache.get(note.id);
        if (
          !value ||
          value.body !== note.body ||
          value.format !== note.format ||
          value.attach !== attach
        ) {
          const plain = searchTrim(
            plainBody(note.body, note.format) + (attach ? " " + attach : ""),
          );
          value = {
            body: note.body,
            format: note.format,
            attach,
            plain,
            lower: plain.toLowerCase(),
          };
          cache.set(note.id, value);
        }
        const i = value.lower.indexOf(needle);
        if (i >= 0) {
          rank = 3;
          const a = Math.max(0, i - 24),
            b = Math.min(value.plain.length, i + needle.length + 76);
          snippet =
            (a ? "…" : "") +
            searchTrim(value.plain.slice(a, b)) +
            (b < value.plain.length ? "…" : "");
        }
      }
      return rank < 0 ? [] : [{ note, rank, snippet }];
    })
    .sort(
      (a, b) =>
        a.rank - b.rank ||
        b.note.created.localeCompare(a.note.created) ||
        b.note.id.localeCompare(a.note.id),
    );
}
export function emptySearch(
  notes: Note[],
  recent: { id: string; at: number }[],
  live: Set<string>,
): Hit[] {
  const result: Hit[] = [];
  for (const r of recent) {
    const note = notes.find((n) => n.id === r.id);
    if (note && !live.has(note.id))
      result.push({ note, rank: 0, snippet: "", group: "최근 연 노트" });
    if (result.length === 20) return result;
  }
  for (const note of [...notes].sort(
    (a, b) => b.created.localeCompare(a.created) || b.id.localeCompare(a.id),
  ))
    if (
      searchTrim(note.title) &&
      !live.has(note.id) &&
      !result.some((r) => r.note.id === note.id)
    ) {
      result.push({ note, rank: 0, snippet: "", group: "최근 만든 노트" });
      if (result.length === 20) break;
    }
  return result;
}
