// 후속 노트 탭: 탭 바의 이름과 ↳ 표시, 마우스를 올렸을 때의 경로 설명, 상위 노트가 없는 탭 걸러내기.
import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { TabBar, tabAskDot, tabTitle, tabTooltip } from '../src/navigation/TabBar.tsx';
import { holdSubChain, subNoteAt } from '../src/navigation/subNotes.ts';

const note = (id, title) => ({ id, title, date: '2026-11-14', time: '13:00' });
const state = (route, openIds, subNotes = {}) => ({
  route, prefs: { titleDate: false }, tabs: { openIds },
  workspace: { path: '/vault', notes: [note('a', '노트 읽기 코드 인수인계'), note('b', '다른 노트')] },
  vault: { subNotes },
});
const loaded = { a: [note('c1', 'vault 훑기 자세히')], c1: [note('g', '손자 노트')] };
const controller = { selectTab() {}, closeTab() {}, reorderTab() {} };
const bar = (s) => renderToStaticMarkup(React.createElement(TabBar, { controller, state: s })).replace(/<!-- -->/g, '');

test('후속 노트 탭 이름은 제목만이고, 읽기 전에는 「후속 노트」다', () => {
  assert.equal(tabTitle('sub:a/c1', state('/doc/a', [], loaded)), 'vault 훑기 자세히');
  assert.equal(tabTitle('sub:a/c1/g', state('/doc/a', [], loaded)), '손자 노트');
  assert.equal(tabTitle('sub:a/c1', state('/doc/a', [])), '후속 노트');
  assert.equal(tabTitle('a', state('/doc/a', [])), '노트 읽기 코드 인수인계');
});

test('마우스를 올리면 최상위 노트부터 이 노트까지 제목을 ›로 이어 보여 준다', () => {
  assert.equal(tabTooltip('sub:a/c1/g', state('/doc/a', [], loaded)), '노트 읽기 코드 인수인계 › vault 훑기 자세히 › 손자 노트');
  assert.equal(tabTooltip('sub:a/c1', state('/doc/a', [])), '노트 읽기 코드 인수인계 › 후속 노트');
  assert.equal(tabTooltip('a', state('/doc/a', [])), '노트 읽기 코드 인수인계');
});

test('탭 바는 후속 노트 탭에만 ↳ 표시를 붙이고, 보던 탭을 선택 상태로 둔다', () => {
  const html = bar(state('/sub/a/c1', ['a', 'sub:a/c1', 'b'], loaded));
  assert.equal((html.match(/class="tab-sub-mark"/g) || []).length, 1);
  assert.match(html, /<span class="tab-sub-mark" aria-hidden="true">↳<\/span><span class="tab-label">vault 훑기 자세히<\/span>/);
  assert.match(html, /aria-label="vault 훑기 자세히 탭 닫기"/);
  assert.match(html, /title="노트 읽기 코드 인수인계 › vault 훑기 자세히"/);
  const selected = html.match(/tab-selected tabx-selected[^>]*data-tab-id="([^"]+)"/);
  assert.equal(selected?.[1], 'sub:a/c1');
});

test('최상위 노트가 없는 후속 노트 탭은 탭 바에 그리지 않는다', () => {
  const html = bar(state('/doc/b', ['sub:gone/x', 'b', 'sub:a/c1'], loaded));
  assert.doesNotMatch(html, /sub:gone\/x/); assert.match(html, /data-tab-id="sub:a\/c1"/);
});

// 사슬 읽기: 가짜 vault로 목록을 한 단계씩 읽는 순서, 낡은 읽기 재시도, 실패, 놓기를 잰다.
function fakeController(lists, { stale = 0 } = {}) {
  const log = [], retained = new Map(); let staleLeft = stale;
  return {
    log, retained,
    getSnapshot: () => ({ workspace: { notes: [note('a', '루트')] } }),
    data: {
      retainSubNotes(parent) { log.push('retain ' + parent); retained.set(parent, (retained.get(parent) || 0) + 1); return () => { log.push('release ' + parent); retained.set(parent, retained.get(parent) - 1); }; },
      async loadSubNotes(parent) { log.push('load ' + parent); await null; if (staleLeft > 0) { staleLeft--; return []; } return lists[parent] || []; },
    },
  };
}

test('사슬 읽기는 위에서부터 한 단계씩 읽고, 읽는 목록을 붙들었다가 놓는다', async () => {
  const c = fakeController(loaded), held = holdSubChain(c, 'a', ['c1', 'g']);
  await held.done;
  assert.deepEqual(c.log, ['retain a', 'load a', 'retain c1', 'load c1']);
  assert.deepEqual([...c.retained.values()], [1, 1]);
  held.release(); assert.deepEqual([...c.retained.values()], [0, 0]);
});

test('먼저 요청한 읽기가 낡은 목록을 돌려줘도 한 번 더 읽어 없는 노트로 잘못 알리지 않는다', async () => {
  const c = fakeController(loaded, { stale: 1 });
  await holdSubChain(c, 'a', ['c1']).done;
  assert.equal(c.log.filter((entry) => entry === 'load a').length, 2);
});

test('사슬의 노트가 없거나 최상위 노트가 없으면 오류로 끝나고, 붙든 것은 놓을 수 있다', async () => {
  const c = fakeController(loaded), held = holdSubChain(c, 'a', ['c1', 'missing']);
  await assert.rejects(held.done, /후속 노트를 찾을 수 없어요/); held.release(); assert.deepEqual([...c.retained.values()], [0, 0]);
  await assert.rejects(holdSubChain(c, 'gone', ['c1']).done, /상위 노트를 찾을 수 없어요/);
});

test('놓은 뒤에는 더 읽지 않는다', async () => {
  const c = fakeController(loaded), held = holdSubChain(c, 'a', ['c1', 'g']);
  held.release(); await held.done;
  assert.ok(!c.log.includes('load c1'), '놓은 뒤에도 다음 단계를 읽었다');
});

test('사슬의 마지막 후속 노트를 상태에서 찾는다', () => {
  assert.equal(subNoteAt(state('/x', [], loaded), 'a', ['c1', 'g'])?.title, '손자 노트');
  assert.equal(subNoteAt(state('/x', [], loaded), 'a', ['nope']), undefined);
});

test('후속 노트의 AI 답은 그 후속 노트 탭에 점으로 알리고, 최상위 노트 탭에는 하위 노트 점으로 알린다', () => {
  const unseen = { c1: 'a' };
  assert.equal(tabAskDot('sub:a/c1', unseen), 'AI 답이 왔어요');
  assert.equal(tabAskDot('a', unseen), '하위 노트에 AI 답이 왔어요');
  assert.equal(tabAskDot('b', unseen), null);
  assert.equal(tabAskDot('sub:a/c1/g', unseen), null, '손자 노트에 답이 오지 않았는데 점이 떴다');
  assert.equal(tabAskDot('sub:b/c1', unseen), null, '다른 최상위 노트의 같은 id에 점이 떴다');
  assert.equal(tabAskDot('a', { a: 'a' }), 'AI 답이 왔어요');
  assert.equal(tabAskDot('a', {}), null); assert.equal(tabAskDot('sub:a/c1', {}), null);
});
