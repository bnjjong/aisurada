#!/usr/bin/env node
// Reproducible, private runtime for the macOS Apple Silicon trial build.
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const host = fileURLToPath(new URL('..', import.meta.url));
const lock = JSON.parse(readFileSync(join(host, 'runtime-lock.json'), 'utf8'));
if (process.platform !== 'darwin' || process.arch !== 'arm64') {
  throw Error('This trial release supports builds on Apple Silicon macOS only.');
}
const release = join(host, '.release');
const cache = join(release, 'cache');
mkdirSync(cache, { recursive: true });
const archive = join(cache, lock.archive);
const sha256 = path => createHash('sha256').update(readFileSync(path)).digest('hex');
if (!existsSync(archive) || sha256(archive) !== lock.sha256) {
  const partial = archive + '.partial';
  try {
    execFileSync('curl', ['--fail', '--location', '--retry', '2', '--connect-timeout', '30',
      '--max-time', '600', '--output', partial, lock.url], { stdio: 'inherit' });
    if (sha256(partial) !== lock.sha256) throw Error('Java runtime SHA-256 mismatch.');
    cpSync(partial, archive);
  } finally { rmSync(partial, { force: true }); }
}
const staging = mkdtempSync(join(tmpdir(), 'aisurada-runtime-'));
try {
  execFileSync('tar', ['-xzf', archive, '-C', staging]);
  const roots = readdirSync(staging).filter(name => !name.startsWith('.'));
  if (roots.length !== 1) throw Error('Unexpected Java archive layout.');
  const source = join(staging, roots[0], 'Contents', 'Home');
  for (const file of ['bin/java', 'release', 'legal/java.base/LICENSE']) {
    if (!existsSync(join(source, file))) throw Error(`Java runtime is missing ${file}`);
  }
  const runtime = join(release, 'runtime');
  rmSync(runtime, { recursive: true, force: true });
  cpSync(source, runtime, { recursive: true, verbatimSymlinks: true });
  // Upstream license/CDS files are read-only. Tauri's xattr cleanup needs owner write
  // permission even though their contents stay unchanged.
  execFileSync('chmod', ['-R', 'u+w', runtime]);
  const result = execFileSync(join(runtime, 'bin/java'), ['-version'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  if (result) process.stdout.write(result);
  console.log(`Java ${lock.version}: archive SHA-256 verified, runtime and licenses prepared.`);
} finally { rmSync(staging, { recursive: true, force: true }); }
