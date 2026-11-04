import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToString } from 'react-dom/server';
import { RailProjects, matchAgenda, setSessionStoreState, useProjectsList } from '../src/plugs/claude/projects.tsx';

const agenda = (id, repo, extra = {}) => ({ id, slug: id, title: id, repo, ...extra });
const session = (sessionId, cwd, extra = {}) => ({ sessionId, cwd, startedAt: Date.now(), kind: 'interactive', status: 'idle', ...extra });

test('6: useProjectsList returns an array inside a component', () => {
  let captured;
  function TestComp() {
    captured = useProjectsList();
    return null;
  }
  renderToString(React.createElement(TestComp));
  assert(Array.isArray(captured));
});

test('6.8 L2: every trailing slash goes, a repo of nothing but slashes is not looked at, the longest repo wins, ties go to the earlier agenda', () => {
  const list = [agenda('a', '/Users/me/kcd/ads-platform/'), agenda('b', '/Users/me/kcd/ads-platform-domain'), agenda('c', '/'), agenda('d', ''), agenda('e', '/Users/me/kcd/ads-platform')];
  assert.equal(matchAgenda('/Users/me/kcd/ads-platform', list).id, 'a', 'equal length: the earlier of a and e');
  assert.equal(matchAgenda('/Users/me/kcd/ads-platform-domain/apps/web', list).id, 'b');
  assert.equal(matchAgenda('/Users/me/kcd/ads-platform-frontend', list), null, 'a text head is not a path head');
  assert.equal(matchAgenda('/Users/me/kcd', list), null, 'the "/" repo does not take every session');
  assert.equal(matchAgenda('/USERS/me/kcd/ads-platform', list), null, 'case is not folded');
  assert.equal(matchAgenda('/x/y//', [agenda('z', '/x/y///')]).id, 'z');
});

function draw(state) {
  return renderToString(React.createElement(RailProjects, { state }));
}
const workspace = (agendas, projects = []) => ({ path: '/vault', name: 'v', sample: false, today: '2026-11-04', agendas, projects, notes: [], templates: [], reports: [], trash: [], skipped: [], conflicts: [], noteReadFailed: false });

test('6.10 L1: with no session and no line to write, the section is not drawn', () => {
  setSessionStoreState({ status: 'ok', sessions: [], skippedCount: 0, endedDroppedCount: 0, error: null, readAt: Date.now() });
  assert.equal(draw({ workspace: workspace([]), prefs: {}, route: '/calendar' }), '');
});

test('6.8 6.13: unmatched sessions are counted in a line that starts with the contract words, and a project group is drawn', () => {
  setSessionStoreState({ status: 'ok', sessions: [session('s1', '/tmp/elsewhere'), session('s2', '/Users/me/kcd/ads-platform/apps')], skippedCount: 0, endedDroppedCount: 0, error: null, readAt: Date.now() });
  const html = draw({ workspace: workspace([agenda('ag1', '/Users/me/kcd/ads-platform', { project: 'p1' })], [{ slug: 'p1', title: '광고', hue: '1' }]), prefs: { expandedSessionProjects: ['sess:p1'] }, route: '/calendar' }).replace(/<!-- -->/g, '');
  assert.match(html, /data-rail-section="projects"/);
  assert.match(html, /아젠다에 안 붙은 세션 1개/);
  assert.match(html, /class="sess-toggle"/);
  assert.match(html, /광고/);
});

test('6.5 6.10: the cut ended sessions line keeps the section even when there is nothing else to show', () => {
  setSessionStoreState({ status: 'ok', sessions: [], skippedCount: 0, endedDroppedCount: 50, error: null, readAt: Date.now() });
  const html = draw({ workspace: workspace([]), prefs: {}, route: '/calendar' }).replace(/<!-- -->/g, '');
  assert.match(html, /data-note="ended-dropped"/);
  assert.match(html, /이레 안에 끝난 세션 중 오래된 50개는 안 보여요\. 한 번에 읽는 수를 넘었어요\./);
  setSessionStoreState({ status: 'failed', sessions: [], endedDroppedCount: 0, error: '세션 목록을 못 받았어요: boom' });
  assert.match(draw({ workspace: workspace([]), prefs: {}, route: '/calendar' }), /sess-note-bad/);
  setSessionStoreState({ status: 'idle', sessions: [], endedDroppedCount: 0, error: null });
});
