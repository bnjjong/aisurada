import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { parseRoute, projectAddress } from '../src/navigation/routes.ts';
import {
  emptyForm, editedLine, explainError, formChanged, formFrom, formToInput, getResource, kindLabel, listResources, listTrash,
  permissionLabel, saveArgv, saveResource, scopeLabel, selectableSkills, setReference, skillsToReference, toResource, unreferencedGlobals,
  usableResources, validateForm,
} from '../src/ui/resourcesClient.ts';
import { ResourceItem, resourceMeta } from '../src/ui/ResourceItem.tsx';
import { ResourcesPage } from '../src/ui/ResourcesPage.tsx';
import { ProjectPage } from '../src/ui/ProjectPage.tsx';
import { Rail } from '../src/ui/Rail.tsx';

const raw = (extra = {}) => ({ id: 'skmwd7nzi4-05049130', kind: 'skill', scope: 'global', name: '번역 검수', description: '번역을 검수한다', path: '.aisurada/resources/global/skills/번역-검수/SKILL.md', ...extra });
const res = (extra = {}) => toResource(raw(extra));
const agent = (extra = {}) => res({ id: 'apmwd7nzn1-5efcfe47', kind: 'agent', name: '코드 리뷰어', permission: 'edit', skills: ['skmwd7nzi4-05049130'], path: '.aisurada/resources/global/agents/코드-리뷰어.md', ...extra });
const usable = (extra = {}) => ({ project: '앱', skills: [], agents: [], broken: [], missingSkills: [], ...extra });

test('toResource reads an engine row, fills defaults, and refuses a row with the wrong shape', () => {
  const r = res({ tools: { claude: { a: 1 } }, referencedBy: ['앱'], editedSession: 's1' });
  assert.equal(r.kind, 'skill');
  assert.deepEqual(r.skills, []);
  assert.deepEqual(r.tools, { claude: { a: 1 } });
  assert.deepEqual(r.referencedBy, ['앱']);
  assert.equal(res().referencedBy, undefined);
  assert.equal(agent().permission, 'edit');
  assert.equal(agent({ permission: 'root' }).permission, undefined, 'unknown permission is not trusted');
  for (const bad of [null, 'x', [], raw({ id: undefined }), raw({ kind: 'tool' }), raw({ scope: 'local' }), raw({ name: undefined })]) {
    assert.equal(toResource(bad), null);
  }
});

test('saveArgv sends only the fields that are present, and clears with empty values', () => {
  assert.deepEqual(saveArgv({ kind: 'skill', scope: 'global', name: '이름', description: '설명', body: '' }), ['save-resource', 'kind=skill', 'scope=global', 'name=이름', 'description=설명', 'body=']);
  assert.deepEqual(saveArgv({ id: 'ap1', f1: 'f1:abc', skills: ['sk1', 'sk2'], repo: '', tools: '{}' }), ['save-resource', 'id=ap1', 'skills=["sk1","sk2"]', 'repo=', 'tools={}', 'f1=f1:abc']);
  assert.deepEqual(saveArgv({ skills: [] }), ['save-resource', 'skills=[]']);
});

test('formToInput for a new resource: global has no repo, a project agent keeps skills, repo, permission and tools', () => {
  const skill = { ...emptyForm('skill'), name: ' 번역 검수 ', description: ' 설명 ', body: '# 지침' };
  assert.deepEqual(formToInput(skill, {}), { kind: 'skill', scope: 'global', name: '번역 검수', description: '설명', body: '# 지침' });
  const form = { ...emptyForm('agent'), name: '웹 담당', description: '웹', permission: 'publish', skills: ['sk1'], repo: 'web', toolsText: '{"claude":{}}' };
  assert.deepEqual(formToInput(form, { project: '앱' }), {
    kind: 'agent', project: '앱', name: '웹 담당', description: '웹', body: '', permission: 'publish', skills: ['sk1'], repo: 'web', tools: '{"claude":{}}',
  });
  const global = formToInput(form, {});
  assert.equal(global.scope, 'global');
  assert.equal(global.repo, undefined, 'a global agent has no repo');
  assert.equal(global.project, undefined);
});

test('formToInput for an edit sends only what changed, and leaves unchanged tool cells untouched so unknown tools survive', () => {
  const existing = agent({ body: '본문', precondition: 'f1:old', project: '앱', scope: 'project', repo: 'web', tools: { gemini: { flag: true } } });
  const same = formToInput(formFrom(existing), { project: '앱', existing });
  assert.deepEqual(same, { id: existing.id, f1: 'f1:old' }, 'nothing changed means only id and f1');
  const changed = formToInput({ ...formFrom(existing), name: '새 이름', skills: [], permission: 'read', repo: '' }, { project: '앱', existing });
  assert.deepEqual(changed, { id: existing.id, f1: 'f1:old', name: '새 이름', permission: 'read', skills: [], repo: '' });
  const toolsEdited = formToInput({ ...formFrom(existing), toolsText: '{"codex":{}}' }, { project: '앱', existing });
  assert.equal(toolsEdited.tools, '{"codex":{}}');
  const toolsCleared = formToInput({ ...formFrom(existing), toolsText: '' }, { project: '앱', existing });
  assert.equal(toolsCleared.tools, '{}', 'clearing the box clears the tool cells');
  const globalEdit = agent({ body: '', precondition: 'f1:g', repo: undefined });
  assert.equal(formToInput({ ...formFrom(globalEdit), repo: 'web' }, { existing: globalEdit }).repo, undefined, 'a global agent never sends repo');
});

test('validateForm tells the user what the engine would refuse, before saving', () => {
  const ok = { ...emptyForm('skill'), name: '번역', description: '설명' };
  assert.equal(validateForm(ok), null);
  assert.match(validateForm({ ...ok, name: '  ' }), /이름이 필요/);
  assert.match(validateForm({ ...ok, name: 'x'.repeat(61) }), /60자/);
  assert.match(validateForm({ ...ok, name: 'a/b' }), /\/ 와/);
  assert.match(validateForm({ ...ok, description: ' ' }), /설명이 필요/);
  assert.match(validateForm({ ...ok, description: 'd'.repeat(1001) }), /1000자/);
  assert.match(validateForm({ ...ok, body: 'b'.repeat(100_001) }), /지침은/);
  assert.match(validateForm({ ...ok, toolsText: '{' }), /JSON이 아니에요/);
  assert.match(validateForm({ ...ok, toolsText: '[1]' }), /JSON 객체/);
  assert.match(validateForm({ ...ok, toolsText: '{"Claude":{}}' }), /소문자 도구 이름/);
  assert.equal(validateForm({ ...ok, toolsText: '{"claude":{"allowed-tools":["Read"]},"codex":{}}' }), null);
});

test('formChanged compares with the starting values, for asking before throwing away typed text', () => {
  const e = res({ body: '본문' });
  assert.equal(formChanged(formFrom(e), e, 'skill'), false);
  assert.equal(formChanged({ ...formFrom(e), body: '다름' }, e, 'skill'), true);
  assert.equal(formChanged(emptyForm('agent'), undefined, 'agent'), false);
  assert.equal(formChanged({ ...emptyForm('agent'), skills: ['sk1'] }, undefined, 'agent'), true);
});

test('labels: a scope is put in front of the name so equal names stay apart, and who edited last is told', () => {
  assert.equal(scopeLabel({ scope: 'global', name: '번역' }), '글로벌 · 번역');
  assert.equal(scopeLabel({ scope: 'project', name: '번역' }), '프로젝트 · 번역');
  assert.equal(kindLabel('skill'), '스킬');
  assert.equal(kindLabel('agent'), '에이전트');
  assert.equal(permissionLabel('publish'), '내보내기');
  assert.equal(permissionLabel(undefined), '읽기만');
  assert.equal(editedLine({}), '');
  assert.equal(editedLine({ editedAt: '2026-11-12T10:00:00+09:00' }), '사람이 고쳤어요');
  assert.equal(editedLine({ editedAt: 'x', editedSession: 'abcdefghijkl' }), 'AI가 고쳤어요 · Claude Code · 세션 abcdefgh');
  assert.equal(editedLine({ editedAt: 'x', editedSession: 's1', editedAgent: 'codex' }), 'AI가 고쳤어요 · Codex · 세션 s1');
  assert.match(resourceMeta(agent({ repo: 'web' })), /권한 수정 · 스킬 1개 · 저장소 web/);
});

test('reference helpers: what is still unreferenced, which skills to ask about, and which skills an agent form may pick', () => {
  const skillA = res({ id: 'skmwa-00000001', name: 'A' });
  const skillB = res({ id: 'skmwb-00000002', name: 'B' });
  const agentX = agent({ skills: [skillA.id, skillB.id, 'skmwgone-00000003'] });
  const globals = [skillA, skillB, agentX];
  const u = usable({ skills: [{ ...skillA, source: 'ref' }], agents: [] });
  assert.deepEqual(unreferencedGlobals(globals, u).map((r) => r.name), ['B', '코드 리뷰어'], 'A is referenced, the rest are not');
  assert.deepEqual(skillsToReference(agentX, globals, u).map((r) => r.name), ['B'], 'A is usable already and the gone skill cannot be referenced');
  const own = res({ id: 'skmwp-00000004', name: '배포', scope: 'project', project: '앱' });
  assert.deepEqual(selectableSkills({ globals: [skillA, agentX], projectSkills: [] }).map((r) => r.name), ['A'], 'a global agent picks global skills only');
  assert.deepEqual(selectableSkills({ project: '앱', globals: [skillA], projectSkills: [own, res({ id: 'skmwo-00000005', name: '남의', scope: 'project', project: '문서' })] }).map((r) => r.name), ['배포', 'A'], 'a project agent also picks its own project skills');
});

test('explainError turns engine refusals into plain words', () => {
  assert.match(explainError({ stderr: 'AIS-412 읽은 뒤에 파일이 바뀌었다' }), /먼저 고쳤어요/);
  assert.equal(explainError({ stderr: 'AIS-400 같은 이름의 스킬이 이 범위에 이미 있다: 번역' }), '같은 이름의 스킬이 이 범위에 이미 있다: 번역');
  assert.equal(explainError(new Error('boom')), 'boom');
  assert.equal(explainError({ stderr: '' }), '알 수 없는 오류가 났어요.');
});

test('the engine calls use the right commands and arguments', async () => {
  const seen = [];
  const answers = {
    'list-resources': { resources: [raw()], skipped: ['x.md: 머리말이 없다'] },
    'get-resource': raw({ body: '본문', precondition: 'f1:z', referencedBy: ['앱'] }),
    'usable-resources': { project: '앱', skills: [raw({ source: 'ref' })], agents: [], broken: [{ id: 'sk1', hint: '지운 것' }, { hint: 'no id' }], missingSkills: [{ agent: { id: 'ap1', name: '에이전트' }, skillId: 'sk2', skill: { name: '스킬', scope: 'global' }, canReference: true }, { skillId: 'bad' }] },
    'set-resource-ref': { project: '앱' },
    'save-resource': raw({ precondition: 'f1:new' }),
  };
  const call = async (argv) => {
    seen.push(argv);
    if (argv[0] === 'list-resources' && argv.includes('trash=true')) return { trash: [{ id: 'sk9', kind: 'skill', scope: 'project', project: '앱', name: '지운 것', deletedAt: 't' }, { id: 'x' }] };
    return answers[argv[0]];
  };
  const l = await listResources(call, { global: true, kind: 'skill' });
  assert.deepEqual(seen.at(-1), ['list-resources', 'scope=global', 'kind=skill']);
  assert.equal(l.resources.length, 1);
  assert.deepEqual(l.skipped, ['x.md: 머리말이 없다']);
  await listResources(call, { project: '앱' });
  assert.deepEqual(seen.at(-1), ['list-resources', 'project=앱']);
  assert.equal((await getResource(call, 'sk1')).precondition, 'f1:z');
  const u = await usableResources(call, '앱');
  assert.deepEqual(seen.at(-1), ['usable-resources', '앱']);
  assert.equal(u.skills[0].source, 'ref');
  assert.deepEqual(u.broken, [{ id: 'sk1', hint: '지운 것' }], 'a broken row without an id is dropped');
  assert.equal(u.missingSkills.length, 1);
  assert.equal(u.missingSkills[0].canReference, true);
  const t = await listTrash(call);
  assert.deepEqual(seen.at(-1), ['list-resources', 'trash=true']);
  assert.equal(t.length, 1, 'a trash row without the needed fields is dropped');
  await setReference(call, '앱', 'ap1');
  assert.deepEqual(seen.at(-1), ['set-resource-ref', '앱', 'ap1']);
  await setReference(call, '앱', 'ap1', { withSkills: true });
  assert.deepEqual(seen.at(-1), ['set-resource-ref', '앱', 'ap1', 'with-skills=true']);
  await setReference(call, '앱', 'ap1', { remove: true });
  assert.deepEqual(seen.at(-1), ['set-resource-ref', '앱', 'ap1', 'remove=true']);
  assert.equal((await saveResource(call, { id: 'sk1', f1: 'f1:z', name: 'n' })).precondition, 'f1:new');
  await assert.rejects(saveResource(async () => ({ nonsense: 1 }), { name: 'x' }), /모양이 틀려요/);
  await assert.rejects(getResource(async () => 5, 'sk1'), /모양이 틀려요/);
});

test('routes: /resources and /project/<slug> are screens of their own', () => {
  assert.deepEqual(parseRoute('/resources'), { kind: 'resources' });
  assert.deepEqual(parseRoute('/project/%EC%95%B1'), { kind: 'project', slug: '앱' });
  assert.equal(projectAddress('앱'), '/project/%EC%95%B1');
  assert.deepEqual(parseRoute(projectAddress('a b/c')), { kind: 'project', slug: 'a b/c' });
  assert.equal(parseRoute('/project').kind, 'unknown');
  assert.equal(parseRoute('/project/a/b').kind, 'unknown');
});

const fakeController = () => ({
  navigate() {}, form() {}, filterAgendas() {}, preference() {}, settings() {}, launcher() {},
  callEngine: async () => ({}), isNative: () => true,
});
const workspace = (extra = {}) => ({
  path: '/vault', name: 'v', sample: false, today: '2026-11-12', notes: [], templates: [], trash: [], skipped: [], conflicts: [], noteReadFailed: false,
  agendas: [{ id: 'ag1', slug: 'a1', title: '앱 아젠다', project: '앱' }, { id: 'ag2', slug: 'a2', title: '남의 아젠다', project: '문서' }],
  projects: [{ id: 'pj1', slug: '앱', title: '앱', hue: 'blue', summary: '앱 프로젝트', repos: [{ name: 'web', path: '/Users/me/web' }, { name: 'docs', github: 'me/docs', githubId: 1 }] }],
  ...extra,
});
const state = (route, extra = {}) => ({ workspace: workspace(extra), prefs: { collapsedProjects: [] }, hiddenAgendas: [], theme: 'dark', route });

test('ResourceItem draws the name, kind, badge, description and the actions it is given', () => {
  const html = renderToStaticMarkup(React.createElement(ResourceItem, { row: agent({ repo: 'web' }), badge: React.createElement('span', { className: 'res-badge' }, '글로벌에서 참조'), actions: React.createElement('button', null, '참조 끄기'), notes: React.createElement('p', null, '경고') }));
  assert.match(html, /data-resource-kind="agent"/);
  assert.match(html, /코드 리뷰어/);
  assert.match(html, /글로벌에서 참조/);
  assert.match(html, /에이전트/);
  assert.match(html, /권한 수정/);
  assert.match(html, /참조 끄기/);
  assert.match(html, /경고/);
  assert.doesNotMatch(renderToStaticMarkup(React.createElement(ResourceItem, { row: res() })), /res-actions/, 'no actions, no action box');
});

test('the global page opens with its explanation and the two create buttons', () => {
  const html = renderToStaticMarkup(React.createElement(ResourcesPage, { controller: fakeController() }));
  assert.match(html, /<h1>스킬·에이전트<\/h1>/);
  assert.match(html, /참조하지 않은 것은/);
  assert.match(html, />새 스킬</);
  assert.match(html, />새 에이전트</);
  assert.match(html, /새로고침/);
});

test('the project page shows the project, its repos and agendas, and states the reference rule', () => {
  const html = renderToStaticMarkup(React.createElement(ProjectPage, { slug: '앱', state: state('/project/%EC%95%B1'), controller: fakeController() })).replace(/<!-- -->/g, '');
  assert.match(html, /<h1>앱<\/h1>/);
  assert.match(html, /앱 프로젝트/);
  assert.match(html, /web · \/Users\/me\/web/);
  assert.match(html, /docs · me\/docs/);
  assert.match(html, /앱 아젠다/);
  assert.doesNotMatch(html, /남의 아젠다/, 'only this project\'s agendas');
  assert.match(html, /참조하지 않은 것은 쓰지 않아요/);
  assert.match(html, /쓸 수 있는 스킬/);
  assert.match(html, /쓸 수 있는 에이전트/);
  assert.match(html, /프로젝트 편집/);
  assert.match(html, /글로벌 자원 참조하기/);
  assert.match(renderToStaticMarkup(React.createElement(ProjectPage, { slug: '없음', state: state('/project/x'), controller: fakeController() })), /이 프로젝트를 찾을 수 없어요/);
});

test('the agents tab links to the global page and to each project page, and the notes tab no longer does', () => {
  const html = renderToStaticMarkup(React.createElement(Rail, { controller: fakeController(), state: state('/resources') }));
  assert.match(html, /class="nav-item active">스킬·에이전트/);
  assert.match(html, /앱 프로젝트 페이지 열기/);
  assert.doesNotMatch(html, /rail-agenda active/, 'no project row is active on the global page');
  const onProject = renderToStaticMarkup(React.createElement(Rail, { controller: fakeController(), state: state('/project/' + encodeURIComponent('앱')) }));
  assert.match(onProject, /class="rail-agenda active"/, 'the open project row is marked');
  const notes = renderToStaticMarkup(React.createElement(Rail, { controller: fakeController(), state: state('/calendar') }));
  assert.doesNotMatch(notes, /프로젝트 페이지 열기/, 'the notes tab keeps only the project edit and add buttons');
  assert.doesNotMatch(notes, /스킬·에이전트</);
});
