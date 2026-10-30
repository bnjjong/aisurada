#!/usr/bin/env node
// 설명서 34.1의 자가 점검용 가짜 구현. 받은 argv와 글자가 같은 케이스를 찾아 기대값을 그대로 찍는다.
// ["reindex"]에는 아무것도 안 하고 0으로 끝난다. 토큰은 엔진과 다른 규칙(rp- 머리)으로 지어서,
// 이 가짜가 통과하면 실행기가 토큰 값을 계산하지 않는다는 것도 함께 선다.
// 변형 점검이면 실행기가 WLX_SELFTEST_MUTANT와 WLX_SELFTEST_TARGETS로 바꿀 케이스를 알려 준다.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { fileURLToPath } from 'node:url';
import { parseTemplate } from '../lib/template.mjs';

const BUNDLE = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..');
const argv = process.argv.slice(2);

if (argv.length === 1 && argv[0] === 'reindex') process.exit(0);

const found = findCase(argv);
if (!found) {
  process.stderr.write(`replay: 모르는 argv 다: ${JSON.stringify(argv)}\n`);
  process.exit(1);
}

const mutant = process.env.WLX_SELFTEST_MUTANT ?? '';
const targeted = (process.env.WLX_SELFTEST_TARGETS ?? '').split('\n').includes(found.id);
const vars = { vault: process.env.WITHLOG_VAULT, home: process.env.HOME };
const tokenFor = p => 'rp-' + crypto.createHash('sha256').update(targeted && mutant === 'token' ? `${p}\n${found.id}` : p).digest('hex').slice(0, 24);

const e = found.expect;
let code = e.code;
let out;
let value = null;
if ('stdout' in e) out = e.stdout;
else if ('stdoutText' in e) out = fill(e.stdoutText);
else {
  value = fillJson(e.stdoutJson);
  out = JSON.stringify(value, null, 2) + '\n';
}

if (targeted) {
  if (mutant === 'byte') out = out.length === 0 ? 'x' : out[0] + (out[1] === 'x' ? 'y' : 'x') + out.slice(2);
  else if (mutant === 'bom') out = '﻿' + out;
  else if (mutant === 'code') code = code + 1;
  else if (mutant === 'compact' && value !== null) out = JSON.stringify(value) + '\n';
  else if (mutant === 'indent' && value !== null) out = JSON.stringify(value, null, 4) + '\n';
  else if (mutant === 'newline') out = out.endsWith('\n') ? out.slice(0, -1) : out;
  else if (mutant === 'key-order' && value && typeof value === 'object' && !Array.isArray(value)) {
    const keys = Object.keys(value);
    const swapped = {};
    [keys[1], keys[0], ...keys.slice(2)].forEach(k => { swapped[k] = value[k]; });
    out = JSON.stringify(swapped, null, 2) + '\n';
  }
}

// 기대값에 wl 칸이 있으면 그 번호를 stderr 1줄로 남긴다(34.1의 4번 검사)
if ('wl' in e) process.stderr.write(`${e.wl} replay\n`);
process.stdout.write(Buffer.from(out, 'utf8'), () => process.exit(code));

function findCase(args) {
  const lanes = path.join(BUNDLE, 'lanes');
  for (const lane of fs.readdirSync(lanes)) {
    const dir = path.join(lanes, lane);
    if (!fs.statSync(dir).isDirectory()) continue;
    for (const name of fs.readdirSync(dir)) {
      if (name.startsWith('.') || !name.endsWith('.json')) continue;
      const doc = JSON.parse(fs.readFileSync(path.join(dir, name), 'utf8'));
      const c = doc.cases.find(x => JSON.stringify(x.argv) === JSON.stringify(args));
      if (c) return c;
    }
  }
  return null;
}

function fill(text) {
  return parseTemplate(text, 'replay').map(p => {
    if (p.lit !== undefined) return p.lit;
    if (p.ph === 'vault') return vars.vault;
    if (p.ph === 'vault-uri') return encodeURIComponent(vars.vault);
    if (p.ph === 'home') return vars.home;
    return tokenFor(p.path);
  }).join('');
}

function fillJson(v) {
  if (typeof v === 'string') return fill(v);
  if (Array.isArray(v)) return v.map(fillJson);
  if (v && typeof v === 'object') {
    const o = {};
    for (const [k, x] of Object.entries(v)) o[k] = fillJson(x);
    return o;
  }
  return v;
}
