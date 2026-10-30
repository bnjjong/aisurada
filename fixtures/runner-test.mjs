#!/usr/bin/env node
// fixtures/expect 실행기를 재는 시험. 설명서 34.1의 옵션 문법, 무결성, --out, 거르개, 어댑터, 준비 걸음,
// 판정 9가지, 경고, 구현 환경, 신호를 시험용 작은 묶음과 대본대로 답하는 가짜 엔진으로 잰다.
// 마지막에 진짜 묶음의 자가 점검(--mutants)과, dist/wl-engine.jar가 있으면 이 엔진 회차를 돈다.
//
//   node fixtures/runner-test.mjs
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import crypto from 'node:crypto';
import { spawn, spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));
const BUNDLE = path.join(HERE, 'expect');
const REPO = path.dirname(HERE);
const TMP = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), 'wlx-runner-test-')));
let pass = 0;
let fail = 0;

function check(desc, actual, expected) {
  const a = JSON.stringify(actual);
  const e = JSON.stringify(expected);
  if (a === e) {
    pass++;
    console.log(`  ✓ ${desc}`);
  } else {
    fail++;
    console.log(`  ✗ ${desc}\n     기대: ${e}\n     실제: ${a}`);
  }
}

let outCount = 0;
const freshOut = () => path.join(TMP, 'out', `o${++outCount}`);

function runner(bundle, args, env = {}) {
  const r = spawnSync(process.execPath, [path.join(bundle, 'run.mjs'), ...args], {
    encoding: 'utf8', env: { PATH: process.env.PATH, ...env }, cwd: TMP
  });
  return { code: r.status, stderr: r.stderr, stdout: r.stdout };
}
const why = r => (r.stderr.match(/실행기가 못 돌았다: (.*)/) || [])[1] ?? null;
const report = out => JSON.parse(fs.readFileSync(path.join(out, 'report.json'), 'utf8'));

// ---------------------------------------------------------------------------
// 시험용 묶음과 가짜 엔진
// ---------------------------------------------------------------------------

const sha = b => crypto.createHash('sha256').update(b).digest('hex');

/** 진짜 묶음의 실행기를 복사하고 나무와 레인을 갈아 끼운 묶음. bundle.json의 sha256을 다시 적는다. */
function makeBundle(name, lanes, trees) {
  const dir = path.join(TMP, 'bundles', name);
  fs.mkdirSync(dir, { recursive: true });
  for (const rel of ['run.mjs', 'lib', 'selftest', 'adapters', 'README.md', '.gitattributes']) {
    fs.cpSync(path.join(BUNDLE, rel), path.join(dir, rel), { recursive: true });
  }
  for (const [treeName, tree] of Object.entries(trees)) writeJson(path.join(dir, 'trees', `${treeName}.json`), tree);
  for (const [rel, doc] of Object.entries(lanes)) writeJson(path.join(dir, 'lanes', rel), doc);
  rehash(dir);
  return dir;
}

function rehash(dir) {
  const files = {};
  const walk = rel => {
    for (const n of fs.readdirSync(path.join(dir, rel))) {
      const r = rel ? `${rel}/${n}` : n;
      if (fs.statSync(path.join(dir, r)).isDirectory()) walk(r);
      else if (r !== 'bundle.json') files[r] = sha(fs.readFileSync(path.join(dir, r)));
    }
  };
  walk('');
  writeJson(path.join(dir, 'bundle.json'), { format: 'wlx-bundle', version: 1, node: '>=20', note: 'test', counts: {}, provenance: {}, history: [], files });
}

function writeJson(file, value) {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  fs.writeFileSync(file, JSON.stringify(value, null, 2) + '\n');
}

const TINY = { format: 'wlx-tree', version: 1, source: 'test', note: '', dirs: ['notes', 'empty'], files: [
  { path: 'notes/a.md', lines: ['---', 'id: a', '---', '본문'], eol: true },
  { path: 'bin.dat', base64: Buffer.from([0xff, 0x00, 0xfe]).toString('base64') }
] };
const HOME_TREE = { format: 'wlx-tree', version: 1, source: 'test', note: '', dirs: ['.claude'], files: [{ path: '.claude/x.txt', lines: ['h'], eol: false }] };

const lane = (cases, extra = {}) => ({ format: 'wlx-cases', version: 1, lane: 'read', note: '', tree: 'tiny.v1', prepare: [['reindex']], cases, ...extra });
const kase = (id, argv, expect, part = 1) => ({ id, part, argv, expect });

// 가짜 엔진: 받은 argv를 대본(WLX_IMPL_SCRIPT의 JSON)에서 찾아 그대로 답한다
const FAKE = path.join(TMP, 'fake-engine.mjs');
fs.writeFileSync(FAKE, `
import fs from 'node:fs';
import path from 'node:path';
const argv = process.argv.slice(2);
const script = JSON.parse(fs.readFileSync(process.env.WLX_IMPL_SCRIPT, 'utf8'));
const a = script[JSON.stringify(argv)] ?? (argv[0] === 'reindex' ? { code: 0 } : { code: 99, stderr: 'no script for ' + JSON.stringify(argv) });
if (a.writeVault) fs.writeFileSync(path.join(process.env.WITHLOG_VAULT, a.writeVault), 'x');
if (a.pidFile) fs.writeFileSync(a.pidFile, String(process.pid));
let stdin = '';
try { stdin = fs.readFileSync(0, 'utf8'); } catch { stdin = 'unreadable'; }
let out = a.stdout ?? '';
if (a.envDump) out = JSON.stringify({ vault: process.env.WITHLOG_VAULT, home: process.env.HOME, tz: process.env.TZ, lang: process.env.LANG,
  lcAll: process.env.LC_ALL, state: !!process.env.WLX_STATE_DIR, tmp: !!process.env.TMPDIR, impl: process.env.WLX_IMPL_EXTRA ?? null,
  leak: process.env.LEAK_ME ?? null, stdin, cwd: process.cwd() }, null, 2) + '\\n';
if (a.stderr) process.stderr.write(a.stderr + '\\n');
const bytes = a.stdoutBase64 ? Buffer.from(a.stdoutBase64, 'base64') : Buffer.from(out, 'utf8');
const finish = () => process.stdout.write(bytes, () => process.exit(a.code));
if (a.sleepMs) setTimeout(finish, a.sleepMs); else finish();
`);
const ADAPTER = path.join(TMP, 'fake.adapter.json');
writeJson(ADAPTER, { format: 'wlx-adapter', version: 1, surface: 'engine', name: 'fake', prefix: [process.execPath, FAKE], suffix: ['--x=${WITHLOG_VAULT}'] });

function script(map) {
  const file = path.join(TMP, `script-${crypto.randomUUID()}.json`);
  const obj = {};
  for (const [argv, answer] of map) obj[JSON.stringify(argv)] = answer;
  writeJson(file, obj);
  return file;
}
/** 가짜 엔진은 어댑터의 suffix(--x=<vault>)까지 argv로 받는다. */
const withSuffix = (argv, vault) => [...argv, `--x=${vault}`];

// ---------------------------------------------------------------------------
console.log('[1] 옵션 문법(34.1): 어기면 종료 코드 2');
{
  const bad = [
    [['--self-check', '--out', freshOut(), '--jobs=4'], '모르는 옵션이다: --jobs=4'],
    [['--self-check', '--out'], '옵션 값이 빠졌다: --out'],
    [['--self-check', '--out', '--jobs', '2'], '옵션 값이 빠졌다: --out'],
    [['--self-check', '--self-check', '--out', freshOut()], '옵션을 다시 줬다: --self-check'],
    [['--engine', ADAPTER, '--out', freshOut(), '--only', 'a,,b'], '빈 목록 항목이 있다: --only a,,b'],
    [['--self-check', '--out', freshOut(), '--jobs', 'x'], '정수가 아니다: --jobs x'],
    [['--self-check', '--out', freshOut(), '--jobs', '1.5'], '정수가 아니다: --jobs 1.5'],
    [['--self-check', '--out', freshOut(), '--jobs', '0'], '--jobs 는 1 이상이어야 한다: 0'],
    [['--self-check'], '--out 이 필요하다'],
    [['--out', freshOut()], '--engine 이 필요하다'],
    [['--self-check', '--engine', ADAPTER, '--out', freshOut()], '--self-check 는 --engine 와 함께 못 쓴다'],
    [['--mutants', '--engine', ADAPTER, '--out', freshOut()], '--mutants 는 --self-check 와 함께만 쓴다'],
    [['--engine', ADAPTER, '--out', freshOut(), 'extra'], '모르는 옵션이다: extra']
  ];
  for (const [args, msg] of bad) {
    const r = runner(BUNDLE, args);
    check(`${args.join(' ')} → 2`, [r.code, why(r)], [2, msg]);
  }
  const loose = runner(BUNDLE, ['--self-check', '--out', freshOut(), '--jobs', ' 4.0 ']);
  check('정수는 Number로 읽어 " 4.0 "도 4', loose.code, 0);
}

console.log('\n[2] 묶음 무결성');
{
  const copy = makeBundle('integrity', { 'read/t.json': lane([kase('t/a', ['t'], { code: 0, stdout: '' })]) }, { 'tiny.v1': TINY });
  fs.writeFileSync(path.join(copy, '.DS_Store'), 'x');
  fs.writeFileSync(path.join(copy, 'lanes', '._t.json'), 'x');
  check('운영체제 파일(.DS_Store, ._*)은 안 센다', runner(copy, ['--self-check', '--out', freshOut()]).code, 0);
  fs.appendFileSync(path.join(copy, 'lanes', 'read', 't.json'), ' ');
  check('목록과 sha256 이 다르면 2', why(runner(copy, ['--self-check', '--out', freshOut()])), '묶음 파일의 sha256 이 목록과 다르다: lanes/read/t.json');
  rehash(copy);
  fs.writeFileSync(path.join(copy, 'extra.txt'), 'x');
  check('목록에 없는 파일이 있으면 2', why(runner(copy, ['--self-check', '--out', freshOut()])), '목록에 없는 파일이 묶음에 있다: extra.txt');
  fs.rmSync(path.join(copy, 'extra.txt'));
  fs.rmSync(path.join(copy, 'README.md'));
  check('목록의 파일이 없으면 2', why(runner(copy, ['--self-check', '--out', freshOut()])), '묶음 파일이 없다: README.md');
}

console.log('\n[3] --out');
{
  const full = freshOut();
  fs.mkdirSync(full, { recursive: true });
  fs.writeFileSync(path.join(full, 'x'), 'x');
  check('비어 있지 않으면 2', why(runner(BUNDLE, ['--self-check', '--out', full])), `--out 폴더가 비어 있지 않다: ${full}`);
  check('폴더가 아니면 2', why(runner(BUNDLE, ['--self-check', '--out', path.join(full, 'x')])), `--out 이 폴더가 아니다: ${path.join(full, 'x')}`);
  check('묶음 폴더 안이면 2', runner(BUNDLE, ['--self-check', '--out', path.join(BUNDLE, 'zz-out')]).code, 2);
  check('그때 묶음 안에 폴더를 안 남긴다', fs.existsSync(path.join(BUNDLE, 'zz-out')), false);
  const vault = path.join(TMP, 'parent-vault');
  fs.mkdirSync(vault, { recursive: true });
  check('부모 셸의 WITHLOG_VAULT 안이면 2', runner(BUNDLE, ['--self-check', '--out', path.join(vault, 'o')], { WITHLOG_VAULT: vault }).code, 2);
  check('부모 셸의 WITHLOG_VAULT 자체(빈 폴더)여도 2', why(runner(BUNDLE, ['--self-check', '--out', vault], { WITHLOG_VAULT: vault })), `--out 이 WITHLOG_VAULT와 겹친다: ${vault}`);
  const link = path.join(TMP, 'link-to-vault');
  fs.symlinkSync(vault, link);
  const viaLink = runner(BUNDLE, ['--self-check', '--out', path.join(link, 'n')], { WITHLOG_VAULT: vault });
  check('링크로 돌아 들어가도 장치와 inode로 잡는다', [viaLink.code, fs.existsSync(path.join(vault, 'n'))], [2, false]);
  const upper = path.join(BUNDLE.replace(/expect$/, 'EXPECT'), 'zz-upper');
  if (fs.existsSync(path.dirname(upper))) {
    const r = runner(BUNDLE, ['--self-check', '--out', upper]);
    check('대소문자만 다른 묶음 경로도 겹침이다(대소문자를 안 가리는 파일 시스템)', [r.code, fs.existsSync(path.join(BUNDLE, 'zz-upper'))], [2, false]);
  }
}

console.log('\n[4] 거르개와 케이스 꼴');
{
  const copy = makeBundle('select', { 'read/t.json': lane([
    kase('get_doc/a', ['a'], { code: 0, stdout: '' }),
    kase('session/b', ['b'], { code: 0, stdout: '' }, 4)
  ]) }, { 'tiny.v1': TINY });
  const run = args => why(runner(copy, ['--engine', ADAPTER, '--out', freshOut(), ...args], { WLX_IMPL_SCRIPT: script([]) }));
  check('--lanes 에 없는 레인', run(['--lanes', 'write']), '--lanes 의 항목에 걸리는 케이스가 없다: write');
  check('--parts 에 안 걸리는 파트', run(['--parts', '1,2']), '--parts 의 항목에 걸리는 케이스가 없다: 2');
  check('--only 에 안 걸리는 머리', run(['--only', 'get_doc/,zz']), '--only 의 항목에 걸리는 케이스가 없다: zz');
  check('항목마다는 걸려도 함께 0건이면 2', run(['--parts', '4', '--only', 'get_doc/']), '고른 케이스가 없다');
  const badCase = makeBundle('badcase', { 'read/t.json': lane([kase('x', ['x'], { code: 0, stdout: '', stdoutText: '' })]) }, { 'tiny.v1': TINY });
  check('stdout 꼴이 2개면 구현을 부르기 전에 2', why(runner(badCase, ['--engine', ADAPTER, '--out', freshOut()])), 'lanes/read/t.json 의 1번째 케이스 (x): stdoutJson, stdoutText, stdout 가운데 정확히 1개가 있어야 한다');
  const badPh = makeBundle('badph', { 'read/t.json': lane([kase('x', ['x'], { code: 0, stdoutText: '⟦nope⟧' })]) }, { 'tiny.v1': TINY });
  check('모르는 자리표는 2', why(runner(badPh, ['--engine', ADAPTER, '--out', freshOut()])), 'lanes/read/t.json 의 1번째 케이스 (x): 모르는 자리표다: ⟦nope⟧');
  const dupId = makeBundle('dupid', { 'read/a.json': lane([kase('x', ['x'], { code: 0, stdout: '' })]), 'read/b.json': lane([kase('x', ['y'], { code: 0, stdout: '' })]) }, { 'tiny.v1': TINY });
  check('id 가 겹치면 2', why(runner(dupId, ['--engine', ADAPTER, '--out', freshOut()])), 'lanes/read/b.json 의 1번째 케이스: id 가 겹친다: x');
  const badTree = makeBundle('badtree', { 'read/t.json': lane([kase('x', ['x'], { code: 0, stdout: '' })]) }, { 'tiny.v1': { ...TINY, files: [{ path: '../evil', lines: [], eol: false }] } });
  check('나무 경로가 밖을 가리키면 2', why(runner(badTree, ['--engine', ADAPTER, '--out', freshOut()])), 'trees/tiny.v1.json: 펼칠 자리 밖을 가리키는 경로다: "../evil"');
}

console.log('\n[5] 어댑터와 준비 걸음');
{
  const copy = makeBundle('adapter', { 'read/t.json': lane([kase('t/a', ['a'], { code: 0, stdout: '' })]) }, { 'tiny.v1': TINY });
  const ad = (name, doc) => { const f = path.join(TMP, name); writeJson(f, doc); return f; };
  const run = (adapter, env = {}) => runner(copy, ['--engine', adapter, '--out', freshOut()], env);
  check('어댑터 파일이 없으면 2', run(path.join(TMP, 'none.json')).code, 2);
  check('format 이 틀리면 2', why(run(ad('a1.json', { format: 'x', version: 1, surface: 'engine', prefix: ['a'] }))), '어댑터는 format wlx-adapter, version 1, surface engine 이어야 한다');
  check('prefix 가 비면 2', why(run(ad('a2.json', { format: 'wlx-adapter', version: 1, surface: 'engine', prefix: [] }))), '어댑터의 prefix 는 1칸 이상의 문자열 배열이어야 한다');
  check('환경에 없는 ${이름}은 첫 준비 걸음에서 2', why(run(ad('a3.json', { format: 'wlx-adapter', version: 1, surface: 'engine', prefix: ['${WLX_IMPL_NOPE}'] }))), '어댑터의 ${WLX_IMPL_NOPE} 를 채울 환경 변수가 없다');
  check('구현을 못 띄우면 준비 걸음에서 2', run(ad('a4.json', { format: 'wlx-adapter', version: 1, surface: 'engine', prefix: [path.join(TMP, 'no-such-bin')] })).code, 2);
  const failPrep = script([[['reindex'], { code: 7, stderr: 'WL-500 색인을 못 지었다' }]]);
  check('준비 걸음이 0이 아니면 2', why(runner(copy, ['--engine', ad('a5.json', { format: 'wlx-adapter', version: 1, surface: 'engine', prefix: [process.execPath, FAKE] }), '--out', freshOut()], { WLX_IMPL_SCRIPT: failPrep })), '준비 걸음 ["reindex"] 이 종료 코드 7 로 끝났다. stderr: WL-500 색인을 못 지었다');
  const noName = ad('my-engine.adapter.json', { format: 'wlx-adapter', version: 1, surface: 'engine', prefix: [process.execPath, FAKE], suffix: ['--x=${WITHLOG_VAULT}'], unknownKey: 1 });
  const o = freshOut();
  const vault = path.join(o, 'read-0', 'vault');
  runner(copy, ['--engine', noName, '--out', o], { WLX_IMPL_SCRIPT: script([[withSuffix(['a'], vault), { code: 0 }]]) });
  check('name 이 없으면 어댑터 파일 이름을 확장자까지, 모르는 키는 무시', report(o).engine, 'my-engine.adapter.json');
  check('펼친 vault 는 realpath 를 지난 경로이고 빈 폴더도 만든다', [fs.existsSync(path.join(vault, 'empty')), fs.readFileSync(path.join(vault, 'bin.dat')).toString('hex')], [true, 'ff00fe']);
}

console.log('\n[6] 판정 9가지와 보고서');
{
  const tok = '⟦token:notes/a.md⟧';
  const cases = [
    kase('ok/json', ['ok-json'], { code: 0, stdoutJson: { a: 1, link: 'w://x?ws=⟦vault-uri⟧', p: tok } }),
    kase('ok/text', ['ok-text'], { code: 0, stdoutText: 'vault=⟦vault⟧ home=⟦home⟧\n' }),
    kase('ok/share', ['ok-share'], { code: 0, stdoutJson: { p: tok } }),
    kase('bad/code', ['code'], { code: 0, stdout: '' }),
    kase('bad/wl', ['wl'], { code: 3, stdout: '', wl: 'WL-404' }),
    kase('bad/utf8', ['utf8'], { code: 0, stdout: '' }),
    kase('bad/json', ['json'], { code: 0, stdoutJson: { a: 1 } }),
    kase('bad/keys', ['keys'], { code: 0, stdoutJson: { a: 1, b: 2 } }),
    kase('bad/negzero', ['negzero'], { code: 0, stdoutJson: { n: 0 } }),
    kase('bad/format', ['format'], { code: 0, stdoutJson: { n: 1 } }),
    kase('bad/bom', ['bom'], { code: 0, stdoutJson: { a: 1 } }),
    kase('bad/token-inner', ['token-inner'], { code: 0, stdoutJson: { x: tok, y: tok } }),
    kase('bad/token-space', ['token-space'], { code: 0, stdoutJson: { p: tok } }),
    kase('bad/token-cross', ['token-cross'], { code: 0, stdoutJson: { p: tok } })
  ];
  const copy = makeBundle('judge', { 'read/t.json': lane(cases, { home: 'home.v1' }) }, { 'tiny.v1': TINY, 'home.v1': HOME_TREE });
  const o = freshOut();
  const vault = path.join(o, 'read-0', 'vault');
  const home = path.join(o, 'read-0', 'home');
  const uri = encodeURIComponent(vault);
  const J = v => JSON.stringify(v, null, 2) + '\n';
  const s = script([
    [withSuffix(['reindex'], vault), { code: 0 }],
    [withSuffix(['ok-json'], vault), { code: 0, stdout: J({ a: 1, link: `w://x?ws=${uri}`, p: 'f1:abc' }) }],
    [withSuffix(['ok-text'], vault), { code: 0, stdout: `vault=${vault} home=${home}\n` }],
    [withSuffix(['ok-share'], vault), { code: 0, stdout: J({ p: 'f1:abc' }), sleepMs: 300 }],
    [withSuffix(['code'], vault), { code: 3, stderr: '로그 줄\nWL-404 Not found: x\n마지막 줄' }],
    [withSuffix(['wl'], vault), { code: 3, stderr: 'WL-400 다른 번호' }],
    [withSuffix(['utf8'], vault), { code: 0, stdoutBase64: Buffer.from([0xc3, 0x28]).toString('base64') }],
    [withSuffix(['json'], vault), { code: 0, stdout: '{"a":' }],
    [withSuffix(['keys'], vault), { code: 0, stdout: J({ b: 2, a: 1 }) }],
    [withSuffix(['negzero'], vault), { code: 0, stdout: '{\n  "n": -0\n}\n' }],
    [withSuffix(['format'], vault), { code: 0, stdout: '{\n  "n": 1.0\n}\n' }],
    [withSuffix(['bom'], vault), { code: 0, stdout: '\uFEFF' + J({ a: 1 }) }],
    [withSuffix(['token-inner'], vault), { code: 0, stdout: J({ x: 'f1:abc', y: 'f1:def' }) }],
    [withSuffix(['token-space'], vault), { code: 0, stdout: J({ p: 'f1 abc' }) }],
    [withSuffix(['token-cross'], vault), { code: 0, stdout: J({ p: 'f1:zzz' }), sleepMs: 900 }]
  ]);
  const r = runner(copy, ['--engine', ADAPTER, '--out', o, '--jobs', '4'], { WLX_IMPL_SCRIPT: s });
  const rep = report(o);
  const byId = Object.fromEntries(rep.failures.map(f => [f.id, f]));
  check('어긋난 케이스가 있으면 종료 코드 1', r.code, 1);
  check('맞은 케이스: JSON 자리표, 글 자리표(⟦vault⟧, ⟦home⟧), 같은 토큰', ['ok/json', 'ok/text', 'ok/share'].map(id => byId[id] ?? 'pass'), ['pass', 'pass', 'pass']);
  check('검사 이름', Object.fromEntries(rep.failures.map(f => [f.id, f.check])), {
    'bad/code': 'code', 'bad/wl': 'wl', 'bad/utf8': 'utf8', 'bad/json': 'stdout', 'bad/keys': 'stdout', 'bad/negzero': 'stdout',
    'bad/format': 'format', 'bad/bom': 'stdout', 'bad/token-inner': 'token', 'bad/token-space': 'stdout', 'bad/token-cross': 'token'
  });
  check('code 의 detail 은 stderr 에서 WL 번호가 든 첫 줄을 고른다', byId['bad/code'].detail, '종료 코드 기대 0, 실제 3. stderr: WL-404 Not found: x');
  check('failures 는 케이스 차례', rep.failures.map(f => f.id)[0], 'bad/code');
  check('totals', [rep.totals.cases, rep.totals.passed, rep.totals.failed, rep.totals.launches], [14, 3, 11, 15]);
  check('report.json 키 차례', [Object.keys(rep), Object.keys(rep.totals)], [['engine', 'totals', 'failures', 'warnings'], ['cases', 'passed', 'failed', 'launches', 'wallMs']]);
  check('stderr 요약 첫 줄', r.stderr.split('\n')[0].replace(/[0-9.]+초\)$/, 'N초)'), '엔진 fake: 케이스 14건 중 3건 맞음, 11건 어긋남 (구현 호출 15번, N초)');
  check('요약에 어긋난 케이스 줄', r.stderr.includes('  ✗ bad/wl [wl] 기대 WL-404, 실제 WL-400'), true);
}

console.log('\n[7] 구현 환경과 경고');
{
  const copy = makeBundle('env', { 'read/t.json': lane([
    kase('env/dump', ['env'], { code: 0, stdoutJson: { vault: '⟦vault⟧', home: '⟦home⟧', tz: 'Asia/Seoul', lang: 'en_US.UTF-8', lcAll: 'en_US.UTF-8', state: true, tmp: true, impl: 'yes', leak: null, stdin: '', cwd: TMP } }),
    kase('env/write', ['write'], { code: 0, stdout: '' })
  ]) }, { 'tiny.v1': TINY });
  const o = freshOut();
  const vault = path.join(o, 'read-0', 'vault');
  const s = script([
    [withSuffix(['env'], vault), { code: 0, envDump: true }],
    [withSuffix(['write'], vault), { code: 0, writeVault: 'notes/new.md' }]
  ]);
  const r = runner(copy, ['--engine', ADAPTER, '--out', o], { WLX_IMPL_SCRIPT: s, WLX_IMPL_EXTRA: 'yes', LEAK_ME: 'secret', WITHLOG_VAULT: path.join(TMP, 'parent-vault') });
  const rep = report(o);
  check('PATH 와 WLX_IMPL_* 만 넘어가고, 부모의 WITHLOG_VAULT 와 다른 변수는 안 닿는다. stdin 은 빈 입력, 작업 폴더는 실행기의 것', rep.failures, []);
  check('읽기 레인이 vault 를 바꾸면 경고로만 남고 종료 코드는 0', [r.code, rep.warnings], [0, ['vault notes/new.md: 파일이 새로 생겼다']]);
  check('요약에 경고 줄', r.stderr.includes('  ⚠ vault notes/new.md: 파일이 새로 생겼다'), true);
}

console.log('\n[8] 신호');
await (async () => {
  const copy = makeBundle('signal', { 'read/t.json': lane([kase('s/slow', ['slow'], { code: 0, stdout: '' })]) }, { 'tiny.v1': TINY });
  const o = freshOut();
  const pidFile = path.join(TMP, 'slow.pid');
  const vault = path.join(o, 'read-0', 'vault');
  const s = script([[withSuffix(['slow'], vault), { code: 0, sleepMs: 60000, pidFile }]]);
  const child = spawn(process.execPath, [path.join(copy, 'run.mjs'), '--engine', ADAPTER, '--out', o], { env: { PATH: process.env.PATH, WLX_IMPL_SCRIPT: s }, stdio: 'ignore' });
  const deadline = Date.now() + 20000;
  while (!fs.existsSync(pidFile) && Date.now() < deadline) await new Promise(res => setTimeout(res, 100));
  const pid = Number(fs.readFileSync(pidFile, 'utf8'));
  child.kill('SIGTERM');
  const code = await new Promise(res => child.on('exit', c => res(c)));
  await new Promise(res => setTimeout(res, 300));
  let alive = true;
  try { process.kill(pid, 0); } catch { alive = false; }
  check('SIGTERM 을 받으면 128+15 로 끝나고 도는 구현을 끝낸다', [code, alive], [143, false]);
})();

console.log('\n[9] 진짜 묶음: 자가 점검과 변형 8가지');
{
  const r = runner(BUNDLE, ['--self-check', '--mutants', '--out', freshOut()]);
  check('자가 점검 종료 코드 0, 변형 8가지를 모두 잡음', [r.code, /변형 8가지 가운데 8가지를 잡았다/.test(r.stderr)], [0, true]);
}

const jar = path.join(REPO, 'dist', 'wl-engine.jar');
if (fs.existsSync(jar)) {
  console.log('\n[10] 진짜 묶음: 이 엔진(dist/wl-engine.jar)');
  const javaBin = spawnSync('sh', ['-c', 'command -v java'], { encoding: 'utf8' }).stdout.trim();
  const o = freshOut();
  const r = runner(BUNDLE, ['--engine', path.join(HERE, 'aisurada.adapter.json'), '--out', o], { WLX_IMPL_JAVA: javaBin, WLX_IMPL_JAR: jar });
  const rep = report(o);
  check('20건이 모두 맞고 vault 와 HOME 을 안 바꾼다', [r.code, rep.totals.passed, rep.failures.map(f => `${f.id} [${f.check}] ${f.detail}`), rep.warnings], [0, 20, [], []]);
}

fs.rmSync(TMP, { recursive: true, force: true });
console.log(`\nRUNNER TEST SUMMARY: Passed: ${pass}, Failed: ${fail}`);
process.exit(fail === 0 ? 0 : 1);
