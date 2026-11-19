import test from 'node:test';
import assert from 'node:assert/strict';
import {
  AI_ENGINES,
  MODEL_PRESETS,
  REASONING_EFFORTS,
  DEFAULT_PROCESS_AGENTS,
  DEFAULT_GLOBAL_SKILLS,
  processAgentToSaveInput,
  resourceRowToProcessAgent,
  globalSkillToSaveInput,
  resourceRowToGlobalSkill,
  skillFromForm,
  ENGINE_MODELS,
  MODEL_EFFORTS,
  clampEffort,
  effortInfosFor,
  effortsForModel,
  remainingEngine,
} from '../src/ui/globalResources/types.ts';

test('AI_ENGINES, MODEL_PRESETS and REASONING_EFFORTS cover claude, codex and gemini', () => {
  const engineIds = AI_ENGINES.map((e) => e.id);
  assert.deepEqual(engineIds.sort(), ['claude', 'codex', 'gemini'].sort());

  assert.ok(MODEL_PRESETS.claude.includes('claude-sonnet-5.5'));
  assert.ok(MODEL_PRESETS.claude.includes('claude-fable-5.2'));
  assert.ok(MODEL_PRESETS.codex.includes('gpt-6.1-sol'));
  assert.ok(MODEL_PRESETS.codex.includes('gpt-6-astra'));
  assert.ok(MODEL_PRESETS.codex.includes('gpt-6.2-sol'));
  assert.ok(MODEL_PRESETS.gemini.includes('gemini-3.8-flash'));

  const effortIds = REASONING_EFFORTS.map((e) => e.id);
  assert.ok(effortIds.includes('default'));
  assert.ok(effortIds.includes('low'));
  assert.ok(effortIds.includes('medium'));
  assert.ok(effortIds.includes('high'));
  assert.ok(effortIds.includes('xhigh'));
  assert.ok(effortIds.includes('max'));
});

test('DEFAULT_PROCESS_AGENTS contains 10 agents with valid engines and models', () => {
  assert.equal(DEFAULT_PROCESS_AGENTS.length, 10);
  for (const agent of DEFAULT_PROCESS_AGENTS) {
    assert.ok(['claude', 'codex', 'gemini'].includes(agent.engine), `agent ${agent.name} has valid engine`);
    assert.ok(agent.model.length > 0, `agent ${agent.name} has model`);
    assert.ok(agent.name.length > 0);
    assert.ok(agent.role.length > 0);
    assert.ok(agent.effort, `agent ${agent.name} has effort`);
    // 2순위 폴백 엔진은 1순위와 달라야 함
    if (agent.fallbackEngine) {
      assert.ok(['claude', 'codex', 'gemini'].includes(agent.fallbackEngine));
      assert.notEqual(agent.fallbackEngine, agent.engine, 'fallbackEngine must differ from primary engine');
      assert.ok(agent.fallbackModel && agent.fallbackModel.length > 0);
      assert.ok(agent.fallbackEffort, `agent ${agent.name} has fallbackEffort`);
    }
  }
});

test('processAgentToSaveInput produces an engine-compatible SaveInput with tools.aisurada', () => {
  const agent = DEFAULT_PROCESS_AGENTS[0];
  const saveInput = processAgentToSaveInput(agent);

  assert.equal(saveInput.kind, 'agent');
  assert.equal(saveInput.scope, 'global');
  assert.equal(saveInput.name, agent.name);
  assert.equal(saveInput.permission, 'edit');
  assert.ok(saveInput.tools, 'tools JSON exists');

  const tools = JSON.parse(saveInput.tools);
  assert.equal(tools.aisurada.engine, 'gemini');
  assert.equal(tools.aisurada.model, agent.model);
  assert.equal(tools.aisurada.effort, agent.effort);
  assert.equal(tools.aisurada.role, agent.role);
  assert.equal(tools.aisurada.fallbackEngine, agent.fallbackEngine);
  assert.equal(tools.aisurada.fallbackModel, agent.fallbackModel);
  assert.equal(tools.aisurada.fallbackEffort, agent.fallbackEffort);
  assert.equal(tools.gemini.model, agent.model);
  assert.equal(tools.gemini.effort, agent.effort);
  assert.ok(tools.claude.model);
  assert.equal(tools.claude.effort, agent.fallbackEffort);
  assert.ok(tools.codex.model);
});

test('resourceRowToProcessAgent roundtrips an engine ResourceRow back to ProcessAgent with 2nd priority fallback and efforts', () => {
  const customAgent = {
    ...DEFAULT_PROCESS_AGENTS[1], // ticket agent (1순위 claude)
    engine: 'claude',
    model: 'claude-sonnet-5.5',
    effort: 'xhigh',
    fallbackEngine: 'gemini',
    fallbackModel: 'gemini-3.8-flash',
    fallbackEffort: 'high',
  };
  const saveInput = processAgentToSaveInput(customAgent);

  const fakeRow = {
    id: 'ap-ticket-99',
    kind: 'agent',
    scope: 'global',
    name: saveInput.name,
    description: saveInput.description,
    body: saveInput.body,
    permission: saveInput.permission,
    skills: saveInput.skills,
    tools: JSON.parse(saveInput.tools),
    path: '.aisurada/resources/global/agents/티켓-에이전트.md',
    precondition: 'f1:test-token',
  };

  const reconstructed = resourceRowToProcessAgent(fakeRow, 1);
  assert.equal(reconstructed.id, 'ap-ticket-99');
  assert.equal(reconstructed.name, customAgent.name);
  assert.equal(reconstructed.engine, 'claude');
  assert.equal(reconstructed.model, 'claude-sonnet-5.5');
  assert.equal(reconstructed.effort, 'xhigh');
  assert.equal(reconstructed.fallbackEngine, 'gemini');
  assert.equal(reconstructed.fallbackModel, 'gemini-3.8-flash');
  assert.equal(reconstructed.fallbackEffort, 'high');
  assert.equal(reconstructed.role, customAgent.role);
  assert.equal(reconstructed.f1, 'f1:test-token');
  assert.equal(reconstructed.path, '.aisurada/resources/global/agents/티켓-에이전트.md');
});

test('resourceRowToProcessAgent disallows fallbackEngine when identical to primary engine', () => {
  const invalidFakeRow = {
    id: 'ap-dup-engine',
    kind: 'agent',
    scope: 'global',
    name: '중복 엔진 에이전트',
    description: '동일 엔진 테스트',
    body: '테스트',
    permission: 'edit',
    skills: [],
    tools: {
      aisurada: {
        engine: 'claude',
        model: 'claude-sonnet-5.5',
        fallbackEngine: 'claude', // 1순위와 동일 -> 무효화되어야 함
        fallbackModel: 'claude-opus-5.5',
      },
    },
  };

  const parsed = resourceRowToProcessAgent(invalidFakeRow, 0);
  assert.equal(parsed.engine, 'claude');
  assert.equal(parsed.fallbackEngine, undefined, 'identical fallbackEngine must be dropped');
  assert.equal(parsed.fallbackModel, undefined);
});

test('Gemini 2.5 Pro is no longer offered', () => {
  assert.ok(!MODEL_PRESETS.gemini.includes('gemini-2.5-pro'));
  assert.ok(!ENGINE_MODELS.gemini.some((m) => /2\.5/.test(m.id) || /2\.5/.test(m.label)));
});

test('only the models this account can use are offered', () => {
  // Codex(ChatGPT Plus): Codex CLI가 알려 준 목록(models_cache.json에서 visibility가 list인 것) 그대로, CLI의 우선순위 순서
  assert.deepEqual(MODEL_PRESETS.codex, [
    'gpt-6.2-sol', 'gpt-6.2-astra', 'gpt-6.1-terra', 'gpt-6.1-sol', 'gpt-6-astra', 'gpt-6-sol', 'gpt-6-luna', 'gpt-5.6-sol', 'gpt-5.6-terra', 'gpt-5.6-luna', 'inherit',
  ]);
  for (const hidden of ['gpt-oss-120b', 'gpt-reserve', 'codex-auto-review']) assert.ok(!MODEL_PRESETS.codex.includes(hidden), `${hidden} is not offered`);
  // Gemini(Google AI Pro): 쓰는 것이 확인된 모델만
  assert.deepEqual(MODEL_PRESETS.gemini, ['gemini-3.8-flash', 'inherit']);
  // Claude(Pro): Claude Code 카탈로그의 모델. 모델별 강도 표가 있어야 한다
  assert.deepEqual(MODEL_PRESETS.claude, ['claude-sonnet-5.5', 'claude-opus-5.5', 'claude-fable-5.2', 'claude-haiku-5.5', 'claude-sonnet-4.6', 'claude-opus-4.6', 'inherit']);
  // 새로 목록에 넣은 Claude·Codex·Gemini 모델은 모델별 강도 표도 같이 채워야 한다(엔진 기본으로 두면 안 맞을 수 있다)
  for (const models of Object.values(ENGINE_MODELS)) {
    for (const m of models) if (m.id !== 'inherit') assert.ok(MODEL_EFFORTS[m.id], `${m.id} has its own effort row`);
  }
});

test('every seeded agent uses models this account can use', () => {
  for (const agent of DEFAULT_PROCESS_AGENTS) {
    assert.ok(MODEL_PRESETS[agent.engine].includes(agent.model), `${agent.name}: ${agent.model} is offered for ${agent.engine}`);
    if (agent.fallbackEngine) assert.ok(MODEL_PRESETS[agent.fallbackEngine].includes(agent.fallbackModel), `${agent.name}: ${agent.fallbackModel} is offered for ${agent.fallbackEngine}`);
  }
});

test('effortsForModel lists only what the model supports, with default first', () => {
  // Gemini 3.8 Flash는 low/medium/high 셋뿐이다(Gemini CLI 모델 화면)
  assert.deepEqual(effortsForModel('gemini', 'gemini-3.8-flash'), ['default', 'low', 'medium', 'high']);
  // Codex는 CLI의 모델 목록이 알려 주는 값 그대로다: ultra는 일부 모델에만 있다
  assert.deepEqual(effortsForModel('codex', 'gpt-6.1-sol'), ['default', 'low', 'medium', 'high', 'xhigh', 'max', 'ultra']);
  assert.deepEqual(effortsForModel('codex', 'gpt-6-luna'), ['default', 'low', 'medium', 'high', 'xhigh', 'max']);
  assert.deepEqual(effortsForModel('codex', 'gpt-6.2-astra'), ['default', 'low', 'medium', 'high', 'xhigh', 'max', 'ultra']);
  assert.deepEqual(effortsForModel('codex', 'gpt-5.6-luna'), ['default', 'low', 'medium', 'high', 'xhigh', 'max']);
  // Claude: xhigh는 5.x 모델에만 있고 4.6 모델은 high에서 max로 간다
  assert.deepEqual(effortsForModel('claude', 'claude-sonnet-5.5'), ['default', 'low', 'medium', 'high', 'xhigh', 'max']);
  assert.deepEqual(effortsForModel('claude', 'claude-opus-4.6'), ['default', 'low', 'medium', 'high', 'max']);
  // 상속한 모델은 어떤 모델인지 몰라서 강도를 정할 수 없다
  for (const engine of ['gemini', 'claude', 'codex']) assert.deepEqual(effortsForModel(engine, 'inherit'), ['default']);
  // 표에 없는 모델(직접 입력)은 그 엔진이 받는 값을 쓴다
  assert.deepEqual(effortsForModel('gemini', 'my-own-model'), ['default', 'low', 'medium', 'high']);
  assert.deepEqual(effortsForModel('claude', 'my-own-model'), ['default', 'low', 'medium', 'high', 'xhigh', 'max']);
});

test('every preset model offers default plus known, non-repeating efforts', () => {
  const known = new Set(REASONING_EFFORTS.map((e) => e.id));
  for (const [engine, models] of Object.entries(ENGINE_MODELS)) {
    for (const m of models) {
      const list = effortsForModel(engine, m.id);
      assert.equal(list[0], 'default', `${m.id} starts with default`);
      assert.equal(new Set(list).size, list.length, `${m.id} has no repeats`);
      assert.ok(list.every((id) => known.has(id)), `${m.id} only lists known efforts`);
      assert.deepEqual(effortInfosFor(engine, m.id).map((e) => e.id), list, `${m.id} labels match`);
    }
  }
  for (const id of Object.keys(MODEL_EFFORTS)) {
    assert.ok(Object.values(ENGINE_MODELS).some((ms) => ms.some((m) => m.id === id)), `${id} is a preset model`);
  }
});

test('clampEffort turns a value the model does not support into default', () => {
  assert.equal(clampEffort('gemini', 'gemini-3.8-flash', 'high'), 'high');
  assert.equal(clampEffort('gemini', 'gemini-3.8-flash', 'xhigh'), 'default');
  assert.equal(clampEffort('codex', 'gpt-6.1-sol', 'ultra'), 'ultra');
  assert.equal(clampEffort('codex', 'gpt-6-luna', 'ultra'), 'default');
  assert.equal(clampEffort('claude', 'claude-opus-4.6', 'xhigh'), 'default');
  assert.equal(clampEffort('claude', 'claude-sonnet-5.5', 'bogus'), 'default');
  assert.equal(clampEffort('claude', 'claude-sonnet-5.5', undefined), 'default');
});

test('resourceRowToProcessAgent drops a saved effort that its model cannot use, and keeps one it can', () => {
  const row = (aisurada) => ({ id: 'ap-x', kind: 'agent', scope: 'global', name: '강도 시험', description: 'd', body: 'b', permission: 'edit', skills: [], tools: { aisurada } });
  const flash = resourceRowToProcessAgent(row({ engine: 'gemini', model: 'gemini-3.8-flash', effort: 'xhigh', fallbackEngine: 'codex', fallbackModel: 'gpt-6.1-sol', fallbackEffort: 'ultra' }), 0);
  assert.equal(flash.effort, 'default', 'xhigh is not offered for Gemini 3.8 Flash');
  assert.equal(flash.fallbackEffort, 'ultra', 'ultra is offered for GPT-6.1-Sol');
  const luna = resourceRowToProcessAgent(row({ engine: 'codex', model: 'gpt-6-luna', effort: 'ultra' }), 0);
  assert.equal(luna.effort, 'default');
});

test('every seeded agent uses efforts its models support', () => {
  for (const agent of DEFAULT_PROCESS_AGENTS) {
    assert.equal(clampEffort(agent.engine, agent.model, agent.effort), agent.effort, `${agent.name} effort ${agent.effort} on ${agent.model}`);
    if (agent.fallbackEngine) {
      assert.equal(clampEffort(agent.fallbackEngine, agent.fallbackModel, agent.fallbackEffort), agent.fallbackEffort, `${agent.name} fallback effort ${agent.fallbackEffort} on ${agent.fallbackModel}`);
    }
  }
});

// ---------------------------------------------------------------- 2순위는 필수, 3순위는 선택

test('remainingEngine gives the one engine that neither the 1st nor the 2nd tier uses', () => {
  assert.equal(remainingEngine('gemini', 'claude'), 'codex');
  assert.equal(remainingEngine('claude', 'gemini'), 'codex');
  assert.equal(remainingEngine('codex', 'gemini'), 'claude');
  assert.equal(remainingEngine('codex', 'claude'), 'gemini');
  assert.equal(remainingEngine('gemini', undefined), undefined, 'no 2nd tier, no 3rd tier');
  assert.equal(remainingEngine('gemini', 'gemini'), undefined, 'the 2nd tier cannot repeat the 1st');
});

const threeTierAgent = () => ({
  ...DEFAULT_PROCESS_AGENTS[0],
  engine: 'gemini', model: 'gemini-3.8-flash', effort: 'medium',
  fallbackEngine: 'claude', fallbackModel: 'claude-opus-5.5', fallbackEffort: 'xhigh',
  thirdEngine: 'codex', thirdModel: 'gpt-6.2-sol', thirdEffort: 'ultra',
});

test('processAgentToSaveInput writes all three tiers, one model per engine', () => {
  const tools = JSON.parse(processAgentToSaveInput(threeTierAgent()).tools);
  assert.deepEqual(tools.gemini, { model: 'gemini-3.8-flash', effort: 'medium' });
  assert.deepEqual(tools.claude, { model: 'claude-opus-5.5', effort: 'xhigh' });
  assert.deepEqual(tools.codex, { model: 'gpt-6.2-sol', effort: 'ultra' });
  assert.equal(tools.aisurada.thirdEngine, 'codex');
  assert.equal(tools.aisurada.thirdModel, 'gpt-6.2-sol');
  assert.equal(tools.aisurada.thirdEffort, 'ultra');
});

test('without a 3rd tier the engine left over keeps its default model and the 3rd fields are empty', () => {
  const two = { ...threeTierAgent(), thirdEngine: undefined, thirdModel: undefined, thirdEffort: undefined };
  const tools = JSON.parse(processAgentToSaveInput(two).tools);
  assert.deepEqual(tools.codex, { model: MODEL_PRESETS.codex[0] }, 'no effort for a tier that is not used');
  assert.equal(tools.aisurada.thirdEngine, null);
  assert.equal(tools.aisurada.thirdModel, null);
  assert.equal(tools.aisurada.thirdEffort, 'default');
});

test('a 3rd tier without a 2nd tier is not written', () => {
  const orphan = { ...threeTierAgent(), fallbackEngine: undefined, fallbackModel: undefined, fallbackEffort: undefined };
  const tools = JSON.parse(processAgentToSaveInput(orphan).tools);
  assert.equal(tools.aisurada.thirdEngine, null);
  assert.deepEqual(tools.codex, { model: MODEL_PRESETS.codex[0] });
});

test('resourceRowToProcessAgent reads the 3rd tier back and drops one that breaks the rules', () => {
  const row = (aisurada) => ({ id: 'ap-3', kind: 'agent', scope: 'global', name: '3순위 시험', description: 'd', body: 'b', permission: 'edit', skills: [], tools: { aisurada } });
  const base = { engine: 'gemini', model: 'gemini-3.8-flash', fallbackEngine: 'claude', fallbackModel: 'claude-sonnet-5.5' };

  const ok = resourceRowToProcessAgent(row({ ...base, thirdEngine: 'codex', thirdModel: 'gpt-6.2-sol', thirdEffort: 'ultra' }), 0);
  assert.equal(ok.thirdEngine, 'codex');
  assert.equal(ok.thirdModel, 'gpt-6.2-sol');
  assert.equal(ok.thirdEffort, 'ultra');

  // 같은 공급자(1순위·2순위와 같은 엔진)나 알 수 없는 엔진, 2순위가 없는 3순위는 버린다
  for (const thirdEngine of ['gemini', 'claude', 'nope']) {
    assert.equal(resourceRowToProcessAgent(row({ ...base, thirdEngine, thirdModel: 'x' }), 0).thirdEngine, undefined, `${thirdEngine} is dropped`);
  }
  const noSecond = resourceRowToProcessAgent(row({ engine: 'gemini', model: 'gemini-3.8-flash', thirdEngine: 'codex', thirdModel: 'gpt-6.2-sol' }), 0);
  assert.equal(noSecond.thirdEngine, undefined);
  assert.equal(noSecond.thirdModel, undefined);
  assert.equal(noSecond.thirdEffort, undefined);

  // 없는 모델 이름은 그 엔진의 기본 모델로, 쓸 수 없는 강도는 default로
  const noModel = resourceRowToProcessAgent(row({ ...base, thirdEngine: 'codex' }), 0);
  assert.equal(noModel.thirdModel, MODEL_PRESETS.codex[0]);
  const luna = resourceRowToProcessAgent(row({ ...base, thirdEngine: 'codex', thirdModel: 'gpt-6-luna', thirdEffort: 'ultra' }), 0);
  assert.equal(luna.thirdEffort, 'default', 'ultra is not offered for GPT-6-Luna');
});

test('a three-tier agent survives a save and a read unchanged', () => {
  const agent = threeTierAgent();
  const input = processAgentToSaveInput(agent);
  const back = resourceRowToProcessAgent({ id: 'ap-rt', kind: 'agent', scope: 'global', name: input.name, description: input.description, body: input.body, permission: 'edit', skills: input.skills, tools: JSON.parse(input.tools) }, 0);
  for (const key of ['engine', 'model', 'effort', 'fallbackEngine', 'fallbackModel', 'fallbackEffort', 'thirdEngine', 'thirdModel', 'thirdEffort']) {
    assert.equal(back[key], agent[key], key);
  }
});

test('globalSkillToSaveInput and resourceRowToGlobalSkill roundtrip', () => {
  const skill = DEFAULT_GLOBAL_SKILLS[0];
  const saveInput = globalSkillToSaveInput(skill);
  assert.equal(saveInput.kind, 'skill');
  assert.equal(saveInput.scope, 'global');
  assert.equal(saveInput.name, skill.name);

  const fakeRow = {
    id: 'sk-proj-mgr-01',
    kind: 'skill',
    scope: 'global',
    name: saveInput.name,
    description: saveInput.description,
    body: saveInput.body,
    skills: [],
    tools: {},
    path: '.aisurada/resources/global/skills/project-manager/SKILL.md',
    precondition: 'f1:skill-token',
  };

  const reconstructed = resourceRowToGlobalSkill(fakeRow);
  assert.equal(reconstructed.id, 'sk-proj-mgr-01');
  assert.equal(reconstructed.name, skill.name);
  assert.equal(reconstructed.f1, 'f1:skill-token');
});

test('skillFromForm carries the write token and path of an edited skill; a new skill has none', () => {
  const now = '2026-11-18T01:00:00.000Z';
  const existing = {
    id: 'sk1-aaaaaaaa', name: 'ai-sync', description: '옛 설명', body: '옛 본문', tags: ['x'],
    createdAt: '2026-11-17T00:00:00.000Z', updatedAt: '2026-11-17T00:00:00.000Z',
    f1: 'f1:abc', path: '.aisurada/resources/global/skills/ai-sync/SKILL.md',
  };
  const edited = skillFromForm(existing, { name: ' ai-sync ', description: ' 새 설명 ', tagsText: 'git, , commit ', body: ' 새 본문 ' }, now);
  assert.equal(edited.id, 'sk1-aaaaaaaa');
  assert.equal(edited.name, 'ai-sync');
  assert.equal(edited.description, '새 설명');
  assert.equal(edited.body, '새 본문');
  assert.deepEqual(edited.tags, ['git', 'commit']);
  assert.equal(edited.createdAt, existing.createdAt);
  assert.equal(edited.updatedAt, now);
  assert.equal(edited.f1, 'f1:abc', 'the write token is carried over');
  assert.equal(edited.path, existing.path);
  // 엔진은 기존 스킬을 고칠 때 id와 f1이 둘 다 있어야 받는다
  const input = globalSkillToSaveInput(edited);
  assert.equal(input.id, 'sk1-aaaaaaaa');
  assert.equal(input.f1, 'f1:abc');

  const fresh = skillFromForm(null, { name: '새 스킬', description: '설명', tagsText: '', body: '' }, now);
  assert.ok(fresh.id.startsWith('gskill-'));
  assert.equal(fresh.f1, undefined);
  assert.equal(fresh.createdAt, now);
  const freshInput = globalSkillToSaveInput(fresh);
  assert.equal(freshInput.id, undefined, 'a new skill is created without an id');
  assert.equal('f1' in freshInput, false);
});

test('the agent modal always asks for a 2nd tier and offers an optional 3rd tier', async () => {
  // 모달은 포털로 그려서 서버 렌더가 못 그린다. 이 시험에서만 포털을 자식 그대로 내고, 모듈을 다 불러온 뒤에 빈 document를 둔다
  const { createRequire } = await import('node:module');
  const require = createRequire(import.meta.url);
  const React = require('react');
  const reactDom = require('react-dom');
  const { renderToStaticMarkup } = require('react-dom/server');
  const originalPortal = reactDom.createPortal;
  reactDom.createPortal = (children) => children;
  try {
    const { ProcessAgentModal } = await import('../src/ui/globalResources/ProcessAgentModal.tsx');
    globalThis.document = { body: {} };
    const render = (agent, extra = {}) => renderToStaticMarkup(React.createElement(ProcessAgentModal, { agent, skills: [], resources: [], onSave() {}, onClose() {}, ...extra })).replace(/<!-- -->/g, '');
    const count = (html, re) => (html.match(re) || []).length;
    const base = DEFAULT_PROCESS_AGENTS[0]; // Gemini 1순위, Claude 2순위

    for (const [label, agent] of [['2순위만 있는 에이전트', { ...base, thirdEngine: undefined }], ['2순위가 없는 옛 에이전트', { ...base, fallbackEngine: undefined, fallbackModel: undefined, fallbackEffort: undefined }], ['새로 등록', null]]) {
      const html = render(agent, { defaultEngineOpen: true });
      assert.equal(count(html, /data-card="1"/g), 1, `${label}: 1순위 카드가 있다`);
      assert.equal(count(html, /data-card="2"/g), 1, `${label}: 2순위 카드가 있다`);
      assert.equal(count(html, /data-card="3"/g), 1, `${label}: 3순위 카드는 꺼져 있어도 자리가 있다`);
      assert.equal(count(html, /2순위 대체 모델 사용/g), 0, `${label}: 2순위를 끄는 스위치는 없다`);
      assert.equal(count(html, /3순위 대체 모델 사용/g), 1, `${label}: 3순위는 스위치로 켠다`);
      assert.equal(count(html, /role="switch"/g), 1, `${label}: 스위치는 하나다`);
      assert.equal(count(html, /남은 엔진 하나로 정해집니다/g), 0, `${label}: 3순위는 꺼져 있다`);
      assert.equal(count(html, /3순위 세부 모델/g), 0, `${label}: 꺼진 3순위는 모델 칸을 안 그린다`);
      assert.equal(count(html, /class="eng-preview"/g), 1, `${label}: 꺼진 3순위는 켜면 정해질 엔진을 미리 보여 준다`);
      assert.equal(count(html, /3순위 없음/g), 1, `${label}: 실행 순서 요약은 3순위 없음을 보인다`);
    }

    // 1순위 Gemini, 2순위 Claude이면 2순위 카드에서 Gemini 타일은 1순위가 쓰는 칸이라 못 고른다
    const two = render({ ...base, thirdEngine: undefined }, { defaultEngineOpen: true });
    assert.equal(count(two, /data-used-by="1순위"/g), 1, '2순위 카드의 1순위 한 칸');
    assert.equal(count(two, /<button[^>]*disabled=""[^>]*data-used-by="1순위"|<button[^>]*data-used-by="1순위"[^>]*disabled=""/g), 1, '못 고르는 타일은 비활성 단추다');
    assert.equal(count(two, /aria-label="1순위 세부 모델"/g), 1);
    assert.equal(count(two, /aria-label="2순위 세부 모델"/g), 1);
    assert.equal(count(two, /aria-label="1순위 추론 강도 \(Effort\)"/g), 1);

    // 고칠 때는 요약 줄만 보이고 설정 상세는 접혀 있다. 새로 만들 때는 펼쳐서 연다
    const closed = render({ ...base, thirdEngine: 'codex', thirdModel: 'gpt-6.2-sol', thirdEffort: 'ultra' });
    assert.equal(count(closed, /aria-expanded="false"/g), 1, '고칠 때는 접혀 있다');
    assert.equal(count(closed, /설정 펼치기/g), 1);
    assert.equal(count(closed, /class="eng-chain"/g), 1, '접혀도 실행 순서 요약 줄은 보인다');
    assert.equal(count(closed, /data-step=/g), 3, '요약 줄에 세 순위가 다 있다');
    for (const hidden of [/data-card=/g, /role="radiogroup"/g, /eng-native/g, /role="switch"/g]) {
      assert.equal(count(closed, hidden), 0, `접히면 설정 상세는 그리지 않는다: ${hidden.source}`);
    }
    const created = render(null);
    assert.equal(count(created, /aria-expanded="true"/g), 1, '새로 만들 때는 펼쳐서 연다');
    assert.equal(count(created, /설정 접기/g), 1);
    assert.equal(count(created, /data-card=/g), 3);
    const forcedClosed = render(base, { defaultEngineOpen: false });
    assert.equal(count(forcedClosed, /aria-expanded="false"/g), 1);
    const opened = render(base, { defaultEngineOpen: true });
    assert.equal(count(opened, /aria-expanded="true"/g), 1);
    assert.equal(count(opened, /aria-controls="[^"]+"/g), 1, '접는 단추가 펼치는 영역을 가리킨다');

    const three = render({ ...base, thirdEngine: 'codex', thirdModel: 'gpt-6.2-sol', thirdEffort: 'ultra' }, { defaultEngineOpen: true });
    assert.equal(count(three, /남은 엔진 하나로 정해집니다/g), 1, '켜 둔 3순위는 본문이 보인다');
    assert.equal(count(three, /3순위 세부 모델/g), 1);
    assert.equal(count(three, /3순위 추론 강도/g), 1);
    assert.equal(count(three, /class="eng-preview"/g), 0, '켜 둔 3순위는 미리보기 대신 본문이다');
    assert.equal(count(three, /3순위 없음/g), 0);
    assert.equal(count(three, /data-used-by=/g), 3, '2순위 카드의 1순위 한 칸과 3순위 카드의 1순위·2순위 두 칸');
    assert.equal(count(three, /data-used-by="2순위"/g), 1, '3순위 카드에서 2순위가 쓰는 칸');
    assert.equal(count(three, /<div[^>]*aria-disabled="true"[^>]*data-engine=/g), 3, '3순위 타일은 고르는 칸이 아니라 정해진 값을 보여 주는 칸이다');
    assert.equal(count(three, /class="eng-meter is-ultra"/g) >= 2, true, 'ultra는 요약 줄과 강도 칸에서 다른 막대로 보인다');
  } finally {
    reactDom.createPortal = originalPortal;
    delete globalThis.document;
  }
});

test('saveProcessAgent inherits real id and f1 when called with proc- mock agent, and updates store', async () => {
  const { saveProcessAgent, getGlobalResourcesSnapshot, syncGlobalResources } = await import(
    '../src/ui/globalResources/globalResourcesStore.ts'
  );
  const rows = [
    {
      id: 'ap-real-123',
      kind: 'agent',
      scope: 'global',
      name: '커밋 푸시 에이전트',
      description: '푸시한다',
      precondition: 'f1:orig',
      path: '.aisurada/resources/global/agents/a.md',
      tools: { aisurada: { key: 'commit-push', engine: 'gemini', model: 'gemini-3.8-flash', order: 1 } },
    },
    {
      id: 'sk-real-1',
      kind: 'skill',
      scope: 'global',
      name: 'project-manager',
      description: '프로젝트 관리',
      path: '.aisurada/resources/global/skills/a.md',
      tools: {},
    },
  ];
  let savedAgentInput = null;
  const mockCall = async (call) => {
    const cmd = call.command;
    if (cmd === 'list_resources') return { resources: rows, skipped: [] };
    if (cmd === 'get_resource') return rows.find((r) => r.id === call.args?.id) || rows[0];
    if (cmd === 'save_resource') {
      if (call.args?.kind === 'agent') {
        savedAgentInput = call.args;
        rows[0] = {
          ...rows[0],
          precondition: 'f1:updated',
          tools: JSON.parse(call.args.tools || '{}'),
        };
      }
      return rows[0];
    }
    return {};
  };

  // 1. 초기 동기화
  await syncGlobalResources(mockCall, true);
  const snapshotBefore = getGlobalResourcesSnapshot();
  const found = snapshotBefore.agents.find((a) => a.name === '커밋 푸시 에이전트');
  assert.ok(found);
  assert.equal(found.id, 'ap-real-123');

  // 2. proc- id로 넘겨도 실 id와 f1을 계승하며, 삭제/부존재 스킬 ID는 자동 제외하고 저장
  const mockAgent = {
    ...found,
    id: 'proc-commit-push',
    f1: undefined,
    model: 'gemini-2.5-pro',
    skills: ['sk-real-1', 'sk-trashed-ghost-id'],
  };
  await saveProcessAgent(mockAgent, mockCall);

  assert.equal(savedAgentInput.id, 'ap-real-123', 'mock proc- id was resolved to real engine id');
  assert.equal(savedAgentInput.f1, 'f1:orig', 'missing f1 was resolved to real token');
  assert.equal(savedAgentInput.skills, JSON.stringify(['sk-real-1']), 'invalid or trashed skills are pruned before engine save');
  const snapshotAfter = getGlobalResourcesSnapshot();
  const updated = snapshotAfter.agents.find((a) => a.id === 'ap-real-123');
  assert.equal(updated.model, 'gemini-2.5-pro', 'store immediately reflects updated model');
});

test('a resizable modal keeps its grip in its own bottom bar, apart from the scrolling body', async () => {
  // 모달은 포털로 그려서 서버 렌더가 못 그린다. 이 시험에서만 포털을 자식 그대로 내고, 모듈을 불러온 뒤에 빈 document를 둔다
  const { createRequire } = await import('node:module');
  const require = createRequire(import.meta.url);
  const React = require('react');
  const reactDom = require('react-dom');
  const { renderToStaticMarkup } = require('react-dom/server');
  const originalPortal = reactDom.createPortal;
  const hadDocument = 'document' in globalThis;
  const originalDocument = globalThis.document;
  reactDom.createPortal = (children) => children;
  try {
    const { Modal } = await import('../src/ui/Modal.tsx');
    globalThis.document = { body: {} };
    const draw = (props) => renderToStaticMarkup(React.createElement(Modal, { title: '창', close() {}, ...props }, React.createElement('p', null, '내용'))).replace(/<!-- -->/g, '');
    const resizable = draw({ resizable: true, defaultWidth: 600, defaultHeight: 400 });
    assert.match(resizable, /<div class="ui-modal-body"><p>내용<\/p><\/div>/, '내용은 스크롤 영역 안에 있다');
    assert.ok(resizable.indexOf('ui-modal-gripbar') > resizable.indexOf('ui-modal-body'), '손잡이 띠는 스크롤 영역 뒤에 따로 있다');
    assert.match(resizable, /<div class="ui-modal-gripbar"><span class="preview-grip"/, '손잡이는 띠 안에 있다');
    assert.equal((resizable.match(/class="preview-grip"/g) || []).length, 1);
    const plain = draw({});
    assert.doesNotMatch(plain, /ui-modal-body|ui-modal-gripbar|preview-grip/, '크기를 못 바꾸는 모달은 그대로다');
    assert.match(plain, /<p>내용<\/p>/);
  } finally {
    reactDom.createPortal = originalPortal;
    if (hadDocument) globalThis.document = originalDocument;
    else delete globalThis.document;
  }
});

test('effortLevel fills the whole meter at the highest effort a model supports', async () => {
  const { effortLevel } = await import('../src/ui/globalResources/EnginePicker.tsx');
  // Gemini 3.8 Flash는 low·medium·high뿐이라 high가 꽉 찬 5칸이고 아래 단계는 고르게 펼쳐진다
  assert.deepEqual(['low', 'medium', 'high'].map((e) => effortLevel('gemini', 'gemini-3.8-flash', e)), [1, 3, 5]);
  // 5단계 이상인 모델은 강도마다 정해진 칸 수를 그대로 쓴다
  assert.deepEqual(['low', 'medium', 'high', 'xhigh', 'max'].map((e) => effortLevel('claude', 'claude-sonnet-5.5', e)), [1, 2, 3, 4, 5]);
  assert.deepEqual(['low', 'medium', 'high', 'xhigh', 'max', 'ultra'].map((e) => effortLevel('codex', 'gpt-6.2-sol', e)), [1, 2, 3, 4, 5, 5]);
  // 4단계 모델도 가장 높은 강도(max)가 꽉 찬다
  assert.equal(effortLevel('claude', 'claude-sonnet-4.6', 'max'), 5);
  // default와 상속한 모델은 강도를 모르므로 채우지 않는다
  assert.equal(effortLevel('gemini', 'gemini-3.8-flash', 'default'), 0);
  assert.equal(effortLevel('claude', 'inherit', 'default'), 0);
  // 표에 없는 모델은 엔진 기본 단계를 따른다(gemini는 세 단계)
  assert.equal(effortLevel('gemini', 'gemini-custom-x', 'high'), 5);
});

test('compact fallback chain keeps all chips on a single line', async () => {
  const { readFileSync } = await import('node:fs');
  const { fileURLToPath } = await import('node:url');
  const css = readFileSync(fileURLToPath(new URL('../src/engine-picker.css', import.meta.url)), 'utf8');
  assert.match(css, /\.eng-chain\.is-compact\s*\{[^}]*flex-wrap:\s*nowrap/);
  assert.match(css, /\.eng-chain\.is-compact\s*\{[^}]*overflow-x:\s*auto/);
});
