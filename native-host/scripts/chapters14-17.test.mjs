import test from 'node:test';
import assert from 'node:assert/strict';
import { extractLogicalLines, calculateBacktickFence, formatPinnedNoteBody } from '../src/ui/terminal/logicalLines.ts';
import { readPreferences, writePreferences, readNoteWindowGeometry, writeNoteWindowGeometry, KEYS, homePath, readTheme, readSampleTemplates, writeSampleTemplates } from '../src/workspace/storage.ts';
import { CLAUDE_ENABLED, waitingAnswers, unsavedAnswers } from '@claude-plug/state';
import { planSessionShell } from '@claude-plug/terminal';
import { NoteAskFrame, EditorAskPlugins, AskComposePopover, AskHoverCard } from '@claude-plug/ask';
import { NoteSessionRail, SessionRoute } from '@claude-plug/session';
import { RailProjects } from '@claude-plug/projects';
import { createHeadlessEditor } from '@lexical/headless';
import { EDITOR_NODES, $importDocument, $exportDocument } from '../src/editor/markdown.ts';
import { OPEN_ASK_COMMAND } from '../src/editor/tools.tsx';

// Mock in-memory storage port
function createStorage(initial = {}) {
  const map = new Map(Object.entries(initial));
  return {
    getItem: (key) => map.has(key) ? map.get(key) : null,
    setItem: (key, val) => map.set(key, val),
    dump: () => Object.fromEntries(map.entries()),
  };
}

// --------------------------------------------------------------------------
// Chapter 14: Terminal logical lines, backtick fence, pinned note formatting
// --------------------------------------------------------------------------

test('Chapter 14: extractLogicalLines trims trailing blank lines and joins correctly', () => {
  const raw = [
    'line 1',
    'line 2  ',
    '',
    'line 4',
    '',
    '',
  ];
  const lines = extractLogicalLines(raw);
  assert.equal(lines.length, 4);
  assert.equal(lines[0], 'line 1');
  assert.equal(lines[1], 'line 2');
  assert.equal(lines[2], '');
  assert.equal(lines[3], 'line 4');
});

test('Chapter 14: calculateBacktickFence respects max backticks + 1, min 3', () => {
  assert.equal(calculateBacktickFence('hello world').length, 3);
  assert.equal(calculateBacktickFence('code: ```python\nprint(1)\n```').length, 4);
  assert.equal(calculateBacktickFence('nested: ````markdown\n```\n````').length, 5);
});

test('Chapter 14: formatPinnedNoteBody adheres to callouts, summary table, and fence specifications', () => {
  const result = formatPinnedNoteBody({
    title: '테스트기록',
    linesSent: 150,
    masked: {
      text: 'echo "hello"\nsecret_masked_text',
      hits: 3,
      maskedValues: 2,
      tableSize: 10,
      short: ['KEY'],
      unreadable: ['PASS'],
      shapes: [{ label: 'AWS_KEY', count: 1 }],
    },
    hadTui: true,
  });

  // 1. Heading
  assert.ok(result.startsWith('# 테스트기록\n'));

  // 2. Summary Table
  assert.ok(result.includes('| 무엇 | 값 |'));
  assert.ok(result.includes('| 굳힌 논리 줄 | 150 |'));
  assert.ok(result.includes('| 가린 자리 | 3 |'));
  assert.ok(result.includes('| 가린 값 종수 | 2 |'));
  assert.ok(result.includes('| 표에 있던 값 | 10종 |'));
  assert.ok(result.includes('| 모양으로 걸린 것 | 1건 |'));

  // 3. Callout ordering: TUI, short, unreadable, shapes
  const tuiIdx = result.indexOf('>[!warning] 굳힐 때 TUI가 떠 있었습니다');
  const shortIdx = result.indexOf('>[!warning] 짧아서 못 지킨 이름 1개');
  const unreadableIdx = result.indexOf('>[!danger] 값을 못 읽은 이름 1개');
  const shapesIdx = result.indexOf('>[!danger] 표에 없는데 비밀처럼 생긴 것 1건');

  assert.ok(tuiIdx !== -1, 'TUI callout present');
  assert.ok(shortIdx !== -1, 'Short callout present');
  assert.ok(unreadableIdx !== -1, 'Unreadable callout present');
  assert.ok(shapesIdx !== -1, 'Shapes callout present');

  assert.ok(tuiIdx < shortIdx, 'TUI precedes short');
  assert.ok(shortIdx < unreadableIdx, 'short precedes unreadable');
  assert.ok(unreadableIdx < shapesIdx, 'unreadable precedes shapes');

  // 4. Fence and body
  assert.ok(result.includes('```text\necho "hello"\nsecret_masked_text\n```\n'));
});

test('Chapter 14: formatPinnedNoteBody adds success callout when shapes is empty', () => {
  const result = formatPinnedNoteBody({
    title: '클린기록',
    linesSent: 50,
    masked: {
      text: 'clear',
      hits: 0,
      maskedValues: 0,
      tableSize: 5,
      short: [],
      unreadable: [],
      shapes: [],
    },
    hadTui: false,
  });

  assert.ok(result.includes('>[!success] 모양 검사에 걸린 것이 0건입니다'));
});

// --------------------------------------------------------------------------
// Chapter 16: Browser Storage, Preferences 21 fields, bounds, geometry
// --------------------------------------------------------------------------

test('Chapter 16: readPreferences produces 21 exact fields with valid defaults', () => {
  const storage = createStorage();
  const prefs = readPreferences(storage);

  assert.equal(prefs.theme, 'light');
  assert.equal(prefs.weekLayout, 'swimlane');
  assert.equal(prefs.railWidth, 264);
  assert.equal(prefs.detailListWidth, 340);
  assert.equal(prefs.docWidth, 820);
  assert.equal(prefs.noteRailWidth, 392);
  assert.equal(prefs.previewWidth, 720);
  assert.equal(prefs.previewHeight, null);
  assert.equal(prefs.askCardWidth, 360);
  assert.equal(prefs.askCardHeight, null);
  assert.equal(prefs.composeWidth, 280);
  assert.equal(prefs.composeInputHeight, null);
  assert.equal(prefs.docSortMode, 'structure');
  assert.equal(prefs.docSortDir, 'desc');
  assert.equal(prefs.titleDatePrefix, true);
  assert.equal(prefs.showRecordTier, false);
  assert.equal(prefs.mermaidShowSource, false);
  assert.equal(prefs.askTimeoutSeconds, 3600);
  assert.equal(prefs.sessionPreviewOpen, true);
  assert.deepEqual(prefs.collapsedProjects, []);
  assert.deepEqual(prefs.expandedSessionProjects, []);
});

test('Chapter 16: readPreferences clamps out-of-range numerical fields and options', () => {
  const storage = createStorage({
    [KEYS.prefs]: JSON.stringify({
      state: {
        railWidth: 9999, // clamp to 420
        docWidth: 100, // clamp to 640
        askTimeoutSeconds: 500, // not in allowed set, resets to 3600
        weekLayout: 'invalid', // resets to swimlane
        docSortMode: 'unknown', // resets to structure
        docSortDir: 'backwards', // resets to desc
        customUnknownField: 'preserved',
      },
      version: 1,
    }),
  });

  const prefs = readPreferences(storage);
  assert.equal(prefs.railWidth, 420);
  assert.equal(prefs.docWidth, 640);
  assert.equal(prefs.askTimeoutSeconds, 3600);
  assert.equal(prefs.weekLayout, 'swimlane');
  assert.equal(prefs.docSortMode, 'structure');
  assert.equal(prefs.docSortDir, 'desc');
  // Unknown field preserved
  assert.equal(prefs.customUnknownField, 'preserved');
});

test('Chapter 16: writeNoteWindowGeometry preserves decimal floating points without rounding', () => {
  const storage = createStorage();
  writeNoteWindowGeometry(storage, {
    width: 900.5,
    height: 820.75,
    x: 450.2,
    y: 167.8,
  });

  const saved = readNoteWindowGeometry(storage);
  assert.equal(saved.width, 900.5);
  assert.equal(saved.height, 820.75);
  assert.equal(saved.x, 450.2);
  assert.equal(saved.y, 167.8);
});

test('Chapter 16: homePath rejects ~/Documents and requires leading slash', () => {
  assert.equal(homePath('~/Documents'), null);
  assert.equal(homePath('relative/path'), null);
  assert.equal(homePath('/valid/absolute/path'), '/valid/absolute/path');
  assert.equal(homePath('  /trimmed/path/  '), '/trimmed/path');
});

// --------------------------------------------------------------------------
// Chapter 17: Part 4 Plug stubs, 5 bundles, 15 exports
// --------------------------------------------------------------------------

test('Chapter 17: Plugs stub exports in Part 4-free build', async () => {
  // 1. state
  assert.equal(CLAUDE_ENABLED, false);
  assert.equal(waitingAnswers(), 0);
  assert.equal(unsavedAnswers(), 0);

  // 2. terminal
  const plan = await planSessionShell('term:main');
  assert.deepEqual(plan, { kind: 'plain' });

  // 3. ask
  assert.equal(NoteAskFrame({ children: 'child' }), 'child');
  assert.equal(EditorAskPlugins(), null);
  assert.equal(AskComposePopover(), null);
  assert.equal(AskHoverCard(), null);

  // 4. session
  assert.equal(NoteSessionRail(), null);
  assert.ok(SessionRoute());

  // 5. projects
  assert.equal(RailProjects(), null);
});

test('Chapter 17: Q&A marker wrap with turnId and OPEN_ASK_COMMAND', () => {
  assert.ok(OPEN_ASK_COMMAND);
  const editor = createHeadlessEditor({ namespace: 'ask-test', nodes: EDITOR_NODES, onError: e => { throw e; } });
  const turnId = `ak${Date.now().toString(36)}-abcdef12`;
  const markdown = `# 질문 테스트\n\n<!--wla:${turnId}-->선택한 본문 질문 문장<!--/wla:${turnId}-->\n`;
  editor.update(() => {
    $importDocument(markdown);
  }, { discrete: true });
  const exported = editor.getEditorState().read($exportDocument);
  assert.equal(exported.trim(), markdown.trim());
});

test('목록 18번(C157): 테마의 처음 값은 light이고 모르는 글자도 그대로 data-theme에 건다',()=>{
  assert.equal(readTheme(createStorage()),'light');
  assert.equal(readTheme(createStorage({[KEYS.prefs]:JSON.stringify({state:{theme:'dark'},version:1})})),'dark');
  assert.equal(readTheme(createStorage({[KEYS.prefs]:JSON.stringify({state:{theme:'purple'},version:1})})),'purple');
  assert.equal(readTheme(createStorage({[KEYS.prefs]:JSON.stringify({state:{theme:3},version:1})})),'light');
});

test('목록 19번(16.3): 판이 1이 아닌 수일 때만 이관해 곧바로 판 1로 다시 쓰고, 판이 없거나 수가 아니면 그대로 읽고 안 쓴다',()=>{
  for(const version of [undefined,null,'1','x']){
    const raw=JSON.stringify({state:{railWidth:9999,askTimeoutSeconds:1200},...(version===undefined?{}:{version})}),storage=createStorage({[KEYS.prefs]:raw});
    const prefs=readPreferences(storage,true);
    assert.equal(storage.getItem(KEYS.prefs),raw,'rewrote a '+String(version)+' version');assert.equal(prefs.railWidth,420);
  }
  const old=createStorage({[KEYS.prefs]:JSON.stringify({state:{railWidth:9999,askTimeoutSeconds:1200},version:0})});
  assert.equal(readPreferences(old,true).askTimeoutSeconds,3600);
  assert.deepEqual(JSON.parse(old.getItem(KEYS.prefs)),{state:{railWidth:9999},version:1},'written from the values before the reading corrections');
  const newer=createStorage({[KEYS.prefs]:JSON.stringify({state:{askTimeoutSeconds:1200,railWidth:300},version:2})});
  readPreferences(newer,true);assert.deepEqual(JSON.parse(newer.getItem(KEYS.prefs)),{state:{askTimeoutSeconds:1200,railWidth:300},version:1});
  const one=createStorage({[KEYS.prefs]:JSON.stringify({state:{railWidth:9999},version:1})});readPreferences(one,true);assert.equal(JSON.parse(one.getItem(KEYS.prefs)).state.railWidth,9999);
  const noteWindow=createStorage({[KEYS.prefs]:JSON.stringify({state:{},version:0})});readPreferences(noteWindow,false);assert.equal(JSON.parse(noteWindow.getItem(KEYS.prefs)).version,0);
});

test('목록 21번(16.10): 샘플의 템플릿 키는 {overrides, custom} 꼴이고 옛 배열도 읽는다',()=>{
  const storage=createStorage();
  writeSampleTemplates(storage,[{id:'tpl-a',category:'doc',name:'이름',type:'free',desc:'',body:'본문',format:'markdown',builtin:false,path:'x'},{id:'rit',category:'ritual',name:'리추얼'}]);
  assert.deepEqual(JSON.parse(storage.getItem(KEYS.sampleTemplates)),{overrides:{},custom:[{id:'tpl-a',name:'이름',type:'free',desc:'',body:'본문',format:'markdown'}]});
  assert.deepEqual(readSampleTemplates(storage),[{id:'tpl-a',name:'이름',type:'free',desc:'',body:'본문',format:'markdown'}]);
  const legacy=createStorage({[KEYS.sampleTemplates]:JSON.stringify([{id:'t1',name:'n'},{name:'id 없음'},7])});
  assert.deepEqual(readSampleTemplates(legacy),[{id:'t1',name:'n'}]);assert.deepEqual(readSampleTemplates(createStorage({[KEYS.sampleTemplates]:'{broken'})),[]);
});
