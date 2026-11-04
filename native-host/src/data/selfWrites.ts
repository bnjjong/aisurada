export const ignoredWatchPath = /\.tmp$|[/\\]\.aisurada[/\\]cache[/\\]|[/\\]\.git([/\\]|$)/;

export class SelfWrites {
  private entries = new Map<string, number>();
  private now: () => number;
  constructor(now: () => number) { this.now = now; }
  record(root: string, relative: string) {
    if (!root || !relative) return;
    const now = this.now();
    for (const [path, expires] of this.entries) if (expires <= now) this.entries.delete(path);
    // Preserve spelling, including trailing separators and symbolic links.
    this.entries.set(`${root}/${relative}`, now + 2000);
  }
  has(path: string) {
    const now = this.now();
    for (const [own, expires] of this.entries) if (expires > now && (path === own || path.startsWith(own + '/'))) return true;
    return false;
  }
  external(paths: string[]) { return paths.filter(path => !ignoredWatchPath.test(path) && !this.has(path)); }
  clear() { this.entries.clear(); }
}
