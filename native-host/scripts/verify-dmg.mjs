#!/usr/bin/env node
// Exercise the copied app after ejecting the DMG, without installing into the user's /Applications.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { mkdtempSync, mkdirSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { once } from 'node:events';
import { fileURLToPath } from 'node:url';

const host = fileURLToPath(new URL('..', import.meta.url));
const version = JSON.parse(readFileSync(join(host, 'package.json'), 'utf8')).version;
const dmg = join(host, `src-tauri/target/release/bundle/dmg/aisurada_${version}_aarch64.dmg`);
const root = realpathSync(mkdtempSync('/tmp/aisurada-install-'));
const mount = join(root, 'mount');
mkdirSync(mount);
let mounted = false, child, log = '';
try {
  execFileSync('hdiutil', ['verify', dmg], { stdio: 'pipe' });
  execFileSync('hdiutil', ['attach', '-readonly', '-nobrowse', '-mountpoint', mount, dmg], { stdio: 'pipe' });
  mounted = true;
  mkdirSync(join(root, 'Applications'));
  const app = join(root, 'Applications/aisurada.app');
  execFileSync('ditto', [join(mount, 'aisurada.app'), app]);
  execFileSync('hdiutil', ['detach', mount], { stdio: 'pipe' });
  mounted = false;
  execFileSync(process.execPath, [join(host, 'scripts/verify-release.mjs'), app], { stdio: 'inherit' });
  const home = join(root, 'home'); mkdirSync(home);
  child = spawn(join(app, 'Contents/MacOS/aisurada-host'), [], { cwd: root,
    env: { HOME: home, PATH: '/usr/bin:/bin:/usr/sbin:/sbin', TMPDIR: root, LANG: 'en_US.UTF-8' },
    stdio: ['ignore', 'pipe', 'pipe'] });
  child.stdout.on('data', b => { log += b; }); child.stderr.on('data', b => { log += b; });
  child.on('error', error => { log += error.message; });
  // A bounded startup smoke check, not a substitute for a clean-Mac manual UI check.
  await new Promise(r => setTimeout(r, 4000));
  assert.ok(child.pid && child.exitCode === null && child.signalCode === null, `Installed app exited: ${log}`);
  const done = once(child, 'exit'); child.kill('SIGTERM');
  const timer = setTimeout(() => child.kill('SIGKILL'), 5000);
  try { await done; } finally { clearTimeout(timer); }
  child = undefined;
  const reportFile = join(host, '.release/artifacts/validation.json');
  const report = JSON.parse(readFileSync(reportFile, 'utf8'));
  report.checks.push('DMG integrity and copied-app engine checks pass after disk image ejection',
    'copied DMG app stays running through isolated-HOME startup');
  writeFileSync(reportFile, JSON.stringify(report, null, 2) + '\n');
  console.log('PASS DMG integrity, ejected-image app resources and native app startup');
} finally {
  if (child?.pid && child.exitCode === null && child.signalCode === null) {
    const done = once(child, 'exit'); child.kill('SIGKILL'); await done;
  }
  if (mounted) execFileSync('hdiutil', ['detach', mount], { stdio: 'pipe' });
  rmSync(root, { recursive: true, force: true });
}
