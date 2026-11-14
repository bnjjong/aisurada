import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { parseRoute, taskAddress } from '../src/navigation/routes.ts';
import {
  BOARD_COLUMNS, NO_FILTER, SHELF, activityText, activityWho, addComment, checkedLine, createArgv, createTask, deleteTask, emptyTaskForm, explainError,
  filterChoices, filterTasks, formFromTask, getTask, githubUrl, groupByStatus, isActiveFilter, linkArgv, linkCandidates, linkTasks, listTaskTrash,
  listTasks, parseLabels, parseRefInput, repoChoices, restoreTask, taskFormChanged, taskMeta, timeLabel, toDetail, toTask, toggleCriterion,
  transitionTask, updateArgv, updateTask, validateTaskForm,
} from '../src/ui/tasksClient.ts';
import { PriorityMark, StatusPill, TasksPage, TypeMark } from '../src/ui/TasksPage.tsx';
import { TaskDetailPage } from '../src/ui/TaskDetailPage.tsx';
import { Rail } from '../src/ui/Rail.tsx';

const raw = (extra = {}) => ({
  key: 'TSK-14', id: 'tkmwd7nzi4-05049130', title: '프로젝트를 지울 때 자원 폴더도 정리하기', type: 'bug', priority: 'high', status: 'draft', repo: 'bnjjong/aisurada',
  labels: ['엔진'], criteriaDone: 0, criteriaTotal: 2, blockedBy: [], blocks: [], path: '.aisurada/tasks/TSK-14.md', ...extra,
});
const task = (extra = {}) => toTask(raw(extra));
const detailRaw = (extra = {}) => raw({
  body: '지우면 폴더가 남아요.', criteria: [{ text: '폴더가 남지 않는다', done: false }, { text: '되살리면 돌아온다', done: true, checkedAt: '2026-11-13T14:05:07+09:00', checkedBy: 'human' }],
  refs: [{ kind: 'note', target: 'ndmw37ezqe-6234ac9a', title: '계획' }], links: [{ type: 'blocked-by', key: 'TSK-12', title: '정렬', status: 'in-progress' }],
  readiness: [{ item: 'title', ok: true, label: '요약이 있어요' }], ready: true,
  transitions: [{ to: 'ready-for-develop', title: 'Ready For Develop', ok: true }, { to: 'cancelled', title: 'Cancelled', ok: false, reason: '막힘' }],
  activity: [{ at: '2026-11-13T14:00:00+09:00', kind: 'created', by: 'human' }], precondition: 'f1:abc', criteriaDone: 1, ...extra,
});
const detail = (extra = {}) => toDetail(detailRaw(extra));

test('routes: /tasks is a plain page and /task/<key> is the detail', () => {
  assert.deepEqual(parseRoute('/tasks'), { kind: 'tasks' });
  assert.deepEqual(parseRoute('/task/TSK-14'), { kind: 'task', key: 'TSK-14' });
  assert.equal(taskAddress('TSK-14'), '/task/TSK-14');
  assert.deepEqual(parseRoute(taskAddress('TSK-14')), { kind: 'task', key: 'TSK-14' });
  assert.equal(parseRoute('/task').kind, 'unknown');
  assert.equal(parseRoute('/tasks/TSK-1').kind, 'unknown');
});

test('toTask reads an engine row, fills defaults, and refuses a row with the wrong shape', () => {
  const t = task();
  assert.equal(t.key, 'TSK-14');
  assert.equal(t.status, 'draft');
  assert.deepEqual(t.labels, ['엔진']);
  assert.equal(task({ type: 'epic' }).type, 'feature', 'unknown type falls back');
  assert.equal(task({ priority: 'urgent' }).priority, 'medium', 'unknown priority falls back');
  assert.deepEqual(task({ labels: undefined }).labels, []);
  for (const bad of [null, 'x', [], raw({ key: undefined }), raw({ id: undefined }), raw({ title: undefined }), raw({ status: 'sleeping' })]) {
    assert.equal(toTask(bad), null);
  }
});

test('toDetail reads criteria, refs, links, readiness, transitions and activity, and drops malformed entries', () => {
  const d = detail();
  assert.equal(d.body, '지우면 폴더가 남아요.');
  assert.equal(d.criteria.length, 2);
  assert.equal(d.criteria[1].done, true);
  assert.equal(d.refs[0].kind, 'note');
  assert.equal(d.links[0].type, 'blocked-by');
  assert.equal(d.ready, true);
  assert.equal(d.transitions[1].reason, '막힘');
  assert.equal(d.precondition, 'f1:abc');
  const dirty = detail({ criteria: [{ text: '' }, 5, { text: 'ok' }], refs: [{ kind: 'url', target: 'x' }], links: [{ type: 'likes', key: 'TSK-1', status: 'draft' }], transitions: [{ to: 'nope' }], activity: [{ kind: 'x' }] });
  assert.equal(dirty.criteria.length, 1);
  assert.equal(dirty.refs.length, 0);
  assert.equal(dirty.links.length, 0);
  assert.equal(dirty.transitions.length, 0);
  assert.equal(dirty.activity.length, 0);
  assert.equal(toDetail(null), null);
});

test('createArgv sends the required fields and only the optional ones that are filled', () => {
  const form = { ...emptyTaskForm(), title: ' 제목 ', repo: 'a/b', criteria: [{ text: ' 첫째 ', done: false }, { text: '', done: false }, { text: '둘째', done: false }] };
  assert.deepEqual(createArgv(form), ['create-task', 'title=제목', 'repo=a/b', 'criteria=["첫째","둘째"]', 'type=feature', 'priority=medium']);
  const full = { ...form, project: '앱', base: 'develop', body: '설명', labelsText: '엔진, 휴지통, 엔진', refs: [{ kind: 'note', target: 'ndmw37ezqe-6234ac9a', title: '계획' }, { kind: 'github', target: 'a/b#5' }], type: 'bug', priority: 'high' };
  const argv = createArgv(full);
  assert.deepEqual(argv.slice(6), ['project=앱', 'base=develop', 'body=설명', 'labels=["엔진","휴지통"]', 'refs=[{"kind":"note","target":"ndmw37ezqe-6234ac9a","title":"계획"},{"kind":"github","target":"a/b#5"}]']);
  assert.ok(argv.includes('type=bug') && argv.includes('priority=high'));
});

test('updateArgv sends only changed fields, always with the read token, and null when nothing changed', () => {
  const d = detail();
  const same = formFromTask(d);
  assert.equal(updateArgv(d, same), null, 'no change, no call');
  assert.deepEqual(updateArgv(d, { ...same, title: '새 제목', priority: 'low' }), ['update-task', 'TSK-14', 'f1=f1:abc', 'title=새 제목', 'priority=low']);
  assert.deepEqual(updateArgv(d, { ...same, labelsText: '' }), ['update-task', 'TSK-14', 'f1=f1:abc', 'labels=[]'], 'clearing labels sends an empty list');
  assert.deepEqual(updateArgv(d, { ...same, base: 'main' }), ['update-task', 'TSK-14', 'f1=f1:abc', 'base=main']);
  const withCrit = updateArgv(d, { ...same, criteria: [{ text: '폴더가 남지 않는다', done: true }, { text: '되살리면 돌아온다', done: true }] });
  assert.ok(withCrit.some((a) => a === 'criteria=[{"text":"폴더가 남지 않는다","done":true},{"text":"되살리면 돌아온다","done":true}]'), 'criteria go with their done flags');
  assert.deepEqual(updateArgv(d, { ...same, refs: [] }), ['update-task', 'TSK-14', 'f1=f1:abc', 'refs=[]']);
});

test('linkArgv speaks about this task and flips the owner for the reversed words', () => {
  assert.deepEqual(linkArgv('TSK-14', 'TSK-18', 'blocks'), ['link-tasks', 'TSK-14', 'TSK-18', 'type=blocks']);
  assert.deepEqual(linkArgv('TSK-18', 'TSK-14', 'blocked-by'), ['link-tasks', 'TSK-14', 'TSK-18', 'type=blocks'], 'blocked-by is stored on the blocker');
  assert.deepEqual(linkArgv('TSK-1', 'TSK-2', 'relates'), ['link-tasks', 'TSK-1', 'TSK-2', 'type=relates']);
  assert.deepEqual(linkArgv('TSK-1', 'TSK-2', 'duplicates'), ['link-tasks', 'TSK-1', 'TSK-2', 'type=duplicates']);
  assert.deepEqual(linkArgv('TSK-2', 'TSK-1', 'duplicated-by', true), ['link-tasks', 'TSK-1', 'TSK-2', 'type=duplicates', 'remove=true']);
});

test('engine calls: argv, token, from and parsed results', async () => {
  const calls = [];
  const engine = (answers) => async (argv) => {
    calls.push(argv);
    const a = answers[argv[0]];
    if (a instanceof Error) throw a;
    return typeof a === 'function' ? a(argv) : a;
  };
  const call = engine({
    'list-tasks': (argv) => (argv.includes('trash=true') ? { trash: [{ key: 'TSK-2', id: 'tkx-00000000', title: '지움', deletedAt: 'now' }, { bad: 1 }] } : { tasks: [raw(), { bad: 1 }], skipped: ['x.md: 못 읽음'] }),
    'get-task': detailRaw(),
    'create-task': detailRaw({ key: 'TSK-15' }),
    'update-task': detailRaw({ title: '바뀜' }),
    'transition-task': detailRaw({ status: 'ready-for-develop' }),
    'link-tasks': {}, 'add-task-comment': {}, 'delete-task': {}, 'restore-task': {},
  });
  const listed = await listTasks(call);
  assert.equal(listed.tasks.length, 1, 'a malformed row is dropped');
  assert.deepEqual(listed.skipped, ['x.md: 못 읽음']);
  assert.deepEqual((await listTaskTrash(call)).map((t) => t.key), ['TSK-2']);
  assert.equal((await getTask(call, 'TSK-14')).key, 'TSK-14');
  assert.deepEqual(calls.at(-1), ['get-task', 'TSK-14', 'activity=100']);
  const created = await createTask(call, { ...emptyTaskForm(), title: 't', repo: 'a/b', criteria: [{ text: 'c', done: false }] });
  assert.equal(created.key, 'TSK-15');
  const d = detail();
  assert.equal((await updateTask(call, d, { ...formFromTask(d), title: '바뀜' })).title, '바뀜');
  const callsBefore = calls.length;
  assert.equal((await updateTask(call, d, formFromTask(d))), d, 'nothing changed returns the same task without a call');
  assert.equal(calls.length, callsBefore);
  await toggleCriterion(call, d, 0);
  assert.deepEqual(calls.at(-1), ['update-task', 'TSK-14', 'f1=f1:abc', 'criteria=[{"text":"폴더가 남지 않는다","done":true},{"text":"되살리면 돌아온다","done":true}]']);
  await toggleCriterion(call, d, 1);
  assert.ok(calls.at(-1).at(-1).includes('"text":"되살리면 돌아온다","done":false'), 'a checked item is unchecked');
  const moved = await transitionTask(call, { key: 'TSK-14', status: 'draft' }, 'ready-for-develop');
  assert.equal(moved.status, 'ready-for-develop');
  assert.deepEqual(calls.at(-1), ['transition-task', 'TSK-14', 'to=ready-for-develop', 'from=draft'], 'from guards against a state that moved');
  await linkTasks(call, 'TSK-14', 'TSK-18', 'blocks');
  await addComment(call, 'TSK-14', ' 안녕 ');
  assert.deepEqual(calls.at(-1), ['add-task-comment', 'TSK-14', 'body=안녕']);
  await deleteTask(call, 'TSK-14');
  await restoreTask(call, 'TSK-14');
  assert.deepEqual(calls.slice(-2), [['delete-task', 'TSK-14'], ['restore-task', 'TSK-14']]);
  await assert.rejects(() => getTask(engine({ 'get-task': { nope: 1 } }), 'TSK-1'), /모양이 틀려요/);
});

test('validateTaskForm blocks what the engine would refuse, and requires at least one completion criterion', () => {
  const ok = { ...emptyTaskForm(), title: '제목', repo: 'a/b', criteria: [{ text: '기준', done: false }] };
  assert.equal(validateTaskForm(ok), null);
  assert.match(validateTaskForm({ ...ok, title: '  ' }), /요약/);
  assert.match(validateTaskForm({ ...ok, repo: '' }), /저장소/);
  assert.match(validateTaskForm({ ...ok, repo: 'abc' }), /owner\/name/);
  assert.match(validateTaskForm({ ...ok, criteria: [] }), /완료 기준/);
  assert.match(validateTaskForm({ ...ok, criteria: [{ text: '  ', done: false }] }), /완료 기준/, 'blank rows do not count');
  assert.match(validateTaskForm({ ...ok, base: '-bad' }), /브랜치/);
  assert.match(validateTaskForm({ ...ok, title: 'a\u0001b' }), /제어 문자/);
  assert.match(validateTaskForm({ ...ok, title: 'x'.repeat(201) }), /200자/);
  assert.match(validateTaskForm({ ...ok, labelsText: Array.from({ length: 21 }, (_, i) => `l${i}`).join(',') }), /20개/);
});

test('taskFormChanged ignores blank criterion rows, and parseLabels trims and de-duplicates', () => {
  const fresh = { ...emptyTaskForm(), criteria: [{ text: '', done: false }] };
  assert.equal(taskFormChanged(fresh, undefined), false, 'a fresh form with an empty row is not a change');
  assert.equal(taskFormChanged({ ...fresh, title: 'x' }, undefined), true);
  const d = detail();
  assert.equal(taskFormChanged(formFromTask(d), d), false);
  assert.equal(taskFormChanged({ ...formFromTask(d), body: '다름' }, d), true);
  assert.deepEqual(parseLabels(' 엔진, 휴지통 ,,엔진\n화면 '), ['엔진', '휴지통', '화면']);
});

test('parseRefInput and githubUrl read GitHub issues, PRs and note ids', () => {
  assert.deepEqual(parseRefInput('bnjjong/aisurada#53'), { kind: 'github', target: 'bnjjong/aisurada#53' });
  assert.deepEqual(parseRefInput(' https://github.com/bnjjong/aisurada/pull/81 '), { kind: 'github', target: 'https://github.com/bnjjong/aisurada/pull/81' });
  assert.deepEqual(parseRefInput('ndmw37ezqe-6234ac9a'), { kind: 'note', target: 'ndmw37ezqe-6234ac9a' });
  for (const bad of ['', 'abc', 'http://github.com/a/b/issues/1', 'a/b#0', 'https://example.com/a/b/issues/1']) assert.equal(parseRefInput(bad), null);
  assert.equal(githubUrl('bnjjong/aisurada#53'), 'https://github.com/bnjjong/aisurada/issues/53');
  assert.equal(githubUrl('https://github.com/a/b/pull/2'), 'https://github.com/a/b/pull/2');
});

test('filter, group and choices: newest first, board columns, shelf, repo and label choices', () => {
  const rows = [
    task({ key: 'TSK-1', title: '검색 따옴표', status: 'done', type: 'bug', labels: ['검색'], repo: 'a/b' }),
    task({ key: 'TSK-3', title: '정렬 기억', status: 'draft', type: 'feature', labels: ['화면'], repo: 'a/c' }),
    task({ key: 'TSK-2', title: '줄 번호', status: 'ready-for-develop', type: 'feature', blockedBy: ['TSK-3'], labels: [] }),
  ];
  assert.deepEqual(filterTasks(rows, NO_FILTER).map((t) => t.key), ['TSK-3', 'TSK-2', 'TSK-1'], 'newest first');
  assert.deepEqual(filterTasks(rows, { ...NO_FILTER, query: ' tsk-2 ' }).map((t) => t.key), ['TSK-2'], 'by key, any case');
  assert.deepEqual(filterTasks(rows, { ...NO_FILTER, query: '검색' }).map((t) => t.key), ['TSK-1']);
  assert.deepEqual(filterTasks(rows, { ...NO_FILTER, repo: 'a/c' }).map((t) => t.key), ['TSK-3']);
  assert.deepEqual(filterTasks(rows, { ...NO_FILTER, type: 'bug' }).map((t) => t.key), ['TSK-1']);
  assert.deepEqual(filterTasks(rows, { ...NO_FILTER, label: '화면' }).map((t) => t.key), ['TSK-3']);
  assert.deepEqual(filterTasks(rows, { ...NO_FILTER, blockedOnly: true }).map((t) => t.key), ['TSK-2']);
  assert.equal(isActiveFilter(NO_FILTER), false);
  assert.equal(isActiveFilter({ ...NO_FILTER, query: ' ' }), false, 'blank query is not a filter');
  assert.equal(isActiveFilter({ ...NO_FILTER, blockedOnly: true }), true);
  const g = groupByStatus(rows);
  assert.equal(g.draft.length, 1);
  assert.equal(g.done.length, 1);
  assert.equal(g['in-review'].length, 0);
  assert.deepEqual(BOARD_COLUMNS, ['draft', 'ready-for-develop', 'in-progress', 'waiting-answer', 'in-review']);
  assert.deepEqual(SHELF, ['done', 'failed', 'cancelled']);
  assert.deepEqual(filterChoices(rows), { repos: ['a/b', 'a/c', 'bnjjong/aisurada'], labels: ['검색', '화면'] });
});

test('linkCandidates leaves out the task itself and the ones already linked', () => {
  const d = detail();
  const rows = [task({ key: 'TSK-14' }), task({ key: 'TSK-12' }), task({ key: 'TSK-18' }), task({ key: 'TSK-20' })];
  assert.deepEqual(linkCandidates(rows, d).map((t) => t.key), ['TSK-20', 'TSK-18']);
});

test('repoChoices lists GitHub repositories of the projects, once each', () => {
  const projects = [
    { slug: '앱', title: '앱', repos: [{ name: 'app', github: 'bnjjong/aisurada' }, { name: 'local', path: '/Users/me/x' }] },
    { slug: '문서', title: '문서', repos: [{ name: 'dup', github: 'bnjjong/aisurada' }, { name: 'docs', github: 'me/docs' }] },
    { slug: '빈', title: '빈' },
  ];
  assert.deepEqual(repoChoices(projects), [
    { repo: 'bnjjong/aisurada', project: '앱', label: 'bnjjong/aisurada · 앱' },
    { repo: 'me/docs', project: '문서', label: 'me/docs · 문서' },
  ]);
});

test('texts: activity lines, checked line, time label, meta and error explanations', () => {
  assert.equal(activityText({ at: 'x', kind: 'created' }), '태스크를 만들었어요');
  assert.equal(activityText({ at: 'x', kind: 'status', from: 'draft', to: 'ready-for-develop' }), '상태를 Draft에서 Ready For Develop(으)로 바꿨어요');
  assert.equal(activityText({ at: 'x', kind: 'edited', fields: ['criteria', 'baseBranch'] }), '완료 기준, 기준 브랜치을(를) 고쳤어요');
  assert.equal(activityText({ at: 'x', kind: 'linked', type: 'blocks', with: 'TSK-3', direction: 'in' }), 'TSK-3와(과)의 연결(막혀 있음)을 걸었어요');
  assert.equal(activityText({ at: 'x', kind: 'unlinked', type: 'duplicates', with: 'TSK-3', direction: 'out' }), 'TSK-3와(과)의 연결(중복)을 풀었어요');
  assert.equal(activityText({ at: 'x', kind: 'comment' }), '댓글을 남겼어요');
  assert.equal(activityWho({ at: 'x', kind: 'created', by: 'human' }), '사람');
  assert.equal(activityWho({ at: 'x', kind: 'created', by: 'runner' }), '러너');
  assert.equal(activityWho({ at: 'x', kind: 'created', by: 'human', session: 's1', agent: 'codex' }), 'AI(Codex)');
  assert.equal(checkedLine({ text: 'a', done: false }), '');
  assert.equal(checkedLine({ text: 'a', done: true, checkedBy: 'human', checkedAt: '2026-11-13T14:05:07+09:00' }), '사람가 확인 · 11월 13일 14:05');
  assert.equal(timeLabel('2026-11-03T09:05:00+09:00'), '11월 3일 09:05');
  assert.equal(timeLabel('이상한 글'), '이상한 글');
  assert.equal(taskMeta(task({ criteriaDone: 1, criteriaTotal: 3, blockedBy: ['TSK-2', 'TSK-5'] })), '완료 기준 1/3 · TSK-2, TSK-5에 막혀 있어요');
  assert.equal(taskMeta(task({ criteriaTotal: 0 })), '');
  assert.match(explainError({ stderr: 'AIS-409 상태가 이미 바뀌었다: 지금 draft' }), /먼저 상태를 바꿨어요/);
  assert.match(explainError({ stderr: 'AIS-412 읽은 뒤에 파일이 바뀌었다' }), /먼저 고쳤어요/);
  assert.equal(explainError({ stderr: 'AIS-400 완료 기준(criteria)을 하나 이상 써야 한다' }), '완료 기준(criteria)을 하나 이상 써야 한다');
});

test('marks render with accessible names and a class per status, type and priority', () => {
  const html = renderToStaticMarkup(React.createElement('div', null,
    React.createElement(StatusPill, { status: 'waiting-answer' }),
    React.createElement(StatusPill, { status: 'done', label: 'Done 12' }),
    React.createElement(TypeMark, { type: 'bug' }),
    React.createElement(PriorityMark, { priority: 'high' })));
  assert.match(html, /task-pill task-pill-waiting-answer/);
  assert.match(html, /Waiting Answer/);
  assert.match(html, /Done 12/);
  assert.match(html, /aria-label="버그"/);
  assert.match(html, /aria-label="우선순위 높음"/);
});

const fakeController = () => ({
  navigate() {}, form() {}, filterAgendas() {}, preference() {}, settings() {}, launcher() {},
  callEngine: async () => ({}), isNative: () => true,
});
const workspace = (extra = {}) => ({
  path: '/vault', name: 'v', sample: false, today: '2026-11-13', notes: [], templates: [], trash: [], skipped: [], conflicts: [], noteReadFailed: false,
  agendas: [], projects: [{ id: 'pj1', slug: '앱', title: '앱', hue: 'blue', summary: '', repos: [{ name: 'app', github: 'bnjjong/aisurada', githubId: 1 }] }], ...extra,
});
const state = (route) => ({ workspace: workspace(), prefs: { collapsedProjects: [] }, hiddenAgendas: [], theme: 'dark', route });

test('the pages draw their loading state before the engine answers, and the rail has a tasks item', () => {
  const board = renderToStaticMarkup(React.createElement(TasksPage, { state: state('/tasks'), controller: fakeController() }));
  assert.match(board, /<h1>태스크<\/h1>/);
  assert.match(board, /불러오는 중이에요/);
  assert.match(board, /aria-label="보기"/);
  assert.match(board, /\+ 만들기/);
  assert.match(board, /aria-label="태스크 검색"/);
  assert.match(board, /막힌 것만/);
  const detailHtml = renderToStaticMarkup(React.createElement(TaskDetailPage, { taskKey: 'TSK-1', state: state('/task/TSK-1'), controller: fakeController() }));
  assert.match(detailHtml, /불러오는 중이에요/);
  const rail = renderToStaticMarkup(React.createElement(Rail, { controller: fakeController(), state: state('/calendar') }));
  assert.doesNotMatch(rail, />태스크</, 'the notes tab has no tasks item');
  assert.match(rail, /role="tab"[^>]*>에이전트/, 'the agents tab is one click away');
  assert.match(renderToStaticMarkup(React.createElement(Rail, { controller: fakeController(), state: state('/resources') })), />태스크</);
  const onList = renderToStaticMarkup(React.createElement(Rail, { controller: fakeController(), state: state('/tasks') }));
  assert.match(onList, /class="nav-item active">태스크/);
  const onDetail = renderToStaticMarkup(React.createElement(Rail, { controller: fakeController(), state: state('/task/TSK-9') }));
  assert.match(onDetail, /class="nav-item active">태스크/, 'the tasks item stays active on a task detail');
  assert.doesNotMatch(onDetail, /class="nav-item active">스킬/);
});

test('the board with data: five columns with counts, blocked note, shelf pills, and the list view table', () => {
  const rows = [
    task({ key: 'TSK-7', title: '개수 보여 주기', status: 'draft', blockedBy: ['TSK-1'], labels: ['휴지통'], criteriaDone: 0, criteriaTotal: 1 }),
    task({ key: 'TSK-1', title: '자원 폴더 정리', status: 'waiting-answer', priority: 'high', criteriaDone: 1, criteriaTotal: 3 }),
    task({ key: 'TSK-8', title: '버전 올리기', status: 'done', type: 'chore' }),
  ];
  const board = renderToStaticMarkup(React.createElement(TasksPage, { state: state('/tasks'), controller: fakeController(), preset: { tasks: rows, view: 'board', shelfOpen: true } }));
  for (const name of ['Draft', 'Ready For Develop', 'In Progress', 'Waiting Answer', 'In Review']) assert.match(board, new RegExp(`aria-label="${name}"`));
  assert.match(board, /data-task-key="TSK-7"/);
  assert.match(board, /TSK-1에 막혀 있어요/);
  assert.match(board, /요약을 쓰고 Enter/, 'the one-line input sits in the Draft column');
  assert.match(board, /Done 1/);
  assert.match(board, /Failed 0/);
  assert.match(board, /<table class="task-table">/, 'the shelf opens as a table of the finished ones');
  assert.match(board, /task-card is-blocked/);
  assert.match(board, /title="완료 기준 1\/3"/);
  const list = renderToStaticMarkup(React.createElement(TasksPage, { state: state('/tasks'), controller: fakeController(), preset: { tasks: rows, view: 'list' } }));
  assert.match(list, /<th scope="col">요약<\/th>/);
  assert.equal((list.match(/data-task-key=/g) ?? []).length, 3, 'the list has every task, finished ones too');
  assert.doesNotMatch(list, /task-board/);
  const empty = renderToStaticMarkup(React.createElement(TasksPage, { state: state('/tasks'), controller: fakeController(), preset: { tasks: [], view: 'board' } }));
  assert.match(empty, /아직 태스크가 없어요/);
});

test('the detail with data: sections, criteria checkboxes, details, readiness and an open status menu with disabled reasons', () => {
  const d = detail({ body: '' });
  const html = renderToStaticMarkup(React.createElement(TaskDetailPage, { taskKey: 'TSK-14', state: state('/task/TSK-14'), controller: fakeController(), preset: { task: d, menuOpen: true } }));
  assert.match(html, /<h1 class="task-title">프로젝트를 지울 때 자원 폴더도 정리하기<\/h1>/);
  for (const section of ['설명', '완료 기준', '연결된 태스크', '연결한 자원', '활동', '세부 정보']) assert.match(html, new RegExp(`aria-label="${section}"`));
  assert.equal((html.match(/type="checkbox"/g) ?? []).length, 2, 'one checkbox per criterion');
  assert.match(html, /checked=""/, 'the checked criterion is checked');
  assert.match(html, /사람가 확인 · 11월 13일 14:05/);
  assert.match(html, /막혀 있음/);
  assert.match(html, /TSK-12/);
  assert.match(html, /role="menu" aria-label="상태 바꾸기"/);
  assert.match(html, /disabled=""[^>]*><span class="task-pill task-pill-cancelled">/, 'a blocked transition is disabled');
  assert.match(html, />막힘</, 'and shows why');
  assert.match(html, /Ready For Develop 조건/);
  assert.match(html, /class="is-ok"/);
  const closed = renderToStaticMarkup(React.createElement(TaskDetailPage, { taskKey: 'TSK-14', state: state('/task/TSK-14'), controller: fakeController(), preset: { task: d } }));
  assert.doesNotMatch(closed, /role="menu"/, 'the menu is closed until it is opened');
  const comments = renderToStaticMarkup(React.createElement(TaskDetailPage, { taskKey: 'TSK-14', state: state('/task/TSK-14'), controller: fakeController(), preset: { task: d, tab: 'comments' } }));
  assert.match(comments, /아직 없어요/, 'the comments tab has no comments yet');
});
