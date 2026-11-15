import test from 'node:test';
import assert from 'node:assert/strict';
import { engineCall, engineReply, readWorkspace, SidecarCommandError } from '../src/workspace/engine.ts';
import { read, write, fromArgv, toArgv } from '../src/data/commands.ts';

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

test('노트를 여는 길: Java 경로는 예전 자리 인자(positional) 목록 그대로 보낸다', async () => {
  const seen = [];
  const h = host({ java: Object.fromEntries(['get-doc', 'list-sub-notes', 'get-sub-note', 'list-comments', 'get-agenda-facts'].map(name => [name, argv => { seen.push(argv); return { status: 0, stdout: '{}', stderr: '' }; }])) });
  await engineCall(h, read.doc('n1'));
  await engineCall(h, read.subNotes('n1/s1'));
  await engineCall(h, read.subNote('n1/s1', 's2'));
  await engineCall(h, read.comments('n1'));
  await engineCall(h, read.comments('n1', 's1'));
  await engineCall(h, read.facts('ag1'));
  assert.deepEqual(seen, [['get-doc', 'n1'], ['list-sub-notes', 'n1/s1', 'bodies=true'], ['get-sub-note', 'n1/s1', 's2'],
    ['list-comments', 'n1'], ['list-comments', 'n1', 's1'], ['get-agenda-facts', 'ag1']]);
});

test('노트를 여는 길: Rust 응답을 옛 모양으로 바꾼다(date·time, 부른 사슬로 채운 ref와 parent)', async () => {
  const row = (id, extra = {}) => ({ id, created: '2026-11-05T16:08:00+09:00', title: id, links: [], tags: [], path: `notes/x/${id}`, folder: false, ...extra });
  const rust = {
    get_doc: args => ({ ...row(args.id, { body: '본문', comments: { state: 'absent' } }) }),
    list_sub_notes: () => page([row('s1', { rootId: 'n1', parentId: 'n1' }), row('s2', { rootId: 'n1', parentId: 'n1' })]),
    get_sub_note: args => row(args.id, { rootId: 'n1', parentId: 's1', body: '후속 본문', comments: { state: 'loaded', comments: [] } }),
    list_comments: () => ({ state: 'loaded', comments: [{ id: 'cm1-a', createdAt: 'x', body: 'b' }] }),
    list_attachments: () => page([{ name: 'a.png', ref: '![a.png](attach/a.png)' }]),
    get_agenda_facts: args => ({ id: args.id, slug: 'demo', limit: 2000, editLimit: 10000, facts: '사실' }),
  };
  const h = host({ commands: Object.keys(rust), rust });

  const doc = await engineCall(h, read.doc('n1'));
  assert.deepEqual([doc.date, doc.time, doc.body, doc.comments.state], ['2026-11-05', '16:08', '본문', 'absent']);
  assert.deepEqual(h.calls[0].args, { command: 'get_doc', args: { id: 'n1' } });

  const subs = await engineCall(h, read.subNotes('n1/s1'));
  assert.equal(subs.parent, 'n1/s1');
  assert.deepEqual(subs.subNotes.map(r => [r.id, r.ref, r.date, r.rootId]), [['s1', 'n1/s1/s1', '2026-11-05', 'n1'], ['s2', 'n1/s1/s2', '2026-11-05', 'n1']]);
  assert.deepEqual(h.calls[1].args, { command: 'list_sub_notes', args: { parent: 'n1/s1', bodies: true } });

  const sub = await engineCall(h, read.subNote('n1/s1', 's9'));
  assert.deepEqual([sub.id, sub.parent, sub.time, sub.body, sub.comments.state], ['s9', 'n1/s1', '16:08', '후속 본문', 'loaded']);

  assert.equal((await engineCall(h, read.comments('n1', 's1'))).comments[0].id, 'cm1-a');
  assert.deepEqual(h.calls[3].args, { command: 'list_comments', args: { id: 'n1', subId: 's1' } });
  assert.equal((await engineCall(h, read.facts('ag1'))).facts, '사실');
  assert.deepEqual(await engineCall(h, { command: 'list_attachments', args: { id: 'n1', subId: 's1' } }), { parent: 'n1/s1', attachments: [{ name: 'a.png', ref: '![a.png](attach/a.png)' }] });
});

test('노트를 여는 길: Rust 엔진이 못 찾으면 not_found 오류가 된다', async () => {
  const h = host({ commands: ['get_doc'], rust: { get_doc: () => { throw { code: 'AIS-404', kind: 'not_found', message: 'Note not found: n9' }; } } });
  await assert.rejects(engineCall(h, read.doc('n9')), error => error.status === 3 && error.kind === 'not_found' && error.stderr === 'AIS-404 Note not found: n9');
});

test('판 목록(history): Rust 엔진으로 가고 응답은 그대로이며, Java 경로는 자리 인자(번호)를 둔다', async () => {
  const response = { id: 'n1', count: 1, noteBody: '본문', versions: [{ index: 1, commit: 'a'.repeat(40) }] };
  const h = host({ commands: ['history'], rust: { history: () => response } });
  assert.deepEqual(await engineCall(h, read.history('n1', { commit: 'a'.repeat(40) })), response);
  assert.deepEqual(h.calls[0].args, { command: 'history', args: { id: 'n1', commit: 'a'.repeat(40) } });

  const seen = [];
  const java = host({ java: { history: argv => { seen.push(argv); return { status: 0, stdout: JSON.stringify(response), stderr: '' }; } } });
  await engineCall(java, read.history('n1', { number: 2, query: '글자' }));
  await engineCall(java, read.history('n1'));
  assert.deepEqual(seen, [['history', 'n1', '2', 'query=글자'], ['history', 'n1']]);
});

test('판 목록(history): 판 번호가 범위 밖이면 conflict 오류가 된다', async () => {
  const h = host({ commands: ['history'], rust: { history: () => { throw { code: 'AIS-409', kind: 'conflict', message: '판 번호가 범위 밖이다(1 부터 2 까지 있다): 3' }; } } });
  await assert.rejects(engineCall(h, read.history('n1', { number: 3 })), error => error.status === 3 && error.kind === 'conflict' && error.stderr.startsWith('AIS-409 판 번호가 범위 밖이다'));
});

test('쓰기 스위치: 호스트가 쓰기 명령을 알리면 아젠다 쓰기와 커밋이 Rust 엔진으로 가고, 아니면 Java 인자 그대로 간다', async () => {
  const row = { id: 'ag1', slug: 'a', title: 'A', hue: 'blue', summary: '', deepLink: 'aisurada://agenda/a?ws=x' };
  const rust = { create_agenda: () => row, update_agenda: () => row, delete_agenda: args => ({ id: args.id, slug: 'a', archivedPath: 'p', notesDir: 'notes/a' }), commit_vault: () => ({ outcome: 'committed' }) };
  const on = host({ commands: Object.keys(rust), rust });
  assert.deepEqual(await engineCall(on, write.createAgenda({ title: 'A', hue: 'blue' })), row);
  assert.deepEqual(await engineCall(on, write.updateAgenda('ag1', { project: '', summary: '요약' })), row);
  assert.equal((await engineCall(on, write.deleteAgenda('ag1'))).archivedPath, 'p');
  assert.deepEqual(await engineCall(on, read.commit()), { outcome: 'committed' });
  assert.deepEqual(on.calls.map(c => c.args), [
    { command: 'create_agenda', args: { title: 'A', hue: 'blue' } },
    { command: 'update_agenda', args: { id: 'ag1', project: '', summary: '요약' } },
    { command: 'delete_agenda', args: { id: 'ag1' } },
    { command: 'commit_vault', args: {} },
  ]);

  // 쓰기 스위치가 꺼져 있으면 호스트가 읽기 명령만 알린다 → 옛 Java 인자 그대로
  const seen = [];
  const ok = () => ({ status: 0, stdout: '{}', stderr: '' });
  const off = host({ commands: ['list_docs'], java: Object.fromEntries(['create-agenda', 'update-agenda', 'delete-agenda', 'commit-vault'].map(n => [n, argv => { seen.push(argv); return ok(); }])) });
  await engineCall(off, write.createAgenda({ title: 'A', repo: undefined }));
  await engineCall(off, write.updateAgenda('ag1', { repo: '', project: '' }));
  await engineCall(off, write.deleteAgenda('ag1'));
  await engineCall(off, read.commit());
  assert.deepEqual(seen, [['create-agenda', 'title=A'], ['update-agenda', 'ag1', 'repo=', 'project='], ['delete-agenda', 'ag1'], ['commit-vault']]);
});

test('쓰기 오류: Rust의 conflict와 invalid_argument는 읽기와 같은 방식으로 오류가 되고 engineRefusal은 이유 글을 돌려준다', async () => {
  const { engineRefusal } = await import('../src/workspace/engine.ts');
  const h = host({ commands: ['delete_agenda'], rust: { delete_agenda: () => { throw { code: 'AIS-400', kind: 'invalid_argument', message: '노트가 2건 있는 아젠다는 지울 수 없다. 먼저 그 노트를 옮기거나 버린다: a' }; } } });
  await assert.rejects(engineCall(h, write.deleteAgenda('ag1')), error => error.status === 3 && error.kind === 'invalid_argument');
  assert.deepEqual(await engineRefusal(h, write.deleteAgenda('ag1')), { ok: false, reason: 'AIS-400 노트가 2건 있는 아젠다는 지울 수 없다. 먼저 그 노트를 옮기거나 버린다: a' });
});

test('쓰기 스위치: 프로젝트 쓰기도 같은 길을 따르고, 꺼져 있으면 옛 Java 인자 그대로 간다', async () => {
  const repos = '[{"name":"web","path":"/tmp/w"}]';
  const row = { id: 'pj1', slug: 'p', title: 'P', hue: 'blue', summary: '' };
  const rust = { create_project: () => row, update_project: () => row, delete_project: args => ({ id: args.id, slug: 'p', archivedPath: 'a' }) };
  const on = host({ commands: Object.keys(rust), rust });
  await engineCall(on, write.createProject({ title: 'P', repos }));
  await engineCall(on, write.updateProject('pj1', { repos: '' }));
  await engineCall(on, write.deleteProject('pj1'));
  assert.deepEqual(on.calls.map(c => c.args), [
    { command: 'create_project', args: { title: 'P', repos } },
    { command: 'update_project', args: { id: 'pj1', repos: '' } },
    { command: 'delete_project', args: { id: 'pj1' } },
  ]);
  const seen = [];
  const off = host({ commands: [], java: Object.fromEntries(['create-project', 'update-project', 'delete-project'].map(n => [n, argv => { seen.push(argv); return { status: 0, stdout: '{}', stderr: '' }; }])) });
  await engineCall(off, write.createProject({ title: 'P', repos }));
  await engineCall(off, write.updateProject('pj1', { repos: '' }));
  await engineCall(off, write.deleteProject('pj1'));
  assert.deepEqual(seen, [['create-project', 'title=P', `repos=${repos}`], ['update-project', 'pj1', 'repos='], ['delete-project', 'pj1']]);
});

test('쓰기 스위치: 템플릿과 사실층 쓰기도 같은 길을 따른다', async () => {
  const rust = { save_template: () => ({ id: 't', action: 'created' }), save_ritual_template: () => ({ id: 'r', action: 'updated' }), delete_doc_template: () => ({ id: 't', removed: [] }), update_agenda_facts: () => ({ id: 'ag', facts: '사실' }) };
  const on = host({ commands: Object.keys(rust), rust });
  await engineCall(on, write.saveTemplate({ name: 'N', body: 'B' }));
  await engineCall(on, write.saveRitual('r', { desc: 'd' }));
  await engineCall(on, write.deleteTemplate('t'));
  await engineCall(on, write.facts('ag', '  사실  '));
  assert.deepEqual(on.calls.map(c => c.args), [
    { command: 'save_template', args: { name: 'N', body: 'B' } },
    { command: 'save_ritual_template', args: { id: 'r', desc: 'd' } },
    { command: 'delete_doc_template', args: { id: 't' } },
    { command: 'update_agenda_facts', args: { id: 'ag', facts: '사실' } },
  ]);
  const seen = [];
  const off = host({ commands: [], java: Object.fromEntries(['save-template', 'save-ritual-template', 'delete-doc-template', 'update-agenda-facts'].map(n => [n, argv => { seen.push(argv); return { status: 0, stdout: '{}', stderr: '' }; }])) });
  await engineCall(off, write.saveTemplate({ name: 'N', body: 'B' }));
  await engineCall(off, write.saveRitual('r', { desc: 'd' }));
  await engineCall(off, write.deleteTemplate('t'));
  await engineCall(off, write.facts('ag', '  사실  '));
  assert.deepEqual(seen, [['save-template', 'name=N', 'body=B'], ['save-ritual-template', 'r', 'desc=d'], ['delete-doc-template', 't'], ['update-agenda-facts', 'ag', 'facts=사실']]);
});

test('쓰기 스위치: init-workspace도 같은 길을 따르고, 꺼져 있으면 옛 인자(templates=[]) 그대로 간다', async () => {
  const on = host({ commands: ['init_workspace'], rust: { init_workspace: () => ({ created: true, seeded: [], skipped: [] }) } });
  assert.equal((await engineCall(on, write.init('내 노트'))).created, true);
  assert.deepEqual(on.calls[0].args, { command: 'init_workspace', args: { name: '내 노트', templates: '[]' } });
  const seen = [];
  const off = host({ commands: [], java: { 'init-workspace': argv => { seen.push(argv); return { status: 0, stdout: '{"created":false}', stderr: '' }; } } });
  await engineCall(off, write.init('내 노트'));
  assert.deepEqual(seen, [['init-workspace', 'name=내 노트', 'templates=[]']]);
});

test('쓰기 스위치: 즐겨찾기·평가·연결 쓰기도 같은 길을 따르고, 확인 값(f1)은 인자 목록 맨 끝으로 풀린다', async () => {
  const rust = { set_favorite: () => ({ id: 'n', changed: true }), evaluate_doc: () => ({ id: 'n' }), link_docs: () => ({ id: 'n', changed: true }) };
  const on = host({ commands: Object.keys(rust), rust });
  await engineCall(on, write.favorite('n', { favorite: true }, 'f1:abc'));
  await engineCall(on, write.evaluate('n', { score: 80, note: '좋다' }, 'f1:abc'));
  await engineCall(on, write.link('a', 'b', 'direct', false, 'f1:abc'));
  // 실제 IPC는 JSON으로 보내서 undefined 값이 든 키는 사라진다
  assert.deepEqual(on.calls.map(c => JSON.parse(JSON.stringify(c.args))), [
    { command: 'set_favorite', args: { id: 'n', favorite: true, f1: 'f1:abc' } },
    { command: 'evaluate_doc', args: { id: 'n', score: 80, note: '좋다', f1: 'f1:abc' } },
    { command: 'link_docs', args: { from: 'a', to: 'b', kind: 'direct', f1: 'f1:abc' } },
  ]);
  const seen = [];
  const off = host({ commands: [], java: Object.fromEntries(['set-favorite', 'evaluate-doc', 'link-docs'].map(n => [n, argv => { seen.push(argv); return { status: 0, stdout: '{}', stderr: '' }; }])) });
  await engineCall(off, write.favorite('n', { favorite: true }, 'f1:abc'));
  await engineCall(off, write.evaluate('n', { score: 80 }));
  await engineCall(off, write.link('a', 'b', 'direct', true, 'f1:abc'));
  assert.deepEqual(seen, [['set-favorite', 'n', 'favorite=true', 'f1=f1:abc'], ['evaluate-doc', 'n', 'score=80'], ['link-docs', 'a', 'b', 'kind=direct', 'remove=true', 'f1=f1:abc']]);
});

test('쓰기 스위치: update_doc도 같은 길을 따르고, 태그는 배열 그대로 가며 옛 길에서는 JSON 글로 풀린다', async () => {
  const on = host({ commands: ['update_doc'], rust: { update_doc: () => ({ id: 'n', path: 'notes/demo/n', precondition: 'f1:new', renamed: false }) } });
  const response = await engineCall(on, write.updateDoc('n', { title: '새 제목', tags: ['a', 'b'], body: '본문' }, 'f1:abc'));
  assert.equal(response.precondition, 'f1:new');
  assert.deepEqual(JSON.parse(JSON.stringify(on.calls[0].args)), { command: 'update_doc', args: { id: 'n', title: '새 제목', tags: ['a', 'b'], body: '본문', f1: 'f1:abc' } });
  const seen = [];
  const off = host({ commands: [], java: { 'update-doc': argv => { seen.push(argv); return { status: 0, stdout: '{}', stderr: '' }; } } });
  await engineCall(off, write.updateDoc('n', { title: '새 제목', tags: ['a', 'b'], time: '09:30' }, 'f1:abc'));
  assert.deepEqual(seen, [['update-doc', 'n', 'title=새 제목', 'tags=["a","b"]', 'time=09:30', 'f1=f1:abc']]);
});

test('쓰기 스위치: create_doc도 같은 길을 따르고, 옛 길에서는 예전 인자 목록 그대로 풀린다', async () => {
  const on = host({ commands: ['create_doc'], rust: { create_doc: () => ({ id: 'ndx-00000001', agenda: 'demo', agendaCreated: false, path: 'notes/demo/x', precondition: 'f1:new' }) } });
  const doc = { agenda: 'demo', type: 'free', title: '새 노트', body: '본문', date: '2026-11-01', time: '12:30', tags: ['a'], template: 't', tier: 'record' };
  assert.equal((await engineCall(on, write.createDoc(doc))).id, 'ndx-00000001');
  assert.deepEqual(JSON.parse(JSON.stringify(on.calls[0].args)), { command: 'create_doc', args: { ...doc, format: 'markdown' } });
  const seen = [];
  const off = host({ commands: [], java: { 'create-doc': argv => { seen.push(argv); return { status: 0, stdout: '{}', stderr: '' }; } } });
  await engineCall(off, write.createDoc({ ...doc, tier: 'reading', template: undefined }));
  assert.deepEqual(seen, [['create-doc', 'agenda=demo', 'type=free', 'title=새 노트', 'body=본문', 'date=2026-11-01', 'time=12:30', 'format=markdown', 'tags=["a"]']]);
});

test('쓰기 스위치: 후속 노트 쓰기 3개도 같은 길을 따르고, 옛 길에서는 부모와 id가 자리 인자로 풀린다', async () => {
  const rust = { create_sub_note: () => ({ id: 'sn1', ref: 'n/sn1' }), update_sub_note: () => ({ id: 'sn1', precondition: 'f1:new' }), delete_sub_note: () => ({ id: 'sn1', deleted: true }) };
  const on = host({ commands: Object.keys(rust), rust });
  await engineCall(on, write.createSub('n'));
  await engineCall(on, write.updateSub('n/sn0', 'sn1', { title: '제목', tags: ['a'] }, 'f1:abc'));
  await engineCall(on, write.deleteSub('n', 'sn1'));
  assert.deepEqual(on.calls.map(c => JSON.parse(JSON.stringify(c.args))), [
    { command: 'create_sub_note', args: { parent: 'n', title: '새 후속 노트', body: '# 새 후속 노트\n\n', type: 'free' } },
    { command: 'update_sub_note', args: { parent: 'n/sn0', id: 'sn1', title: '제목', tags: ['a'], f1: 'f1:abc' } },
    { command: 'delete_sub_note', args: { parent: 'n', id: 'sn1' } },
  ]);
  const seen = [];
  const off = host({ commands: [], java: Object.fromEntries(['create-sub-note', 'update-sub-note', 'delete-sub-note'].map(n => [n, argv => { seen.push(argv); return { status: 0, stdout: '{}', stderr: '' }; }])) });
  await engineCall(off, write.createSub('n'));
  await engineCall(off, write.updateSub('n/sn0', 'sn1', { body: '본문' }, 'f1:abc'));
  await engineCall(off, write.deleteSub('n', 'sn1'));
  assert.deepEqual(seen, [['create-sub-note', 'n', 'title=새 후속 노트', 'body=# 새 후속 노트\n\n', 'type=free'], ['update-sub-note', 'n/sn0', 'sn1', 'body=본문', 'f1=f1:abc'], ['delete-sub-note', 'n', 'sn1']]);
});

test('쓰기 스위치: 코멘트 쓰기 3개도 같은 길을 따르고, 옛 길에서는 노트와 후속 노트 id가 자리 인자로 풀린다', async () => {
  const rust = { create_comment: () => ({ id: 'cm1-a', total: 1 }), update_comment: () => ({ id: 'cm1-a', total: 1 }), delete_comment: () => ({ id: 'cm1-a', remaining: 0, markerStripped: false }) };
  const on = host({ commands: Object.keys(rust), rust });
  await engineCall(on, write.createComment('n', undefined, 'cm1-a', '본문', '인용'));
  await engineCall(on, write.updateComment('n', 'sn1', 'cm1-a', '고침'));
  await engineCall(on, write.deleteComment('n', 'sn1', 'cm1-a'));
  assert.deepEqual(on.calls.map(c => JSON.parse(JSON.stringify(c.args))), [
    { command: 'create_comment', args: { id: 'n', body: '본문', comment_id: 'cm1-a', anchor_quote: '인용' } },
    { command: 'update_comment', args: { id: 'n', subId: 'sn1', comment_id: 'cm1-a', body: '고침' } },
    { command: 'delete_comment', args: { id: 'n', subId: 'sn1', comment_id: 'cm1-a' } },
  ]);
  const seen = [];
  const off = host({ commands: [], java: Object.fromEntries(['create-comment', 'update-comment', 'delete-comment'].map(n => [n, argv => { seen.push(argv); return { status: 0, stdout: '{}', stderr: '' }; }])) });
  await engineCall(off, write.createComment('n', undefined, 'cm1-a', '본문'));
  await engineCall(off, write.updateComment('n', 'sn1', 'cm1-a', '고침'));
  await engineCall(off, write.deleteComment('n', undefined, 'cm1-a'));
  assert.deepEqual(seen, [['create-comment', 'n', 'body=본문', 'comment_id=cm1-a'], ['update-comment', 'n', 'sn1', 'comment_id=cm1-a', 'body=고침'], ['delete-comment', 'n', 'comment_id=cm1-a']]);
});

test('쓰기 스위치: 휴지통 쓰기 4개도 같은 길을 따르고, 옛 길에서는 id가 자리 인자로 풀린다', async () => {
  const rust = { delete_doc: () => ({ trashId: 'tr1', file: 'tr1' }), restore_doc: () => ({ trashId: 'tr1', restored: true }), purge_doc: () => ({ trashId: 'tr1', purged: true }), empty_trash: () => ({ purged: 0, failed: [], skipped: 0, orphans: 0 }) };
  const on = host({ commands: Object.keys(rust), rust });
  await engineCall(on, write.deleteDoc('n'));
  await engineCall(on, write.restore('tr1'));
  await engineCall(on, write.purge('tr1'));
  await engineCall(on, write.emptyTrash());
  assert.deepEqual(on.calls.map(c => JSON.parse(JSON.stringify(c.args))), [
    { command: 'delete_doc', args: { id: 'n' } },
    { command: 'restore_doc', args: { trashId: 'tr1' } },
    { command: 'purge_doc', args: { trashId: 'tr1' } },
    { command: 'empty_trash', args: {} },
  ]);
  const seen = [];
  const off = host({ commands: [], java: Object.fromEntries(['delete-doc', 'restore-doc', 'purge-doc', 'empty-trash'].map(n => [n, argv => { seen.push(argv); return { status: 0, stdout: '{}', stderr: '' }; }])) });
  await engineCall(off, write.deleteDoc('n'));
  await engineCall(off, write.restore('tr1'));
  await engineCall(off, write.purge('tr1'));
  await engineCall(off, write.emptyTrash());
  assert.deepEqual(seen, [['delete-doc', 'n'], ['restore-doc', 'tr1'], ['purge-doc', 'tr1'], ['empty-trash']]);
});

test('태스크 명령: 옛 인자 목록을 호출 객체로 바꿔 Rust로 보내고, 꺼져 있으면 같은 인자 목록으로 Java에 간다', async () => {
  const argvs = [
    ['create-task', 'title=제목', 'repo=a/b', 'criteria=["x"]', 'type=feature'],
    ['get-task', 'TSK-1', 'activity=100'],
    ['list-tasks', 'trash=true'],
    ['update-task', 'TSK-1', 'f1=f1:abc', 'title=a=b', 'labels=[]'],
    ['transition-task', 'TSK-1', 'to=done', 'from=in-review'],
    ['link-tasks', 'TSK-1', 'TSK-2', 'type=blocks', 'remove=true'],
    ['add-task-comment', 'TSK-1', 'body=안녕'],
    ['delete-task', 'TSK-1'],
    ['restore-task', 'TSK-1'],
  ];
  for (const argv of argvs) assert.deepEqual(toArgv(fromArgv(argv)), argv, argv[0]);
  assert.deepEqual(fromArgv(['update-task', 'TSK-1', 'f1=f1:abc', 'title=a=b']), { command: 'update_task', args: { id: 'TSK-1', f1: 'f1:abc', title: 'a=b' } });
  assert.deepEqual(fromArgv(['link-tasks', 'TSK-1', 'TSK-2', 'type=blocks']), { command: 'link_tasks', args: { from: 'TSK-1', to: 'TSK-2', type: 'blocks' } });
  assert.deepEqual(fromArgv(['list-tasks']), { command: 'list_tasks' });
  const names = ['create_task', 'get_task', 'list_tasks', 'update_task', 'transition_task', 'link_tasks', 'add_task_comment', 'delete_task', 'restore_task'];
  const on = host({ commands: names, rust: Object.fromEntries(names.map(n => [n, () => ({ ok: n })])) });
  for (const argv of argvs) await engineCall(on, fromArgv(argv));
  assert.deepEqual(on.calls.map(c => c.args.command), names);
  const seen = [];
  const off = host({ commands: [], java: Object.fromEntries(argvs.map(a => [a[0], argv => { seen.push(argv); return { status: 0, stdout: '{}', stderr: '' }; }])) });
  for (const argv of argvs) await engineCall(off, fromArgv(argv));
  assert.deepEqual(seen, argvs);
});

test('자원·승인 명령: 옛 인자 목록을 호출 객체로 바꿔 Rust로 보내고, 꺼져 있으면 같은 인자 목록으로 Java에 간다', async () => {
  const argvs = [
    ['list-resources', 'scope=global', 'kind=skill'],
    ['get-resource', 'sk1-aaaaaaaa'],
    ['save-resource', 'id=ap1-aaaaaaaa', 'skills=["sk1-aaaaaaaa"]', 'repo=', 'tools={}', 'f1=f1:abc'],
    ['delete-resource', 'sk1-aaaaaaaa'],
    ['restore-resource', 'sk1-aaaaaaaa'],
    ['set-resource-ref', 'alpha', 'ap1-aaaaaaaa', 'remove=true', 'with-skills=true'],
    ['usable-resources', 'alpha'],
    ['list-resource-versions', 'sk1-aaaaaaaa'],
    ['list-resource-changes', 'decided=20'],
    ['get-resource-change', 'pc1-aaaaaaaa'],
    ['approve-resource-change', 'pc1-aaaaaaaa'],
    ['reject-resource-change', 'pc1-aaaaaaaa', 'reason=이유'],
    ['resolve-resource-edit', 'refs:alpha', 'action=revert'],
    ['get-resource-policy'],
    ['set-resource-policy', 'project=approve', 'global=auto'],
  ];
  for (const argv of argvs) assert.deepEqual(toArgv(fromArgv(argv)), argv, argv[0]);
  assert.deepEqual(fromArgv(['set-resource-ref', 'alpha', 'ap1', 'with-skills=true']), { command: 'set_resource_ref', args: { project: 'alpha', id: 'ap1', 'with-skills': 'true' } });
  assert.deepEqual(fromArgv(['resolve-resource-edit', 'refs:alpha', 'action=revert']), { command: 'resolve_resource_edit', args: { id: 'refs:alpha', action: 'revert' } });
  const names = ['list_resources', 'get_resource', 'save_resource', 'delete_resource', 'restore_resource', 'set_resource_ref', 'usable_resources', 'list_resource_versions',
    'list_resource_changes', 'get_resource_change', 'approve_resource_change', 'reject_resource_change', 'resolve_resource_edit', 'get_resource_policy', 'set_resource_policy'];
  const on = host({ commands: names, rust: Object.fromEntries(names.map(n => [n, () => ({ ok: n })])) });
  for (const argv of argvs) await engineCall(on, fromArgv(argv));
  assert.deepEqual(on.calls.map(c => c.args.command), names);
  const seen = [];
  const off = host({ commands: [], java: Object.fromEntries(argvs.map(a => [a[0], argv => { seen.push(argv); return { status: 0, stdout: '{}', stderr: '' }; }])) });
  for (const argv of argvs) await engineCall(off, fromArgv(argv));
  assert.deepEqual(seen, argvs);
});

test('문답·대화 명령: 선택 자리 인자(후속 노트 id)를 이름 칸과 헷갈리지 않고, 옛 응답 모양으로 되돌린다', async () => {
  const note = 'ndfx0002-00000002', sub = 'snfx0002-00000012';
  const argvs = [
    ['get-ask', note], ['get-ask', note, sub], ['list-chats', note], ['list-chats', note, sub],
    ['append-ask-turn', note, 'session-id=s1', 'turn-id=t1', 'question=a=b', 'answer='],
    ['append-ask-turn', note, sub, 'session-id=s1', 'turn-id=t1', 'quote=줄1\n줄2'],
    ['append-chat-turn', note, 'chat-id=c1', 'elapsed-ms=5'],
    ['append-chat-turn', note, sub, 'chat-id=c1', 'elapsed-ms=5'],
  ];
  for (const argv of argvs) assert.deepEqual(toArgv(fromArgv(argv)), argv, argv.join(' '));
  assert.deepEqual(fromArgv(['append-ask-turn', note, 'session-id=s1', 'question=a=b']), { command: 'append_ask_turn', args: { id: note, 'session-id': 's1', question: 'a=b' } });
  assert.deepEqual(fromArgv(['list-chats', note, sub]), { command: 'list_chats', args: { id: note, subId: sub } });
  const names = ['get_ask', 'list_chats', 'append_ask_turn', 'append_chat_turn'];
  // 알리면 Rust로 가고, 응답은 옛 `{status, stdout, stderr}` 모양이다(없는 문답은 stdout `null`)
  const on = host({ commands: names, rust: { get_ask: () => null, list_chats: () => ({ chats: [], unreadable: [] }), append_ask_turn: () => { throw { code: 'AIS-400', kind: 'invalid_argument', message: 'model 이 필요하다' }; }, append_chat_turn: () => ({ chat: {}, path: 'p' }) } });
  assert.deepEqual(await engineReply(on, ['get-ask', note]), { status: 0, stdout: 'null', stderr: '' });
  assert.deepEqual(await engineReply(on, ['list-chats', note, sub]), { status: 0, stdout: '{"chats":[],"unreadable":[]}', stderr: '' });
  assert.deepEqual(await engineReply(on, ['append-ask-turn', note, 'session-id=s']), { status: 3, stdout: '', stderr: 'AIS-400 model 이 필요하다' });
  assert.equal((await engineReply(on, ['append-chat-turn', note, 'chat-id=c'])).status, 0);
  assert.deepEqual(on.calls.map(c => c.args.command), ['get_ask', 'list_chats', 'append_ask_turn', 'append_chat_turn']);
  assert.deepEqual(on.calls[1].args.args, { id: note, subId: sub });
  // 알리지 않으면 인자 목록 그대로 Java로 가고 응답을 건드리지 않는다
  const seen = [];
  const off = host({ commands: [], java: { 'get-ask': argv => { seen.push(argv); return { status: 0, stdout: ' {"a": 1}\n', stderr: '' }; } } });
  assert.deepEqual(await engineReply(off, ['get-ask', note]), { status: 0, stdout: ' {"a": 1}\n', stderr: '' });
  assert.deepEqual(seen, [['get-ask', note]]);
});

test('세션 읽기 명령: 자리 인자 여럿(session-titles)을 배열로 바꾸고, 알리면 Rust로 가며 옛 응답 모양을 돌려준다', async () => {
  const ids = ['aaaaaaaa-1111-2222-3333-444444444444', 'bbbbbbbb-1111-2222-3333-444444444444'];
  const argvs = [['read-session', ids[0], 'limit=100', 'urls=true'], ['session-titles', ...ids], ['session-titles'], ['recent-sessions', 'days=7']];
  for (const argv of argvs) assert.deepEqual(toArgv(fromArgv(argv)), argv, argv.join(' '));
  assert.deepEqual(fromArgv(['session-titles', ...ids]), { command: 'session_titles', args: { ids } });
  assert.deepEqual(fromArgv(['read-session', ids[0], 'limit=100']), { command: 'read_session', args: { id: ids[0], limit: '100' } });
  const names = ['read_session', 'session_titles', 'recent_sessions'];
  const on = host({ commands: names, rust: { read_session: () => ({ turns: [] }), session_titles: () => ({ sessions: [] }), recent_sessions: () => ({ days: 7, dropped: 0, sessions: [] }) } });
  assert.deepEqual(await engineReply(on, ['session-titles', ...ids]), { status: 0, stdout: '{"sessions":[]}', stderr: '' });
  assert.deepEqual(on.calls[0].args.args, { ids });
  assert.equal((await engineReply(on, ['read-session', ids[0], 'limit=5'])).status, 0);
  assert.deepEqual(on.calls[1].args.args, { id: ids[0], limit: '5' });
  const off = host({ commands: [], java: { 'recent-sessions': () => ({ status: 0, stdout: '{"days":7}', stderr: '' }) } });
  assert.deepEqual(await engineReply(off, ['recent-sessions', 'days=7']), { status: 0, stdout: '{"days":7}', stderr: '' });
});
