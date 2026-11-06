// Compiled only into the isolated WKWebView fixture, never into the product.
import { WorkspaceController } from '../src/workspace/controller.ts';
import { workspaceServices } from '../src/workspace/platform.ts';
import { windowContext, asRow, asText } from '../src/workspace/types.ts';
import type { Row } from '../src/workspace/types.ts';
import { read } from '../src/data/commands.ts';
import { createElement } from 'react';
import { createRoot } from 'react-dom/client';
import { VaultBanner } from '../src/data/VaultBanner.tsx';

const cfg = (window as unknown as { __AIS_FIXTURE__: Record<string, string> }).__AIS_FIXTURE__;
const assert = (value: unknown, message: string) => { if (!value) throw new Error(message); };
const pause = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));
async function until(predicate: () => unknown | Promise<unknown>, label: string) {
  const deadline = Date.now() + 25000;
  while (Date.now() < deadline) { if (await predicate()) return; await pause(40); }
  throw new Error('timed out: ' + label);
}
const post = (path: string, value: unknown) => fetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(value) });
const services = workspaceServices(), invoke = services.host.invoke, calls: string[][] = [];
let offset = 0, intercept: ((argv: string[], result: unknown) => Promise<unknown>) | undefined;
services.now = () => new Date(Date.now() + offset);
services.host.invoke = async <T>(command: string, args?: Record<string, unknown>): Promise<T> => {
  const argv = command === 'sidecar_call' ? args?.argv as string[] : undefined;
  if (argv) calls.push([...argv]);
  const result = await invoke<T>(command, args);
  return argv && intercept ? await intercept(argv, result) as T : result;
};
const controller = new WorkspaceController(services, windowContext(location.pathname), location.pathname);
const data = controller.data, current = () => controller.getSnapshot().workspace!, root = () => current().notes.find(note => note.id === cfg.id)!;

if (location.pathname.startsWith('/note-window/')) {
  try {
    await controller.boot();
    assert(calls.every(argv => argv[0] !== 'reindex'), 'note boot reindexed');
    const before = calls.length;
    await post('/note-result', { ready: true });
    await until(() => data.selfWrites.has(`${cfg.raw}/${root().path}`), 'cross-window own-write receipt');
    // The receiving window records the write; it does not reconcile on receipt.
    assert(calls.length === before, 'receipt triggered reconciliation');
    await controller.refresh();
    assert(root().body.includes('다른 창으로 알린 본문'), 'focused note did not catch up');
    await post('/note-result', { ok: true, body: root().body });
    await controller.closeNoteWindow();
  } catch (error) { await post('/note-result', { ok: false, error: String(error) }); }
} else {
  const banner = document.createElement('div'); document.body.append(banner);
  const view = createRoot(banner);
  const unrender = controller.subscribe(() => view.render(createElement(VaultBanner, { state: controller.getSnapshot() })));
  const results: { id: string; ok: boolean; error?: string }[] = [];
  const test = async (id: string, work: () => Promise<void>) => {
    try { await work(); results.push({ id, ok: true }); }
    catch (error) { results.push({ id, ok: false, error: String(error) }); }
    await post('/progress', results.at(-1));
    document.getElementById('probe')!.textContent = results.map(result => `${result.ok ? 'PASS' : 'FAIL'} ${result.id}`).join('\n');
    if (!results.at(-1)!.ok) throw new Error(id);
  };
  const externalEdit = async (text: string) => {
    const path = `${cfg.raw}/${asText(root().path)}/note.md`;
    const previous = await services.files.readTextFile(path);
    const end = /^---\r?\n[\s\S]*?\r?\n---(?:\r?\n|$)/.exec(previous)![0].length;
    await services.files.writeTextFile(path, previous.slice(0, end) + text);
  };
  try {
    await controller.boot();
    await test('real-boot-token-migration-and-report-sanitization', async () => {
      assert(root().body.includes('격리된 vault'), 'body absent'); assert(!!root().precondition, 'first token absent');
      assert(current().notes.some(note => note.id === cfg.legacyId && !asText(note.path).endsWith('.md')), 'migration missing');
      const report = current().reports[0]; assert(asText(report?.body).includes('안전한 회고') && !asText(report?.body).includes('<script>'), 'HTML not sanitized');
    });
    await test('real-fs-watch-applies-external-content', async () => {
      await externalEdit('# 실제 엔진 기록\n\n감시로 받은 바깥 편집\n');
      await until(() => root().body.includes('감시로 받은 바깥 편집'), 'FSEvents reconciliation');
      assert(calls.filter(argv => argv[0] === 'reindex').length >= 2, 'watch did not reindex');
    });
    await test('first-and-second-write-carry-live-engine-tokens', async () => {
      const first = asText(root().precondition), body = '# 실제 엔진 기록\n\n공백 = "따옴표"\n두 번째 줄\n';
      assert((await data.updateNote({ id: cfg.id }, { body })).ok, 'body failed');
      const second = asText(root().precondition); assert(second && first !== second, 'write token did not advance');
      assert((await data.favorite(cfg.id, true)).ok, 'favorite failed');
      const bodyCall = calls.findLast(argv => argv[0] === 'update-doc')!, favorite = calls.findLast(argv => argv[0] === 'set-favorite')!;
      assert(bodyCall.at(-1) === 'f1=' + first && favorite.at(-1) === 'f1=' + second, 'queued token sequence wrong');
      const disk = asRow(await controller.callEngine(read.doc(cfg.id)))!; assert(disk.body === body, 'argv altered text');
    });
    await test('own-write-suppresses-watch-reconciliation', async () => {
      await controller.refresh();
      const before = calls.filter(argv => argv[0] === 'reindex').length;
      assert((await data.updateNote({ id: cfg.id }, { body: '# 실제 엔진 기록\n\n자기 쓰기 감시 확인\n' })).ok, 'own write failed');
      await pause(1100);
      assert(calls.filter(argv => argv[0] === 'reindex').length === before, 'own write caused an extra read');
    });
    await test('external-change-protects-buffer-and-real-status-five', async () => {
      offset += 4000;
      data.edit({ id: cfg.id }, { body: '# 실제 엔진 기록\n\n메모리에 지킬 글\n' });
      const old = root().precondition;
      await externalEdit('# 실제 엔진 기록\n\n디스크에서 먼저 쓴 글\n');
      await until(() => data.getSnapshot().conflicted.includes(cfg.id), 'external conflict');
      assert(root().body.includes('메모리에 지킬 글') && root().precondition === old, 'reconcile overwrote buffer/token');
      const result = await data.updateNote({ id: cfg.id }, { body: root().body });
      assert(result.conflict && !result.ok, 'real engine did not reject stale token');
      await until(() => banner.querySelector('[role="alert"]')?.textContent?.includes('저장 실패'), 'persistent failure banner');
      const disk = asRow(await controller.callEngine(read.doc(cfg.id)))!; assert(asText(disk.body).includes('디스크에서 먼저 쓴 글'), 'external content was overwritten');
      data.clearConflict({ id: cfg.id }); data.protect({ id: cfg.id }, false); await controller.refresh();
      assert(root().body === disk.body, 'disk reload failed');
    });
    await test('real-sub-note-and-comment-round-trip', async () => {
      await data.loadSubNotes(cfg.id);
      const created = await data.createSubNote(cfg.id), id = asText(created.value?.id); assert(created.ok && id, 'sub creation failed');
      assert((await data.updateNote({ parent: cfg.id, id }, { title: '후속 제목', body: '# 후속 제목\n\n본문 = 값\n' })).ok, 'sub update failed');
      const sub = asRow(await controller.callEngine(read.subNote(cfg.id, id)))!; assert(asText(sub.body).includes('본문 = 값'), 'sub text absent');
      const comment = await data.comment({ id: cfg.id }, 'create', ' 코멘트 본문 '), cid = asText(comment.value?.id); assert(comment.ok, 'comment create failed');
      assert((await data.comment({ id: cfg.id }, 'update', '고친 코멘트', cid)).ok, 'comment update failed');
      const list = asRow(await controller.callEngine(read.comments(cfg.id)))!; assert((list.comments as Row[]).some(row => row.id === cid && row.body === '고친 코멘트'), 'comment missing');
      assert((await data.comment({ id: cfg.id }, 'delete', '', cid)).ok, 'comment delete failed');
      await data.updateNote({ id: cfg.id }, { body: root().body + `\n[Child](#sub:\u001c${id}\u001f)\n[BOM](#sub:\ufeff${id}\ufeff)\n` });
      assert((await data.deleteSubNote(cfg.id, id)).ok, 'sub delete failed');
      const after = asRow(await controller.callEngine(read.doc(cfg.id)))!;
      assert(root().body === after.body && root().precondition === after.precondition, 'derived card removal diverged from engine bytes/token');
    });
    await test('real-report-facts-and-template-writes', async () => {
      const aid = current().agendas[0].id;
      const facts = await data.loadFacts(aid); assert(!!facts, 'facts read failed');
      assert((await data.saveFacts(aid, ' 새 사실층 ', asText(facts.facts))).ok, 'facts write failed');
      const refusal = await data.saveFacts(aid, 'stale', asText(facts.facts)); assert(refusal.conflict, 'facts lost-update guard absent');
      data.saveReport('rv-morning-' + cfg.today, '회고 본문', '마지막 메모'); await data.flushReports();
      const reports = await controller.callEngine(read.reports()) as Row[]; assert(JSON.stringify(reports).includes('마지막 메모'), 'report not persisted');
      const template = await data.saveTemplate({ name: '검증 템플릿', type: 'free', desc: '', body: '# 본문\n' }); assert(template.ok, 'template write failed');
      assert((await data.deleteTemplate(asText(template.value?.id))).ok, 'template delete failed');
    });
    await test('real-trash-restore-and-permanent-delete', async () => {
      const note = await data.createDoc({ agendaId: current().agendas[0].id, type: 'free', title: '휴지통 검증', body: '# 휴지통 검증\n', date: cfg.today, time: '12:30', tags: [] });
      const id = asText(note.value?.id); assert(note.ok, 'doc creation failed');
      const moved = await data.deleteDoc(id); assert(moved.ok, 'trash failed');
      assert((await data.trashAction(asText(moved.value?.trashId), 'restore')).ok, 'restore failed');
      assert(current().notes.some(note => note.id === id), 'restore returned before note was read');
      const again = await data.deleteDoc(id); assert(again.ok, 'second trash failed');
      assert((await data.trashAction(asText(again.value?.trashId), 'purge')).ok, 'purge failed');
    });
    await test('real-agenda-project-lifecycle', async () => {
      const project = await data.saveEntity('project', { title: '검증 프로젝트', summary: '' });
      assert(project.ok, 'project creation failed');
      const agenda = await data.saveEntity('agenda', { title: '검증 묶음', project: asText(project.value?.slug), summary: '' });
      assert(agenda.ok, 'agenda creation failed');
      assert((await data.saveEntity('agenda', { title: '바꾼 묶음', summary: '설명' }, asText(agenda.value?.id))).ok, 'agenda update failed');
      assert((await data.deleteEntity('project', asText(project.value?.id))).ok, 'project delete failed');
      assert(!current().agendas.find(row => row.id === agenda.value?.id)?.project, 'project detach failed');
      assert((await data.deleteEntity('agenda', asText(agenda.value?.id))).ok, 'agenda delete failed');
    });
    await test('real-read-write-overlap-keeps-new-token', async () => {
      let release!: () => void, received!: () => void;
      const gate = new Promise<void>(resolve => { release = resolve; }), ready = new Promise<void>(resolve => { received = resolve; });
      intercept = async (argv, value) => { if (argv[0] === 'list-docs') { received(); await gate; } return value; };
      const refresh = controller.refresh(); await ready;
      await data.favorite(cfg.id, false); const token = root().precondition;
      release(); await refresh; intercept = undefined;
      assert(root().precondition === token && root().favorite !== true, 'late read rolled token/metadata back: ' + JSON.stringify({ expected: token, actual: root().precondition, favorite: root().favorite }));
    });
    await test('real-cross-window-self-write-and-note-refresh', async () => {
      await services.host.invoke('open_note_window', { docId: cfg.id, title: root().title });
      await until(async () => (await (await fetch('/note-result')).json())?.ready, 'note readiness');
      assert((await data.updateNote({ id: cfg.id }, { body: '# 실제 엔진 기록\n\n다른 창으로 알린 본문\n' })).ok, 'cross-window write failed');
      let reply: {ok?:boolean;error?:string}; await until(async () => { reply = await (await fetch('/note-result')).json(); return reply?.ok !== undefined; }, 'note receipt');
      assert(reply!.ok, reply!.error || 'note check failed');
      await until(async () => !(await services.host.invoke<string[]>('note_windows')).length, 'note cleanup');
    });
  } catch { /* Report the first dependent failure. */ }
  unrender(); view.unmount(); controller.dispose();
  await post('/result', results);
}
