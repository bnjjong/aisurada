#!/usr/bin/env node
// 설명서 34장 받아들임 검사의 실행기. 케이스마다 구현을 셸 없이 불러 종료 코드와 stdout 바이트를 맞댄다.
// node 20 이상의 내장 모듈만 쓴다. 이 폴더만 복사해도 돈다.
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { RunnerError, expandTree, compareWithTree } from './lib/tree.mjs';
import { checkIntegrity, laneNames, readLaneFiles } from './lib/bundle.mjs';
import { compareJson, compareText, checkTokensWithin, firstLineDiff, renderJson, tokenPaths } from './lib/template.mjs';

const BUNDLE = path.dirname(fileURLToPath(import.meta.url));
const CALL_TIMEOUT_MS = 120_000;
const DRAIN_MS = 5_000;
const SUMMARY_FAILURES = 20;
const SUMMARY_WARNINGS = 10;
const running = new Set();

const OPTIONS = {
  '--engine': 'value', '--out': 'value', '--lanes': 'list', '--parts': 'ints', '--only': 'list', '--jobs': 'int',
  '--self-check': 'flag', '--mutants': 'flag'
};

// ---------------------------------------------------------------------------
// 1. 옵션
// ---------------------------------------------------------------------------

/** 이름 뒤 칸에 값을 둔 꼴만 받는다. 모르는 옵션, 값이 빠진 옵션, 다시 준 옵션, 빈 목록 항목, 정수가 아닌 값은 선다. */
function parseOptions(argv) {
  const opts = {};
  for (let i = 0; i < argv.length; i++) {
    const name = argv[i];
    const kind = OPTIONS[name];
    if (!kind) throw new RunnerError(`모르는 옵션이다: ${name}`);
    if (name in opts) throw new RunnerError(`옵션을 다시 줬다: ${name}`);
    if (kind === 'flag') {
      opts[name] = true;
      continue;
    }
    const raw = argv[i + 1];
    if (raw === undefined || raw.startsWith('--')) throw new RunnerError(`옵션 값이 빠졌다: ${name}`);
    i++;
    if (kind === 'value') opts[name] = raw;
    else if (kind === 'int') opts[name] = toInt(raw, name);
    else {
      const items = raw.split(',');
      if (items.some(x => x === '')) throw new RunnerError(`빈 목록 항목이 있다: ${name} ${raw}`);
      opts[name] = kind === 'ints' ? items.map(x => toInt(x, name)) : items;
    }
  }
  if (!('--out' in opts)) throw new RunnerError('--out 이 필요하다');
  if (opts['--self-check']) {
    for (const n of ['--engine', '--lanes', '--parts', '--only']) {
      if (n in opts) throw new RunnerError(`--self-check 는 ${n} 와 함께 못 쓴다`);
    }
  } else {
    if (opts['--mutants']) throw new RunnerError('--mutants 는 --self-check 와 함께만 쓴다');
    if (!('--engine' in opts)) throw new RunnerError('--engine 이 필요하다');
  }
  return opts;
}

/** 정수는 JavaScript Number로 읽는다. 앞뒤 공백, 4.0, 0x10, 1e1도 정수다. */
function toInt(raw, name) {
  const n = Number(raw);
  if (raw.trim() === '' || !Number.isInteger(n)) throw new RunnerError(`정수가 아니다: ${name} ${raw}`);
  return n;
}

function defaultJobs() {
  const cpus = typeof os.availableParallelism === 'function' ? os.availableParallelism() : os.cpus().length;
  return Math.min(8, Math.max(2, cpus - 2));
}

// ---------------------------------------------------------------------------
// 3. --out
// ---------------------------------------------------------------------------

/** 폴더와 그 위 폴더들의 장치와 inode. 글자가 아니라 이것으로 겹침을 잰다. */
function identityChain(p) {
  const chain = [];
  let cur = fs.realpathSync(p);
  for (;;) {
    const st = fs.statSync(cur);
    chain.push(`${st.dev}:${st.ino}`);
    const up = path.dirname(cur);
    if (up === cur) break;
    cur = up;
  }
  return chain;
}

/** a가 b와 같은 폴더이거나 b 안에 있는가. */
function insideOrSame(a, b) {
  return identityChain(a).includes(identityChain(b)[0]);
}

function overlaps(a, b) {
  return insideOrSame(a, b) || insideOrSame(b, a);
}

function nearestExisting(p) {
  let cur = p;
  while (!fs.existsSync(cur)) cur = path.dirname(cur);
  return cur;
}

/**
 * 있는데 폴더가 아니거나 비어 있지 않으면 선다. 묶음 폴더나 부모 셸의 WITHLOG_VAULT와 겹치면 선다.
 * 없으면 만들고 realpath를 지난 실제 자리로 1번 더 잰다. 그때 겹치면 방금 만든 폴더를 지우고 선다.
 */
function prepareOut(raw) {
  const out = path.resolve(raw);
  const guards = [['묶음 폴더', BUNDLE]];
  if (process.env.WITHLOG_VAULT && fs.existsSync(process.env.WITHLOG_VAULT)) guards.push(['WITHLOG_VAULT', process.env.WITHLOG_VAULT]);
  const created = [];
  if (fs.existsSync(out)) {
    if (!fs.statSync(out).isDirectory()) throw new RunnerError(`--out 이 폴더가 아니다: ${out}`);
    if (fs.readdirSync(out).length > 0) throw new RunnerError(`--out 폴더가 비어 있지 않다: ${out}`);
    for (const [label, g] of guards) if (overlaps(out, g)) throw new RunnerError(`--out 이 ${label}와 겹친다: ${out}`);
  } else {
    const anchor = nearestExisting(path.dirname(out));
    for (const [label, g] of guards) if (insideOrSame(anchor, g)) throw new RunnerError(`--out 이 ${label}와 겹친다: ${out}`);
    let cur = out;
    while (!fs.existsSync(cur)) {
      created.unshift(cur);
      cur = path.dirname(cur);
    }
    for (const dir of created) fs.mkdirSync(dir);
  }
  const real = fs.realpathSync(out);
  for (const [label, g] of guards) {
    if (overlaps(real, g)) {
      for (const dir of [...created].reverse()) fs.rmdirSync(dir);
      throw new RunnerError(`--out 이 ${label}와 겹친다: ${real}`);
    }
  }
  return real;
}

// ---------------------------------------------------------------------------
// 5. 어댑터(34.5)
// ---------------------------------------------------------------------------

function readAdapter(file) {
  const abs = path.resolve(file);
  let doc;
  try {
    doc = JSON.parse(fs.readFileSync(abs, 'utf8'));
  } catch (e) {
    throw new RunnerError(`어댑터를 못 읽었다: ${abs} (${e.message})`);
  }
  const strings = a => Array.isArray(a) && a.every(x => typeof x === 'string');
  if (!doc || typeof doc !== 'object' || Array.isArray(doc)) throw new RunnerError('어댑터가 JSON 객체가 아니다');
  if (doc.format !== 'wlx-adapter' || doc.version !== 1 || doc.surface !== 'engine') {
    throw new RunnerError('어댑터는 format wlx-adapter, version 1, surface engine 이어야 한다');
  }
  if ('name' in doc && typeof doc.name !== 'string') throw new RunnerError('어댑터의 name 은 문자열이어야 한다');
  if (!strings(doc.prefix) || doc.prefix.length === 0) throw new RunnerError('어댑터의 prefix 는 1칸 이상의 문자열 배열이어야 한다');
  if ('suffix' in doc && !strings(doc.suffix)) throw new RunnerError('어댑터의 suffix 는 문자열 배열이어야 한다');
  return { name: doc.name ?? path.basename(abs), prefix: doc.prefix, suffix: doc.suffix ?? [] };
}

/** 칸 안의 ${이름}을 구현 환경에서 채운다. 그 꼴이 아닌 글자는 그대로 남고, 없는 이름이면 선다. */
function substitute(cell, env) {
  return cell.replace(/\$\{([A-Z_][A-Z0-9_]*)\}/g, (_, name) => {
    if (!(name in env)) throw new RunnerError(`어댑터의 \${${name}} 를 채울 환경 변수가 없다`);
    return env[name];
  });
}

// ---------------------------------------------------------------------------
// 11. 구현 부르기
// ---------------------------------------------------------------------------

/** 구현 환경은 처음부터 새로 짓는다. 부모 셸에서는 PATH와 WLX_IMPL_로 시작하는 변수만 넘어간다. */
function implEnv(dirs, extra = {}) {
  const env = {
    WITHLOG_VAULT: dirs.vault, HOME: dirs.home, TMPDIR: dirs.tmp, WLX_STATE_DIR: dirs.state,
    TZ: 'Asia/Seoul', LANG: 'en_US.UTF-8', LC_ALL: 'en_US.UTF-8', PATH: process.env.PATH ?? '/usr/bin:/bin'
  };
  for (const [k, v] of Object.entries(process.env)) if (k.startsWith('WLX_IMPL_')) env[k] = v;
  return { ...env, ...extra };
}

/**
 * 구현을 제 프로세스 그룹으로 1번 띄운다. stdin은 빈 입력이고 작업 폴더는 실행기의 것이다.
 * 120초를 넘기면 그룹 전체에 SIGKILL을 보내고, 그래도 통로가 안 닫히면 5초 뒤 끊는다.
 */
function invoke(argv, env) {
  return new Promise(resolve => {
    let child;
    try {
      child = spawn(argv[0], argv.slice(1), { env, stdio: ['ignore', 'pipe', 'pipe'], detached: true, cwd: process.cwd() });
    } catch (e) {
      resolve({ spawnError: e.message });
      return;
    }
    const out = [];
    const err = [];
    let timedOut = false;
    let done = false;
    let drain = null;
    const finish = r => {
      if (done) return;
      done = true;
      clearTimeout(timer);
      clearTimeout(drain);
      running.delete(child.pid);
      resolve({ ...r, timedOut, stdout: Buffer.concat(out), stderr: Buffer.concat(err) });
    };
    child.stdout.on('data', c => out.push(c));
    child.stderr.on('data', c => err.push(c));
    child.on('error', e => finish({ spawnError: e.message }));
    child.on('close', (code, signal) => finish({ code, signal }));
    if (child.pid) running.add(child.pid);
    const timer = setTimeout(() => {
      timedOut = true;
      killGroup(child.pid);
      drain = setTimeout(() => {
        child.stdout.destroy();
        child.stderr.destroy();
        finish({ code: null, signal: 'SIGKILL' });
      }, DRAIN_MS);
    }, CALL_TIMEOUT_MS);
  });
}

function killGroup(pid) {
  if (!pid) return;
  try {
    process.kill(-pid, 'SIGKILL');
  } catch {
    // 이미 끝난 그룹
  }
}

async function pool(items, jobs, worker) {
  let next = 0;
  const lanes = Array.from({ length: Math.min(jobs, items.length) }, async () => {
    while (next < items.length) {
      const i = next++;
      await worker(items[i], i);
    }
  });
  await Promise.all(lanes);
}

// ---------------------------------------------------------------------------
// 판정(34.1의 표)
// ---------------------------------------------------------------------------

/** stderr에서 고른 1줄. WL- 번호가 든 첫 줄, 없으면 마지막 줄, 200자까지. */
function stderrLine(buf) {
  const lines = buf.toString('utf8').split('\n').filter(l => l !== '');
  const line = lines.find(l => /WL-\d{3}/.test(l)) ?? lines[lines.length - 1] ?? '';
  return line.slice(0, 200);
}

/** 처음 걸린 검사 이름과 까닭. 다 맞으면 {pass:true, tokens}. */
function judge(expect, r, vars) {
  if (r.spawnError) return { check: 'spawn', detail: `구현을 못 띄웠다: ${r.spawnError}` };
  if (r.timedOut) return { check: 'timeout', detail: `${CALL_TIMEOUT_MS / 1000}초 안에 안 끝났다` };
  if (r.code !== expect.code) {
    const got = r.code === null ? `신호 ${r.signal}` : String(r.code);
    return { check: 'code', detail: `종료 코드 기대 ${expect.code}, 실제 ${got}. stderr: ${stderrLine(r.stderr)}` };
  }
  if ('wl' in expect) {
    const m = /WL-\d{3}/.exec(r.stderr.toString('utf8'));
    if (!m || m[0] !== expect.wl) return { check: 'wl', detail: `기대 ${expect.wl}, 실제 ${m ? m[0] : '(없음)'}` };
  }
  let text;
  try {
    text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(r.stdout);
  } catch {
    return { check: 'utf8', detail: 'stdout 이 엄격한 UTF-8 이 아니다' };
  }
  if ('stdout' in expect) {
    return text === expect.stdout ? { pass: true, tokens: [] } : { check: 'stdout', detail: firstLineDiff(expect.stdout, text) };
  }
  if ('stdoutText' in expect) {
    const c = compareText(expect.stdoutText, text, vars);
    if (!c.ok) return { check: 'stdout', detail: c.reason };
    const t = checkTokensWithin(c.tokens);
    return t ? { check: 'token', detail: t } : { pass: true, tokens: c.tokens };
  }
  let value;
  try {
    value = JSON.parse(text);
  } catch (e) {
    return { check: 'stdout', detail: `JSON 이 아니다: ${e.message}` };
  }
  const c = compareJson(expect.stdoutJson, value, vars);
  if (!c.ok) return { check: 'stdout', detail: `${c.pointer}: ${c.reason}` };
  const t = checkTokensWithin(c.tokens);
  if (t) return { check: 'token', detail: t };
  const want = renderJson(c.filled);
  if (want !== text) return { check: 'format', detail: `JSON 값은 같은데 바이트가 다르다. ${firstLineDiff(want, text)}` };
  return { pass: true, tokens: c.tokens };
}

// ---------------------------------------------------------------------------
// 6~13. 회차
// ---------------------------------------------------------------------------

/** 레인, 입력 vault, 입력 HOME, 준비 걸음이 같은 레인 파일을 1벌로 묶는다. */
function groupFiles(files, selected) {
  const groups = new Map();
  const counters = new Map();
  for (const f of files) {
    const cases = f.cases.filter(c => selected.has(c.id));
    if (cases.length === 0) continue;
    const key = JSON.stringify([f.lane, f.treeName, f.homeName, f.prepare]);
    if (!groups.has(key)) {
      const n = counters.get(f.lane) ?? 0;
      counters.set(f.lane, n + 1);
      groups.set(key, { lane: f.lane, index: n, tree: f.tree, home: f.home, prepare: f.prepare, cases: [] });
    }
    groups.get(key).cases.push(...cases);
  }
  return [...groups.values()];
}

/**
 * 묶음 1벌을 돈다. 펼치고, 준비 걸음을 부르고, 케이스를 나란히 부르고, 읽기 레인이면 vault와 HOME을 다시 맞댄다.
 * impl은 {name, prefix, suffix, extraEnv(case)}이다.
 */
async function runGroup(group, outDir, impl, jobs, totals, results, warnings) {
  const base = path.join(outDir, `${group.lane}-${group.index}`);
  const dirs = {};
  for (const d of ['vault', 'home', 'state', 'tmp']) {
    fs.mkdirSync(path.join(base, d), { recursive: true });
    dirs[d] = fs.realpathSync(path.join(base, d));
  }
  expandTree(group.tree, dirs.vault);
  if (group.home) expandTree(group.home, dirs.home);
  const vars = { vault: dirs.vault, vaultUri: encodeURIComponent(dirs.vault), home: dirs.home };
  const argvFor = (argv, env) => [...impl.prefix.map(c => substitute(c, env)), ...argv, ...impl.suffix.map(c => substitute(c, env))];

  for (const step of group.prepare) {
    const env = implEnv(dirs, impl.extraEnv ? impl.extraEnv(null) : {});
    const r = await invoke(argvFor(step, env), env);
    totals.launches++;
    if (r.spawnError) throw new RunnerError(`준비 걸음 ${JSON.stringify(step)} 을 못 띄웠다: ${r.spawnError}`);
    if (r.timedOut) throw new RunnerError(`준비 걸음 ${JSON.stringify(step)} 이 ${CALL_TIMEOUT_MS / 1000}초를 넘겼다`);
    if (r.code !== 0) throw new RunnerError(`준비 걸음 ${JSON.stringify(step)} 이 종료 코드 ${r.code ?? r.signal} 로 끝났다. stderr: ${stderrLine(r.stderr)}`);
  }

  const crossTokens = new Map();
  await pool(group.cases, jobs, async c => {
    const env = implEnv(dirs, impl.extraEnv ? impl.extraEnv(c) : {});
    const r = await invoke(argvFor(c.argv, env), env);
    totals.launches++;
    let verdict = judge(c.expect, r, vars);
    if (verdict.pass) {
      // 9번: 끝까지 맞은 케이스 사이에서 같은 경로의 토큰은 먼저 끝난 케이스의 값을 기준으로 맞댄다
      for (const t of verdict.tokens) {
        const prev = crossTokens.get(t.path);
        if (prev === undefined) crossTokens.set(t.path, { value: t.value, id: c.id });
        else if (prev.value !== t.value) {
          verdict = { check: 'token', detail: `토큰 ${t.path} 이 ${prev.id} 의 ${JSON.stringify(prev.value)} 와 다르다: ${JSON.stringify(t.value)}` };
          break;
        }
      }
    }
    results.set(c.id, verdict);
  });

  if (group.lane === 'read') {
    warnings.push(...compareWithTree('vault', dirs.vault, group.tree));
    warnings.push(...compareWithTree('HOME', dirs.home, group.home));
  }
}

/** 고른 케이스로 회차 1번을 돌고 report.json과 요약을 낸다. 어긋남이 없으면 0, 있으면 1. */
async function runAll(files, selected, outDir, impl, jobs, quiet = false) {
  const started = Date.now();
  const totals = { cases: selected.size, passed: 0, failed: 0, launches: 0, wallMs: 0 };
  const results = new Map();
  const warnings = [];
  for (const group of groupFiles(files, selected)) {
    await runGroup(group, outDir, impl, jobs, totals, results, warnings);
  }
  const failures = [];
  for (const f of files) {
    for (const c of f.cases) {
      if (!selected.has(c.id)) continue;
      const v = results.get(c.id);
      if (v.pass) totals.passed++;
      else {
        totals.failed++;
        failures.push({ id: c.id, check: v.check, detail: v.detail });
      }
    }
  }
  totals.wallMs = Date.now() - started;
  const report = { engine: impl.name, totals, failures, warnings };
  const reportPath = path.join(outDir, 'report.json');
  fs.writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n');
  if (!quiet) printSummary(report, reportPath);
  return { code: failures.length === 0 ? 0 : 1, report };
}

function printSummary(report, reportPath) {
  const t = report.totals;
  const lines = [`엔진 ${report.engine}: 케이스 ${t.cases}건 중 ${t.passed}건 맞음, ${t.failed}건 어긋남 (구현 호출 ${t.launches}번, ${(t.wallMs / 1000).toFixed(1)}초)`];
  for (const f of report.failures.slice(0, SUMMARY_FAILURES)) lines.push(`  ✗ ${f.id} [${f.check}] ${f.detail}`);
  if (report.failures.length > SUMMARY_FAILURES) lines.push(`  … 나머지 ${report.failures.length - SUMMARY_FAILURES}건은 ${reportPath}`);
  for (const w of report.warnings.slice(0, SUMMARY_WARNINGS)) lines.push(`  ⚠ ${w}`);
  process.stderr.write(lines.join('\n') + '\n');
}

/** 7번: 항목마다 고른 레인의 케이스 가운데 1건이라도 걸리는지 따로 잰다. 함께 적용한 결과가 0건이어도 선다. */
function selectCases(files, opts) {
  const all = files.flatMap(f => f.cases);
  for (const lane of opts['--lanes'] ?? []) {
    if (!files.some(f => f.lane === lane && f.cases.length > 0)) throw new RunnerError(`--lanes 의 항목에 걸리는 케이스가 없다: ${lane}`);
  }
  for (const part of opts['--parts'] ?? []) {
    if (!all.some(c => c.part === part)) throw new RunnerError(`--parts 의 항목에 걸리는 케이스가 없다: ${part}`);
  }
  for (const head of opts['--only'] ?? []) {
    if (!all.some(c => c.id.startsWith(head))) throw new RunnerError(`--only 의 항목에 걸리는 케이스가 없다: ${head}`);
  }
  const parts = opts['--parts'];
  const only = opts['--only'];
  const selected = new Set(all.filter(c => (!parts || parts.includes(c.part)) && (!only || only.some(h => c.id.startsWith(h)))).map(c => c.id));
  if (selected.size === 0) throw new RunnerError('고른 케이스가 없다');
  return selected;
}

// ---------------------------------------------------------------------------
// 자가 점검과 변형
// ---------------------------------------------------------------------------

const MUTANTS = [
  { name: 'byte', checks: ['stdout', 'format'], targets: ['json', 'tokenJson', 'text', 'empty'] },
  { name: 'bom', checks: ['stdout'], targets: ['json', 'tokenJson', 'text', 'empty'] },
  { name: 'code', checks: ['code'], targets: ['json', 'tokenJson', 'text', 'empty'] },
  { name: 'compact', checks: ['format'], targets: ['json', 'tokenJson'] },
  { name: 'indent', checks: ['format'], targets: ['json', 'tokenJson'] },
  { name: 'newline', checks: ['format', 'stdout'], targets: ['json', 'tokenJson', 'text'] },
  { name: 'key-order', checks: ['stdout'], targets: ['json', 'tokenJson'] },
  { name: 'token', checks: ['token'], targets: ['tokenJson', 'tokenShare'], atLeastOne: true }
];

/** 변형 대상: 레인에서 처음 맞는 것. 토큰 없는 JSON 객체(키 2개 이상), 토큰 1개인 JSON 객체, 그 토큰을 같이 쓰는 다른 케이스, 글, 빈 출력. */
function pickTargets(cases) {
  const isObj = v => v && typeof v === 'object' && !Array.isArray(v);
  const pick = {};
  pick.json = cases.find(c => 'stdoutJson' in c.expect && isObj(c.expect.stdoutJson) && Object.keys(c.expect.stdoutJson).length >= 2 && tokenPaths(c.expect).length === 0);
  // 토큰 1개인 JSON 객체는 그 토큰을 같이 쓰는 다른 케이스가 있는 것 가운데 처음 것이다
  for (const c of cases) {
    if (!('stdoutJson' in c.expect) || !isObj(c.expect.stdoutJson) || tokenPaths(c.expect).length !== 1) continue;
    const p = tokenPaths(c.expect)[0];
    const share = cases.find(o => o !== c && tokenPaths(o.expect).includes(p));
    if (share) {
      pick.tokenJson = c;
      pick.tokenShare = share;
      break;
    }
  }
  pick.text = cases.find(c => 'stdoutText' in c.expect);
  pick.empty = cases.find(c => 'stdout' in c.expect && c.expect.stdout === '');
  for (const k of ['json', 'tokenJson', 'tokenShare', 'text', 'empty']) {
    if (!pick[k]) throw new RunnerError(`변형 대상을 못 골랐다: ${k}`);
  }
  return pick;
}

async function selfCheck(opts, outDir, jobs) {
  const lanes = laneNames(BUNDLE);
  const started = Date.now();
  const files = readLaneFiles(BUNDLE, lanes);
  const all = files.flatMap(f => f.cases);
  const replay = {
    name: 'self-check/replay',
    prefix: [process.execPath, path.join(BUNDLE, 'selftest', 'replay-impl.mjs')],
    suffix: [],
    extraEnv: () => ({})
  };
  const replayOut = path.join(outDir, 'replay');
  fs.mkdirSync(replayOut);
  const first = await runAll(files, new Set(all.map(c => c.id)), replayOut, replay, jobs);
  if (first.code !== 0 || !opts['--mutants']) return first.code;

  const targets = pickTargets(all);
  let missed = 0;
  const lines = [];
  for (const m of MUTANTS) {
    const ids = [...new Set(m.targets.map(k => targets[k].id))];
    const mutantOut = path.join(outDir, `mutant-${m.name}`);
    fs.mkdirSync(mutantOut);
    const impl = {
      ...replay,
      name: `self-check/mutant-${m.name}`,
      extraEnv: c => (c ? { WLX_SELFTEST_MUTANT: m.name, WLX_SELFTEST_TARGETS: ids.join('\n') } : {})
    };
    const { report } = await runAll(files, new Set(ids), mutantOut, impl, jobs, true);
    const caught = ids.filter(id => report.failures.some(f => f.id === id && m.checks.includes(f.check)));
    const ok = m.atLeastOne ? caught.length >= 1 : caught.length === ids.length;
    if (!ok) missed++;
    lines.push(`  ${ok ? '✓' : '✗'} 변형 ${m.name}: 대상 ${ids.length}건 가운데 ${caught.length}건을 [${m.checks.join(', ')}] 로 잡았다`);
  }
  process.stderr.write(`변형 ${MUTANTS.length}가지 가운데 ${MUTANTS.length - missed}가지를 잡았다 (${((Date.now() - started) / 1000).toFixed(1)}초)\n${lines.join('\n')}\n`);
  return missed === 0 ? 0 : 1;
}

// ---------------------------------------------------------------------------

async function main() {
  const opts = parseOptions(process.argv.slice(2));
  checkIntegrity(BUNDLE);
  const outDir = prepareOut(opts['--out']);
  const jobs = opts['--jobs'] ?? defaultJobs();
  if (jobs < 1) throw new RunnerError(`--jobs 는 1 이상이어야 한다: ${jobs}`);
  if (opts['--self-check']) return selfCheck(opts, outDir, jobs);

  const impl = readAdapter(opts['--engine']);
  const known = laneNames(BUNDLE);
  for (const lane of opts['--lanes'] ?? []) {
    if (!known.includes(lane)) throw new RunnerError(`--lanes 의 항목에 걸리는 케이스가 없다: ${lane}`);
  }
  const files = readLaneFiles(BUNDLE, opts['--lanes'] ?? known);
  const selected = selectCases(files, opts);
  const { code } = await runAll(files, selected, outDir, impl, jobs);
  return code;
}

for (const sig of ['SIGINT', 'SIGTERM', 'SIGHUP', 'SIGQUIT']) {
  process.on(sig, () => {
    for (const pid of running) killGroup(pid);
    process.exit(128 + os.constants.signals[sig]);
  });
}

main().then(
  code => process.exit(code),
  e => {
    process.stderr.write(`실행기가 못 돌았다: ${e instanceof RunnerError ? e.message : (e && e.stack) || e}\n`);
    for (const pid of running) killGroup(pid);
    process.exit(2);
  }
);
