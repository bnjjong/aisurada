#!/usr/bin/env node
import { execFileSync } from 'node:child_process';
import { readFileSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { join } from 'node:path';

const host = fileURLToPath(new URL('..', import.meta.url));
const pkg = JSON.parse(readFileSync(join(host, 'package.json'), 'utf8'));
const config = JSON.parse(readFileSync(join(host, 'src-tauri/tauri.conf.json'), 'utf8'));
const cargo = readFileSync(join(host, 'src-tauri/Cargo.toml'), 'utf8');
const cargoVersion = cargo.match(/^version = "([^"]+)"/m)?.[1];
if (pkg.version !== config.version || pkg.version !== cargoVersion) throw Error('App versions do not match.');
const tag = process.env.AISURADA_RELEASE_TAG || (process.env.GITHUB_REF_TYPE === 'tag' ? process.env.GITHUB_REF_NAME : undefined);
if (tag && tag !== `v${pkg.version}`) throw Error(`Tag ${tag} does not match app version ${pkg.version}.`);
// Never expose a developer-specific name classification table in a public binary.
const env = { ...process.env, AISURADA_RELEASE_BUILD: '1' };
// Drop only generated release resources, including read-only files from older JRE copies.
for (const path of ['src-tauri/target/release/runtime', 'src-tauri/target/release/bundle', '.release/artifacts']) {
  rmSync(join(host, path), { recursive: true, force: true });
}
execFileSync(process.execPath, [join(host, 'scripts/prepare-runtime.mjs')], { cwd: host, env, stdio: 'inherit' });
execFileSync('pnpm', ['tauri', 'build', '--config', 'src-tauri/tauri.release.conf.json', '--bundles', 'app,dmg'], {
  cwd: host, env, stdio: 'inherit',
});
execFileSync(process.execPath, [join(host, 'scripts/verify-release.mjs')], { cwd: host, env, stdio: 'inherit' });
execFileSync(process.execPath, [join(host, 'scripts/verify-dmg.mjs')], { cwd: host, env, stdio: 'inherit' });
execFileSync(process.execPath, [join(host, 'scripts/release-artifacts.mjs')], { cwd: host, env, stdio: 'inherit' });
