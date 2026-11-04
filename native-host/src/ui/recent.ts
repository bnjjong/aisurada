import type { StoragePort } from "../workspace/types.ts";
export const recentKey = (root: string) => "aisurada.recentNotes.v1:" + root;
export function recentNotes(storage: StoragePort, root: string) {
  if (!root) return [];
  try {
    const raw = JSON.parse(storage.getItem(recentKey(root)) || "[]");
    if (!Array.isArray(raw)) return [];
    const map = new Map<string, number>();
    for (const row of raw)
      if (
        typeof row?.id === "string" &&
        typeof row.at === "number" &&
        Number.isFinite(row.at)
      )
        map.set(row.id, Math.max(map.get(row.id) || 0, row.at));
    return [...map]
      .map(([id, at]) => ({ id, at }))
      .sort((a, b) => b.at - a.at)
      .slice(0, 50);
  } catch {
    return [];
  }
}
export class RecentNotes {
  private live = new Map<string, number>();
  private storage: StoragePort;
  constructor(storage: StoragePort) {
    this.storage = storage;
  }
  mounted() {
    return new Set(this.live.keys());
  }
  enter(root: string, id: string) {
    this.live.set(id, (this.live.get(id) || 0) + 1);
    let eligible = false;
    const write = () => {
      if (!eligible || !root) return;
      const rows = [
        { id, at: Date.now() },
        ...recentNotes(this.storage, root).filter((r) => r.id !== id),
      ].slice(0, 50);
      try {
        this.storage.setItem(recentKey(root), JSON.stringify(rows));
      } catch {}
    };
    const timer = setTimeout(() => {
      eligible = true;
      write();
    }, 10000);
    const blur = () => write();
    if (typeof window !== "undefined") window.addEventListener("blur", blur);
    return () => {
      clearTimeout(timer);
      write();
      if (typeof window !== "undefined")
        window.removeEventListener("blur", blur);
      const n = (this.live.get(id) || 1) - 1;
      if (n) this.live.set(id, n);
      else this.live.delete(id);
    };
  }
  list(root: string) {
    return recentNotes(this.storage, root);
  }
}
