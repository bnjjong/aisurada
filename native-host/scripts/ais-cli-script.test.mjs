// 개발용 번들의 ais-cli 스크립트(src-tauri/resources/ais-cli)가 저장소의 빌드 폴더에서 Rust 실행 파일을 찾아 넘기는지 잰다.
// 진짜 셸 스크립트를 가짜 저장소 폴더에서 돌린다. 진짜 엔진은 쓰지 않는다. 정식 번들에는 이 스크립트가 아니라 Rust 실행 파일이 들어간다.
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, copyFileSync, mkdirSync, mkdtempSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';

const script = fileURLToPath(new URL('../src-tauri/resources/ais-cli', import.meta.url));

// 인자를 한 줄에 하나씩 찍고 자기 이름표를 보여 주는 가짜 ais-cli
const fakeBinary = tag => `#!/bin/sh
printf 'WHO:${tag}\\n'
for a in "$@"; do printf 'ARG:%s\\n' "$a"; done
exit "\${FAKE_EXIT:-0}"
`;

function fakeRepo(root, { release = true, debug = false } = {}) {
  const repo = realpathSync(root);
  mkdirSync(join(repo, 'engine-rs/target/release'), { recursive: true });
  mkdirSync(join(repo, 'engine-rs/target/debug'), { recursive: true });
  writeFileSync(join(repo, 'engine-rs/Cargo.toml'), '[workspace]\n');
  for (const [profile, on] of [['release', release], ['debug', debug]]) {
    if (!on) continue;
    const file = join(repo, 'engine-rs/target', profile, 'ais-cli');
    writeFileSync(file, fakeBinary(profile));
    chmodSync(file, 0o755);
  }
  const resources = join(repo, 'native-host/src-tauri/target/debug/bundle/macos/aisurada.app/Contents/Resources');
  mkdirSync(join(resources, 'bin'), { recursive: true });
  copyFileSync(script, join(resources, 'bin/ais-cli'));
  chmodSync(join(resources, 'bin/ais-cli'), 0o755);
  return { repo, launcher: join(resources, 'bin/ais-cli') };
}

function run(file, args, env = {}) {
  const result = spawnSync(file, args, { encoding: 'utf8', env: { PATH: '/usr/bin:/bin', ...env } });
  return { status: result.status, lines: result.stdout.split('\n').filter(Boolean), stderr: result.stderr };
}

test('finds the repository build, prefers release over debug and passes arguments through', () => {
  const root = mkdtempSync(join(tmpdir(), 'ais-cli-script-'));
  try {
    const { launcher } = fakeRepo(root, { release: true, debug: true });
    const out = run(launcher, ['get_doc', '{"id":"x y"}', '한글']);
    assert.equal(out.status, 0);
    assert.deepEqual(out.lines, ['WHO:release', 'ARG:get_doc', 'ARG:{"id":"x y"}', 'ARG:한글']);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('falls back to the debug build and keeps the exit code', () => {
  const root = mkdtempSync(join(tmpdir(), 'ais-cli-script-'));
  try {
    const { launcher } = fakeRepo(root, { release: false, debug: true });
    const out = run(launcher, [], { FAKE_EXIT: '3' });
    assert.equal(out.status, 3);
    assert.deepEqual(out.lines, ['WHO:debug']);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('works when called through a symlink and with an empty PATH', () => {
  const root = mkdtempSync(join(tmpdir(), 'ais-cli-script-'));
  try {
    const { launcher } = fakeRepo(root);
    const bin = join(root, 'home-bin');
    mkdirSync(bin);
    symlinkSync(launcher, join(bin, 'ais-cli'));
    const out = run(join(bin, 'ais-cli'), ['x'], { PATH: '' });
    assert.deepEqual(out.lines, ['WHO:release', 'ARG:x']);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

test('AISURADA_CLI_BIN wins, and a missing build is reported with exit code 127', () => {
  const root = mkdtempSync(join(tmpdir(), 'ais-cli-script-'));
  try {
    const { launcher } = fakeRepo(root, { release: false, debug: false });
    const none = run(launcher, []);
    assert.equal(none.status, 127);
    assert.match(none.stderr, /ais-cli 실행 파일을 못 찾았다/);
    const custom = join(root, 'custom');
    writeFileSync(custom, fakeBinary('custom'));
    chmodSync(custom, 0o755);
    assert.deepEqual(run(launcher, ['a'], { AISURADA_CLI_BIN: custom }).lines, ['WHO:custom', 'ARG:a']);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
