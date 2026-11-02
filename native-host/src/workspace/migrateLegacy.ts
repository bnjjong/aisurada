import { load, dump } from 'js-yaml';
import { asRow, asText } from './types.ts';
import type { FileEntry, FilePort } from './types.ts';
import { atomicWrite, readDirectory } from '../data/fsPrimitives.ts';

const reserved = new Set(['attach', 'sub', 'sub-comments', 'chats']);
const bodies = new Set(['note.md', 'note.json', 'note.html']);
const conflicted = /conflicted copy|\(conflicted\b|충돌(된)?\s*사본|conflicted version/i;
const sortNames = (a: FileEntry, b: FileEntry) => a.name < b.name ? -1 : a.name > b.name ? 1 : 0;

export function legacyFolderName(meta: Record<string, unknown>): string {
  const id = asText(meta.id), created = asText(meta.created);
  const time = /T((?:[01]\d|2[0-3]):[0-5]\d)/.exec(created)?.[1].replace(':', '') || '0000';
  const slug = (asText(meta.title) || id).trim().toLowerCase().replace(/['"]/g, '')
    .replace(/[^\p{L}\p{N}]+/gu, '-').replace(/^-+|-+$/g, '').slice(0, 50) || 'untitled';
  return `${created.slice(0, 10)}-${time}-${slug}-${id.replace(/[^a-z0-9]/gi, '').slice(-4) || '0'}`;
}

export async function migrateLegacy(root: string, files: FilePort, now: Date, uuid: () => string,
  warn: (message: string, error?: unknown) => void) {
  const backup = `${root}/.withlog/backup/notes-${now.toISOString().replace(/[:.]/g, '-')}`;
  const candidates: { agenda: string; relative: string }[] = [];
  async function collect(agenda: string, relative: string, entries: FileEntry[], depth: number) {
    for (const entry of entries.sort(sortNames)) {
      const path = relative ? `${relative}/${entry.name}` : entry.name;
      if (entry.isDirectory && !entry.isSymlink && !reserved.has(entry.name) && depth < 8) {
        const inside = await readDirectory(files, `${root}/notes/${agenda}/${path}`, warn);
        if (!inside.some(e => e.isFile && bodies.has(e.name))) await collect(agenda, path, inside, depth + 1);
      } else if (entry.isFile && /\.(md|html|json)$/.test(entry.name) && entry.name !== 'comments.json' && !conflicted.test(entry.name)) {
        candidates.push({ agenda, relative: path });
      }
    }
  }
  const agendas = await readDirectory(files, `${root}/notes`, warn);
  for (const agenda of agendas.filter(e => e.isDirectory && !e.isSymlink)) {
    await collect(agenda.name, '', await readDirectory(files, `${root}/notes/${agenda.name}`, warn), 0);
  }
  const occupied = new Map<string, Set<string>>();
  const failures: string[] = [];
  let moved = 0;
  for (const candidate of candidates) {
    const original = `${root}/notes/${candidate.agenda}/${candidate.relative}`;
    try {
      const text = await files.readTextFile(original);
      const saved = `${backup}/${candidate.agenda}/${candidate.relative}`;
      await files.mkdir(saved.slice(0, saved.lastIndexOf('/')), { recursive: true });
      await atomicWrite(files, saved, text, uuid);
      let frontYaml: string | null = null;
      let frontLen = 0;
      const front = /^---\r?\n([\s\S]*?)\r?\n---(?:\r?\n|$)/.exec(text);
      if (front && front[1].length <= 16000) {
        frontYaml = front[1];
        frontLen = front[0].length;
      } else if (text.startsWith('<!--withlog')) {
        const htmlFront = /^<!--withlog\r?\n([\s\S]*?)\r?\n-->(?:\r?\n|$)/.exec(text);
        if (htmlFront && htmlFront[1].length <= 16000) {
          frontYaml = htmlFront[1];
          frontLen = htmlFront[0].length;
        }
      }
      const meta = frontYaml ? asRow(load(frontYaml)) : null;
      if (!meta || !asText(meta.id).trim()) throw new Error('front matter와 id가 필요해요.');
      const immediate = candidate.relative.split('/').slice(0, -1).pop() || '';
      const month = /^\d{4}-(?:0[1-9]|1[0-2])$/.test(immediate) ? `/${immediate}` : '';
      const parent = `${root}/notes/${candidate.agenda}${month}`;
      if (!occupied.has(parent)) occupied.set(parent, new Set((await readDirectory(files, parent, warn)).filter(e => e.isDirectory).map(e => e.name)));
      const names = occupied.get(parent)!;
      const base = legacyFolderName(meta);
      let name = base, suffix = 2;
      while (names.has(name)) name = `${base}-${suffix++}`;
      names.add(name);
      const target = `${parent}/${name}`;
      await files.mkdir(target, { recursive: true });
      if (candidate.relative.endsWith('.html')) {
        await files.mkdir(`${target}/attach`, { recursive: true });
        await atomicWrite(files, `${target}/attach/index.html`, text.slice(frontLen), uuid);
        const title = asText(meta.title) || asText(meta.id);
        const note = `---\n${dump(meta, { lineWidth: -1 })}---\n# ${title}\n\n이 노트는 원래 HTML로 작성됐어요. 아래 첨부를 브라우저로 여세요.\n\n[브라우저로 열기](attach/index.html)\n`;
        await atomicWrite(files, `${target}/note.md`, note, uuid);
      } else {
        await atomicWrite(files, `${target}/note.${candidate.relative.endsWith('.json') ? 'json' : 'md'}`, text, uuid);
      }
      if (await files.exists(original)) await files.remove(original, { recursive: true });
      moved++;
    } catch (error) { failures.push(original); warn(`옛 노트를 옮기지 못했어요: ${original}`, error); }
  }
  return { moved, failures, backup: candidates.length ? backup : null };
}
