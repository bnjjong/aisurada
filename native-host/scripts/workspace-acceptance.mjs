#!/usr/bin/env node
// Exercise the built React application in an isolated, actual macOS WKWebView.
import { createServer } from 'node:http';
import { spawn } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, realpathSync, rmSync, chmodSync } from 'node:fs';
import { tmpdir, homedir } from 'node:os';
import { join, resolve, extname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { assertScreenBundle } from './screen-contracts.mjs';

const host = fileURLToPath(new URL('..', import.meta.url)), repo = resolve(host, '..');
const part4Check = process.argv.includes('--part4');
const askMarksCheck = part4Check || process.argv.includes('--ask-marks');
const navigationCheck = process.argv.includes('--navigation');
const interactionCheck = process.argv.includes('--interaction');
const dataCheck = process.argv.includes('--data');
const editorCheck = process.argv.includes('--editor');
const autosaveCheck = process.argv.includes('--autosave');
const viewsCheck = process.argv.includes('--views');
let editorCases = editorCheck ? JSON.parse(readFileSync(join(host, 'fixtures/editor-roundtrip/cases.json'), 'utf8')) : [];
const onlyCases = process.argv.find(arg => arg.startsWith('--only='))?.slice(7).split(',');
if (onlyCases) editorCases = editorCases.filter(c => onlyCases.includes(c.id));
let dataDriver;
const probeAssets = new Map();
if (dataCheck || autosaveCheck) {
  const { build } = await import('vite');
  const compiled = await build({ configFile: false, logLevel: 'silent', define: { 'process.env.NODE_ENV': JSON.stringify('production') }, build: { write: false, minify: false, target: 'safari15',
    lib: { entry: join(host, autosaveCheck ? 'scripts/autosave-checks.ts' : 'scripts/data-checks.ts'), formats: ['es'], fileName: 'probe' } } });
  const output = (Array.isArray(compiled) ? compiled[0] : compiled).output;
  dataDriver = output.find(output => output.type === 'chunk' && output.isEntry).code;
  for (const asset of output) probeAssets.set('/' + asset.fileName, asset.type === 'chunk' ? asset.code : asset.source);
}
assertScreenBundle(join(host, 'dist'), { part4: part4Check });
const root = realpathSync(mkdtempSync(join(tmpdir(), 'ais-p3-'))), home = join(root, 'home'), vault = join(root, 'vault');
const mkdir = path => mkdirSync(path, { recursive: true });
const write = (path, text) => { mkdir(resolve(path, '..')); writeFileSync(path, text); };
const id = 'ndfixture-00000001', legacyId = 'ndfixture-00000002';
const date = new Date();
const today = `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, '0')}-${String(date.getDate()).padStart(2, '0')}`;
const noteHead = (nid, title) => `---\nid: ${nid}\ntitle: ${title}\nagenda: demo\ntype: free\ncreated: '${today}T09:30:00+09:00'\n---\n`;
write(join(vault, '.aisurada/agendas/demo.md'), '---\nid: agfixture-00000001\nslug: demo\ntitle: 검증 아젠다\n---\n');
write(join(vault, 'notes/demo/root/note.md'), noteHead(id, '실제 엔진 기록') + '# 실제 엔진 기록\n\n격리된 vault의 본문이에요.\n' + (navigationCheck ? '\n긴 기록의 문단이에요.\n'.repeat(200) : ''));
write(join(vault, 'notes/demo/legacy.md'), noteHead(legacyId, '옛 노트') + '# 옛 노트\n\n이관 전 본문\n');
mkdir(home); mkdir(join(root, 'zdot'));
const identifier = `co.kcd.aisurada.part3.check${process.pid}`;
const external = join(root, 'external/외부 기록.md');
const rootNote = join(vault, 'notes/demo/root/note.md');
const childId = 'snfixture-00000001', childNote = join(vault, 'notes/demo/root/sub/child/note.md');
const grandchildNote = join(vault, 'notes/demo/root/sub/child/sub/grandchild/note.md');
if (askMarksCheck || interactionCheck || dataCheck || editorCheck || autosaveCheck || viewsCheck) {
  write(childNote, noteHead(childId, '하위 파일 노트') + '# 하위 파일 노트\n\n하위 Markdown 파일의 본문이에요.\n');
  write(grandchildNote, noteHead('snfixture-00000002', '두 단계 하위 노트') + '# 두 단계 하위 노트\n\n두 단계 사슬의 본문이에요.\n');
}
write(external, '# 외부 기록\n\n별도의 Markdown 본문이에요.\n');
if (editorCheck) {
  write(rootNote, noteHead(id, '실제 엔진 기록') + '# 실제 엔진 기록\n\n![검증 HTML](attach/frame.html)\n\n```mermaid\ngraph TD; A-->B\n```\n\n기능 검사 끝');
  write(join(vault, 'notes/demo/root/attach/frame.html'), '<!doctype html><html><body><p>격리된 HTML 첨부</p></body></html>');
}
if (navigationCheck) for (const key of ['one', 'two']) write(join(vault, `.aisurada/templates/doc/${key}.md`), `---\nid: ${key}\nname: 템플릿 ${key}\nformat: markdown\ntype: free\nbuiltin: false\n---\n본문 ${key}\n`);
if (dataCheck) write(join(vault, `.aisurada/reports/rv-morning-${today}.json`), JSON.stringify({id:`rv-morning-${today}`,kind:'morning',date:today,body:'<p>안전한 회고<script>unsafe()</script></p>',memo:'  '}));
if (viewsCheck) {
  write(rootNote, noteHead(id, '실제 엔진 기록') + '# 실제 엔진 기록\n\n이전 판만의 문장\n');
  write(join(vault, '.aisurada/templates/doc/tplfixture-00000001.md'), '---\nid: tplfixture-00000001\nname: 화면 검증 템플릿\nformat: markdown\ntype: plan\nbuiltin: false\n---\n# [제목]\n\n템플릿 본문\n');
  write(join(vault, 'notes/demo/record/note.md'), noteHead('ndfixture-00000003', '숨긴 기록') .replace('type: free', 'type: free\ntier: record') + '# 숨긴 기록\n\n기록 전용 검색어\n');
  write(join(vault, 'notes/demo/html/note.html'), noteHead('ndfixture-00000004', 'HTML 문서').replace('type: free', 'type: free\nformat: html') + '<h1>HTML 읽기</h1><p>안전한 HTML 본문<script>window.__UNSAFE__=1</script></p>');
  write(external, '# 외부 기록\n\n<div>HTML 블록 <b>굵게</b><script>window.__UNSAFE__=1</script></div>\n\n![원격 그림](https://example.invalid/remote.png)\n\n![로컬 그림](pic%20one.png)\n\n[다음 파일](./next.MARKDOWN#part)\n\n```mermaid\ngraph TD; A-->B\n```');
  write(join(root, 'external/next.MARKDOWN'), '# 다음 파일\n\n상대 경로로 연 읽기 전용 파일');
  write(join(root, 'external/pic one.png'), Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a6n8AAAAASUVORK5CYII=', 'base64'));
}
if (askMarksCheck) {
  write(rootNote, noteHead(id, '실제 엔진 기록') + '# 실제 엔진 기록\n\n격리된 vault의 본문이에요.\n\n| 열 | 값 |\n| --- | --- |\n| <!--aisa:akfixture-12345678-->표 문답<!--/aisa:akfixture-12345678--> | 값 |\n\n[하위](#sub:' + childId + ')\n');
  const fakeClaude = join(root, 'fake-claude');
  // `claude mcp get|remove|add-json`는 등록 상태를 파일 하나에 두고 흉내 낸다(설정 화면의 Claude 연결 점검용).
  write(fakeClaude, [
    '#!/bin/sh',
    `if [ "$1" = agents ]; then printf '[]\\n'`,
    `elif [ "$1" = mcp ]; then state="$(dirname "$0")/mcp-state"`,
    `  case "$2" in`,
    `    get) if [ -f "$state" ]; then printf 'aisurada:\\n  Status: Connected\\n  Args: %s\\n' "$(cat "$state")"; else echo none >&2; exit 1; fi ;;`,
    `    remove) rm -f "$state" ;;`,
    `    add-json) printf '%s' "$4" > "$state" ;;`,
    `  esac`,
    `else cat >/dev/null; printf 'fixture answer\\n'; fi`,
    '',
  ].join('\n'));
  chmodSync(fakeClaude, 0o700);
}
// 4부를 켠 판만 전사를 쓴다. 엔진은 격리 HOME의 ~/.claude/projects를 읽는다(래퍼가 -Duser.home을 준다).
const sessionId = 'f1a50000-0000-4000-8000-000000000001';
if (part4Check) {
  const stamp = new Date().toISOString();
  write(join(home, '.claude/projects/-Users-test-vault', `${sessionId}.jsonl`), [
    { type: 'user', entrypoint: 'cli', cwd: '/Users/test/vault', timestamp: stamp, message: { content: '세션 점검용 첫 물음' } },
    { type: 'assistant', timestamp: stamp, message: { model: 'claude-opus-4', content: [{ type: 'text', text: '세션 점검용 첫 답' }] } },
    { type: 'custom-title', customTitle: '점검 세션' },
  ].map(line => JSON.stringify(line)).join('\n') + '\n');
}
const cfg = { part4Check, askMarksCheck, sessionId, parent: root, vault, raw: dataCheck || editorCheck || autosaveCheck || viewsCheck ? vault : vault + '/.', id, legacyId, external, rootNote, childId, childNote, grandchildNote, navigationCheck, interactionCheck, dataCheck, editorCheck, autosaveCheck, viewsCheck, today };
const java = process.env.JAVA_HOME ? join(process.env.JAVA_HOME, 'bin/java') : '/usr/bin/java';
const quote = text => "'" + text.replaceAll("'", "'\\''") + "'";
const wrapper = join(root, 'java');
const callLog = join(root, 'java-calls.jsonl');
const logger = join(root, 'record-java.py');
write(logger, `import os,sys,json,time\nfd=os.open(${JSON.stringify(callLog)}, os.O_WRONLY|os.O_CREAT|os.O_APPEND, 0o600)\nos.write(fd,(json.dumps({'args':sys.argv[1:],'start':time.time_ns()//1000000})+'\\n').encode())\nos.close(fd)\nos.execv(${JSON.stringify(java)},[${JSON.stringify(java)},${JSON.stringify('-Duser.home=' + home)},*sys.argv[1:]])\n`);
write(wrapper, `#!/bin/sh\nexec /usr/bin/python3 ${quote(logger)} "$@"\n`); chmodSync(wrapper, 0o700);
const env = { ...process.env, HOME: home, ZDOTDIR: join(root, 'zdot'), HISTFILE: join(root, 'history'),
  CARGO_HOME: process.env.CARGO_HOME || join(homedir(), '.cargo'), RUSTUP_HOME: process.env.RUSTUP_HOME || join(homedir(), '.rustup'),
  AISURADA_VAULT: vault, AISURADA_CLI_JAR: join(repo, 'dist/ais-cli.jar'), AISURADA_SIDECAR_JAVA: wrapper,
  AISURADA_SIDECAR_SOCKET: join(root, 'engine.sock'), AISURADA_VAULT_AUTOCOMMIT_ENABLED: autosaveCheck || viewsCheck ? 'true' : 'false' };
if (askMarksCheck) env.AISURADA_CLAUDE_BIN = join(root, 'fake-claude');
for (const key of ['AISURADA_SIDECAR_JAR', 'CLAUDE_CODE_SESSION_ID', 'AISURADA_ASK_RUNTIME', 'AISURADA_ASK_USE_AGENT']) delete env[key];
if (!existsSync(join(repo, 'dist/ais-sidecar.jar'))) throw Error('Run bash build.sh first');
let finish, noteResult;
const completed = new Promise(resolve => { finish = resolve; });
const boot = `window.__AIS_FIXTURE__=${JSON.stringify(cfg)};
window.__AIS_WRITES__=[];
if(!location.pathname.startsWith('/note-window/')){localStorage.setItem('aisurada.lastWorkspace.v1',JSON.stringify({path:window.__AIS_FIXTURE__.raw,name:'검증 공간'}));localStorage.setItem('aisurada.prefs.v1',JSON.stringify({state:{theme:'dark'},version:1}));}
if(window.__AIS_FIXTURE__.navigationCheck&&!location.pathname.startsWith('/note-window/')&&!sessionStorage.getItem('navigation-seeded')){localStorage.setItem('aisurada.tabs.v1',JSON.stringify({state:{workspaceRoot:window.__AIS_FIXTURE__.raw,openIds:[window.__AIS_FIXTURE__.id,window.__AIS_FIXTURE__.id,'term:old','ext:/old.md','sess:old']},version:1}));sessionStorage.setItem('navigation-seeded','yes');}
const originalSet=Storage.prototype.setItem;Storage.prototype.setItem=function(key,value){window.__AIS_WRITES__.push(key);return originalSet.call(this,key,value)};`;
const server = createServer(async (req, res) => {
  const path = new URL(req.url, 'http://localhost').pathname;
  if (req.method === 'POST') {
    let body = ''; for await (const chunk of req) body += chunk;
    const value = JSON.parse(body);
    if (path === '/result') finish(value);
    if (path === '/note-result') noteResult = value;
    if (path === '/progress') console.log(`${value.ok ? 'PASS' : 'FAIL'} ${value.id}${value.error ? ' ' + value.error : ''}`);
    res.writeHead(200, { 'Content-Type': 'application/json' }); res.end('{}'); return;
  }
  if (path === '/note-result') { res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(noteResult || null)); return; }
  if (path === '/editor-cases') { res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(editorCases)); return; }
  if (path === '/calls') {
    const calls = existsSync(callLog) ? readFileSync(callLog, 'utf8').trim().split('\n').filter(Boolean).map(line => JSON.parse(line)) : [];
    const commands = calls.filter(call => call.args[1] === join(repo, 'dist/ais-cli.jar')).map(call => ({ args: { argv: call.args.slice(2) }, start: call.start }));
    res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(commands)); return;
  }
  if (path === '/bootstrap.js' || path === '/workspace-checks.js') {
    res.writeHead(200, { 'Content-Type': 'text/javascript' });
    res.end(path === '/bootstrap.js' ? boot : dataCheck || autosaveCheck ? dataDriver : readFileSync(join(host, askMarksCheck ? 'scripts/part4-checks.js' : viewsCheck ? 'scripts/views-checks.js' : editorCheck ? 'scripts/editor-checks.js' : interactionCheck ? 'scripts/interaction-checks.js' : navigationCheck ? 'scripts/navigation-checks.js' : 'scripts/workspace-checks.js'))); return;
  }
  if (probeAssets.has(path)) { res.writeHead(200, { 'Content-Type': path.endsWith('.css') ? 'text/css' : 'text/javascript' }); res.end(probeAssets.get(path)); return; }
  if (path.startsWith('/assets/')) {
    const asset = resolve(host, 'dist', '.' + path);
    if (!asset.startsWith(join(host, 'dist/assets/')) || !existsSync(asset)) { res.writeHead(404).end(); return; }
    res.writeHead(200, { 'Content-Type': extname(asset) === '.css' ? 'text/css' : 'text/javascript' }); res.end(readFileSync(asset)); return;
  }
  let html = dataCheck || autosaveCheck ? '<!doctype html><html lang="ko"><head><meta charset="utf-8">' + [...probeAssets.keys()].filter(path => path.endsWith('.css')).map(path => `<link rel="stylesheet" href="${path}">`).join('') + '</head><body><main id="probe">저장 연결을 확인하는 중…</main></body></html>' : readFileSync(join(host, 'dist/index.html'), 'utf8');
  html = html.replace('<head>', '<head><script src="/bootstrap.js"></script>').replace('</body>', '<script type="module" src="/workspace-checks.js"></script></body>');
  res.writeHead(200, { 'Content-Type': 'text/html; charset=utf-8' }); res.end(html);
});
await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
const override = join(root, 'tauri.json');
// Tauri's deny-emit rules apply to the entire local origin, including main.
// Only this isolated fixture permits JS event injection. Product capabilities
// remain unchanged; Rust's emit_window unit test covers native target scoping.
const noteCapability = JSON.parse(readFileSync(join(host, 'src-tauri/capabilities/note.json'), 'utf8'));
const eventInjection = interactionCheck ? { security: { capabilities: ['main', {
  ...noteCapability,
  identifier: 'fixture-note',
  permissions: noteCapability.permissions
    .filter(permission => !['core:event:deny-emit', 'core:event:deny-emit-to'].includes(permission)),
}] } } : {};
write(override, JSON.stringify({ identifier, build: { devUrl: `http://127.0.0.1:${server.address().port}`, beforeDevCommand: null },
  app: { withGlobalTauri: false, ...eventInjection }, bundle: { resources: null } }));
const children = []; let log = '', failure, results = [];
const start = (command, args) => {
  const child = spawn(command, args, { cwd: host, env, detached: true, stdio: ['ignore', 'pipe', 'pipe'] }); children.push(child);
  child.stdout.on('data', chunk => { log += chunk; }); child.stderr.on('data', chunk => { log += chunk; }); return child;
};
let timer;
try {
  if (autosaveCheck || viewsCheck) {
    for (const args of [['init', '--initial-branch=main'], ['config', 'user.name', 'Aisurada Fixture'], ['config', 'user.email', 'fixture@example.invalid'], ['add', '.'], ['commit', '-m', 'Seed isolated autosave fixture']]) {
      const git = spawn('/usr/bin/git', ['-C', vault, ...args], { env, stdio: 'ignore' });
      if (await new Promise(resolve => git.on('exit', resolve))) throw Error('Fixture git initialization failed');
    }
  }
  if (viewsCheck) {
    write(rootNote, noteHead(id, '실제 엔진 기록') + '# 실제 엔진 기록\n\n현재 판의 문장\n\n## 첫 대목\n\n본문 검색어 찾을 내용\n\n## 둘째 대목\n\n[하위 파일 노트](#sub:' + childId + ')\n');
    for (const args of [['add', '.'], ['commit', '-m', 'Current fixture note']]) {
      const git = spawn('/usr/bin/git', ['-C', vault, ...args], { env, stdio: 'ignore' });
      if (await new Promise(resolve => git.on('exit', resolve))) throw Error('Fixture second version failed');
    }
  }
  const engine = start(wrapper, ['-jar', join(repo, 'dist/ais-sidecar.jar'), 'serve']);
  for (let n = 0; n < 600 && !existsSync(env.AISURADA_SIDECAR_SOCKET); n++) {
    if (engine.exitCode !== null) throw Error('engine exited');
    await new Promise(resolve => setTimeout(resolve, 25));
  }
  if (!existsSync(env.AISURADA_SIDECAR_SOCKET)) throw Error('engine not ready');
  if (editorCheck) {
    const initialized = spawn(wrapper, ['-jar', join(repo, 'dist/ais-cli.jar'), 'init-workspace', 'name=검증 공간', 'templates=[]'], { cwd: host, env });
    let initializationError = ''; initialized.stdout.resume(); initialized.stderr.on('data', chunk => { initializationError += chunk; });
    if (await new Promise(resolve => initialized.on('exit', resolve))) throw Error(initializationError);
    // Seed through the real engine before the UI starts, so watch reads cannot
    // mix with the roundtrip measurements. These are independent PDF fixtures.
    for (const c of editorCases) {
      c.original = `# 왕복 ${c.id}\n\n${c.input}\n\n왕복 사례 끝`;
      const child = spawn(wrapper, ['-jar', join(repo, 'dist/ais-cli.jar'), 'create-doc', 'agenda=demo', 'type=free', `title=왕복 ${c.id}`, `body=${c.original}`, 'format=markdown'], { cwd: host, env });
      let out = '', err = ''; child.stdout.on('data', chunk => { out += chunk; }); child.stderr.on('data', chunk => { err += chunk; });
      const code = await new Promise(resolve => child.on('exit', resolve));
      if (code) throw Error(`Seed ${c.id}: ${err}`);
      c.noteId = JSON.parse(out).id;
      if (!c.noteId) throw Error(`Seed missing id ${c.id}: ${out}`);
    }
    console.log(`Seeded ${editorCases.length} independent editor cases`);
  }
  // Tauri CLI re-adds the crate's default feature list. The explicit off regression runs cargo directly
  // with the same isolated config, so it really contains no Claude host commands.
  if (askMarksCheck && !part4Check) env.TAURI_CONFIG = readFileSync(override, 'utf8');
  const app = askMarksCheck && !part4Check
    ? start('cargo', ['run', '--manifest-path', join(host, 'src-tauri/Cargo.toml'), '--no-default-features', '--'])
    : start(join(host, 'node_modules/.bin/tauri'), ['dev', '--config', override, '--no-watch', '--', ...(part4Check ? [] : ['--no-default-features'])]);
  app.on('exit', code => { if (code) finish([{ id: 'native-process', ok: false, error: `exited ${code}` }]); });
  timer = setTimeout(() => finish([{ id: 'timeout', ok: false, error: 'No page result within fixture deadline' }]), editorCheck || autosaveCheck || viewsCheck ? 420_000 : 180_000);
  results = await completed;
  if (!results.length || results.some(result => !result.ok)) throw Error('Workspace checks failed');
  if (!existsSync(join(vault, '.aisurada/workspace.json')) || existsSync(join(vault, 'notes/demo/legacy.md'))) throw Error('Workspace initialization or migration missing on disk');
  results.push({ id: 'disk-init-and-migration', ok: true });
  console.log(`${results.length}/${results.length} workspace checks passed`);
} catch (error) { failure = error; console.error(String(error)); }
finally {
  clearTimeout(timer);
  for (const child of children.reverse()) { try { process.kill(-child.pid, 'SIGTERM'); } catch {} }
  await new Promise(resolve => setTimeout(resolve, 700));
  for (const child of children) { try { process.kill(-child.pid, 'SIGKILL'); } catch {} }
  server.close();
  if (process.env.AISURADA_WORKSPACE_REPORT) write(process.env.AISURADA_WORKSPACE_REPORT, JSON.stringify({ ok: !failure, results, root }, null, 2));
  if (failure) { write(join(root, 'host.log'), log); console.error(`Diagnostics retained: ${root}`); }
  else rmSync(root, { recursive: true, force: true });
}
if (failure) process.exitCode = 1;
