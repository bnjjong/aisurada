#!/usr/bin/env node
import { createHash } from 'node:crypto';
import { copyFileSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const host = fileURLToPath(new URL('..', import.meta.url));
const version = JSON.parse(readFileSync(join(host, 'package.json'), 'utf8')).version;
const artifacts = join(host, '.release/artifacts');
mkdirSync(artifacts, { recursive: true });
const dmgDir = join(host, 'src-tauri/target/release/bundle/dmg');
const dmgs = readdirSync(dmgDir).filter(name => name.endsWith(`_${version}_aarch64.dmg`));
if (dmgs.length !== 1) throw Error(`Expected one ${version} ARM64 DMG, found ${dmgs.length}.`);
copyFileSync(join(dmgDir, dmgs[0]), join(artifacts, `aisurada_${version}_aarch64.dmg`));
copyFileSync(join(host, '../docs/user-guide.html'), join(artifacts, 'user-guide.html'));
copyFileSync(join(host, `../docs/releases/${version}.md`), join(artifacts, 'release-notes.md'));
const digest = file => createHash('sha256').update(readFileSync(file)).digest('hex');
const reportPath = join(artifacts, 'validation.json');
const report = JSON.parse(readFileSync(reportPath, 'utf8'));
report.artifacts = { dmg: `aisurada_${version}_aarch64.dmg`, sha256: digest(join(artifacts, `aisurada_${version}_aarch64.dmg`)) };
writeFileSync(reportPath, JSON.stringify(report, null, 2) + '\n');
const names = [`aisurada_${version}_aarch64.dmg`, 'user-guide.html', 'validation.json'];
writeFileSync(join(artifacts, 'SHA256SUMS'), names.map(name => `${digest(join(artifacts, name))}  ${name}\n`).join(''));
console.log(`Release assets prepared: ${artifacts}`);
