#!/usr/bin/env node
// Exercise the built React application in an isolated, actual macOS WKWebView.
import { createServer } from 'node:http';
import { spawn, spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync, existsSync, realpathSync, rmSync, chmodSync } from 'node:fs';
import { tmpdir, homedir } from 'node:os';
import { join, resolve, extname } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
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
const resourcesCheck = process.argv.includes('--resources');
const tasksCheck = process.argv.includes('--tasks');
// 노트가 많을 때 앱이 노트를 읽어 오는 시간(이슈 #59). 점검이라기보다 재는 일이라 통과·실패 대신 숫자를 찍는다.
const perfCheck = process.argv.includes('--perf');
const perfNotes = Number(process.env.AIS_PERF_NOTES || 2000);
// 러너와 4부는 Claude 기능을 켠 호스트가 있어야 화면 명령이 생긴다(태스크 점검은 가짜 claude도 쓴다)
const claudeHost = part4Check || tasksCheck;
let editorCases = editorCheck ? JSON.parse(readFileSync(join(host, 'fixtures/editor-roundtrip/cases.json'), 'utf8')) : [];
const onlyCases = process.argv.find(arg => arg.startsWith('--only='))?.slice(7).split(',');
if (onlyCases) editorCases = editorCases.filter(c => onlyCases.includes(c.id));
let dataDriver;
const probeAssets = new Map();
if (dataCheck || autosaveCheck) {
  const { build } = await import('vite');
  // 자동 저장 점검만 확인 창(네이티브 대화 창)에 답해야 해서, 이 번들에서만 대화상자 플러그인을 대역으로 바꾼다.
  const alias = autosaveCheck ? { '@tauri-apps/plugin-dialog': join(host, 'scripts/acceptance/dialog-stub.ts') } : {};
  const compiled = await build({ configFile: false, logLevel: 'silent', resolve: { alias }, define: { 'process.env.NODE_ENV': JSON.stringify('production') }, build: { write: false, minify: false, target: 'safari15',
    lib: { entry: join(host, autosaveCheck ? 'scripts/autosave-checks.ts' : 'scripts/data-checks.ts'), formats: ['es'], fileName: 'probe' } } });
  const output = (Array.isArray(compiled) ? compiled[0] : compiled).output;
  dataDriver = output.find(output => output.type === 'chunk' && output.isEntry).code;
  for (const asset of output) probeAssets.set('/' + asset.fileName, asset.type === 'chunk' ? asset.code : asset.source);
}
assertScreenBundle(join(host, 'dist'), { part4: claudeHost });
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
if (viewsCheck) {
  write(rootNote, noteHead(id, '실제 엔진 기록') + '# 실제 엔진 기록\n\n이전 판만의 문장\n');
  write(join(vault, '.aisurada/templates/doc/tplfixture-00000001.md'), '---\nid: tplfixture-00000001\nname: 화면 검증 템플릿\nformat: markdown\ntype: plan\nbuiltin: false\n---\n# [제목]\n\n템플릿 본문\n');
  write(join(vault, 'notes/demo/record/note.md'), noteHead('ndfixture-00000003', '숨긴 기록') .replace('type: free', 'type: free\ntier: record') + '# 숨긴 기록\n\n기록 전용 검색어\n');
  write(join(vault, 'notes/demo/html/note.html'), noteHead('ndfixture-00000004', 'HTML 문서').replace('type: free', 'type: free\nformat: html') + '<h1>HTML 읽기</h1><p>안전한 HTML 본문<script>window.__UNSAFE__=1</script></p>');
  write(external, '# 외부 기록\n\n<div>HTML 블록 <b>굵게</b><script>window.__UNSAFE__=1</script></div>\n\n![원격 그림](https://example.invalid/remote.png)\n\n![로컬 그림](pic%20one.png)\n\n[다음 파일](./next.MARKDOWN#part)\n\n```mermaid\ngraph TD; A-->B\n```');
  write(join(root, 'external/next.MARKDOWN'), '# 다음 파일\n\n상대 경로로 연 읽기 전용 파일');
  write(join(root, 'external/pic one.png'), Buffer.from('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+a6n8AAAAASUVORK5CYII=', 'base64'));
}
if (askMarksCheck || tasksCheck) {
  if (askMarksCheck) write(rootNote, noteHead(id, '실제 엔진 기록') + '# 실제 엔진 기록\n\n격리된 vault의 본문이에요.\n\n| 열 | 값 |\n| --- | --- |\n| <!--aisa:akfixture-12345678-->표 문답<!--/aisa:akfixture-12345678--> | 값 |\n\n[하위](#sub:' + childId + ')\n');
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
  // 노트와 하위 노트에 세션 칩을 달아, 칩의 「열기」로 세션 레일이 본문 옆에 뜨는지 잰다.
  for (const file of [rootNote, childNote]) {
    write(file, readFileSync(file, 'utf8').replace('type: free', `type: free\nsessionId: ${sessionId}`));
  }
  const stamp = new Date().toISOString();
  write(join(home, '.claude/projects/-Users-test-vault', `${sessionId}.jsonl`), [
    { type: 'user', entrypoint: 'cli', cwd: '/Users/test/vault', timestamp: stamp, message: { content: '세션 점검용 첫 물음' } },
    { type: 'assistant', timestamp: stamp, message: { model: 'claude-opus-4', content: [{ type: 'text', text: '세션 점검용 첫 답' }] } },
    { type: 'custom-title', customTitle: '점검 세션' },
  ].map(line => JSON.stringify(line)).join('\n') + '\n');
}
// `--only=<점검 이름,…>`: 편집기 점검은 그 사례만, 화면 점검(--views)은 그 점검만 돌린다(앞 점검이 깨졌을 때 한 점검을 따로 보려고)
const cfg = { only: viewsCheck ? onlyCases : undefined, part4Check, askMarksCheck, sessionId, parent: root, vault, raw: dataCheck || editorCheck || autosaveCheck || viewsCheck || resourcesCheck || tasksCheck || perfCheck ? vault : vault + '/.', id, legacyId, external, rootNote, childId, childNote, grandchildNote, navigationCheck, interactionCheck, dataCheck, editorCheck, autosaveCheck, viewsCheck, resourcesCheck, tasksCheck, perfCheck, perf: { notes: perfNotes }, today };
const quote = text => "'" + text.replaceAll("'", "'\\''") + "'";
// 앱이 엔진을 부르는 길은 둘이다: 문자열 목록을 받는 `sidecar_call`과 호출 객체를 받는 `engine_call`. 호스트가 두 길의 호출을 이 파일에 적고
// (`AISURADA_CALL_LOG`), 점검은 어느 길로 갔든 같은 문자열 목록(argv)으로 센다.
const hostCallLog = join(root, 'host-calls.jsonl');
const { toArgv } = await import(pathToFileURL(join(host, 'src/data/commands.ts')).href);
// 점검 준비(워크스페이스 초기화, 편집기 사례 심기)에 쓰는 ais-cli 실행 파일. 앱이 쓰는 것과 같은 엔진이다.
const cli = process.env.AISURADA_CLI_BIN || join(repo, 'engine-rs/target/debug/ais-cli');
if (!existsSync(cli)) throw Error('Build ais-cli first: (cd engine-rs && cargo build -p ais-cli)');
const env = { ...process.env, HOME: home, ZDOTDIR: join(root, 'zdot'), HISTFILE: join(root, 'history'),
  CARGO_HOME: process.env.CARGO_HOME || join(homedir(), '.cargo'), RUSTUP_HOME: process.env.RUSTUP_HOME || join(homedir(), '.rustup'),
  AISURADA_VAULT: vault, AISURADA_LOCK_DIR: join(root, 'locks'), AISURADA_CALL_LOG: hostCallLog, AISURADA_VAULT_AUTOCOMMIT_ENABLED: autosaveCheck || viewsCheck ? 'true' : 'false' };
if (askMarksCheck || tasksCheck) env.AISURADA_CLAUDE_BIN = join(root, 'fake-claude');
if (resourcesCheck) env.AISURADA_APPROVALS_DIR = join(root, 'approvals');
for (const key of ['CLAUDE_CODE_SESSION_ID', 'AISURADA_ASK_RUNTIME', 'AISURADA_ASK_USE_AGENT']) delete env[key];
// 점검 준비용 ais-cli 호출. 사람의 호출이다(AI 세션 변수는 위에서 지웠다). AI의 호출은 `ai`로 세션 변수를 준다.
const cliRun = async (args, extra = {}) => {
  const child = spawn(cli, args, { cwd: host, env: { ...env, ...extra } });
  let out = '', err = ''; child.stdout.on('data', chunk => { out += chunk; }); child.stderr.on('data', chunk => { err += chunk; });
  const code = await new Promise(resolve => child.on('exit', resolve));
  if (code) throw Error(`${args[0]} 실패(${code}): ${err}`);
  return out ? JSON.parse(out) : null;
};
const ai = { AISURADA_SESSION_ID: 'ai-session-1', AISURADA_SESSION_AGENT: 'codex' };
if (resourcesCheck) {
  // 스킬·에이전트와 승인: 사람이 만든 자원 셋, AI가 올린 대기 변경 둘, 승인 없이 파일을 직접 고친 자원 하나, 저장소가 있는 프로젝트 하나
  await cliRun(['init-workspace', 'name=검증 공간', 'templates=[]']);
  mkdir(join(root, 'repos/web'));
  cfg.resources = {};
  const project = await cliRun(['create-project', 'title=앱', `repos=${JSON.stringify([{ name: 'web', path: join(root, 'repos/web'), github: 'me/web' }])}`]);
  cfg.resources.projectSlug = project.slug;
  const skill = await cliRun(['save-resource', 'kind=skill', 'scope=global', 'name=번역 검수', 'description=번역을 검수한다', 'body=# 원래 지침']);
  const dirty = await cliRun(['save-resource', 'kind=skill', 'scope=global', 'name=직접 고칠 스킬', 'description=원래 설명', 'body=본문']);
  const agent = await cliRun(['save-resource', 'kind=agent', 'scope=global', 'name=리뷰어', 'description=리뷰한다', 'permission=read', 'body=지침']);
  Object.assign(cfg.resources, { skillId: skill.id, dirtyId: dirty.id, agentId: agent.id });
  const token = (await cliRun(['get-resource', skill.id])).precondition;
  const edit = await cliRun(['save-resource', `id=${skill.id}`, `f1=${token}`, 'body=# AI가 고친 지침', 'description=AI가 바꾼 설명'], ai);
  const proposal = await cliRun(['save-resource', 'kind=skill', 'scope=global', 'name=AI 제안 스킬', 'description=AI가 만들자고 한다', 'body=제안'], ai);
  if (!edit.queued || !proposal.queued) throw Error('AI 변경이 승인 대기에 오르지 않았다');
  Object.assign(cfg.resources, { editChangeId: edit.changeId, proposalChangeId: proposal.changeId });
  const dirtyFile = join(vault, dirty.path);
  writeFileSync(dirtyFile, readFileSync(dirtyFile, 'utf8').replace('description: 원래 설명', 'description: 직접 고친 설명'));
}
if (perfCheck) {
  // 합성 노트 N건(엔진 기준선 `perf_baseline`과 같은 모양: 한 건 약 1.5KB, 아젠다 10개, 월별 폴더). 진짜 vault 내용은 없다.
  const sentences = ['오늘은 배포 파이프라인의 성능을 다시 살펴보고 병목이 어디인지 정리했다.', '회의에서 나온 결정 사항은 아래 목록과 같고 각자 맡은 일을 이번 주 안에 끝내기로 했다.', '코드 리뷰에서 지적받은 부분을 고치고 시험을 다시 돌려 통과하는 것을 확인했다.', '다음 단계로 넘어가기 전에 위험 요소를 적어 두고 되돌리는 방법도 같이 남겼다.', '사용자가 보낸 의견을 읽고 화면의 문구와 순서를 손보기로 했다.', '데이터가 늘어났을 때 목록과 검색이 얼마나 느려지는지는 아직 재 보지 않았다.', '참고한 문서와 링크는 맨 아래에 모았고 나중에 다시 찾기 쉽게 이름을 붙였다.', '오래 걸리는 작업은 쪼개서 하루 단위로 확인할 수 있게 계획을 세웠다.'];
  await cliRun(['init-workspace', 'name=검증 공간', 'templates=[]']);
  for (let a = 0; a < 10; a++) write(join(vault, `.aisurada/agendas/a${a}.md`), `---\nid: agperf${a}-${a.toString(16).padStart(8, '0')}\nslug: a${a}\ntitle: 아젠다 ${a}\n---\n# 아젠다 ${a}\n`);
  for (let i = 0; i < perfNotes; i++) {
    let body = `# 기록 ${i}\n\n`;
    for (let k = 0; k < 6; k++) { body += sentences[(i + k) % sentences.length] + ' '; if (k % 2 === 1) body += '\n\n'; }
    if (i % 7 === 0) body += '\n성능 점검 항목을 따로 적는다.\n';
    if (i === Math.floor(perfNotes / 2)) { body += '\n이 노트에만 희귀어가 들어 있다.\n'; cfg.perfTarget = `ndperf${i}-${i.toString(16).padStart(8, '0')}`; }
    const month = String(1 + i % 12).padStart(2, '0'), day = String(1 + i % 28).padStart(2, '0');
    write(join(vault, `notes/a${i % 10}/2026-${month}/n${i}/note.md`), `---\nid: ndperf${i}-${i.toString(16).padStart(8, '0')}\nagenda: a${i % 10}\ntype: free\ntitle: 기록 ${i}\ncreated: '2026-${month}-${day}T09:30:00+09:00'\ntags: [tag${i % 20}, 공통]\n---\n${body}`);
  }
}
if (tasksCheck) {
  // 태스크: 저장소가 있는 프로젝트 하나와 Draft 태스크 하나. 러너의 저장소 받기는 GitHub 앱 자격이 있어야 해서 격리 환경에서는 늘 실패한다.
  await cliRun(['init-workspace', 'name=검증 공간', 'templates=[]']);
  await cliRun(['create-project', 'title=앱', `repos=${JSON.stringify([{ name: 'web', github: 'me/web' }])}`]);
  const seeded = await cliRun(['create-task', 'title=저장소 읽기 조사', 'repo=me/web', 'type=chore', 'criteria=["결론을 댓글로 남긴다"]', 'body=조사할 내용을 적어 둔 설명']);
  for (const key of ['orchestrator', 'dev', 'qa', 'pr-review-merge']) {
    const tools = JSON.stringify({ aisurada: { key, engine: 'claude', model: 'sonnet', enabled: true } });
    await cliRun(['save-resource', 'kind=agent', 'scope=global', `name=${key}`, 'description=실행 점검용', 'permission=edit', 'body=실행 점검용 지침', `tools=${tools}`]);
  }
  cfg.tasks = { seededKey: seeded.key };
}
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
    if (path === '/progress') console.log(`${value.ok ? 'PASS' : 'FAIL'} ${value.id}${value.error ? ' ' + value.error : ''}${value.detail ? ' ' + JSON.stringify(value.detail) : ''}`);
    res.writeHead(200, { 'Content-Type': 'application/json' }); res.end('{}'); return;
  }
  if (path === '/rss') {
    // 호스트 프로세스(엔진이 안에서 돈다)의 상주 메모리. 화면을 그리는 웹뷰는 다른 프로세스라 여기 안 들어 있다.
    const ps = spawnSync('ps', ['-axo', 'rss=,command=']).stdout.toString().split('\n').filter(line => /target\/(release|debug)\/aisurada-host/.test(line));
    res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify({ hostMb: ps.reduce((sum, line) => sum + Number(line.trim().split(/\s+/)[0]), 0) / 1024 })); return;
  }
  if (path === '/note-result') { res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(noteResult || null)); return; }
  if (path === '/editor-cases') { res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(editorCases)); return; }
  if (path === '/calls') {
    const lines = existsSync(hostCallLog) ? readFileSync(hostCallLog, 'utf8').trim().split('\n').filter(Boolean).map(line => JSON.parse(line)) : [];
    const commands = lines.map(call => ({ args: { argv: call.kind === 'sidecar' ? call.argv : toArgv({ command: call.command, args: call.args }) }, start: call.start }));
    res.writeHead(200, { 'Content-Type': 'application/json' }); res.end(JSON.stringify(commands)); return;
  }
  if (path === '/bootstrap.js' || path === '/workspace-checks.js') {
    res.writeHead(200, { 'Content-Type': 'text/javascript' });
    res.end(path === '/bootstrap.js' ? boot : dataCheck || autosaveCheck ? dataDriver : readFileSync(join(host, perfCheck ? 'scripts/perf-checks.js' : resourcesCheck ? 'scripts/resources-checks.js' : tasksCheck ? 'scripts/tasks-checks.js' : askMarksCheck ? 'scripts/part4-checks.js' : viewsCheck ? 'scripts/views-checks.js' : editorCheck ? 'scripts/editor-checks.js' : interactionCheck ? 'scripts/interaction-checks.js' : navigationCheck ? 'scripts/navigation-checks.js' : 'scripts/workspace-checks.js'))); return;
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
  if (editorCheck) {
    const initialized = spawn(cli, ['init-workspace', 'name=검증 공간', 'templates=[]'], { cwd: host, env });
    let initializationError = ''; initialized.stdout.resume(); initialized.stderr.on('data', chunk => { initializationError += chunk; });
    if (await new Promise(resolve => initialized.on('exit', resolve))) throw Error(initializationError);
    // Seed through the real engine before the UI starts, so watch reads cannot
    // mix with the roundtrip measurements. These are independent PDF fixtures.
    for (const c of editorCases) {
      c.original = `# 왕복 ${c.id}\n\n${c.input}\n\n왕복 사례 끝`;
      const child = spawn(cli, ['create-doc', 'agenda=demo', 'type=free', `title=왕복 ${c.id}`, `body=${c.original}`, 'format=markdown'], { cwd: host, env });
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
    : start(join(host, 'node_modules/.bin/tauri'), ['dev', ...(perfCheck ? ['--release'] : []), '--config', override, '--no-watch', '--', ...(claudeHost ? [] : ['--no-default-features'])]);
  app.on('exit', code => { if (code) finish([{ id: 'native-process', ok: false, error: `exited ${code}` }]); });
  timer = setTimeout(() => finish([{ id: 'timeout', ok: false, error: 'No page result within fixture deadline' }]), perfCheck ? 1_800_000 : editorCheck || autosaveCheck || viewsCheck || resourcesCheck || tasksCheck ? 420_000 : 180_000);
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
