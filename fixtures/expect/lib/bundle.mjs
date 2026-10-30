// 설명서 34.1의 묶음 목록과 무결성, 레인 파일과 입력 나무 읽기.
import fs from 'node:fs';
import path from 'node:path';
import crypto from 'node:crypto';
import { RunnerError, checkTree, compareUtf16 } from './tree.mjs';
import { checkExpectation } from './template.mjs';

/** 운영체제가 남기는 파일. 무결성 검사에서 안 센다. */
export function isOsLitter(name) {
  return name === '.DS_Store' || name.startsWith('._') || name === 'Thumbs.db' || name === 'desktop.ini';
}

export function sha256(bytes) {
  return crypto.createHash('sha256').update(bytes).digest('hex');
}

/** 묶음 안의 파일을 상대 경로로 모두 모은다. bundle.json 자신과 운영체제 파일은 뺀다. */
export function listBundleFiles(root) {
  const out = [];
  const walk = rel => {
    for (const name of fs.readdirSync(rel ? path.join(root, rel) : root)) {
      if (isOsLitter(name)) continue;
      const childRel = rel ? `${rel}/${name}` : name;
      const st = fs.lstatSync(path.join(root, childRel));
      if (st.isDirectory()) walk(childRel);
      else if (childRel !== 'bundle.json') out.push(childRel);
    }
  };
  walk('');
  return out.sort(compareUtf16);
}

/**
 * 34.1의 2번: bundle.json이 없거나, 목록의 파일이 없거나, sha256이 다르거나, 목록에 없는 파일이 있으면 선다.
 * 손으로 고친 기대값이 기록 없이 들어오는 길을 막는 장치다.
 */
export function checkIntegrity(root) {
  const manifestPath = path.join(root, 'bundle.json');
  let manifest;
  try {
    manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));
  } catch (e) {
    throw new RunnerError(`묶음 목록을 못 읽었다: ${manifestPath} (${e.message})`);
  }
  if (!manifest || typeof manifest.files !== 'object' || Array.isArray(manifest.files)) {
    throw new RunnerError('묶음 목록에 files 객체가 없다');
  }
  for (const [rel, want] of Object.entries(manifest.files)) {
    const abs = path.join(root, rel);
    if (!fs.existsSync(abs)) throw new RunnerError(`묶음 파일이 없다: ${rel}`);
    if (sha256(fs.readFileSync(abs)) !== want) throw new RunnerError(`묶음 파일의 sha256 이 목록과 다르다: ${rel}`);
  }
  for (const rel of listBundleFiles(root)) {
    if (!(rel in manifest.files)) throw new RunnerError(`목록에 없는 파일이 묶음에 있다: ${rel}`);
  }
  return manifest;
}

/** trees/<이름>.json을 읽어 꼴을 잰다. 이름은 ^[a-z0-9][a-z0-9.-]*$ 이다. */
export function readTree(root, name, where) {
  if (typeof name !== 'string' || !/^[a-z0-9][a-z0-9.-]*$/.test(name)) throw new RunnerError(`${where}: 나무 이름 꼴이 아니다: ${JSON.stringify(name)}`);
  const file = path.join(root, 'trees', `${name}.json`);
  let tree;
  try {
    tree = JSON.parse(fs.readFileSync(file, 'utf8'));
  } catch (e) {
    throw new RunnerError(`${where}: 나무 파일을 못 읽었다: trees/${name}.json (${e.message})`);
  }
  checkTree(tree, `trees/${name}.json`);
  return tree;
}

/** 묶음의 레인 이름(lanes/ 아래 폴더). */
export function laneNames(root) {
  const dir = path.join(root, 'lanes');
  if (!fs.existsSync(dir)) return [];
  return fs.readdirSync(dir).filter(n => !isOsLitter(n) && fs.lstatSync(path.join(dir, n)).isDirectory()).sort(compareUtf16);
}

/**
 * 34.1의 6번: 레인 파일을 읽고, 구현을 부르기 전에 모든 케이스의 꼴과 자리표를 잰다.
 * 이름이 .으로 시작하거나 .json으로 안 끝나는 파일은 안 읽는다. id는 고른 레인 전체에서 겹치면 안 된다.
 */
export function readLaneFiles(root, lanes) {
  const files = [];
  const ids = new Set();
  for (const lane of lanes) {
    const dir = path.join(root, 'lanes', lane);
    const names = fs.readdirSync(dir).filter(n => !n.startsWith('.') && n.endsWith('.json')).sort(compareUtf16);
    for (const name of names) {
      const where = `lanes/${lane}/${name}`;
      let doc;
      try {
        doc = JSON.parse(fs.readFileSync(path.join(dir, name), 'utf8'));
      } catch (e) {
        throw new RunnerError(`${where}: JSON 이 아니다 (${e.message})`);
      }
      if (!doc || typeof doc !== 'object' || Array.isArray(doc)) throw new RunnerError(`${where}: JSON 객체가 아니다`);
      if (doc.format !== 'wlx-cases' || doc.version !== 1) throw new RunnerError(`${where}: format 이 wlx-cases 이고 version 이 1 이어야 한다`);
      if ('lane' in doc && doc.lane !== lane) throw new RunnerError(`${where}: lane 이 폴더 이름과 다르다: ${JSON.stringify(doc.lane)}`);
      const tree = readTree(root, doc.tree, where);
      const home = 'home' in doc ? readTree(root, doc.home, where) : null;
      const prepare = 'prepare' in doc ? doc.prepare : [];
      if (!Array.isArray(prepare) || !prepare.every(isArgv)) throw new RunnerError(`${where}: prepare 는 argv 배열의 배열이어야 한다`);
      if (!Array.isArray(doc.cases)) throw new RunnerError(`${where}: cases 는 배열이어야 한다`);
      const cases = doc.cases.map((c, i) => checkCase(c, `${where} 의 ${i + 1}번째 케이스`, ids));
      files.push({ lane, name, treeName: doc.tree, homeName: doc.home ?? null, tree, home, prepare, cases });
    }
  }
  return files;
}

function isArgv(a) {
  return Array.isArray(a) && a.length > 0 && a.every(x => typeof x === 'string');
}

function checkCase(c, where, ids) {
  if (!c || typeof c !== 'object' || Array.isArray(c)) throw new RunnerError(`${where}: 객체가 아니다`);
  if (typeof c.id !== 'string' || c.id === '') throw new RunnerError(`${where}: id 는 비지 않은 글자여야 한다`);
  if (ids.has(c.id)) throw new RunnerError(`${where}: id 가 겹친다: ${c.id}`);
  ids.add(c.id);
  if (!Number.isInteger(c.part)) throw new RunnerError(`${where} (${c.id}): part 는 정수여야 한다`);
  if (!isArgv(c.argv)) throw new RunnerError(`${where} (${c.id}): argv 는 글자 1개 이상의 배열이어야 한다`);
  const e = c.expect;
  if (!e || typeof e !== 'object' || Array.isArray(e)) throw new RunnerError(`${where} (${c.id}): expect 가 객체가 아니다`);
  if (!Number.isInteger(e.code)) throw new RunnerError(`${where} (${c.id}): expect.code 는 정수여야 한다`);
  const kinds = ['stdoutJson', 'stdoutText', 'stdout'].filter(k => k in e);
  if (kinds.length !== 1) throw new RunnerError(`${where} (${c.id}): stdoutJson, stdoutText, stdout 가운데 정확히 1개가 있어야 한다`);
  if ('stdoutText' in e && typeof e.stdoutText !== 'string') throw new RunnerError(`${where} (${c.id}): stdoutText 는 글자여야 한다`);
  if ('stdout' in e && typeof e.stdout !== 'string') throw new RunnerError(`${where} (${c.id}): stdout 은 글자여야 한다`);
  if ('wl' in e && !(typeof e.wl === 'string' && /^WL-\d{3}$/.test(e.wl))) throw new RunnerError(`${where} (${c.id}): wl 은 "WL-409" 같은 글자여야 한다`);
  checkExpectation(e, `${where} (${c.id})`);
  return { id: c.id, part: c.part, argv: c.argv, expect: e };
}
