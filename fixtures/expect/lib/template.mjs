// 설명서 34.1의 자리표 맞대기와 기대값 다시 찍기.
// 자리표는 ⟦vault⟧, ⟦vault-uri⟧, ⟦home⟧, ⟦token:<vault 상대 경로>⟧ 4가지다. 토큰 값은 계산하지 않고 모양과 같음만 본다.
import { RunnerError } from './tree.mjs';

const PLACEHOLDER = /⟦([^⟦⟧]*)⟧/g;
const TOKEN_BODY = '[^\\s"\\\\⟦⟧]+';

/** 글 1개를 글자 조각과 자리표 조각으로 가른다. 모르는 자리표나 닫히지 않은 괄호는 기대값을 잘못 적은 것이다. */
export function parseTemplate(text, where) {
  const parts = [];
  let last = 0;
  for (const m of text.matchAll(PLACEHOLDER)) {
    if (m.index > last) parts.push({ lit: text.slice(last, m.index) });
    const name = m[1];
    if (name === 'vault' || name === 'vault-uri' || name === 'home') parts.push({ ph: name });
    else if (name.startsWith('token:') && name.length > 'token:'.length) parts.push({ ph: 'token', path: name.slice('token:'.length) });
    else throw new RunnerError(`${where}: 모르는 자리표다: ⟦${name}⟧`);
    last = m.index + m[0].length;
  }
  if (last < text.length) parts.push({ lit: text.slice(last) });
  for (const p of parts) {
    if (p.lit !== undefined && (p.lit.includes('⟦') || p.lit.includes('⟧'))) {
      throw new RunnerError(`${where}: 자리표 괄호가 짝이 안 맞는다`);
    }
  }
  return parts;
}

/**
 * 기대값 전체를 훑어 자리표를 잰다. JSON 값은 문자열 값만 자리표를 품는다.
 * stdout은 자리표 없이 글자 그대로 맞대는 꼴이라, 자리표 모양 글자가 들면 기대값을 잘못 적은 것으로 보고 선다.
 */
export function checkExpectation(expect, where) {
  if ('stdoutJson' in expect) walkStrings(expect.stdoutJson, s => parseTemplate(s, where));
  if ('stdoutText' in expect) parseTemplate(expect.stdoutText, where);
  if ('stdout' in expect && new RegExp(PLACEHOLDER.source).test(expect.stdout)) {
    throw new RunnerError(`${where}: stdout 은 자리표 없는 글이다. 자리표를 쓰려면 stdoutText 로 적는다`);
  }
}

function walkStrings(value, fn) {
  if (typeof value === 'string') fn(value);
  else if (Array.isArray(value)) value.forEach(v => walkStrings(v, fn));
  else if (value && typeof value === 'object') Object.values(value).forEach(v => walkStrings(v, fn));
}

/** 기대값에 든 토큰 경로 목록(차례대로, 겹침 포함). 변형 대상을 고를 때 쓴다. */
export function tokenPaths(expect) {
  const paths = [];
  const collect = s => parseTemplate(s, 'expect').forEach(p => { if (p.ph === 'token') paths.push(p.path); });
  if ('stdoutJson' in expect) walkStrings(expect.stdoutJson, collect);
  if ('stdoutText' in expect) collect(expect.stdoutText);
  return paths;
}

function escapeRegex(s) {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

/**
 * 틀 1개로 글 전체를 맞댄다. 경로 자리표 3가지는 그 값과 글자 그대로 같아야 하고, 토큰은 허용 글자 1개 이상을
 * 욕심껏 차지한다. 맞으면 토큰 값의 배열 [{path, value}]을, 안 맞으면 null을 돌려준다.
 */
export function matchTemplate(parts, actual, vars) {
  let source = '^';
  const tokens = [];
  for (const p of parts) {
    if (p.lit !== undefined) source += escapeRegex(p.lit);
    else if (p.ph === 'vault') source += escapeRegex(vars.vault);
    else if (p.ph === 'vault-uri') source += escapeRegex(vars.vaultUri);
    else if (p.ph === 'home') source += escapeRegex(vars.home);
    else {
      source += `(${TOKEN_BODY})`;
      tokens.push(p.path);
    }
  }
  const m = new RegExp(source + '$', 'u').exec(actual);
  if (!m) return null;
  return tokens.map((p, i) => ({ path: p, value: m[i + 1] }));
}

/** 자리표를 채운 글. 토큰은 이번 출력에서 맞은 값으로 채운다. */
function fillParts(parts, vars, tokenValues) {
  let i = 0;
  return parts.map(p => {
    if (p.lit !== undefined) return p.lit;
    if (p.ph === 'vault') return vars.vault;
    if (p.ph === 'vault-uri') return vars.vaultUri;
    if (p.ph === 'home') return vars.home;
    return tokenValues[i++].value;
  }).join('');
}

/**
 * 34.1의 6번 stdout 검사(JSON). 값을 키 차례와 배열 길이까지 맞대고, 수는 Object.is로 본다.
 * 맞으면 {ok:true, filled, tokens}, 안 맞으면 {ok:false, pointer, reason}이다.
 */
export function compareJson(expected, actual, vars) {
  const tokens = [];
  const fail = (pointer, reason) => ({ ok: false, pointer: pointer || '/', reason });
  const walk = (exp, act, pointer) => {
    if (typeof exp === 'string') {
      if (typeof act !== 'string') return fail(pointer, `문자열이어야 한다. 실제 ${typeName(act)}`);
      const parts = parseTemplate(exp, pointer);
      if (parts.every(p => p.lit !== undefined)) {
        return act === exp ? { ok: true, filled: act } : fail(pointer, `글이 다르다. 기대 ${JSON.stringify(exp)}, 실제 ${JSON.stringify(act)}`);
      }
      const got = matchTemplate(parts, act, vars);
      if (!got) return fail(pointer, `자리표 틀과 안 맞는다. 기대 ${JSON.stringify(exp)}, 실제 ${JSON.stringify(act)}`);
      got.forEach(t => tokens.push({ ...t, pointer }));
      return { ok: true, filled: fillParts(parts, vars, got) };
    }
    if (typeof exp === 'number') {
      return typeof act === 'number' && Object.is(exp, act) ? { ok: true, filled: act } : fail(pointer, `수가 다르다. 기대 ${String(exp)}, 실제 ${JSON.stringify(act)}`);
    }
    if (exp === null || typeof exp === 'boolean') {
      return act === exp ? { ok: true, filled: act } : fail(pointer, `값이 다르다. 기대 ${JSON.stringify(exp)}, 실제 ${JSON.stringify(act)}`);
    }
    if (Array.isArray(exp)) {
      if (!Array.isArray(act)) return fail(pointer, `배열이어야 한다. 실제 ${typeName(act)}`);
      if (act.length !== exp.length) return fail(pointer, `배열 길이가 다르다. 기대 ${exp.length}, 실제 ${act.length}`);
      const filled = [];
      for (let i = 0; i < exp.length; i++) {
        const r = walk(exp[i], act[i], `${pointer}/${i}`);
        if (!r.ok) return r;
        filled.push(r.filled);
      }
      return { ok: true, filled };
    }
    if (!act || typeof act !== 'object' || Array.isArray(act)) return fail(pointer, `객체여야 한다. 실제 ${typeName(act)}`);
    const ek = Object.keys(exp);
    const ak = Object.keys(act);
    for (let i = 0; i < Math.max(ek.length, ak.length); i++) {
      if (ek[i] !== ak[i]) {
        if (ek[i] === undefined) return fail(pointer, `키가 더 있다: ${ak[i]}`);
        if (ak[i] === undefined) return fail(pointer, `키가 빠졌다: ${ek[i]}`);
        return fail(pointer, `${i + 1}번째 키가 다르다. 기대 ${ek[i]}, 실제 ${ak[i]}`);
      }
    }
    const filled = {};
    for (const k of ek) {
      const r = walk(exp[k], act[k], `${pointer}/${escapePointer(k)}`);
      if (!r.ok) return r;
      filled[k] = r.filled;
    }
    return { ok: true, filled };
  };
  const r = walk(expected, actual, '');
  return r.ok ? { ok: true, filled: r.filled, tokens } : r;
}

/** stdoutText: 자리표만 채워 글 전체를 맞댄다. */
export function compareText(expected, actual, vars) {
  const parts = parseTemplate(expected, 'stdoutText');
  const got = matchTemplate(parts, actual, vars);
  if (!got) return { ok: false, reason: firstLineDiff(fillParts(parts, vars, tokenGuess(parts)), actual) };
  return { ok: true, tokens: got.map(t => ({ ...t, pointer: '' })) };
}

/** 안 맞은 글의 줄 비교를 위해 토큰 자리를 자리표 글 그대로 둔다. */
function tokenGuess(parts) {
  return parts.filter(p => p.ph === 'token').map(p => ({ value: `⟦token:${p.path}⟧` }));
}

/** 처음 갈린 줄 번호와 기대 줄, 실제 줄. */
export function firstLineDiff(expected, actual) {
  const e = expected.split('\n');
  const a = actual.split('\n');
  for (let i = 0; i < Math.max(e.length, a.length); i++) {
    if (e[i] !== a[i]) {
      return `${i + 1}번째 줄이 다르다. 기대 ${e[i] === undefined ? '(없음)' : JSON.stringify(e[i])}, 실제 ${a[i] === undefined ? '(없음)' : JSON.stringify(a[i])}`;
    }
  }
  return '바이트가 다르다';
}

/** 출력 1개 안에서 같은 경로의 토큰이 같은 값인지(34.1의 7번). 어긋나면 글, 맞으면 null. */
export function checkTokensWithin(tokens) {
  const seen = new Map();
  for (const t of tokens) {
    const prev = seen.get(t.path);
    if (prev === undefined) seen.set(t.path, t.value);
    else if (prev !== t.value) return `토큰 ${t.path} 이 출력 1개 안에서 ${JSON.stringify(prev)} 와 ${JSON.stringify(t.value)} 로 갈렸다`;
  }
  return null;
}

/** 기대값을 채운 JSON을 JSON.stringify(값, null, 2)와 LF 1개로 다시 찍는다. */
export function renderJson(filled) {
  return JSON.stringify(filled, null, 2) + '\n';
}

function escapePointer(key) {
  return key.replace(/~/g, '~0').replace(/\//g, '~1');
}

function typeName(v) {
  if (v === null) return 'null';
  if (Array.isArray(v)) return '배열';
  return { string: '문자열', number: '수', boolean: '불리언', object: '객체' }[typeof v] || typeof v;
}
