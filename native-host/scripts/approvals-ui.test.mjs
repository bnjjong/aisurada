import test from 'node:test';
import assert from 'node:assert/strict';
import React from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import {
  DEFAULT_POLICY, DIFF_LIMIT, FIXED_NOTE, POLICY_ROWS, approveChange, badgeText, badgeTotal, fieldDiffs, getChange, loadApprovals, loadCounts,
  proposerLabel, rejectChange, resolveEdit, savePolicy, sideBySide, targetLabel, toChange, toCounts, toDetail, toOverview, toPolicy, toUnapproved, toView,
} from '../src/ui/approvalsClient.ts';
import {
  POLL_MS, getApprovalsSnapshot, refreshApprovalCounts, resetApprovalsStore, setApprovalCounts, startApprovalsWatch,
} from '../src/ui/approvalsStore.ts';
import { ApprovalsView, Compare } from '../src/ui/ApprovalPanel.tsx';
import { ResourcesPage } from '../src/ui/ResourcesPage.tsx';
import { Rail } from '../src/ui/Rail.tsx';
import { toResource } from '../src/ui/resourcesClient.ts';

const view = (extra = {}) => ({
  id: 'ap1-aaaaaaaa', kind: 'agent', scope: 'project', project: '앱', name: '웹 담당', description: '웹을 맡는다', permission: 'read', skills: [], body: '원래 지침\n둘째 줄', ...extra,
});

const change = (extra = {}) => ({
  id: 'pcmw1-aaaaaaaa', status: 'pending', command: 'save-resource', createdAt: '2026-11-14T01:02:03+09:00', summary: "프로젝트 에이전트 '웹 담당' 고치기(권한 read → publish)",
  target: { kind: 'agent', scope: 'project', project: '앱', id: 'ap1-aaaaaaaa', name: '웹 담당' }, proposer: { session: 'abcdef1234567890', channel: 'cli' }, fixed: true,
  reasons: [{ category: 'permission-raise', label: '권한 올림', fixed: true }], ...extra,
});

const overview = (extra = {}) => ({
  pending: [toChange(change())], unapproved: [], decided: [], counts: { pending: 1, unapproved: 0 }, policy: { ...DEFAULT_POLICY }, ...extra,
});

/** 엔진 호출을 기록하는 가짜. 답은 첫 낱말(명령 이름)로 정한다. */
const fakeEngine = (answers = {}) => {
  const seen = [];
  const call = async (argv) => {
    seen.push(argv);
    const a = answers[argv[0]];
    if (a instanceof Error) throw a;
    return typeof a === 'function' ? a(argv) : a;
  };
  return { call, seen };
};

test('toChange / toDetail / toUnapproved / toPolicy read the engine rows and refuse broken ones', () => {
  const c = toChange(change());
  assert.equal(c.status, 'pending');
  assert.equal(c.fixed, true);
  assert.deepEqual(c.reasons, [{ category: 'permission-raise', label: '권한 올림', fixed: true }]);
  assert.equal(c.proposer.session, 'abcdef1234567890');
  for (const bad of [null, 'x', [], { id: 'pc', status: 'unknown' }, { status: 'pending' }, { id: '', status: 'pending' }]) assert.equal(toChange(bad), null);
  assert.equal(toChange({ id: 'pcx', status: 'stale' }).summary, '스킬·에이전트 변경', 'a missing summary has a fallback');
  const d = toDetail({ ...change(), before: view(), after: view({ permission: 'publish' }), changedFields: ['permission'], ref: { project: '앱', added: ['sk1'], removed: [] } });
  assert.equal(d.after.permission, 'publish');
  assert.deepEqual(d.changedFields, ['permission']);
  assert.deepEqual(d.ref, { project: '앱', added: ['sk1'], removed: [] });
  assert.equal(toDetail({ nonsense: 1 }), null);
  assert.equal(toView({ id: 'x' }), undefined, 'a view without the needed fields is dropped');
  const u = toUnapproved({ key: 'ap1', type: 'resource', state: 'dirty', id: 'ap1', name: '웹', approved: view(), live: view({ permission: 'publish' }) });
  assert.equal(u.state, 'dirty');
  assert.equal(u.live.permission, 'publish');
  const r = toUnapproved({ key: 'refs:앱', type: 'refs', state: 'new', project: '앱', approved: ['sk1'], live: ['sk1', 'sk2'] });
  assert.deepEqual(r.live, ['sk1', 'sk2'], 'a refs list is a list of ids');
  for (const bad of [null, { key: 'a', type: 'x', state: 'dirty' }, { key: 'a', type: 'refs', state: 'old' }, { type: 'refs', state: 'new' }]) assert.equal(toUnapproved(bad), null);
  assert.deepEqual(toPolicy(null), DEFAULT_POLICY);
  assert.deepEqual(toPolicy({ project: 'approve', global: 'bogus' }), { ...DEFAULT_POLICY, project: 'approve' }, 'a bad value keeps the default');
  assert.deepEqual(toCounts({ pending: 2, unapproved: -1 }), { pending: 2, unapproved: 0 });
  assert.deepEqual(toCounts('x'), { pending: 0, unapproved: 0 });
});

test('defaults: only the project content is applied at once, the rest waits for a person', () => {
  assert.deepEqual(DEFAULT_POLICY, { project: 'auto', global: 'approve', refs: 'approve', permission: 'approve' });
  assert.deepEqual(POLICY_ROWS.map((r) => r.key), ['project', 'global', 'refs', 'permission']);
  assert.match(FIXED_NOTE, /권한 올림, 도구 칸 변경, 참조 켜기/);
});

test('toOverview takes the counts from the engine and falls back to the lists', () => {
  const o = toOverview({
    counts: { pending: 2, unapproved: 1 }, pending: [change(), change({ id: 'pcmw2-bbbbbbbb' }), { broken: 1 }], unapproved: [{ key: 'ap1', type: 'resource', state: 'new' }],
    decided: [change({ status: 'approved' })], policy: { global: 'auto' },
  });
  assert.equal(o.pending.length, 2, 'a broken row is dropped');
  assert.deepEqual(o.counts, { pending: 2, unapproved: 1 });
  assert.equal(o.policy.global, 'auto');
  assert.deepEqual(toOverview({ pending: [change()], unapproved: [] }).counts, { pending: 1, unapproved: 0 });
  assert.deepEqual(toOverview(null).pending, []);
});

test('the engine calls carry the arguments the engine reads', async () => {
  const { call, seen } = fakeEngine({
    'list-resource-changes': { counts: { pending: 3, unapproved: 2 }, pending: [change()], unapproved: [], decided: [], policy: {} },
    'get-resource-change': { ...change(), before: view(), after: view({ permission: 'publish' }), changedFields: ['permission'] },
    'set-resource-policy': { policy: { project: 'approve', global: 'approve', refs: 'approve', permission: 'approve' } },
  });
  assert.equal((await loadApprovals(call)).pending.length, 1);
  assert.deepEqual(seen.at(-1), ['list-resource-changes', 'decided=20']);
  assert.deepEqual(await loadCounts(call), { pending: 3, unapproved: 2 });
  assert.deepEqual(seen.at(-1), ['list-resource-changes', 'decided=0']);
  assert.equal((await getChange(call, 'pcmw1-aaaaaaaa')).after.permission, 'publish');
  assert.deepEqual(seen.at(-1), ['get-resource-change', 'pcmw1-aaaaaaaa']);
  await approveChange(call, 'pc1');
  assert.deepEqual(seen.at(-1), ['approve-resource-change', 'pc1']);
  await rejectChange(call, 'pc1', '  이유  ');
  assert.deepEqual(seen.at(-1), ['reject-resource-change', 'pc1', 'reason=이유'], 'the reason is trimmed');
  await rejectChange(call, 'pc1', '   ');
  assert.deepEqual(seen.at(-1), ['reject-resource-change', 'pc1'], 'a blank reason is not sent');
  await resolveEdit(call, 'refs:앱', 'revert');
  assert.deepEqual(seen.at(-1), ['resolve-resource-edit', 'refs:앱', 'action=revert']);
  await resolveEdit(call, 'ap1', 'approve');
  assert.deepEqual(seen.at(-1), ['resolve-resource-edit', 'ap1', 'action=approve']);
  const saved = await savePolicy(call, { project: 'approve', global: 'approve', refs: 'approve', permission: 'approve' });
  assert.deepEqual(seen.at(-1), ['set-resource-policy', 'project=approve', 'global=approve', 'refs=approve', 'permission=approve']);
  assert.equal(saved.project, 'approve');
  await assert.rejects(getChange(async () => ({ nonsense: 1 }), 'pc1'), /모양이 틀려요/);
});

test('labels: badge, who proposed, and what a change touches', () => {
  assert.equal(badgeText({ pending: 0, unapproved: 0 }), '');
  assert.equal(badgeText({ pending: 2, unapproved: 1 }), '3');
  assert.equal(badgeText({ pending: 99, unapproved: 5 }), '99+');
  assert.equal(badgeTotal({ pending: 2, unapproved: 1 }), 3);
  assert.equal(proposerLabel({ session: 'abcdef1234567890' }), 'Claude 세션 abcdef12');
  assert.equal(proposerLabel({ session: 'thread-9', agent: 'codex' }), 'Codex 세션 thread-9');
  assert.equal(proposerLabel({ channel: 'mcp' }), 'MCP로 연결된 AI');
  assert.equal(proposerLabel({}), 'AI');
  assert.equal(targetLabel({ kind: 'agent', scope: 'project', project: '앱', name: '웹 담당' }), '프로젝트 앱 · 에이전트 · 웹 담당');
  assert.equal(targetLabel({ kind: 'skill', scope: 'global', name: '번역' }), '글로벌 · 스킬 · 번역');
  assert.equal(targetLabel({}), '');
});

test('fieldDiffs lists only what changed; a skill has no agent fields; create and delete list the filled fields', () => {
  const d = fieldDiffs(view(), view({ permission: 'publish', body: '원래 지침\n바뀐 줄' }));
  assert.deepEqual(d.map((x) => x.key), ['permission', 'body']);
  assert.deepEqual([d[0].before, d[0].after, d[0].kind], ['read', 'publish', 'line']);
  assert.equal(d[1].kind, 'lines', 'the body is compared line by line');
  assert.deepEqual(fieldDiffs(view(), view()), [], 'no change, no rows');
  const skillA = { id: 'sk1', kind: 'skill', scope: 'global', name: '번역', description: 'd', body: 'a' };
  assert.deepEqual(fieldDiffs(skillA, { ...skillA, permission: 'publish', repo: 'web', skills: ['x'] }).map((x) => x.key), [], 'a skill ignores agent-only fields');
  const created = fieldDiffs(undefined, view({ skills: ['sk1'], tools: { claude: { 'allowed-tools': ['Read'] } } }));
  assert.deepEqual(created.map((x) => x.key), ['name', 'description', 'permission', 'skills', 'tools', 'body'], 'fields come in the order of the form');
  assert.ok(created.some((x) => x.key === 'tools' && x.after.includes('allowed-tools')), 'a tools box shows as JSON');
  const removed = fieldDiffs(view(), undefined);
  assert.ok(removed.every((x) => x.after === ''), 'a deletion empties every filled field');
  assert.deepEqual(fieldDiffs(undefined, undefined), []);
});

test('sideBySide: equal lines on both sides, changed blocks paired, the extra side left empty', () => {
  assert.deepEqual(sideBySide('a\nb', 'a\nb').map((r) => r.kind), ['same', 'same']);
  const one = sideBySide('a\nb\nc', 'a\nB\nc');
  assert.deepEqual(one.map((r) => [r.kind, r.left, r.right]), [['same', 'a', 'a'], ['change', 'b', 'B'], ['same', 'c', 'c']]);
  const added = sideBySide('a', 'a\nx\ny');
  assert.deepEqual(added.map((r) => [r.kind, r.left, r.right]), [['same', 'a', 'a'], ['add', null, 'x'], ['add', null, 'y']]);
  const dropped = sideBySide('a\nx\nb', 'a\nb');
  assert.deepEqual(dropped.map((r) => [r.kind, r.left, r.right]), [['same', 'a', 'a'], ['del', 'x', null], ['same', 'b', 'b']]);
  const uneven = sideBySide('a\nb\nc', 'a\nX');
  assert.deepEqual(uneven.map((r) => r.kind), ['same', 'change', 'del']);
  assert.deepEqual(sideBySide('', ''), [], 'two empty texts have no lines');
  assert.deepEqual(sideBySide('', 'x').map((r) => r.kind), ['add']);
  assert.deepEqual(sideBySide('x', '').map((r) => r.kind), ['del']);
  // 너무 큰 글은 정확한 비교 대신 통째로 바뀐 것으로 본다
  const big = (n, t) => Array.from({ length: n }, (_, i) => `${t}${i}`).join('\n');
  const huge = sideBySide(big(2100, 'a'), big(2100, 'b'));
  assert.ok(2100 * 2100 > DIFF_LIMIT);
  assert.equal(huge.length, 2100, 'paired as one changed block');
  assert.ok(huge.every((r) => r.kind === 'change'));
});

test('the store keeps the counts, ignores a failed read, and watches focus and the clock', async () => {
  resetApprovalsStore();
  assert.deepEqual(getApprovalsSnapshot(), { counts: { pending: 0, unapproved: 0 }, loaded: false });
  await refreshApprovalCounts(async () => ({ counts: { pending: 2, unapproved: 1 } }));
  assert.deepEqual(getApprovalsSnapshot().counts, { pending: 2, unapproved: 1 });
  assert.equal(getApprovalsSnapshot().loaded, true);
  await refreshApprovalCounts(async () => {
    throw new Error('엔진이 낡았어요');
  });
  assert.deepEqual(getApprovalsSnapshot().counts, { pending: 2, unapproved: 1 }, 'a failed read leaves the last counts alone');
  setApprovalCounts({ pending: 0, unapproved: 0 });
  assert.equal(badgeText(getApprovalsSnapshot().counts), '');
  // 지켜보기: 바로 한 번 읽고, 창이 눈에 띌 때마다 읽고, 끄면 멈춘다
  resetApprovalsStore();
  let reads = 0;
  const handlers = new Map();
  const target = { addEventListener: (t, f) => handlers.set(t, f), removeEventListener: (t) => handlers.delete(t) };
  const stop = startApprovalsWatch(async () => {
    reads++;
    return { counts: { pending: reads, unapproved: 0 } };
  }, target);
  await new Promise((r) => setTimeout(r, 5));
  assert.equal(reads, 1, 'reads once right away');
  handlers.get('focus')();
  await new Promise((r) => setTimeout(r, 5));
  assert.equal(reads, 2, 'reads again when the window comes back');
  assert.equal(getApprovalsSnapshot().counts.pending, 2);
  stop();
  assert.equal(handlers.size, 0, 'stopping removes the listener');
  assert.ok(POLL_MS >= 30_000, 'a slow background poll: every read starts a JVM');
  resetApprovalsStore();
});

const baseProps = (extra = {}) => ({
  overview: overview(), tab: 'pending', onTab() {}, details: {}, open: new Set(), onToggle() {}, names: {}, busy: null, rejecting: null, rejectText: '', onRejectText() {},
  onRejectOpen() {}, onApprove() {}, onReject() {}, onResolve() {}, onPolicy() {}, error: '', ...extra,
});
const render = (props) => renderToStaticMarkup(React.createElement(ApprovalsView, baseProps(props))).replace(/<!-- -->/g, '');

test('the pending tab: summary, who and why, the fixed mark, and approve/reject/compare buttons', () => {
  const html = render();
  assert.match(html, /role="tablist"/);
  assert.match(html, /승인 대기<span class="apv-count">1<\/span>/);
  assert.match(html, /data-change-id="pcmw1-aaaaaaaa"/);
  assert.match(html, /프로젝트 에이전트 &#x27;웹 담당&#x27; 고치기\(권한 read → publish\)/);
  assert.match(html, /Claude 세션 abcdef12가 올림/);
  assert.match(html, /class="apv-chip apv-fixed"[^>]*>권한 올림/, 'a category that is always approved is marked');
  for (const label of ['이전 판과 비교', '거절', '승인']) assert.match(html, new RegExp(`>${label}</button>`));
  assert.doesNotMatch(html, /apv-compare/, 'the comparison is folded until asked');
  assert.match(render({ overview: overview({ pending: [], counts: { pending: 0, unapproved: 0 } }) }), /승인을 기다리는 변경이 없어요/);
  assert.match(render({ error: '다른 곳에서 먼저 고쳤어요' }), /role="alert" class="apv-error">다른 곳에서 먼저 고쳤어요/);
});

test('an opened change shows the old and new version side by side; a load in progress says so', () => {
  const detail = toDetail({ ...change(), before: view(), after: view({ permission: 'publish', body: '원래 지침\n바뀐 줄' }), changedFields: ['permission', 'body'] });
  const html = render({ open: new Set(['pcmw1-aaaaaaaa']), details: { 'pcmw1-aaaaaaaa': detail } });
  assert.match(html, /aria-expanded="true">비교 접기/);
  assert.match(html, /data-field="permission"/);
  assert.match(html, /<del>read<\/del>[\s\S]*<ins>publish<\/ins>/);
  assert.match(html, /aria-label="줄 단위 비교"/);
  assert.match(html, /<th scope="col">이전 판<\/th><th scope="col">새 판<\/th>/);
  assert.match(html, /apv-row-same/);
  assert.match(html, /apv-row-change/);
  assert.match(render({ open: new Set(['pcmw1-aaaaaaaa']), details: { 'pcmw1-aaaaaaaa': 'loading' } }), /불러오는 중이에요/);
  const created = toDetail({ ...change({ summary: '새 스킬 만들기' }), after: view({ kind: 'skill', permission: undefined, skills: undefined }), changedFields: [] });
  assert.match(renderToStaticMarkup(React.createElement(Compare, { before: created.before, after: created.after, note: '새로 만들어요.' })), /새로 만들어요\./);
  assert.match(renderToStaticMarkup(React.createElement(Compare, { before: view(), after: view() })), /달라진 칸이 없어요/);
});

test('a reference change lists the resources it turns on and off by name', () => {
  const detail = toDetail({ ...change({ summary: "프로젝트 '앱'의 글로벌 자원 참조 1개 켜기", command: 'set-resource-ref', fixed: true }), ref: { project: '앱', added: ['sk1'], removed: ['sk2'] }, changedFields: [] });
  const html = render({ open: new Set(['pcmw1-aaaaaaaa']), details: { 'pcmw1-aaaaaaaa': detail }, names: { sk1: '번역 검수' } });
  assert.match(html, /프로젝트 「앱」의 글로벌 자원 참조를 바꿔요/);
  assert.match(html, /<ins>번역 검수<\/ins>/);
  assert.match(html, /<del>sk2<\/del>/, 'an unknown id is shown as it is');
});

test('rejecting asks for a reason first and locks the buttons while working', () => {
  const html = render({ rejecting: 'pcmw1-aaaaaaaa', rejectText: '안 돼요' });
  assert.match(html, /<span>거절 이유\(선택\)<\/span>/);
  assert.match(html, /<textarea[^>]*>안 돼요<\/textarea>/);
  assert.match(html, />거절하기<\/button>/);
  assert.doesNotMatch(html, />이전 판과 비교<\/button>/, 'the reject form replaces the usual buttons');
  const busy = render({ busy: 'pcmw1-aaaaaaaa' });
  assert.match(busy, /disabled=""[^>]*>거절<\/button>/);
  assert.match(busy, /disabled=""[^>]*>승인<\/button>/);
});

test('the direct-edit tab names what happened, offers revert or adopt, and compares by field or by id', () => {
  const o = overview({
    unapproved: [
      toUnapproved({ key: 'ap1-aaaaaaaa', type: 'resource', state: 'dirty', id: 'ap1-aaaaaaaa', kind: 'agent', scope: 'project', project: '앱', name: '웹 담당', approved: view(), live: view({ permission: 'publish' }) }),
      toUnapproved({ key: 'skmwhand000-0000abcd', type: 'resource', state: 'new', id: 'skmwhand000-0000abcd', kind: 'skill', scope: 'global', name: '손으로', live: view({ id: 'skmwhand000-0000abcd', kind: 'skill', scope: 'global', name: '손으로' }) }),
      toUnapproved({ key: 'refs:앱', type: 'refs', state: 'dirty', project: '앱', approved: ['sk1'], live: ['sk1', 'sk2'] }),
    ],
    counts: { pending: 1, unapproved: 3 },
  });
  const html = render({ overview: o, tab: 'direct', open: new Set(['ap1-aaaaaaaa', 'refs:앱']), names: { sk2: '몰래 켠 스킬' } });
  assert.match(html, /직접 수정<span class="apv-count">3<\/span>/);
  assert.match(html, /승인한 뒤에 파일이 직접 고쳐졌어요\. 지금 쓰이는 것은 승인해 둔 판이에요\./);
  assert.match(html, /승인 기록 없이 생긴 새 파일이에요\. 승인하기 전에는 어디에서도 쓰이지 않아요\./);
  assert.match(html, />승인한 판으로 되돌리기<\/button>/);
  assert.match(html, />지우기\(휴지통으로\)<\/button>/, 'a new file is reverted by trashing it');
  assert.match(html, />이 내용을 승인<\/button>/);
  assert.match(html, /<del>read<\/del>[\s\S]*<ins>publish<\/ins>/, 'the approved and the live version are compared');
  assert.match(html, /프로젝트 「앱」의 참조 목록/);
  assert.match(html, /<ins>몰래 켠 스킬<\/ins>/, 'the extra reference is listed as turned on');
  assert.match(render({ tab: 'direct' }), /승인 안 된 직접 수정이 없어요/);
});

test('the decided tab shows the outcome and the reason; the policy tab has four selects and the fixed note', () => {
  const o = overview({ decided: [toChange(change({ status: 'rejected', decidedAt: '2026-11-14T02:00:00+09:00', reason: '권한을 이렇게 올리면 안 된다' })), toChange(change({ id: 'pcmw3-cccccccc', status: 'stale' }))] });
  const html = render({ overview: o, tab: 'decided' });
  assert.match(html, /data-status="rejected"/);
  assert.match(html, /apv-status-rejected">거절함/);
  assert.match(html, /apv-status-stale">적용 못 함/);
  assert.match(html, /권한을 이렇게 올리면 안 된다/);
  assert.match(html, /프로젝트 앱 · 에이전트 · 웹 담당/);
  assert.match(render({ tab: 'decided' }), /아직 결정한 변경이 없어요/);
  const policy = render({ tab: 'policy' });
  assert.equal((policy.match(/<select /g) ?? []).length, 4);
  for (const row of POLICY_ROWS) assert.ok(policy.includes(row.label), row.label);
  assert.match(policy, /<option value="approve" selected="">승인 필요<\/option>/);
  assert.match(policy, /<option value="auto" selected="">바로 적용<\/option>/, 'the project row is on auto by default');
  assert.ok(policy.includes(FIXED_NOTE));
});

test('the engine rows carry the dirty mark to the list, the rail shows the badge, and the page shows the banner', () => {
  assert.equal(toResource({ id: 'sk1', kind: 'skill', scope: 'global', name: 'a', approval: 'dirty' }).approval, 'dirty');
  assert.equal(toResource({ id: 'sk1', kind: 'skill', scope: 'global', name: 'a', approval: 'whatever' }).approval, undefined);
  const controller = {
    navigate() {}, form() {}, filterAgendas() {}, preference() {}, settings() {}, launcher() {}, callEngine: async () => ({}), isNative: () => true,
  };
  const state = { workspace: { path: '/vault', name: 'v', sample: false, today: '2026-11-14', notes: [], templates: [], trash: [], skipped: [], conflicts: [], noteReadFailed: false, agendas: [], projects: [] }, prefs: { collapsedProjects: [] }, hiddenAgendas: [], theme: 'dark', route: '/resources' };
  resetApprovalsStore();
  assert.doesNotMatch(renderToStaticMarkup(React.createElement(Rail, { controller, state })), /nav-badge/, 'no badge while nothing waits');
  setApprovalCounts({ pending: 2, unapproved: 1 });
  const rail = renderToStaticMarkup(React.createElement(Rail, { controller, state })).replace(/<!-- -->/g, '');
  assert.match(rail, /class="nav-item active">스킬·에이전트<span class="nav-badge" role="img" aria-label="승인이 필요한 변경 3개">3<\/span>/);
  const page = renderToStaticMarkup(React.createElement(ResourcesPage, { controller })).replace(/<!-- -->/g, '');
  assert.match(page, /승인이 필요한 변경이 3건 있어요\./);
  assert.match(page, />승인 대기함 열기<\/button>/);
  assert.match(page, />승인 대기함 3<\/button>/);
  resetApprovalsStore();
  const quiet = renderToStaticMarkup(React.createElement(ResourcesPage, { controller }));
  assert.doesNotMatch(quiet, /apv-banner/, 'no banner while nothing waits');
  assert.match(quiet, />승인 대기함<\/button>/, 'the button is always there');
});
