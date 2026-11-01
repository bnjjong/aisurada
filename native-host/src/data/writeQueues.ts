export interface WriteStamp { inFlight: Set<string>; revisions: Map<string, number>; }

export class WriteQueues {
  private queues = new Map<string, Promise<unknown>>();
  private inFlight = new Set<string>();
  private revisions = new Map<string, number>();
  snapshot(): WriteStamp { return { inFlight: new Set(this.inFlight), revisions: new Map(this.revisions) }; }
  advance(key: string) { this.revisions.set(key, (this.revisions.get(key) || 0) + 1); }
  overlapped(key: string, start: WriteStamp) { return start.inFlight.has(key) || (start.revisions.get(key) || 0) !== (this.revisions.get(key) || 0); }
  run<T>(key: string, work: () => Promise<T>, count = true): Promise<T> {
    const result = (this.queues.get(key) || Promise.resolve()).catch(() => {}).then(async () => {
      if (count) this.advance(key);
      if (count) this.inFlight.add(key);
      try { return await work(); } finally { if (count) this.inFlight.delete(key); }
    });
    this.queues.set(key, result);
    void result.finally(() => { if (this.queues.get(key) === result) this.queues.delete(key); }).catch(() => {});
    return result;
  }
  async drain(): Promise<void> {
    for (let attempt = 0; attempt < 5; attempt++) {
      const pending = [...this.queues.values()];
      if (!pending.length) return;
      await Promise.allSettled(pending);
    }
  }
  pending() { return this.queues.size > 0; }
}
