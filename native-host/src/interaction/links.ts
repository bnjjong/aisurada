export type AppLink =
  | { kind: 'doc'; id: string; ws: string | null; window: boolean; chain: string[] }
  | { kind: 'agenda'; slug: string; ws: string | null }
  | { kind: 'external'; path: string; ws: null };

export function parseWithlog(value: string): Exclude<AppLink, { kind: 'external' }> | null {
  const match = /^withlog:\/\/(doc|agenda)\/([^/?#]+)(?:\?([^#]*))?(?:#.*)?$/i.exec(value.trim());
  if (!match) return null;
  try {
    const id = decodeURIComponent(match[2]);
    if (!id) return null;
    const query = new URLSearchParams(match[3] || ''), ws = query.get('ws') || null;
    return match[1].toLowerCase() === 'doc'
      ? { kind: 'doc', id, ws, window: query.get('window') === '1', chain: [] }
      : { kind: 'agenda', slug: id, ws };
  } catch { return null; }
}

export function fileURLPath(value: string): string | null {
  try {
    const url = new URL(value.trim());
    if (url.protocol !== 'file:' || url.hostname && url.hostname !== 'localhost') return null;
    const path = decodeURIComponent(url.pathname);
    return /\.(md|markdown)$/i.test(path) ? path : null;
  } catch { return null; }
}

export function latestInput(values: string[]): { link: AppLink } | { file: string } | null {
  for (let index = values.length - 1; index >= 0; index--) {
    const value = values[index];
    if (typeof value !== 'string') continue;
    const link = parseWithlog(value);
    if (link) return { link };
    const file = fileURLPath(value);
    if (file) return { file };
  }
  return null;
}

export interface DeepLinkSource {
  listen(receive: (urls: string[]) => void): Promise<() => void>;
  current(): Promise<string[] | null>;
}

export async function connectLinkSource(source: DeepLinkSource, receive: (urls: string[]) => void,
  revision: () => number, warn: (error: unknown) => void): Promise<() => void> {
  const before = revision();
  let received = false, stopped = false;
  const unlisten = await source.listen(urls => { received = true; if (!stopped) receive(urls); });
  // Install live delivery first. A late cold-start result must not replace newer input.
  void source.current().then(urls => {
    if (!stopped && !received && revision() === before && urls?.length) receive(urls);
  }).catch(warn);
  return () => { stopped = true; unlisten(); };
}
