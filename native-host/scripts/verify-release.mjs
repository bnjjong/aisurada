#!/usr/bin/env node
// Verifies a real bundle, including a separate Java process and CLI socket round trip.
// Uses a temporary HOME/vault/socket and never registers a LaunchAgent.
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { execFileSync, spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { once } from 'node:events';

const host = fileURLToPath(new URL('..', import.meta.url));
const version = JSON.parse(readFileSync(join(host, 'package.json'), 'utf8')).version;
const app = resolve(process.argv[2] || join(host, 'src-tauri/target/release/bundle/macos/aisurada.app'));
const resources = join(app, 'Contents/Resources');
const java = join(resources, 'runtime/bin/java');
const engine = join(resources, 'ais-sidecar.jar');
const cli = join(resources, 'ais-cli.jar');
const lock = JSON.parse(readFileSync(join(resources, 'runtime-source.json'), 'utf8'));
const digest = file => createHash('sha256').update(readFileSync(file)).digest('hex');
const checks = [];
const pass = label => { checks.push(label); console.log(`PASS ${label}`); };
for (const file of [java, engine, cli, join(resources, 'runtime/legal/java.base/LICENSE'),
  join(resources, 'LICENSE.txt'), join(resources, 'user-guide.html')]) assert.ok(existsSync(file), `Missing ${file}`);
pass('app contains Java, both JARs, license notices and user guide');
for (const jar of ['ais-cli.jar', 'ais-sidecar.jar']) {
  assert.equal(digest(join(resources, jar)), digest(join(host, '../dist', jar)), `${jar} differs from build`);
}
pass('bundled Java JARs match this build');
const plist = execFileSync('plutil', ['-extract', 'CFBundleShortVersionString', 'raw', '-o', '-', join(app, 'Contents/Info.plist')], { encoding: 'utf8' }).trim();
assert.equal(plist, version);
pass(`bundle version is ${version}`);
const arch = execFileSync('lipo', ['-archs', join(app, 'Contents/MacOS/aisurada-host')], { encoding: 'utf8' }).trim();
assert.equal(arch, 'arm64');
execFileSync('codesign', ['--verify', '--deep', '--strict', '--verbose=2', app], { stdio: 'pipe' });
pass('ARM64 app and nested code signatures verify');
const javaRelease = readFileSync(join(resources, 'runtime/release'), 'utf8');
assert.ok(javaRelease.includes('OS_ARCH="aarch64"'));
assert.ok(javaRelease.includes(`JAVA_VERSION="${lock.version.split('+')[0]}"`));
pass(`runtime is ${lock.vendor} ${lock.version}, aarch64`);

const root = realpathSync(mkdtempSync('/tmp/aisurada-release-'));
const home = join(root, 'home');
const vault = join(root, '한글 vault');
const socket = join(root, 'engine.sock');
mkdirSync(home); mkdirSync(vault);
const env = { HOME: home, PATH: '/nonexistent', LANG: 'en_US.UTF-8',
  AISURADA_VAULT: vault, AISURADA_SIDECAR_SOCKET: socket, AISURADA_VAULT_AUTOCOMMIT_ENABLED: 'false' };
const base = ['-Duser.home=' + home];
const runEngine = (...args) => JSON.parse(execFileSync(java, [...base, '-jar', engine, ...args], { env, encoding: 'utf8', timeout: 30000 }));
const runCli = (...args) => JSON.parse(execFileSync(java, [...base, '-jar', cli, ...args], { env, encoding: 'utf8', timeout: 30000 }));
let child;
let engineLog = '';
const start = async () => {
  child = spawn(java, [...base, '-jar', engine, 'serve'], { env, stdio: ['ignore', 'pipe', 'pipe'] });
  child.stdout.on('data', b => { engineLog += b; });
  child.stderr.on('data', b => { engineLog += b; });
  child.on('error', e => { engineLog += e.message; });
  const end = Date.now() + 15000;
  while (!existsSync(socket)) {
    if (child.exitCode !== null || Date.now() > end) throw Error(`Bundled engine did not start: ${engineLog}`);
    await new Promise(r => setTimeout(r, 50));
  }
};
const stop = async () => {
  if (!child || child.exitCode !== null) return;
  const exiting = once(child, 'exit');
  child.kill('SIGTERM');
  const timer = setTimeout(() => child.kill('SIGKILL'), 5000);
  try { await exiting; } finally { clearTimeout(timer); }
  child = undefined;
};
try {
  runEngine('init-workspace', 'name=배포 검증', 'templates=[]');
  pass('bundled runtime initializes a workspace in a Korean/space path');
  await start();
  const created = runCli('create_doc', JSON.stringify({ agenda: '시험 배포', type: 'free', title: '배포 시험 노트', body: '# 배포 시험 노트\n\nJava 동봉 검증.' }));
  const id = created.id;
  assert.ok(id, `create_doc returned no id: ${JSON.stringify(created)}`);
  const doc = runCli('get_doc', JSON.stringify({ id }));
  assert.ok(JSON.stringify(doc).includes('Java 동봉 검증.'));
  pass('bundled CLI and separate engine create/read a note over a Unix socket');
  // 앱에 든 ais-cli 스크립트: PATH에 Java가 없어도(위 env의 PATH는 /nonexistent) 번들 Java로 같은 앱의 jar를 돈다
  const script = join(resources, 'bin/ais-cli');
  assert.ok(existsSync(script) && (statSync(script).mode & 0o111) !== 0, 'bin/ais-cli is missing or not executable');
  const viaScript = JSON.parse(execFileSync(script, ['get_doc', JSON.stringify({ id })], { env, encoding: 'utf8', timeout: 30000 }));
  assert.ok(JSON.stringify(viaScript).includes('Java 동봉 검증.'));
  pass('bundled bin/ais-cli script reads the note with the bundled Java and no Java on PATH');
  // Claude에 등록될 MCP 서버 명령도 번들 Java와 번들 jar여야 한다(등록은 하지 않고 조각만 본다)
  const registered = JSON.parse(execFileSync(script, ['claude-mcp', 'config'], { env, encoding: 'utf8', timeout: 30000 })).mcpServers.aisurada;
  assert.equal(realpathSync(registered.command), realpathSync(java));
  assert.deepEqual(registered.args.slice(0, 1), ['-jar']);
  assert.equal(realpathSync(registered.args[1]), realpathSync(cli));
  assert.equal(registered.args[2], 'mcp');
  pass('Claude MCP registration points at the bundled Java and CLI jar');
  await stop();
  await start();
  const persisted = runCli('get_doc', JSON.stringify({ id }));
  assert.ok(JSON.stringify(persisted).includes('Java 동봉 검증.'));
  pass('saved note survives engine restart');
  const report = { version, platform: 'macOS arm64', runtime: lock.version, checks,
    limits: ['Local package/engine checks; clean Mac and Finder installation require manual validation.',
      'Ad-hoc signed, not Apple notarized.', 'Git and Claude CLI are not bundled.'] };
  const output = join(host, '.release/artifacts');
  mkdirSync(output, { recursive: true });
  writeFileSync(join(output, 'validation.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(`Release bundle verification passed (${checks.length} checks).`);
} finally {
  await stop();
  rmSync(root, { recursive: true, force: true });
}
