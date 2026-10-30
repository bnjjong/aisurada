// 설명서 34.1의 입력 나무: 펼치기와 다시 읽기.
// 나무는 git 설정이나 .gitignore가 바이트를 바꾸지 못하게 파일을 JSON 글로 담은 꼴이다.
import fs from 'node:fs';
import path from 'node:path';

/** 실행기가 못 도는 까닭. 종료 코드 2로 선다. */
export class RunnerError extends Error {}

/** 나무 안 경로가 펼칠 자리 밖을 가리키면 선다. 빈 칸, 절대 경로, `.`과 `..` 조각, 역슬래시. */
export function checkRelPath(rel, where) {
  if (typeof rel !== 'string' || rel === '' || rel.startsWith('/') || rel.includes('\\')) {
    throw new RunnerError(`${where}: 펼칠 자리 밖을 가리키는 경로다: ${JSON.stringify(rel)}`);
  }
  for (const seg of rel.split('/')) {
    if (seg === '' || seg === '.' || seg === '..') {
      throw new RunnerError(`${where}: 펼칠 자리 밖을 가리키는 경로다: ${JSON.stringify(rel)}`);
    }
  }
}

/** 파일 칸 1개의 바이트. `lines`를 LF로 잇고 `eol`이 참이면 LF 1개를 더한다. UTF-8로 못 읽는 파일은 `base64`다. */
export function fileBytes(entry) {
  if (typeof entry.base64 === 'string') return Buffer.from(entry.base64, 'base64');
  const text = entry.lines.join('\n') + (entry.eol ? '\n' : '');
  return Buffer.from(text, 'utf8');
}

/** 나무 파일의 꼴을 잰다. 실행기는 format과 version만 맞대고 source와 note는 사람이 읽는 칸이다. */
export function checkTree(tree, where) {
  if (!tree || typeof tree !== 'object' || Array.isArray(tree)) throw new RunnerError(`${where}: JSON 객체가 아니다`);
  if (tree.format !== 'wlx-tree' || tree.version !== 1) throw new RunnerError(`${where}: format 이 wlx-tree 이고 version 이 1 이어야 한다`);
  if (!Array.isArray(tree.dirs) || !Array.isArray(tree.files)) throw new RunnerError(`${where}: dirs 와 files 는 배열이어야 한다`);
  for (const d of tree.dirs) checkRelPath(d, where);
  for (const f of tree.files) {
    checkRelPath(f && f.path, where);
    const isText = Array.isArray(f.lines) && f.lines.every(l => typeof l === 'string') && typeof f.eol === 'boolean';
    const isBinary = typeof f.base64 === 'string';
    if (isText === isBinary) throw new RunnerError(`${where}: 파일 칸은 {path, lines, eol} 이나 {path, base64} 꼴이어야 한다: ${f.path}`);
  }
}

/** `dirs`의 폴더를 모두 만들고 나서 `files`를 차례로 쓴다. 권한은 node의 기본이고 수정 시각은 펼친 순간이다. */
export function expandTree(tree, dest) {
  for (const d of tree.dirs) fs.mkdirSync(path.join(dest, d), { recursive: true });
  for (const f of tree.files) {
    const target = path.join(dest, f.path);
    fs.mkdirSync(path.dirname(target), { recursive: true });
    fs.writeFileSync(target, fileBytes(f));
  }
}

/** 폴더를 다시 읽어 상대 경로마다 종류와 바이트를 모은다. 링크는 따라가지 않는다. */
export function readDisk(root) {
  const entries = new Map();
  const walk = rel => {
    const abs = rel ? path.join(root, rel) : root;
    for (const name of fs.readdirSync(abs)) {
      const childRel = rel ? `${rel}/${name}` : name;
      const st = fs.lstatSync(path.join(root, childRel));
      if (st.isDirectory()) {
        entries.set(childRel, { kind: 'dir' });
        walk(childRel);
      } else if (st.isFile()) {
        entries.set(childRel, { kind: 'file', bytes: fs.readFileSync(path.join(root, childRel)) });
      } else {
        entries.set(childRel, { kind: 'other' });
      }
    }
  };
  walk('');
  return entries;
}

/**
 * 34.1의 12번: 다시 읽은 폴더를 입력 나무와 바이트로 맞대고 경고 글을 낸다. 나무가 없으면 빈 폴더와 맞댄다.
 * 새 파일이 든 새 폴더는 파일만 적고, 빈 새 폴더만 폴더로 적는다.
 */
export function compareWithTree(label, root, tree) {
  let disk;
  try {
    disk = readDisk(root);
  } catch (e) {
    return [`${label}: 다시 읽지 못했다. ${e.message}`];
  }
  const wantFiles = new Map();
  const wantDirs = new Set();
  if (tree) {
    for (const d of tree.dirs) for (const p of ancestorsAndSelf(d)) wantDirs.add(p);
    for (const f of tree.files) {
      wantFiles.set(f.path, fileBytes(f));
      for (const p of ancestorsAndSelf(path.posix.dirname(f.path))) if (p !== '.') wantDirs.add(p);
    }
  }
  const warnings = [];
  for (const [rel, bytes] of wantFiles) {
    const got = disk.get(rel);
    if (!got || got.kind === 'dir') warnings.push(`${label} ${rel}: 파일이 사라졌다`);
    else if (got.kind === 'other') warnings.push(`${label} ${rel}: 일반 파일이 아닌 것으로 바뀌었다`);
    else if (!got.bytes.equals(bytes)) warnings.push(`${label} ${rel}: 바이트가 바뀌었다`);
  }
  for (const rel of wantDirs) {
    const got = disk.get(rel);
    if (!got || got.kind !== 'dir') warnings.push(`${label} ${rel}: 폴더가 사라졌다`);
  }
  for (const [rel, got] of disk) {
    if (wantFiles.has(rel) || wantDirs.has(rel)) continue;
    if (got.kind === 'file') warnings.push(`${label} ${rel}: 파일이 새로 생겼다`);
    else if (got.kind === 'other') warnings.push(`${label} ${rel}: 일반 파일이 아닌 것(심볼릭 링크, 소켓, FIFO)이 생겼다`);
    else if (![...disk.keys()].some(k => k.startsWith(rel + '/'))) warnings.push(`${label} ${rel}: 빈 폴더가 새로 생겼다`);
  }
  return warnings.sort();
}

function ancestorsAndSelf(rel) {
  const out = [];
  let cur = rel;
  while (cur && cur !== '.') {
    out.push(cur);
    cur = path.posix.dirname(cur);
  }
  return out;
}

/** 폴더 1개를 나무 꼴로 담는다. 묶음을 짓는 쪽이 쓰는 도구이고, 실행기는 안 부른다. */
export function treeFromDisk(root, source, note) {
  const dirs = [];
  const files = [];
  const walk = rel => {
    const abs = rel ? path.join(root, rel) : root;
    const names = fs.readdirSync(abs).sort(compareUtf16);
    for (const name of names) {
      const childRel = rel ? `${rel}/${name}` : name;
      const st = fs.lstatSync(path.join(root, childRel));
      if (st.isDirectory()) {
        dirs.push(childRel);
        walk(childRel);
      } else if (st.isFile()) {
        const bytes = fs.readFileSync(path.join(root, childRel));
        let text = null;
        try {
          text = new TextDecoder('utf-8', { fatal: true, ignoreBOM: true }).decode(bytes);
        } catch {
          text = null;
        }
        if (text === null) {
          files.push({ path: childRel, base64: bytes.toString('base64') });
        } else {
          const eol = text.endsWith('\n');
          files.push({ path: childRel, lines: (eol ? text.slice(0, -1) : text).split('\n'), eol });
        }
      } else {
        throw new Error(`나무에 담을 수 없는 항목이다: ${childRel}`);
      }
    }
  };
  walk('');
  return { format: 'wlx-tree', version: 1, source, note, dirs, files };
}

export function compareUtf16(a, b) {
  return a < b ? -1 : a > b ? 1 : 0;
}
