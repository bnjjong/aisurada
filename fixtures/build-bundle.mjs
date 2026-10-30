#!/usr/bin/env node
// fixtures/expect 묶음을 다시 짓는 도구. 묶음 밖에 두어야 무결성 검사의 「목록에 없는 파일」로 안 걸린다.
//
//   node fixtures/build-bundle.mjs [--tree-from <vault 폴더>]
//
// --tree-from을 주면 그 폴더를 trees/rebuild-fixture.v1.json으로 다시 담는다.
// 그다음 묶음 안 파일마다 sha256과 케이스 수를 bundle.json에 다시 적는다. note, provenance, history는 그대로 둔다.
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { treeFromDisk } from './expect/lib/tree.mjs';
import { listBundleFiles, sha256, laneNames, readLaneFiles } from './expect/lib/bundle.mjs';

const BUNDLE = path.join(path.dirname(fileURLToPath(import.meta.url)), 'expect');
const TREE_NAME = 'rebuild-fixture.v1';

const args = process.argv.slice(2);
if (args[0] === '--tree-from') {
  const src = path.resolve(args[1]);
  const tree = treeFromDisk(src, 'withlog-p1-rebuild/selftest/vault',
    '다시 짜기 개발 폴더의 픽스처 vault. 설명서 34.1의 fixture.v1(파일 149개, 폴더 149개)이 이 기기에 없어서 대신 담았다.');
  fs.writeFileSync(path.join(BUNDLE, 'trees', `${TREE_NAME}.json`), JSON.stringify(tree, null, 2) + '\n');
  process.stdout.write(`trees/${TREE_NAME}.json: 파일 ${tree.files.length}개, 폴더 ${tree.dirs.length}개\n`);
} else if (args.length > 0) {
  process.stderr.write('쓰는 법: node fixtures/build-bundle.mjs [--tree-from <vault 폴더>]\n');
  process.exit(2);
}

const manifestPath = path.join(BUNDLE, 'bundle.json');
const old = fs.existsSync(manifestPath) ? JSON.parse(fs.readFileSync(manifestPath, 'utf8')) : {};
const files = {};
for (const rel of listBundleFiles(BUNDLE)) files[rel] = sha256(fs.readFileSync(path.join(BUNDLE, rel)));
const counts = {};
for (const f of readLaneFiles(BUNDLE, laneNames(BUNDLE))) {
  counts[`lanes/${f.lane}/${f.name}`] = f.cases.length;
}
const manifest = {
  format: 'wlx-bundle',
  version: 1,
  node: '>=20',
  note: old.note ?? '',
  counts,
  provenance: old.provenance ?? {},
  history: old.history ?? [],
  files
};
fs.writeFileSync(manifestPath, JSON.stringify(manifest, null, 2) + '\n');
process.stdout.write(`bundle.json: 파일 ${Object.keys(files).length}개, 케이스 ${Object.values(counts).reduce((a, b) => a + b, 0)}건\n`);
