import test from 'node:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { assertScreenBundle, PART4_COMMANDS } from './screen-contracts.mjs';

const valid = '<!doctype html><html lang="ko"><head><script type="module" src="/assets/app.js"></script><link rel="stylesheet" href="/assets/app.css"></head><body><div id="root"></div></body></html>';

function bundle(t, html = valid) {
  const directory = mkdtempSync(join(tmpdir(), 'ais-screen-contract-'));
  t.after(() => rmSync(directory, { recursive: true, force: true }));
  mkdirSync(join(directory, 'assets/workers'), { recursive: true });
  writeFileSync(join(directory, 'index.html'), html);
  writeFileSync(join(directory, 'assets/app.js'), 'console.log("aisurada");');
  writeFileSync(join(directory, 'assets/app.css'), 'body { margin: 0; }');
  return directory;
}

test('accepts external assets and ignores markup inside comments', t => {
  const directory = bundle(t, valid.replace('<head>', '<head><!-- <style>not a style element</style> -->'));
  assert.deepEqual(assertScreenBundle(directory), { javascript: 1, inlineStyles: 0, part4Commands: 0 });
});

test('rejects inline CSS, including case variants and template content', t => {
  for (const markup of ['<STYLE>body{}</STYLE>', '<p STYLE="color:red">text</p>', '<template><div style=""></div></template>']) {
    const directory = bundle(t, valid.replace('<div id="root"></div>', markup));
    assert.throws(() => assertScreenBundle(directory), /인라인 스타일/);
  }
});

test('rejects inline scripts and event handlers under the production CSP', t => {
  for (const markup of ['<script>window.test=true</script>', '<button onclick="test()">test</button>']) {
    const directory = bundle(t, valid.replace('<div id="root"></div>', markup));
    assert.throws(() => assertScreenBundle(directory), /스크립트/);
  }
});

test('rejects relative, remote and wrong-extension asset URLs and missing files', t => {
  for (const url of ['assets/app.js', '//example.com/app.js', 'https://example.com/app.js', '/assets/app.txt', '/assets/missing.js']) {
    const directory = bundle(t, valid.replace('/assets/app.js', url));
    assert.throws(() => assertScreenBundle(directory));
  }
  const missingHref = bundle(t, valid.replace(' href="/assets/app.css"', ''));
  assert.throws(() => assertScreenBundle(missingHref), /자원 주소/);
});

test('requires explicit head and closing body for acceptance script insertion', t => {
  for (const html of [valid.replace('<head>', ''), valid.replace('</body>', '')]) {
    const directory = bundle(t, html);
    assert.throws(() => assertScreenBundle(directory), /head|body/);
  }
});

test('rejects each of the nine Part 4 names even inside a nested worker string', t => {
  for (const command of PART4_COMMANDS) {
    const directory = bundle(t);
    writeFileSync(join(directory, 'assets/workers/nested.js'), `const unused = ${JSON.stringify(command)};`);
    assert.throws(() => assertScreenBundle(directory), new RegExp(command));
  }
});
