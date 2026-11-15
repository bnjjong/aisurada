#!/usr/bin/env node
// Verifies a real bundle: the Rust ais-cli binary creates and reads a note, serves MCP over stdio, and no Java is shipped.
// Uses a temporary HOME/vault and never registers anything with an agent or launchd.
import assert from 'node:assert/strict';
import { execFileSync, spawn } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const host = fileURLToPath(new URL('..', import.meta.url));
const version = JSON.parse(readFileSync(join(host, 'package.json'), 'utf8')).version;
const app = resolve(process.argv[2] || join(host, 'src-tauri/target/release/bundle/macos/aisurada.app'));
const resources = join(app, 'Contents/Resources');
const cli = join(resources, 'bin/ais-cli');
const checks = [];
const pass = label => { checks.push(label); console.log(`PASS ${label}`); };
const kib = path => Number(execFileSync('du', ['-sk', path], { encoding: 'utf8' }).split(/\s+/)[0]);

for (const file of [cli, join(resources, 'LICENSE.txt'), join(resources, 'user-guide.html')]) assert.ok(existsSync(file), `Missing ${file}`);
assert.ok((statSync(cli).mode & 0o111) !== 0, 'bin/ais-cli is not executable');
pass('app contains the ais-cli binary, license notice and user guide');
// 5단계 뒤의 앱에는 Java 실행 환경도 jar도 없다
assert.ok(!existsSync(join(resources, 'runtime')), 'a Java runtime is still bundled');
const jars = readdirSync(resources).filter(name => name.endsWith('.jar'));
assert.deepEqual(jars, [], `jars are still bundled: ${jars}`);
pass('no Java runtime and no JAR in the bundle');
const plist = execFileSync('plutil', ['-extract', 'CFBundleShortVersionString', 'raw', '-o', '-', join(app, 'Contents/Info.plist')], { encoding: 'utf8' }).trim();
assert.equal(plist, version);
pass(`bundle version is ${version}`);
for (const binary of [join(app, 'Contents/MacOS/aisurada-host'), cli]) {
  assert.equal(execFileSync('lipo', ['-archs', binary], { encoding: 'utf8' }).trim(), 'arm64', `${binary} is not arm64`);
}
execFileSync('codesign', ['--verify', '--deep', '--strict', '--verbose=2', app], { stdio: 'pipe' });
pass('ARM64 app, ais-cli and nested code signatures verify');

const root = realpathSync(mkdtempSync('/tmp/aisurada-release-'));
const home = join(root, 'home');
const vault = join(root, '한글 vault');
mkdirSync(home); mkdirSync(vault);
// PATH에 아무것도 없어도(Java도, git도) 돈다
const env = { HOME: home, PATH: '/nonexistent', LANG: 'en_US.UTF-8', AISURADA_VAULT: vault, AISURADA_VAULT_AUTOCOMMIT_ENABLED: 'false' };
const run = (...args) => execFileSync(cli, args, { env, encoding: 'utf8', timeout: 30000 });
const runJson = (...args) => JSON.parse(run(...args));
try {
  run('init-workspace', 'name=배포 검증', 'templates=[]');
  pass('bundled ais-cli initializes a workspace in a Korean/space path');
  const created = runJson('create_doc', JSON.stringify({ agenda: '시험 배포', type: 'free', title: '배포 시험 노트', body: '# 배포 시험 노트\n\nRust 동봉 검증.' }));
  const id = created.id;
  assert.ok(id, `create_doc returned no id: ${JSON.stringify(created)}`);
  // 별도 프로세스(= 재시작)에서 읽어도 노트가 있다
  assert.ok(JSON.stringify(runJson('get_doc', JSON.stringify({ id }))).includes('Rust 동봉 검증.'));
  pass('bundled CLI creates a note and a separate process reads it back, with nothing on PATH');
  // Claude에 등록될 MCP 서버 명령은 이 번들의 ais-cli여야 한다(등록은 하지 않고 조각만 본다)
  const registered = runJson('claude-mcp', 'config').mcpServers.aisurada;
  assert.equal(realpathSync(registered.command), realpathSync(cli));
  assert.deepEqual(registered.args, ['mcp']);
  pass('Claude MCP registration points at the bundled ais-cli');

  // MCP 서버를 실제로 띄워 초기화·도구 목록·도구 호출을 한다
  const child = spawn(cli, ['mcp'], { env, stdio: ['pipe', 'pipe', 'ignore'] });
  const lines = [];
  let buffered = '';
  child.stdout.on('data', chunk => {
    buffered += chunk;
    let at;
    while ((at = buffered.indexOf('\n')) >= 0) { lines.push(JSON.parse(buffered.slice(0, at))); buffered = buffered.slice(at + 1); }
  });
  const send = message => child.stdin.write(JSON.stringify(message) + '\n');
  send({ jsonrpc: '2.0', id: 1, method: 'initialize', params: { protocolVersion: '2025-06-18' } });
  send({ jsonrpc: '2.0', id: 2, method: 'tools/list' });
  send({ jsonrpc: '2.0', id: 3, method: 'tools/call', params: { name: 'get_doc', arguments: { id } } });
  const end = Date.now() + 15000;
  while (lines.length < 3 && Date.now() < end) await new Promise(r => setTimeout(r, 50));
  child.stdin.end();
  assert.equal(lines.length, 3, `MCP server answered ${lines.length} of 3 requests`);
  assert.equal(lines[0].result.serverInfo.name, 'aisurada');
  assert.ok(lines[1].result.tools.length > 0 && lines[1].result.tools.every(t => t.annotations.readOnlyHint === true), 'read-only server must list only read tools');
  assert.equal(lines[2].result.isError, false);
  assert.ok(lines[2].result.content[0].text.includes('Rust 동봉 검증.'));
  pass('bundled ais-cli serves MCP over stdio (initialize, tools/list, tools/call)');

  const report = { version, platform: 'macOS arm64', engine: 'Rust ais-cli (no Java, no sidecar)', checks,
    sizes: { appKiB: kib(app), cliKiB: Math.round(statSync(cli).size / 1024) },
    limits: ['Local package/engine checks; clean Mac and Finder installation require manual validation.',
      'Ad-hoc signed, not Apple notarized.', 'Git and Claude CLI are not bundled.'] };
  const output = join(host, '.release/artifacts');
  mkdirSync(output, { recursive: true });
  writeFileSync(join(output, 'validation.json'), JSON.stringify(report, null, 2) + '\n');
  console.log(`Release bundle verification passed (${checks.length} checks, app ${report.sizes.appKiB} KiB, ais-cli ${report.sizes.cliKiB} KiB).`);
} finally {
  rmSync(root, { recursive: true, force: true });
}
