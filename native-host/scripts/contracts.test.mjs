import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
const read = name => JSON.parse(readFileSync(new URL('../src-tauri/' + name, import.meta.url)));
const config = read('tauri.conf.json');
test('bundle identity, deferred main creation and flat resource map', () => {
  assert.equal(config.identifier, 'co.kcd.aisurada');
  assert.equal(config.productName, 'aisurada');
  assert.deepEqual(config.app.windows, [{title:'aisurada',width:800,height:600,create:false}]);
  assert.equal(config.bundle.resources['../../dist/ais-cli.jar'], 'ais-cli.jar');
  assert.equal(config.bundle.resources['../../dist/ais-sidecar.jar'], 'ais-sidecar.jar');
  for (const source of Object.keys(config.bundle.resources)) assert.ok(existsSync(new URL('../src-tauri/' + source, import.meta.url)));
  assert.deepEqual(config.plugins['deep-link'].desktop.schemes, ['aisurada']);
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
  assert.ok(config.app.security.csp.endsWith('frame-src aisframe: http://aisframe.localhost'));
  assert.ok(!/<style\b|\sstyle\s*=/i.test(readFileSync(new URL('../index.html',import.meta.url),'utf8')));
  assert.equal(config.build.frontendDist, '../dist');
});

// 1.1, 10.10: the Part 4 modules and the nine commands exist only with the cargo feature `claude`, so a build made with
// --no-default-features has no Part 4 command (the host answers `Command <name> not found`).
test('Part 4 host modules and commands sit behind the claude feature', () => {
  const main = readFileSync(new URL('../src-tauri/src/main.rs', import.meta.url), 'utf8').split('\n');
  const gated = (pattern) => {
    const at = main.findIndex((line) => pattern.test(line));
    assert.ok(at > 0, `${pattern} is in main.rs`);
    assert.equal(main[at - 1].trim(), '#[cfg(feature = "claude")]', `${pattern} is gated`);
  };
  for (const name of ['ask', 'agents', 'session_image', 'dictation', 'session_shell']) gated(new RegExp(`^(pub )?mod ${name};`));
  for (const command of ['ask::ask_claude', 'ask::ask_runtime', 'agents::list_claude_agents', 'agents::alive_pids', 'agents::held_agent_sessions', 'agents::release_agent_session', 'session_image::write_session_image', 'dictation::start_dictation', 'session_shell::pty_start_session']) {
    gated(new RegExp(`^\\s*${command},?$`));
  }
});
