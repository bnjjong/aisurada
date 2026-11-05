// 설정 화면의 「Claude 연결」: 상태 글 읽기와 화면 상태. 단추를 눌러 호스트까지 가는 왕복은
// `pnpm test:part4:acceptance`가 실제 창에서 잰다.
import assert from 'node:assert/strict';
import test from 'node:test';
import React from 'react';
import { renderToString } from 'react-dom/server';
import { ConnectView, connectFailure, parseConnectStatus } from '../src/plugs/claude/connect.tsx';
import * as emptyPlug from '../src/plugs/empty/connect.tsx';

const noop = () => {};
const view = (props) => renderToString(React.createElement(ConnectView, {
  status: null, busy: false, error: '', write: false, desktop: '', onWrite: noop, onConnect: noop, onDisconnect: noop, ...props,
}));
const status = (over) => ({ claudeFound: true, registered: false, write: false, detail: '', ...over });

test('상태 글을 읽는다', () => {
  assert.deepEqual(parseConnectStatus('{"claudeFound":true,"registered":true,"write":true,"detail":"x"}'),
    { claudeFound: true, registered: true, write: true, detail: 'x' });
  assert.deepEqual(parseConnectStatus('{"claudeFound":false,"registered":false}'),
    { claudeFound: false, registered: false, write: false, detail: '' }, 'write와 detail이 없어도 기본값을 채운다');
  for (const bad of ['', 'not json', '[]', '{}', '{"claudeFound":"yes","registered":false}', 'null']) {
    assert.equal(parseConnectStatus(bad), null, `모양이 틀린 글: ${bad}`);
  }
});

test('실패 이유는 표준 오류, 없으면 표준 출력, 그것도 없으면 종료 코드', () => {
  assert.equal(connectFailure({ status: 1, stdout: 'out', stderr: ' 연결을 못 했다 \n' }), '연결을 못 했다');
  assert.equal(connectFailure({ status: 1, stdout: ' only out ', stderr: '' }), 'only out');
  assert.match(connectFailure({ status: 7, stdout: '', stderr: '' }), /종료 코드 7/);
});

test('확인하는 동안에는 단추가 없다', () => {
  const html = view({ status: null });
  assert.match(html, /확인하는 중이에요/);
  assert.doesNotMatch(html, /<button/);
});

test('Claude Code가 없으면 연결 단추 대신 안내만 보인다', () => {
  const html = view({ status: status({ claudeFound: false }) });
  assert.match(html, /Claude Code를 찾지 못했어요/);
  assert.doesNotMatch(html, /<button|type="checkbox"/);
});

test('연결 전: 연결 단추만 있고 끊기는 없다. 쓰기는 기본이 꺼짐', () => {
  const html = view({ status: status() });
  assert.match(html, /연결되어 있지 않아요/);
  assert.match(html, /Claude Code에 연결/);
  assert.doesNotMatch(html, /연결 끊기/);
  assert.doesNotMatch(html, /checked/, '쓰기 허용은 꺼져 있다');
});

test('연결됨: 읽기 전용과 읽고 쓰기를 구분해 말하고, 다시 연결과 끊기가 있다', () => {
  const read = view({ status: status({ registered: true, write: false }) });
  assert.match(read, /노트를 읽기만 해요/);
  assert.match(read, /이 설정으로 다시 연결/);
  assert.match(read, /연결 끊기/);
  const write = view({ status: status({ registered: true, write: true }), write: true });
  assert.match(write, /노트를 읽고 쓸 수 있어요/);
  assert.match(write, /checked/);
});

test('일하는 동안에는 단추와 쓰기 선택이 잠긴다', () => {
  const html = view({ status: status({ registered: true }), busy: true });
  assert.equal((html.match(/disabled/g) || []).length >= 3, true, '연결, 끊기, 쓰기 선택이 모두 잠긴다');
});

test('오류는 alert로 보인다', () => {
  assert.match(view({ status: status(), error: '연결을 못 했다: boom' }), /role="alert"[^>]*>연결을 못 했다: boom/);
  assert.doesNotMatch(view({ status: status() }), /role="alert"/);
});

test('Claude Desktop용 조각은 있을 때만 접힌 칸으로 보인다', () => {
  assert.doesNotMatch(view({ status: status() }), /<details/);
  const html = view({ status: status(), desktop: '{\n  "mcpServers": {}\n}' });
  assert.match(html, /<details/);
  assert.match(html, /claude_desktop_config\.json/);
  assert.match(html, /mcpServers/);
});

test('4부를 끈 판의 빈 구현은 아무것도 그리지 않는다', () => {
  assert.equal(emptyPlug.ClaudeConnect(), null);
});
