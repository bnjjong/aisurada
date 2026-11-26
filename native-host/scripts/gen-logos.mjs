#!/usr/bin/env node
// C4 요소 아이콘의 서비스 로고 표(`src/ui/architecture/logos.ts`)를 만든다. 로고 이름의 원본은 엔진의
// `engine-rs/crates/ais-domain/src/c4/icons.rs`의 `LOGOS`이고, 그림(SVG 경로)과 브랜드 색은 Simple Icons 패키지에서 가져온다.
//
// 사용법: node scripts/gen-logos.mjs <simple-icons 패키지 폴더>
//   예) npm pack simple-icons@16.38.1 && tar -xzf simple-icons-16.38.1.tgz -C /tmp/si && node scripts/gen-logos.mjs /tmp/si/package
// 패키지는 앱에 들어가지 않는다(필요한 로고 경로만 `logos.ts`에 글자로 복사한다).
import { readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const pkgDir = process.argv[2];
if (!pkgDir) throw Error('사용법: node scripts/gen-logos.mjs <simple-icons 패키지 폴더>');
const root = fileURLToPath(new URL('..', import.meta.url));
const rust = readFileSync(join(root, '../engine-rs/crates/ais-domain/src/c4/icons.rs'), 'utf8');
const block = rust.match(/pub const LOGOS: \[&str; \d+\] = \[(.*?)\];/s)?.[1];
if (!block) throw Error('icons.rs에서 LOGOS를 찾지 못했다');
const slugs = [...block.matchAll(/"([^"]+)"/g)].map(match => match[1]);
const meta = new Map(JSON.parse(readFileSync(join(pkgDir, 'data/simple-icons.json'), 'utf8')).map(item => [item.slug, item]));
const version = JSON.parse(readFileSync(join(pkgDir, 'package.json'), 'utf8')).version;

const rows = slugs.map(slug => {
  const info = meta.get(slug);
  if (!info) throw Error(`Simple Icons에 ${slug}가 없다`);
  const svg = readFileSync(join(pkgDir, 'icons', `${slug}.svg`), 'utf8');
  const path = svg.match(/<path d="([^"]+)"/)?.[1];
  if (!path) throw Error(`${slug}.svg에서 경로를 못 찾았다`);
  return `  ${JSON.stringify(slug)}: { title: ${JSON.stringify(info.title)}, hex: ${JSON.stringify(info.hex)}, path: ${JSON.stringify(path)} },`;
});
const out = `// 서비스 로고 ${slugs.length}개. Simple Icons ${version}(CC0-1.0)에서 \`scripts/gen-logos.mjs\`로 만든 파일이라 직접 고치지 않는다.
// 로고 이름의 원본은 엔진의 \`ais-domain/src/c4/icons.rs\`다(이름을 바꾸면 이 파일을 다시 만든다).
// 상표권은 각 회사에 있다. 앱 안에서 그 서비스를 가리키는 표시로만 쓴다. 자세한 안내는 \`docs/developer-guide.md\`의 C4 절에 있다.
// 그림은 24×24 칸의 한 덩어리 경로이고 \`fill\`로 채운다. \`hex\`는 브랜드 색(# 없이)이다.

export type Logo = { title: string; hex: string; path: string };

export const LOGOS: Record<string, Logo> = {
${rows.join('\n')}
};
`;
writeFileSync(join(root, 'src/ui/architecture/logos.ts'), out);
console.log(`logos.ts: ${slugs.length}개, ${out.length}자 (Simple Icons ${version})`);
