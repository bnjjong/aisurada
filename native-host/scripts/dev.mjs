import { createServer } from 'node:http';
import { readFile } from 'node:fs/promises';
const files = new Map([['/', ['index.html', 'text/html; charset=utf-8']], ['/shell.css', ['shell.css', 'text/css']]]);
createServer(async (req, res) => {
  const file = files.get(new URL(req.url, 'http://localhost').pathname);
  if (!file) { res.writeHead(404).end(); return; }
  try { res.setHeader('Content-Type', file[1]); res.end(await readFile(new URL('../web/' + file[0], import.meta.url))); }
  catch { res.writeHead(500).end(); }
}).listen(1420, 'localhost');
