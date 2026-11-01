import test from 'node:test';
import assert from 'node:assert/strict';
import * as fs from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join, dirname } from 'node:path';
import { migrateLegacy, legacyFolderName } from '../src/workspace/migrateLegacy.ts';

const now = new Date('2026-10-31T06:12:03.123Z');
const head = (id, title = '한글 Test', created = "'2026-06-04T09:30:00+09:00'") => `---\nid: ${id}\ntitle: ${title}\ncreated: ${created}\n---\n`;
const exists = async path => { try { await fs.access(path); return true; } catch { return false; } };

async function fixture(t) {
  const root = await fs.mkdtemp(join(tmpdir(), 'wl-migration-'));
  t.after(() => fs.rm(root, { recursive: true, force: true }));
  const write = async (relative, text) => { const path = join(root, relative); await fs.mkdir(dirname(path), { recursive: true }); await fs.writeFile(path, text); };
  const files = { exists, readDir: async path => (await fs.readdir(path, { withFileTypes: true })).map(entry => ({ name: entry.name,
    isFile: entry.isFile(), isDirectory: entry.isDirectory(), isSymlink: entry.isSymbolicLink() })),
    readTextFile: async path => new TextDecoder().decode(await fs.readFile(path)), writeTextFile: fs.writeFile,
    mkdir: fs.mkdir, rename: fs.rename, remove: fs.rm };
  const run = () => migrateLegacy(root, files, now, () => crypto.randomUUID(), () => {});
  return { root, files, write, run };
}

test('moves markdown and JSON verbatim after backing up and converts HTML into an attachment', async t => {
  const h = await fixture(t);
  const md = head('ndtest-0001') + '# 한글\r\n\r\n본문\r\n';
  const json = head('ndtest-0002', 'JSON') + '{"text":"그대로"}\n';
  const html = head('ndtest-0003', 'HTML') + '<h1>기록</h1>\n';
  await h.write('notes/demo/old.md', md); await h.write('notes/demo/old.json', json); await h.write('notes/demo/old.html', html);
  const result = await h.run();
  assert.equal(result.moved, 3); assert.deepEqual(result.failures, []);
  assert.equal(await fs.readFile(join(result.backup, 'demo/old.md'), 'utf8'), md);
  assert.equal(await fs.readFile(join(h.root, 'notes/demo/2026-06-04-0930-한글-test-0001/note.md'), 'utf8'), md);
  assert.equal(await fs.readFile(join(h.root, 'notes/demo/2026-06-04-0930-json-0002/note.json'), 'utf8'), json);
  const htmlFolder = join(h.root, 'notes/demo/2026-06-04-0930-html-0003');
  assert.equal(await fs.readFile(join(htmlFolder, 'attach/index.html'), 'utf8'), '<h1>기록</h1>\n');
  assert.match(await fs.readFile(join(htmlFolder, 'note.md'), 'utf8'), /\[브라우저로 열기\]\(attach\/index.html\)/);
  assert.equal(await exists(join(h.root, 'notes/demo/old.md')), false);
  const second = await h.run(); assert.equal(second.moved, 0); assert.equal(second.backup, null);
});

test('preserves month folders, flattens other nesting and avoids existing folder names', async t => {
  const h = await fixture(t);
  await h.write('notes/demo/2026-07/old.md', head('ndtest-0001') + 'month');
  await h.write('notes/demo/misc/deep/old.md', head('ndtest-0002') + 'flatten');
  await h.write('notes/demo/2026-06-04-0930-한글-test-0002/note.md', head('ndexisting-0002') + 'existing');
  assert.equal((await h.run()).moved, 2);
  assert.equal(await exists(join(h.root, 'notes/demo/2026-07/2026-06-04-0930-한글-test-0001/note.md')), true);
  assert.equal(await fs.readFile(join(h.root, 'notes/demo/2026-06-04-0930-한글-test-0002-2/note.md'), 'utf8'), head('ndtest-0002') + 'flatten');
});

test('skips existing note containers, reserved trees, conflict copies, symlinks and root files', async t => {
  const h = await fixture(t);
  for (const relative of ['notes/root.md', 'notes/demo/attach/a.md', 'notes/demo/sub/a.md', 'notes/demo/sub-comments/a.md', 'notes/demo/chats/a.md',
    'notes/demo/conflicted copy.md', 'notes/demo/충돌된 사본.md', 'notes/demo/comments.json', 'notes/demo/container/note.md', 'notes/demo/container/other.md']) await h.write(relative, head('ndskip-0001') + 'keep');
  await fs.symlink(join(h.root, 'notes/demo/attach'), join(h.root, 'notes/demo/link'));
  const result = await h.run(); assert.equal(result.moved, 0); assert.equal(result.backup, null);
  assert.equal(await exists(join(h.root, 'notes/demo/container/other.md')), true);
});

test('backs up invalid front matter but leaves the original untouched', async t => {
  const h = await fixture(t); await h.write('notes/demo/README.md', '# Not a note\n');
  const result = await h.run(); assert.equal(result.moved, 0); assert.equal(result.failures.length, 1);
  assert.equal(await fs.readFile(join(result.backup, 'demo/README.md'), 'utf8'), '# Not a note\n');
  assert.equal(await fs.readFile(join(h.root, 'notes/demo/README.md'), 'utf8'), '# Not a note\n');
});

test('backup and new body follow plugin UTF-8 decoding, including BOM and invalid bytes', async t => {
  const h = await fixture(t);
  const bytes = Buffer.concat([Buffer.from([239, 187, 191]), Buffer.from(head('ndtest-0001')), Buffer.from([255, 65])]);
  await h.write('notes/demo/old.md', bytes);
  const result = await h.run(), expected = new TextDecoder().decode(bytes);
  assert.equal(await fs.readFile(join(result.backup, 'demo/old.md'), 'utf8'), expected);
  assert.equal(await fs.readFile(join(h.root, 'notes/demo/2026-06-04-0930-한글-test-0001/note.md'), 'utf8'), expected);
});

test('a failed atomic rename retains the original, cleans its temporary and continues', async t => {
  const h = await fixture(t); await h.write('notes/demo/bad.md', head('ndtest-0001')); await h.write('notes/demo/good.md', head('ndtest-0002'));
  const rename = h.files.rename;
  h.files.rename = async (from, to) => { if (to.endsWith('0001/note.md')) throw Error('rename failed'); return rename(from, to); };
  const result = await h.run(); assert.equal(result.moved, 1); assert.equal(result.failures.length, 1);
  assert.equal(await exists(join(h.root, 'notes/demo/bad.md')), true);
  assert.equal(await exists(join(h.root, 'notes/demo/good.md')), false);
  assert.deepEqual(await fs.readdir(join(h.root, 'notes/demo/2026-06-04-0930-한글-test-0001')), []);
});

test('failed original removal leaves the backup and produces a suffixed duplicate on the next opening', async t => {
  const h = await fixture(t), original = join(h.root, 'notes/demo/old.md');
  await h.write('notes/demo/old.md', head('ndtest-0001') + 'keep');
  const remove = h.files.remove;
  h.files.remove = async (path, options) => { if (path === original) throw Error('source busy'); return remove(path, options); };
  assert.equal((await h.run()).failures.length, 1);
  h.files.remove = remove;
  assert.equal((await h.run()).moved, 1);
  for (const suffix of ['', '-2']) assert.equal(await fs.readFile(join(h.root, `notes/demo/2026-06-04-0930-한글-test-0001${suffix}/note.md`), 'utf8'), head('ndtest-0001') + 'keep');
});

test('unquoted YAML timestamps retain the documented empty date and HTML UTC serialization', async t => {
  const h = await fixture(t); await h.write('notes/demo/time.html', head('ndtest-0001', 'Time', '2026-06-04T09:30:00+09:00') + '<p>x</p>');
  assert.equal((await h.run()).moved, 1);
  const note = await fs.readFile(join(h.root, 'notes/demo/-0000-time-0001/note.md'), 'utf8');
  assert.match(note, /created: 2026-06-04T00:30:00\.000Z/);
  assert.equal(legacyFolderName({ id: 'ndtest-ABCD', title: ' "한글" / Hello ', created: '2026-06-04T99:99' }), '2026-06-04-0000-한글-hello-ABCD');
});

test('stops descending after eight levels below the agenda', async t => {
  const h = await fixture(t), eight = Array(8).fill('level').join('/');
  await h.write(`notes/demo/${eight}/yes.md`, head('ndtest-0001'));
  await h.write(`notes/demo/${eight}/ninth/no.md`, head('ndtest-0002'));
  assert.equal((await h.run()).moved, 1);
  assert.equal(await exists(join(h.root, `notes/demo/${eight}/ninth/no.md`)), true);
});
