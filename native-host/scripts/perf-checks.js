/* 실제 WKWebView와 릴리스 호스트로, 노트가 많을 때 앱이 노트를 읽어 오는 시간을 잰다(이슈 #59). 통과·실패를 따지지 않고 숫자를 찍는다.
   엔진 단독 기준선(`perf_baseline`)에 없는 앱 쪽 값이다: 호스트를 거친 호출 시간(IPC와 JSON 변환), 응답 크기, 앱 열기, 화면 검색 반응, 호스트 메모리. */
const cfg = window.__AIS_FIXTURE__;
const N = cfg.perf.notes;
const invoke = (...args) => window.__TAURI_INTERNALS__.invoke(...args);
const assert = (condition, message) => { if (!condition) throw Error(message || 'assertion failed'); };
const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
async function until(predicate, what = '화면 상태', timeout = 240000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) { if (await predicate()) return; await wait(40); }
  throw Error(`timed out: ${what} (${location.pathname})`);
}
const post = (path, value) => fetch(path, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(value) });
const rss = async () => (await (await fetch('/rss')).json()).hostMb;
const median = values => { const sorted = [...values].sort((a, b) => a - b); return sorted[Math.floor(sorted.length / 2)]; };
const round = value => Math.round(value * 10) / 10;
const frame = () => new Promise(resolve => requestAnimationFrame(() => setTimeout(resolve, 0)));

const results = [];
async function test(id, work) {
  try { const detail = await work(); results.push({ id, ok: true, detail }); }
  catch (error) { results.push({ id, ok: false, error: error instanceof Error ? String(error) : JSON.stringify(error) }); }
  await post('/progress', results.at(-1));
  if (!results.at(-1).ok) throw Error(id);
}
// 한 호출을 n번 재서 첫 값, 가운데 값, 응답 크기(UTF-8 바이트)를 낸다
async function timeCall(command, args, n = 5) {
  const times = []; let bytes = 0, count = 0;
  for (let i = 0; i < n; i++) {
    const start = performance.now();
    const value = await invoke('engine_call', { command, args });
    times.push(performance.now() - start);
    if (i === 0) { bytes = new TextEncoder().encode(JSON.stringify(value)).length; count = Array.isArray(value) ? value.length : Array.isArray(value?.items) ? value.items.length : undefined; }
  }
  return { first: round(times[0]), median: round(median(times)), kb: Math.round(bytes / 1024), count };
}

try {
  await test('app-boot-until-the-note-list-is-ready', async () => {
    await until(() => document.querySelector('[data-rail-section="nav"]') && document.querySelector('.workspace-shell')?.getAttribute('aria-busy') === 'false', '앱이 뜸');
    const ready = performance.now();
    const hostMb = await rss();
    // 창이 가려지면(visibility hidden) WebKit이 타이머와 화면 갱신을 늦춰서 재는 값이 크게 부푼다. visible이 아니면 다시 잰다.
    return { notes: N, visibility: document.visibilityState, bootMs: Math.round(ready), hostRssMb: Math.round(hostMb) };
  });
  await test('boot-call-timeline', async () => {
    // 앱이 열릴 때 부른 엔진 호출의 시작 시각(첫 호출 기준 ms). 이웃한 시작 사이가 앞 묶음이 걸린 시간이다. 끝 시각은 기록에 없다.
    const log = (await (await fetch('/calls')).json()).filter(call => call.start);
    const first = log[0]?.start ?? 0;
    return { calls: log.length, timeline: log.slice(0, 24).map(call => `${call.args.argv[0]}@${Math.round(call.start - first)}`) };
  });
  // 앱이 열릴 때 init-workspace 다음 호출까지 걸린 시간에서 이 값을 빼면, 그 사이에 화면이 한 일(옛 노트 이관의 폴더 훑기, `migrateLegacy`)의 시간이 남는다.
  await test('ipc-init-workspace-as-the-screen-calls-on-open', async () => timeCall('init_workspace', { name: '검증 공간', templates: '[]' }, 3));
  await test('ipc-list-docs-with-bodies-as-the-screen-reads-on-refresh', async () => timeCall('list_docs', { limit: 0, bodies: true }));
  await test('ipc-list-docs-metadata-only', async () => timeCall('list_docs', { limit: 0 }));
  await test('ipc-search-docs-common-word', async () => timeCall('search_docs', { query: '배포' }));
  await test('ipc-search-docs-rare-word', async () => timeCall('search_docs', { query: '희귀어' }));
  await test('ipc-get-doc-one-note', async () => timeCall('get_doc', { id: cfg.perfTarget }));
  await test('screen-quick-search-latency', async () => {
    // Cmd+K로 빠른 검색을 열고 글자를 친다. 검색어가 없을 때도 결과 줄이 떠 있으므로 「줄이 있다」로는 끝을 못 안다.
    // 입력 이벤트의 동기 계산 시간(React가 findNotes를 돌린 시간)과, 결과에 검색어가 든 맥락 글(.cmdk-snippet)이 뜰 때까지의 전체 시간을 따로 잰다.
    // 첫 검색은 노트마다 본문을 평문으로 만든 값을 채운다.
    const visibility = document.visibilityState;
    const idleFrame = await (async () => { const start = performance.now(); await frame(); return round(performance.now() - start); })();
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'k', metaKey: true, bubbles: true }));
    await until(() => document.querySelector('.cmdk-input'), '빠른 검색');
    const input = document.querySelector('.cmdk-input');
    const type = async value => {
      const start = performance.now();
      Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value').set.call(input, value);
      input.dispatchEvent(new Event('input', { bubbles: true }));
      const syncMs = round(performance.now() - start);
      await until(() => [...document.querySelectorAll('.cmdk-snippet')].some(node => node.textContent.includes(value)), `${value} 맥락 글`, 600000);
      return { syncMs, totalMs: round(performance.now() - start) };
    };
    const first = await type('희귀어');
    const warm = [];
    for (const word of ['배포', '리뷰', '위험', '링크']) warm.push(await type(word));
    document.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', bubbles: true }));
    return { visibility, idleFrameMs: idleFrame, first, warmTotalMedianMs: round(median(warm.map(w => w.totalMs))), warm };
  });
  await test('host-memory-after-the-reads', async () => {
    // 재는 동안 창이 가려졌다면 값을 믿을 수 없으니 통과가 아니라 실패로 알린다
    assert(document.visibilityState === 'visible', `재는 동안 창이 가려졌다(${document.visibilityState}). 창을 앞에 둔 채 다시 잰다`);
    return { hostRssMb: Math.round(await rss()), visibility: document.visibilityState };
  });
} catch { /* 첫 실패 뒤는 재지 않는다 */ }
await post('/result', results);
