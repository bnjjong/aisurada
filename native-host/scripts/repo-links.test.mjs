import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import {
  agendaRepoView, cleanRepos, defaultRepoName, entriesToClone, localPathOf, parseRepos, projectRepoField,
  repoLabel, reposField, reposJson, sameRepos, validateRepos,
} from '../src/ui/repoLinks.ts';
import {
  cloneRepo, cloneSummary, connectGithub, disconnectGithub, getGithubSnapshot, listGithubRepos, refreshGithub, resetGithubStore,
} from '../src/ui/githubStore.ts';
import { GithubConnectView } from '../src/ui/GithubConnect.tsx';
import { RepoListEditor, cloneLine } from '../src/ui/RepoListEditor.tsx';
import { GUIDE_STEPS, RepoGuideScene, cloneFolderText } from '../src/ui/RepoGuide.tsx';
import { AgendaRepoLabel } from '../src/ui/AgendaRepoLabel.tsx';
import { matchAgenda } from '../src/plugs/claude/projects.tsx';

const CLONE_ROOT = '/Users/me/Library/Application Support/aisurada/repos';

// ---------------------------------------------------------------------------
// repoLinks: 순수 함수
// ---------------------------------------------------------------------------

test('parseRepos keeps well-formed entries, drops the rest, and reads the JSON text a fresh save leaves behind', () => {
  const list = [{ name: 'app', path: '/a' }, { name: 'docs', github: 'me/docs', githubId: 7 }, { path: '/no-name' }, 'x', null, { name: 'bad', githubId: -1 }];
  assert.deepEqual(parseRepos(list), [{ name: 'app', path: '/a' }, { name: 'docs', github: 'me/docs', githubId: 7 }, { name: 'bad' }]);
  assert.deepEqual(parseRepos(JSON.stringify(list.slice(0, 2))), [{ name: 'app', path: '/a' }, { name: 'docs', github: 'me/docs', githubId: 7 }]);
  assert.deepEqual(parseRepos('not json'), []);
  assert.deepEqual(parseRepos(undefined), []);
});

test('reposJson writes name, path, github, githubId in that order, trims, and is empty for an empty list', () => {
  const json = reposJson([{ githubId: 9, github: ' me/docs ', name: ' docs ', path: ' ' }, { name: 'app', path: '/a', githubId: 3 }]);
  assert.equal(json, '[{"name":"docs","github":"me/docs","githubId":9},{"name":"app","path":"/a"}]');
  assert.equal(reposJson([]), '');
  assert.deepEqual(cleanRepos([{ name: 'a', githubId: 5 }]), [{ name: 'a' }], 'githubId without github is dropped');
});

test('sameRepos ignores spacing and key order but sees a real change', () => {
  assert.ok(sameRepos([{ name: 'a', path: '/x' }], [{ path: ' /x', name: 'a ' }]));
  assert.ok(!sameRepos([{ name: 'a', path: '/x' }], [{ name: 'a', path: '/y' }]));
});

test('validateRepos mirrors the engine: name, unique, absolute path, owner/name, one of path or github, at most 20', () => {
  assert.equal(validateRepos([]), null);
  assert.equal(validateRepos([{ name: 'app', path: '/a' }, { name: 'docs', github: 'me/docs' }, { name: 'both', path: '/b', github: 'me/b' }]), null);
  assert.match(validateRepos([{ name: '', path: '/a' }]), /이름이 필요/);
  assert.match(validateRepos([{ name: 'a/b', path: '/a' }]), /빗금/);
  assert.match(validateRepos([{ name: 'x'.repeat(51), path: '/a' }]), /50자/);
  assert.match(validateRepos([{ name: 'a', path: '/a' }, { name: 'a', path: '/b' }]), /겹쳐요: a/);
  assert.match(validateRepos([{ name: 'a', path: 'relative/dir' }]), /전체 경로/);
  assert.match(validateRepos([{ name: 'a', github: 'no-slash' }]), /owner\/name/);
  assert.match(validateRepos([{ name: 'a', github: 'me/name with space' }]), /owner\/name/);
  assert.match(validateRepos([{ name: 'a' }]), /폴더나 GitHub/);
  assert.match(validateRepos(Array.from({ length: 21 }, (_, i) => ({ name: `r${i}`, path: '/a' }))), /20개/);
  assert.equal(validateRepos(Array.from({ length: 20 }, (_, i) => ({ name: `r${i}`, path: '/a' }))), null);
});

test('defaultRepoName takes the repo part and numbers it when taken', () => {
  assert.equal(defaultRepoName('me/docs', []), 'docs');
  assert.equal(defaultRepoName('me/docs', ['docs']), 'docs-2');
  assert.equal(defaultRepoName('me/docs', ['docs', 'docs-2']), 'docs-3');
  assert.equal(defaultRepoName('me/a b', []), 'a-b');
});

test('localPathOf: a folder wins; a GitHub-only entry lives at <cloneRoot>/<owner>/<name>; nothing to resolve gives undefined', () => {
  assert.equal(localPathOf({ name: 'a', path: '/mine', github: 'me/a' }, CLONE_ROOT), '/mine');
  assert.equal(localPathOf({ name: 'a', github: 'me/a' }, CLONE_ROOT + '/'), `${CLONE_ROOT}/me/a`);
  assert.equal(localPathOf({ name: 'a', github: 'me/a' }), undefined, 'clone root not known yet');
  assert.equal(localPathOf({ name: 'a' }, CLONE_ROOT), undefined);
});

test('entriesToClone skips entries with a folder and entries that were already there', () => {
  const next = [{ name: 'app', path: '/a', github: 'me/app' }, { name: 'docs', github: 'me/docs' }, { name: 'api', github: 'me/api' }];
  assert.deepEqual(entriesToClone(next, [{ name: 'docs', github: 'me/docs' }]), [{ name: 'api', github: 'me/api' }]);
  assert.deepEqual(entriesToClone(next).map((e) => e.name), ['docs', 'api']);
  assert.deepEqual(entriesToClone([{ name: 'docs', github: 'me/renamed' }], [{ name: 'docs', github: 'me/docs' }]).map((e) => e.github), ['me/renamed']);
});

test('reposField and projectRepoField send only what changed', () => {
  assert.deepEqual(reposField(undefined, [], true), {});
  assert.deepEqual(reposField(undefined, [{ name: 'a', path: '/a' }], true), { repos: '[{"name":"a","path":"/a"}]' });
  assert.deepEqual(reposField([{ name: 'a', path: '/a' }], [{ name: 'a', path: '/a' }], false), {});
  assert.deepEqual(reposField([{ name: 'a', path: '/a' }], [], false), { repos: '' }, 'emptying the list clears the line');
  assert.deepEqual(projectRepoField('', '', true), {});
  assert.deepEqual(projectRepoField('', 'app', true), { projectRepo: 'app' });
  assert.deepEqual(projectRepoField('app', 'app', false), {});
  assert.deepEqual(projectRepoField('app', '', false), { projectRepo: '' });
  assert.deepEqual(projectRepoField('', 'docs', false), { projectRepo: 'docs' });
});

const projects = [
  { id: 'p1', slug: 'platform', repos: [{ name: 'app', path: '/work/app' }, { name: 'docs', github: 'me/docs', githubId: 1 }] },
  { id: 'p2', slug: 'empty' },
];

test('agendaRepoView follows projectRepo into the project, falls back to the old repo text, and flags a dangling name', () => {
  const local = agendaRepoView({ project: 'platform', projectRepo: 'app', repo: '/old' }, projects, CLONE_ROOT);
  assert.equal(local.path, '/work/app');
  assert.equal(local.label, 'app · /work/app');
  assert.equal(local.dangling, false);
  const hub = agendaRepoView({ project: 'platform', projectRepo: 'docs' }, projects, CLONE_ROOT);
  assert.equal(hub.path, `${CLONE_ROOT}/me/docs`);
  assert.equal(hub.label, 'docs · me/docs');
  const gone = agendaRepoView({ project: 'platform', projectRepo: 'removed', repo: '/old' }, projects, CLONE_ROOT);
  assert.deepEqual([gone.path, gone.label, gone.dangling], ['/old', '/old', true]);
  const legacy = agendaRepoView({ project: 'empty', repo: '/legacy/' }, projects, CLONE_ROOT);
  assert.deepEqual([legacy.path, legacy.label, legacy.dangling], ['/legacy/', '/legacy/', false]);
  const none = agendaRepoView({ project: '' }, projects);
  assert.deepEqual([none.path, none.label], [undefined, '']);
  assert.equal(repoLabel({ name: 'x', github: 'a/b' }), 'x · a/b');
});

// ---------------------------------------------------------------------------
// 세션 붙이기: matchAgenda가 저장소 폴더를 풀어 쓴다
// ---------------------------------------------------------------------------

test('matchAgenda keeps its two-argument behaviour and takes a resolver for project repos', () => {
  const a = { id: 'a', slug: 'a', title: 'a', project: 'platform', projectRepo: 'app' };
  const b = { id: 'b', slug: 'b', title: 'b', repo: '/old/b' };
  assert.equal(matchAgenda('/old/b/src', [a, b]).id, 'b', 'default resolver reads the old repo text');
  assert.equal(matchAgenda('/work/app/src', [a, b]), null, 'without a resolver a projectRepo agenda has no folder');
  const repoOf = (agenda) => agendaRepoView(agenda, projects, CLONE_ROOT).path;
  assert.equal(matchAgenda('/work/app/src', [a, b], repoOf).id, 'a');
  assert.equal(matchAgenda('/old/b', [a, b], repoOf).id, 'b');
});

// ---------------------------------------------------------------------------
// githubStore: 가짜 호스트로 상태 흐름을 본다
// ---------------------------------------------------------------------------

const connectedStatus = { connected: true, appId: '42', appName: 'aisurada-me', appSlug: 'aisurada-me', cloneRoot: CLONE_ROOT };

function fakeHost(handlers) {
  const calls = [];
  const call = async (command, args) => {
    calls.push([command, args]);
    if (!handlers[command]) throw new Error(`unexpected ${command}`);
    return handlers[command](args);
  };
  return { call, calls };
}

test('refreshGithub stores the status, or keeps an error and no status when the host cannot be called', async () => {
  resetGithubStore();
  const host = fakeHost({ github_status: () => connectedStatus });
  assert.deepEqual(await refreshGithub(host.call), connectedStatus);
  assert.equal(getGithubSnapshot().status.appName, 'aisurada-me');
  const broken = fakeHost({ github_status: () => { throw 'host is gone'; } });
  assert.equal(await refreshGithub(broken.call), null);
  assert.equal(getGithubSnapshot().status, null);
  assert.equal(getGithubSnapshot().error, 'host is gone');
});

test('connectGithub passes the App ID and the key file path (never key text) and keeps the host error text on failure', async () => {
  resetGithubStore();
  const host = fakeHost({ github_connect: () => connectedStatus });
  await connectGithub(host.call, ' 42 ', '/Users/me/key.pem');
  assert.deepEqual(host.calls, [['github_connect', { appId: '42', pemPath: '/Users/me/key.pem' }]]);
  assert.equal(getGithubSnapshot().status.connected, true);
  resetGithubStore();
  const failing = fakeHost({ github_connect: () => { throw 'App ID가 맞지 않아요'; } });
  assert.equal(await connectGithub(failing.call, '1', '/k.pem'), null);
  assert.equal(getGithubSnapshot().error, 'App ID가 맞지 않아요');
  assert.equal(getGithubSnapshot().status, null);
});

test('cloneRepo refuses without a connection, shares one in-flight clone, and records the outcome per repository', async () => {
  resetGithubStore();
  const none = fakeHost({});
  const refused = await cloneRepo(none.call, 'me/docs');
  assert.equal(refused.phase, 'error');
  assert.equal(none.calls.length, 0, 'the host is not called without a connection');

  await refreshGithub(fakeHost({ github_status: () => connectedStatus }).call);
  let release;
  const gate = new Promise((resolve) => { release = resolve; });
  const host = fakeHost({ github_clone: async ({ fullName }) => { await gate; return { path: `${CLONE_ROOT}/${fullName}`, cloned: true }; } });
  const first = cloneRepo(host.call, 'me/docs');
  const second = cloneRepo(host.call, 'me/docs');
  assert.equal(getGithubSnapshot().clones['me/docs'].phase, 'cloning');
  release();
  const [a, b] = await Promise.all([first, second]);
  assert.deepEqual(a, b);
  assert.equal(host.calls.length, 1, 'two calls shared one clone');
  assert.deepEqual(host.calls[0], ['github_clone', { fullName: 'me/docs' }]);
  assert.equal(getGithubSnapshot().clones['me/docs'].phase, 'done');

  const failing = fakeHost({ github_clone: () => { throw '저장소를 찾지 못했다'; } });
  const lost = await cloneRepo(failing.call, 'me/gone');
  assert.deepEqual(lost, { phase: 'error', error: '저장소를 찾지 못했다' });
  assert.match(cloneSummary('me/gone', lost), /받지 못했어요: 저장소를 찾지 못했다/);
  assert.match(cloneSummary('me/docs', a), /받았어요/);
  assert.match(cloneSummary('me/docs', { phase: 'done', path: '/p', cloned: false }), /이미 받아 둔 폴더/);
});

test('disconnectGithub asks the host, refreshes the status, and forgets the clone results', async () => {
  resetGithubStore();
  let connected = true;
  const host = fakeHost({
    github_status: () => ({ ...connectedStatus, connected }),
    github_clone: () => ({ path: '/p', cloned: true }),
    github_disconnect: () => { connected = false; },
  });
  await refreshGithub(host.call);
  await cloneRepo(host.call, 'me/docs');
  assert.ok(await disconnectGithub(host.call));
  assert.equal(getGithubSnapshot().status.connected, false);
  assert.deepEqual(getGithubSnapshot().clones, {});
});

test('listGithubRepos returns the host list as it is', async () => {
  const list = { repositories: [{ fullName: 'me/a', id: 1, private: true, defaultBranch: 'main', htmlUrl: 'u', installationId: 2 }], errors: [] };
  assert.deepEqual(await listGithubRepos(fakeHost({ github_repositories: () => list }).call), list);
});

// ---------------------------------------------------------------------------
// 화면: 서버 렌더로 글과 단추만 본다
// ---------------------------------------------------------------------------

const noop = () => {};
const drawConnect = (props) => renderToStaticMarkup(React.createElement(GithubConnectView, {
  status: null, error: '', busy: false, appId: '', pemName: '', native: true,
  onAppId: noop, onPickKey: noop, onConnect: noop, onDisconnect: noop, ...props,
}));

test('GithubConnectView: not connected shows the two inputs and a disabled connect button until both are given', () => {
  const empty = drawConnect({ status: { connected: false, cloneRoot: CLONE_ROOT } });
  assert.match(empty, /연결되어 있지 않아요/);
  assert.match(empty, /App ID/);
  assert.match(empty, /아직 고르지 않았어요/);
  assert.match(empty, /<button[^>]*disabled[^>]*>GitHub에 연결<\/button>/);
  const ready = drawConnect({ status: { connected: false, cloneRoot: CLONE_ROOT }, appId: '42', pemName: 'key.pem' });
  assert.match(ready, /key\.pem/);
  assert.doesNotMatch(ready, /<button[^>]*disabled[^>]*>GitHub에 연결<\/button>/);
});

test('GithubConnectView: connected shows the app, the clone folder and a disconnect button; no inputs', () => {
  const html = drawConnect({ status: connectedStatus });
  assert.match(html, /연결되어 있어요 · aisurada-me \(App ID 42\)/);
  assert.match(html, new RegExp(CLONE_ROOT.replace(/[/ ]/g, (c) => (c === ' ' ? ' ' : '\\/'))));
  assert.match(html, /연결 끊기/);
  assert.doesNotMatch(html, /개인 키\(\.pem\)/);
});

test('GithubConnectView: a browser preview without the host says so and offers no buttons; errors are shown as alerts', () => {
  const web = drawConnect({ native: false });
  assert.match(web, /데스크톱 앱에서만/);
  assert.doesNotMatch(web, /GitHub에 연결<\/button>/);
  const failed = drawConnect({ status: { connected: false, cloneRoot: CLONE_ROOT }, error: '개인 키 파일을 읽지 못했다' });
  assert.match(failed, /role="alert"[^>]*>개인 키 파일을 읽지 못했다/);
});

test('cloneLine says what will happen for a GitHub-only entry and stays quiet for the rest', () => {
  const hub = { name: 'docs', github: 'me/docs' };
  assert.equal(cloneLine({ name: 'a', path: '/a', github: 'me/a' }, undefined, true), '', 'a folder means no clone');
  assert.equal(cloneLine({ name: 'a' }, undefined, true), '');
  assert.match(cloneLine(hub, undefined, true), /저장하면 앱이 받아 둬요/);
  assert.match(cloneLine(hub, undefined, false), /GitHub를 연결하면/);
  assert.match(cloneLine(hub, { phase: 'cloning' }, true), /받는 중/);
  assert.match(cloneLine(hub, { phase: 'done', path: '/p', cloned: true }, true), /받았어요 · \/p/);
  assert.match(cloneLine(hub, { phase: 'done', path: '/p', cloned: false }, true), /이미 받아 둔 폴더/);
  assert.match(cloneLine(hub, { phase: 'error', error: 'nope' }, true), /받지 못했어요 · nope/);
});

test('RepoListEditor draws one row per entry with its fields and the add button', () => {
  resetGithubStore();
  const host = { callHost: async () => { throw new Error('no host'); }, pickDirectory: async () => null };
  const html = renderToStaticMarkup(React.createElement(RepoListEditor, {
    entries: [{ name: 'app', path: '/work/app' }, { name: 'docs', github: 'me/docs', githubId: 1 }],
    onChange: noop, host,
  }));
  assert.equal((html.match(/data-testid="repo-row"/g) || []).length, 2);
  assert.match(html, /value="\/work\/app"/);
  assert.match(html, /value="me\/docs"/);
  assert.match(html, /쓰는 폴더 · \/work\/app/);
  assert.match(html, /GitHub를 연결하면 저장할 때 받아 둬요/);
  assert.match(html, /<button[^>]*disabled[^>]*>내 저장소에서 고르기<\/button>/, 'the picker needs a connection');
  assert.match(html, /저장소 추가/);
});

test('AgendaRepoLabel draws the resolved repo, and names a project repo that went missing', () => {
  resetGithubStore();
  const draw = (agenda) => renderToStaticMarkup(React.createElement(AgendaRepoLabel, { agenda, projects }));
  assert.equal(draw({ project: 'platform', projectRepo: 'app' }), 'app · /work/app');
  assert.equal(draw({ project: 'empty', repo: '/legacy' }), '/legacy');
  assert.match(draw({ project: 'platform', projectRepo: 'gone', repo: '/old' }), /\/old.*gone는 프로젝트에서 빠졌어요/);
});

test('cloneFolderText names the real clone folder once the status is known, and the default place before that', () => {
  assert.equal(cloneFolderText(CLONE_ROOT + '/'), `${CLONE_ROOT}/<소유자>/<이름>`);
  assert.match(cloneFolderText(), /앱 전용 폴더.*aisurada\/repos\/<소유자>\/<이름>/);
});

test('the guide link sits in the repo editor (the popup itself is a portal, so it stays closed in a server render)', () => {
  resetGithubStore();
  const host = { callHost: async () => { throw new Error('no host'); }, pickDirectory: async () => null };
  const html = renderToStaticMarkup(React.createElement(RepoListEditor, { entries: [], onChange: noop, host }));
  assert.match(html, /<button[^>]*class="repo-guide-open"[^>]*aria-haspopup="dialog"[^>]*>왜 필요해요\?<\/button>/);
  assert.doesNotMatch(html, /프로젝트 저장소 안내/, 'closed until it is clicked');
});

const GUIDE_WORDS = [
  ['프로젝트', '저장소 목록', '아젠다', '고름', '자동 묶임', 'Claude Code', '앞으로: 앱 안 개발'],
  ['내 컴퓨터 폴더', '그대로 사용', '복사 안 함', 'GitHub 저장소', '앱이 받아 둠', 'GitHub 연결 필요', '앱 전용 폴더'],
  ['개인 키는 키체인에만', '읽기와 받기만 해요', '올리기·수정은 아직 없어요', '빼도 폴더는 그대로', '목록은 노트 폴더에'],
];

test('the guide is three pictures, each with a one-line title and caption (text stays short on purpose)', () => {
  assert.deepEqual(GUIDE_STEPS.map((s) => s.id), ['why', 'how', 'safe']);
  for (const step of GUIDE_STEPS) {
    assert.ok(step.title.length <= 20, `title too long: ${step.title}`);
    assert.ok(step.caption.length <= 40, `caption too long: ${step.caption}`);
  }
});

test('each guide picture carries its labels and an accessible description, and nothing active', () => {
  GUIDE_WORDS.forEach((words, step) => {
    const html = renderToStaticMarkup(React.createElement(RepoGuideScene, { step }));
    assert.match(html, /role="img" aria-label="[^"]{30,}"/, `step ${step} needs a spoken description`);
    for (const word of words) assert.ok(html.includes(word), `step ${step}: ${word}`);
    assert.doesNotMatch(html, /<script|<style|foreignObject/);
  });
  assert.equal(renderToStaticMarkup(React.createElement(RepoGuideScene, { step: 99 })), renderToStaticMarkup(React.createElement(RepoGuideScene, { step: 0 })), 'out of range falls back to the first picture');
});
