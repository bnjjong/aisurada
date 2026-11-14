import test from 'node:test';
import assert from 'node:assert/strict';
import { engineCall, readWorkspace, SidecarCommandError } from '../src/workspace/engine.ts';
import { read } from '../src/data/commands.ts';

// 가짜 호스트: 기록을 남기고, 명령마다 정해진 응답을 돌려준다.
function host({ commands, java = {}, rust = {} } = {}) {
  const calls = [];
  return {
    calls,
    invoke: async (command, args) => {
      calls.push({ command, args });
      if (command === 'sidecar_call') {
        const handler = java[args.argv[0]];
        if (!handler) throw new Error(`Java 응답이 없다: ${args.argv[0]}`);
        return handler(args.argv);
      }
      if (command === 'engine_call') {
        const handler = rust[args.command];
        if (!handler) throw { code: 'AIS-409', kind: 'unsupported', message: `없는 명령 ${args.command}` };
        return handler(args.args);
      }
      throw new Error(`모르는 호스트 명령: ${command}`);
    },
    ...(commands ? { engineCommands: async () => (commands instanceof Error ? Promise.reject(commands) : new Set(commands)) } : {}),
  };
}
const javaOk = value => () => ({ status: 0, stdout: JSON.stringify(value), stderr: '' });
const page = items => ({ items, total: items.length, returned: items.length, truncated: false });

test('engineCommands가 없는 호스트(시험, 브라우저)는 호출 객체도 Java 엔진으로 풀어서 보낸다', async () => {
  const h = host({ java: { 'list-docs': javaOk([{ id: 'a' }]) } });
  assert.deepEqual(await engineCall(h, read.docs()), [{ id: 'a' }]);
  assert.deepEqual(h.calls, [{ command: 'sidecar_call', args: { argv: ['list-docs', 'limit=0', 'bodies=true'] } }]);
});

test('문자열 목록(argv)은 Rust 엔진이 지원하는 이름이어도 늘 Java 엔진으로 간다', async () => {
  const h = host({ commands: ['list_docs'], java: { 'list-docs': javaOk([]) } });
  await engineCall(h, ['list-docs', 'limit=0']);
  assert.deepEqual(h.calls.map(c => c.command), ['sidecar_call']);
});

test('호스트가 지원한다고 알린 명령은 Rust 엔진으로 가고, 응답을 기존 모양으로 바꾼다', async () => {
  const h = host({ commands: ['list_docs', 'list_agendas'], rust: {
    list_docs: () => page([{ id: 'n1', created: '2026-11-05T16:08:00+09:00', title: 't', folder: true }]),
    list_agendas: () => page([{ id: 'ag1', slug: 'demo', title: '데모' }]),
  } });
  const docs = await engineCall(h, read.docs());
  assert.deepEqual(h.calls[0], { command: 'engine_call', args: { command: 'list_docs', args: { limit: 0, bodies: true } } });
  // 목록은 배열로 풀리고, 빼기로 한 date·time은 created에서 다시 채운다
  assert.deepEqual(docs, [{ id: 'n1', created: '2026-11-05T16:08:00+09:00', title: 't', folder: true, date: '2026-11-05', time: '16:08' }]);
  assert.deepEqual(await engineCall(h, read.agendas()), [{ id: 'ag1', slug: 'demo', title: '데모' }]);
  assert.deepEqual(h.calls.map(c => c.command), ['engine_call', 'engine_call']);
});

test('호스트가 지원 목록에 안 넣은 호출 객체는 Java 엔진으로 간다', async () => {
  const h = host({ commands: ['list_agendas'], java: { 'list-trash': javaOk([{ trashId: 't' }]) } });
  assert.deepEqual(await engineCall(h, read.trash()), [{ trashId: 't' }]);
  assert.deepEqual(h.calls.map(c => c.command), ['sidecar_call']);
});

test('지원 목록을 못 받으면(오류) Java 엔진으로 간다', async () => {
  const h = host({ commands: new Error('호스트에 그 명령이 없다'), java: { reindex: javaOk({ mode: 'full' }) } });
  assert.deepEqual(await engineCall(h, read.reindex()), { mode: 'full' });
  assert.deepEqual(h.calls.map(c => c.command), ['sidecar_call']);
});

test('Rust 엔진의 오류는 종류와 옛 종료 코드와 글을 담은 SidecarCommandError가 된다', async () => {
  const cases = [
    [{ code: 'AIS-404', kind: 'not_found', message: '없다' }, 3, 'AIS-404 없다'],
    [{ code: 'AIS-400', kind: 'invalid_argument', message: '틀렸다' }, 3, 'AIS-400 틀렸다'],
    [{ code: 'AIS-412', kind: 'precondition_failed', message: '바뀌었다' }, 5, 'AIS-412 바뀌었다'],
    [{ code: 'AIS-500', kind: 'internal', message: '터졌다' }, 1, 'AIS-500 터졌다'],
    ['Command engine_call not found', 1, 'Command engine_call not found'],
  ];
  for (const [failure, status, stderr] of cases) {
    const h = host({ commands: ['list_docs'], rust: { list_docs: () => { throw failure; } } });
    await assert.rejects(engineCall(h, read.docs()), error => {
      assert.ok(error instanceof SidecarCommandError);
      assert.equal(error.status, status);
      assert.equal(error.stderr, stderr);
      assert.equal(error.kind, typeof failure === 'object' ? failure.kind : undefined);
      return true;
    });
  }
});

test('Java 엔진의 오류에는 kind가 없고 종료 코드와 글이 그대로 남는다', async () => {
  const h = host({ java: { 'list-docs': () => ({ status: 5, stdout: '', stderr: 'AIS-412 확인 값이 어긋난다\n' }) } });
  await assert.rejects(engineCall(h, read.docs()), error => error.status === 5 && error.kind === undefined && error.stderr === 'AIS-412 확인 값이 어긋난다');
});

test('readWorkspace는 Rust 엔진 응답으로도 같은 작업 공간을 만든다', async () => {
  const note = (id, created, extra = {}) => ({ id, agenda: 'demo', type: 'free', title: `제목 ${id}`, created, format: 'markdown', links: [], tags: [], path: `notes/demo/${id}`, folder: true, body: `본문 ${id}`, ...extra });
  const rust = {
    reindex: () => ({ mode: 'full', agendas: 1, scanned: 2, indexed: 2, skipped: [{ kind: 'note', path: 'notes/demo/x', reason: '프론트매터가 없다' }], conflicts: [{ kind: 'note', path: 'notes/demo/y (conflicted copy)' }], unsharded: [] }),
    list_templates: () => page([{ id: 'plan', category: 'doc', name: '계획' }, { id: 'r', category: 'ritual', name: '회고' }, { id: 'z', category: 'other', name: '버림' }]),
    list_trash: () => page([{ trashId: 'tr1', docId: 'n9' }]),
    list_agendas: () => page([{ id: 'ag1', slug: 'demo', title: '데모' }]),
    list_projects: () => page([{ id: 'pj1', slug: 'p', title: 'P' }]),
    list_docs: () => page([note('n1', '2026-11-05T09:30:00+09:00'), note('n2', '2026-11-06T10:00:00Z')]),
  };
  const h = host({ commands: Object.keys(rust), rust });
  const workspace = await readWorkspace(h, true);
  assert.deepEqual(h.calls.map(c => c.command).sort(), Array(6).fill('engine_call'));
  assert.deepEqual(workspace.notes.map(n => [n.id, n.agendaId, n.date, n.time, n.body]), [['n1', 'ag1', '2026-11-05', '09:30', '본문 n1'], ['n2', 'ag1', '2026-11-06', '10:00', '본문 n2']]);
  assert.deepEqual(workspace.templates.map(t => t.id), ['plan', 'r']);
  assert.equal(workspace.trash.length, 1);
  assert.equal(workspace.projects.length, 1);
  assert.deepEqual(workspace.skipped.map(s => s.reason), ['프론트매터가 없다']);
  assert.equal(workspace.conflicts.length, 1);
  assert.equal(workspace.noteReadFailed, false);
});

test('readWorkspace: Rust 엔진이 노트 목록만 못 읽으면 noteReadFailed가 된다', async () => {
  const rust = {
    reindex: () => ({ skipped: [], conflicts: [] }), list_templates: () => page([]), list_trash: () => page([]),
    list_agendas: () => page([]), list_projects: () => page([]),
    list_docs: () => { throw { code: 'AIS-500', kind: 'internal', message: '터졌다' }; },
  };
  const h = host({ commands: Object.keys(rust), rust });
  const workspace = await readWorkspace(h, false);
  assert.equal(workspace.noteReadFailed, true);
  assert.deepEqual(workspace.notes, []);
});
