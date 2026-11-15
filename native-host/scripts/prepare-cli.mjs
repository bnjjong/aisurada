#!/usr/bin/env node
// 앱에 동봉할 Rust `ais-cli` 실행 파일을 release 빌드해서 `.release/bin/ais-cli`에 둔다.
// Java 실행 환경과 jar는 더 이상 동봉하지 않는다. 이 시험판은 Apple Silicon macOS에서만 짓는다.
import { execFileSync } from 'node:child_process';
import { chmodSync, copyFileSync, existsSync, mkdirSync, statSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const host = fileURLToPath(new URL('..', import.meta.url));
if (process.platform !== 'darwin' || process.arch !== 'arm64') {
  throw Error('This trial release supports builds on Apple Silicon macOS only.');
}
const engine = join(host, '../engine-rs');
execFileSync('cargo', ['build', '--release', '--locked', '-p', 'ais-cli', '--manifest-path', join(engine, 'Cargo.toml')], { stdio: 'inherit' });
const built = join(engine, 'target/release/ais-cli');
if (!existsSync(built)) throw Error(`ais-cli was not built: ${built}`);
const target = join(host, '.release/bin/ais-cli');
mkdirSync(join(host, '.release/bin'), { recursive: true });
copyFileSync(built, target);
chmodSync(target, 0o755);
console.log(`ais-cli ${(statSync(target).size / 1024 / 1024).toFixed(1)} MiB prepared: ${target}`);
