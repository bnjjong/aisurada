// 앱에 든 ais-cli 스크립트(src-tauri/resources/ais-cli)가 번들 Java로 번들 jar를 부르는지 잰다.
// 진짜 셸 스크립트를 가짜 앱 폴더에서 돌린다. 진짜 Java나 엔진은 쓰지 않는다.
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const script = fileURLToPath(new URL('../src-tauri/resources/ais-cli', import.meta.url));

// 인자를 한 줄에 하나씩 찍고 jar 환경 변수를 보여 주는 가짜 java
const FAKE_JAVA = `#!/bin/sh
for a in "$@"; do printf 'ARG:%s\\n' "$a"; done
printf 'SIDECAR_JAR:%s\\n' "\${AISURADA_SIDECAR_JAR-unset}"
exit "\${FAKE_JAVA_EXIT:-0}"
`;

function fakeApp(root, { withRuntime = true } = {}) {
  const resources = join(root, 'aisurada.app', 'Contents', 'Resources');
  mkdirSync(join(resources, 'bin'), { recursive: true });
  copyFileSync(script, join(resources, 'bin', 'ais-cli'));
  chmodSync(join(resources, 'bin', 'ais-cli'), 0o755);
  writeFileSync(join(resources, 'ais-cli.jar'), '');
  writeFileSync(join(resources, 'ais-sidecar.jar'), '');
  if (withRuntime) {
    mkdirSync(join(resources, 'runtime', 'bin'), { recursive: true });
    writeFileSync(join(resources, 'runtime', 'bin', 'java'), FAKE_JAVA);
    chmodSync(join(resources, 'runtime', 'bin', 'java'), 0o755);
  }
  return realpathSync(resources);
}

function run(file, args, env = {}) {
  const result = spawnSync(file, args, { encoding: 'utf8', env: { PATH: '/usr/bin:/bin', ...env } });
  const lines = result.stdout.split('\n').filter(Boolean);
  return {
    status: result.status,
    args: lines.filter(l => l.startsWith('ARG:')).map(l => l.slice(4)),
    sidecar: lines.find(l => l.startsWith('SIDECAR_JAR:'))?.slice('SIDECAR_JAR:'.length),
    stderr: result.stderr,
  };
}

function withTemp(fn) {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'ais-cli-script-')));
  try { return fn(root); } finally { rmSync(root, { recursive: true, force: true }); }
}

test('번들 Java로 번들 ais-cli.jar를 부르고 인자를 그대로 넘긴다', () => withTemp(root => {
  const resources = fakeApp(root);
  const r = run(join(resources, 'bin', 'ais-cli'), ['get_doc', '{"id":"nd 1","q":"it\'s"}', '한글']);
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual(r.args, ['-jar', join(resources, 'ais-cli.jar'), 'get_doc', '{"id":"nd 1","q":"it\'s"}', '한글']);
  assert.equal(r.sidecar, join(resources, 'ais-sidecar.jar'), '엔진 jar도 앱에 든 것으로 넘긴다');
}));

test('종료 코드를 그대로 돌려준다', () => withTemp(root => {
  const resources = fakeApp(root);
  assert.equal(run(join(resources, 'bin', 'ais-cli'), ['x'], { FAKE_JAVA_EXIT: '7' }).status, 7);
}));

test('심볼릭 링크로 불러도 앱 안의 자리를 찾는다(절대 링크, 상대 링크, 링크의 링크)', () => withTemp(root => {
  const resources = fakeApp(root);
  const real = join(resources, 'bin', 'ais-cli');
  mkdirSync(join(root, 'home', '.local', 'bin'), { recursive: true });
  const absolute = join(root, 'home', '.local', 'bin', 'ais-cli');
  symlinkSync(real, absolute);
  const second = join(root, 'home', 'ais');
  symlinkSync('.local/bin/ais-cli', second); // 상대 링크가 다른 링크를 가리킨다
  for (const file of [absolute, second]) {
    const r = run(file, ['list_tags']);
    assert.equal(r.status, 0, `${file}: ${r.stderr}`);
    assert.deepEqual(r.args, ['-jar', join(resources, 'ais-cli.jar'), 'list_tags'], file);
  }
}));

test('PATH가 비어 있어도 돈다(Finder나 launchd가 띄운 환경에는 dirname 같은 명령이 PATH에 없다)', () => withTemp(root => {
  const resources = fakeApp(root);
  const real = join(resources, 'bin', 'ais-cli');
  mkdirSync(join(root, 'home', '.local', 'bin'), { recursive: true });
  const link = join(root, 'home', '.local', 'bin', 'ais-cli');
  symlinkSync(real, link);
  for (const file of [real, link]) {
    const r = run(file, ['list_tags'], { PATH: '/nonexistent' });
    assert.equal(r.status, 0, `${file}: ${r.stderr}`);
    assert.deepEqual(r.args, ['-jar', join(resources, 'ais-cli.jar'), 'list_tags'], file);
  }
}));

test('사용자가 AISURADA_SIDECAR_JAR를 정해 두었으면 그것을 따른다', () => withTemp(root => {
  const resources = fakeApp(root);
  const r = run(join(resources, 'bin', 'ais-cli'), ['x'], { AISURADA_SIDECAR_JAR: '/my/own.jar' });
  assert.equal(r.sidecar, '/my/own.jar');
}));

test('개발용 빌드(target/…/bundle) 안에서는 낡을 수 있는 엔진 jar를 넘기지 않는다', () => withTemp(root => {
  const dev = join(root, 'repo', 'native-host', 'src-tauri', 'target', 'debug', 'bundle', 'macos');
  const resources = fakeApp(dev);
  const r = run(join(resources, 'bin', 'ais-cli'), ['x']);
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.sidecar, 'unset');
  assert.deepEqual(r.args, ['-jar', join(resources, 'ais-cli.jar'), 'x'], 'cli jar는 그대로 쓴다');
}));

test('번들 Java가 없는 앱(개발용 빌드)은 PATH의 java를 쓴다', () => withTemp(root => {
  const resources = fakeApp(root, { withRuntime: false });
  const bin = join(root, 'path-bin');
  mkdirSync(bin);
  writeFileSync(join(bin, 'java'), FAKE_JAVA);
  chmodSync(join(bin, 'java'), 0o755);
  const r = run(join(resources, 'bin', 'ais-cli'), ['list_docs'], { PATH: `${bin}:/usr/bin:/bin` });
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual(r.args, ['-jar', join(resources, 'ais-cli.jar'), 'list_docs']);
}));

test('스크립트는 실행 권한이 있고 sh 문법이 맞다', () => {
  execFileSync('/bin/sh', ['-n', script]);
  execFileSync('test', ['-x', script]);
});
