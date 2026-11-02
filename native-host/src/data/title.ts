import { plainInline } from './mdInline.ts';

// Shared by autosave's title gate and the store's one permitted title retry.
export function extractTitle(body: string, format = 'markdown'): string {
  let text = '';
  if (format === 'json') {
    try {
      const tree = JSON.parse(body);
      const node = tree.root?.children?.find((child: { type?: string }) => child.type === 'heading');
      const plain = (item: { text?: string; children?: unknown[] }): string => typeof item.text === 'string' ? item.text : (item.children || []).map(child => plain(child as typeof item)).join('');
      text = node ? plain(node).replace(/\s+/g, ' ').trim() : '';
    } catch { return ''; }
  } else {
    for (const line of body.split('\n')) {
      const match = /^\s{0,3}#{1,6}\s+(.+?)\s*#*\s*$/.exec(line);
      // C188: the title is the heading as the editor shows it, so saving
      // `# snake\_case` does not turn the title into `snake\_case`.
      if (match) { text = plainInline(match[1]).trim(); break; }
    }
    if (!text) {
      const html = /<h[1-6][^>]*>([\s\S]*?)<\/h[1-6]>/i.exec(body)?.[1];
      if (html !== undefined) text = html.replace(/<[^>]+>/g, '').replace(/&amp;/g, '&').replace(/&lt;/g, '<').replace(/&gt;/g, '>')
        .replace(/&quot;/g, '"').replace(/&#0*39;/g, "'").replace(/&apos;/g, "'").replace(/&nbsp;/g, ' ').replace(/\s+/g, ' ').trim();
    }
  }
  return text === '[제목]' ? '' : text;
}
