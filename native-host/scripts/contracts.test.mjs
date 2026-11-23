import test from 'node:test';
import assert from 'node:assert/strict';
import {readFileSync, existsSync} from 'node:fs';
const read = name => JSON.parse(readFileSync(new URL('../src-tauri/' + name, import.meta.url)));
const config = read('tauri.conf.json');
test('bundle identity, deferred main creation and flat resource map', () => {
  assert.equal(config.identifier, 'co.kcd.aisurada');
  assert.equal(config.productName, 'aisurada');
  assert.deepEqual(config.app.windows, [{title:'aisurada',width:1040,height:720,minWidth:900,minHeight:540,create:false}]);
  // 5단계 뒤의 번들에는 Java 실행 환경도 jar도 없다. 개발용 번들은 bin/ais-cli 스크립트만 싣는다
  assert.deepEqual(config.bundle.resources, {'resources/ais-cli': 'bin/ais-cli'});
  for (const source of Object.keys(config.bundle.resources)) assert.ok(existsSync(new URL('../src-tauri/' + source, import.meta.url)));
  assert.deepEqual(config.plugins['deep-link'].desktop.schemes, ['aisurada']);
  assert.deepEqual(config.bundle.fileAssociations[0].ext, ['md','markdown']);
});
test('release bundle ships the Rust ais-cli binary and no Java', () => {
  const release = read('tauri.release.conf.json');
  // 릴리스 설정이 개발용 스크립트를 빼고(null) 지은 실행 파일을 같은 자리에 넣는다(JSON merge patch)
  assert.equal(release.bundle.resources['resources/ais-cli'], null);
  assert.equal(release.bundle.resources['../.release/bin/ais-cli'], 'bin/ais-cli');
  const merged = {...config.bundle.resources};
  for (const [key, value] of Object.entries(release.bundle.resources)) { if (value === null) delete merged[key]; else merged[key] = value; }
  assert.deepEqual(Object.values(merged).filter(target => target === 'bin/ais-cli'), ['bin/ais-cli'], 'exactly one source for bin/ais-cli');
  for (const target of Object.values(merged)) assert.ok(!/\.jar$|^runtime/.test(target), `Java resource still bundled: ${target}`);
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
  assert.ok(config.app.security.csp.endsWith('frame-src aisframe: http://aisframe.localhost https://embed.diagrams.net'));
  assert.ok(!/<style\b|\sstyle\s*=/i.test(readFileSync(new URL('../index.html',import.meta.url),'utf8')));
  assert.equal(config.build.frontendDist, '../dist');
});

// 1.1, 10.10: the Part 4 modules, the runner and their commands exist only with the cargo feature `claude`, so a build made with
// --no-default-features has no Part 4 command (the host answers `Command <name> not found`).
test('Part 4 host modules and commands sit behind the claude feature', () => {
  const main = readFileSync(new URL('../src-tauri/src/main.rs', import.meta.url), 'utf8').split('\n');
  const gated = (pattern) => {
    const at = main.findIndex((line) => pattern.test(line));
    assert.ok(at > 0, `${pattern} is in main.rs`);
    assert.equal(main[at - 1].trim(), '#[cfg(feature = "claude")]', `${pattern} is gated`);
  };
  for (const name of ['ask', 'agents', 'session_image', 'dictation', 'session_shell', 'runner']) gated(new RegExp(`^(pub )?mod ${name};`));
  for (const command of ['ask::ask_claude', 'ask::ask_runtime', 'agents::list_claude_agents', 'agents::alive_pids', 'agents::held_agent_sessions', 'agents::release_agent_session', 'session_image::write_session_image', 'dictation::start_dictation', 'session_shell::pty_start_session',
    'runner::runner_status', 'runner::runner_settings_set', 'runner::runner_run', 'runner::runner_stop', 'runner::runner_answer', 'runner::runner_runs', 'runner::runner_recent_runs']) {
    gated(new RegExp(`^\\s*${command},?$`));
  }
});
