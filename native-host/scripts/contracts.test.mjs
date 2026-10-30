import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
const read = name => JSON.parse(readFileSync(new URL('../src-tauri/' + name, import.meta.url)));
const config = read('tauri.conf.json');
test('bundle identity, deferred main creation and flat resource map', () => {
  assert.equal(config.identifier, 'co.kcd.withlog');
  assert.equal(config.productName, 'withlog');
  assert.deepEqual(config.app.windows, [{title:'withlog',width:800,height:600,create:false}]);
  assert.equal(config.bundle.resources['../../dist/wl-cli.jar'], 'wl-cli.jar');
  assert.equal(config.bundle.resources['../../dist/wl-sidecar.jar'], 'wl-sidecar.jar');
  for (const source of Object.keys(config.bundle.resources)) assert.ok(existsSync(new URL('../src-tauri/' + source, import.meta.url)));
  assert.deepEqual(config.plugins['deep-link'].desktop.schemes, ['withlog']);
  assert.deepEqual(config.bundle.fileAssociations[0].ext, ['md','markdown']);
});
test('empty filesystem scope and exact distinct window capabilities', () => {
  const main = read('capabilities/main.json'), note = read('capabilities/note.json');
  assert.deepEqual(main.windows, ['main']); assert.deepEqual(note.windows, ['note-*']);
  assert.deepEqual(note.permissions, main.permissions.filter(p=>p!=='deep-link:default').concat(['core:event:deny-emit','core:event:deny-emit-to']));
  for (const capability of [main,note]) {
    assert.ok(capability.permissions.includes('fs:allow-watch'));
    assert.ok(capability.permissions.includes('fs:deny-read-file'));
    assert.ok(!capability.permissions.includes('core:window:allow-close'));
    assert.ok(!capability.permissions.includes('core:window:allow-destroy'));
    assert.ok(capability.permissions.every(p=>typeof p==='string'));
  }
  assert.deepEqual(config.app.security.assetProtocol, {enable:true,scope:[]});
});
test('production and development CSP retain their separate boundaries', () => {
  assert.ok(config.app.security.csp.includes("script-src 'self';"));
  assert.ok(config.app.security.csp.includes("frame-ancestors 'none'"));
  assert.ok(config.app.security.devCsp.includes('ws://localhost:1420'));
  assert.ok(!config.app.security.devCsp.includes('media-src'));
  assert.ok(config.app.security.csp.endsWith('frame-src wlframe: http://wlframe.localhost'));
  assert.ok(!/<style\b|\sstyle\s*=/i.test(readFileSync(new URL('../web/index.html',import.meta.url),'utf8')));
});
