import { readFileSync, readdirSync } from 'node:fs';
import { resolve, relative, isAbsolute, extname, join } from 'node:path';
import { parse } from 'parse5';

// PDF 1.1: even an unused string containing one of these fails the off build.
export const PART4_COMMANDS = [
  'alive_pids', 'list_claude_agents', 'ask_claude', 'ask_runtime',
  'start_dictation', 'write_session_image', 'pty_start_session',
  'held_agent_sessions', 'release_agent_session',
];

function* elements(node) {
  if (node.tagName) yield node;
  for (const child of node.childNodes || []) yield* elements(child);
  if (node.content) yield* elements(node.content);
}

function* files(directory) {
  for (const entry of readdirSync(directory, { withFileTypes: true })) {
    const path = join(directory, entry.name);
    if (entry.isDirectory()) yield* files(path);
    else if (entry.isFile()) yield path;
  }
}

function check(condition, message) {
  if (!condition) throw new Error(`화면 빌드 계약 위반: ${message}`);
}

export function assertScreenBundle(directory, { part4 = false } = {}) {
  const root = resolve(directory);
  const html = readFileSync(join(root, 'index.html'), 'utf8');
  const nodes = [...elements(parse(html, { sourceCodeLocationInfo: true }))];
  check(nodes.some(n => n.tagName === 'head' && n.sourceCodeLocation?.startTag), '<head>가 필요합니다.');
  check(nodes.some(n => n.tagName === 'body' && n.sourceCodeLocation?.endTag), '</body>가 필요합니다.');
  let scripts = 0;
  let stylesheets = 0;
  for (const node of nodes) {
    const attrs = Object.fromEntries(node.attrs.map(a => [a.name, a.value]));
    check(node.tagName !== 'style' && !Object.hasOwn(attrs, 'style'), '인라인 스타일은 0개여야 합니다.');
    check(!Object.keys(attrs).some(a => a.startsWith('on')), '인라인 이벤트 스크립트를 사용할 수 없습니다.');
    check(node.tagName !== 'base' && node.tagName !== 'object', '<base>와 <object>를 사용할 수 없습니다.');
    let asset;
    if (node.tagName === 'script') {
      check(attrs.src && !node.childNodes?.some(n => n.value?.trim()), '스크립트는 외부 .js 파일이어야 합니다.');
      check(attrs.type === 'module', '진입 스크립트는 module이어야 합니다.');
      asset = attrs.src;
      scripts++;
    } else if (node.tagName === 'link' && ['stylesheet', 'modulepreload'].includes(attrs.rel)) {
      check(Boolean(attrs.href), '스타일시트와 modulepreload에는 자원 주소가 필요합니다.');
      asset = attrs.href;
      if (attrs.rel === 'stylesheet') stylesheets++;
    }
    if (asset !== undefined) {
      check(asset.startsWith('/') && !asset.startsWith('//') && !asset.includes('\\'), `자원 주소는 /로 시작해야 합니다: ${asset}`);
      const path = resolve(root, '.' + new URL(asset, 'http://localhost').pathname);
      const rel = relative(root, path);
      check(rel && rel !== '..' && !rel.startsWith('../') && !isAbsolute(rel), '자원이 화면 폴더 밖을 가리킵니다.');
      const extension = node.tagName === 'script' || attrs.rel === 'modulepreload' ? '.js' : '.css';
      check(extname(path) === extension, `자원 확장자가 ${extension}여야 합니다: ${asset}`);
      readFileSync(path);
    }
  }
  check(scripts > 0 && stylesheets > 0, 'JS와 CSS 자원 파일이 필요합니다.');
  const javascript = [...files(join(root, 'assets'))].filter(path => extname(path) === '.js');
  check(javascript.length > 0, 'assets/에 JS 파일이 필요합니다.');
  let part4Commands = 0;
  for (const path of javascript) {
    const source = readFileSync(path, 'utf8');
    for (const command of PART4_COMMANDS) {
      if (part4) {
        if (source.includes(command)) part4Commands++;
      } else {
        check(!source.includes(command), `${relative(root, path)}에 4부 커맨드 ${command}가 남았습니다.`);
      }
    }
  }
  // 1.8: the Part 4 screen names a Part 4 command in at least one .js file.
  if (part4) check(part4Commands > 0, '4부를 켠 화면 번들에 4부 커맨드 이름이 하나도 없습니다.');
  return { javascript: javascript.length, inlineStyles: 0, part4Commands };
}
