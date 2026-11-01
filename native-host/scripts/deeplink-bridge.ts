import type { IncomingMessage, ServerResponse } from 'node:http';
import type { Plugin } from 'vite';
import { parseWithlog } from '../src/interaction/links.ts';

export function handleDeepLinkRequest(req: IncomingMessage, res: ServerResponse,
  connected: () => boolean, send: (url: string) => void) {
  const reply = (status: number, message: string) => {
    res.writeHead(status, { 'Content-Type': 'text/plain; charset=utf-8' }); res.end(message);
  };
  if (req.method !== 'POST') { reply(405, 'POST로 보낸다.'); return; }
  let bytes = 0, responded = false;
  const chunks: Buffer[] = [];
  req.on('data', (chunk: Buffer) => {
    bytes += chunk.length;
    if (bytes > 4096) {
      if (!responded) { responded = true; chunks.length = 0; reply(413, '본문이 4096바이트를 넘는다.'); }
    } else chunks.push(chunk);
  });
  req.on('end', () => {
    if (responded) return;
    const url = Buffer.concat(chunks).toString('utf8').trim();
    if (!url) { reply(400, '본문이 비었다.'); return; }
    if (!parseWithlog(url)) { reply(400, `withlog:// 딥링크가 아니다: ${url.slice(0, 80)}`); return; }
    if (!connected()) { reply(503, '붙어 있는 페이지가 없다.'); return; }
    send(url); reply(200, `보냈다: ${url}`);
  });
}

export function deepLinkBridge(): Plugin {
  return {
    name: 'withlog-development-deep-links', apply: 'serve',
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        if (new URL(req.url || '/', 'http://localhost').pathname !== '/__deeplink') { next(); return; }
        handleDeepLinkRequest(req, res, () => server.ws.clients.size > 0, url => server.ws.send('withlog:deeplink', { url }));
      });
    },
  };
}
