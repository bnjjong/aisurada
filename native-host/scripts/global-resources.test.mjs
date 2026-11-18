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

test('globalSkillToSaveInput and resourceRowToGlobalSkill roundtrip', () => {
  const skill = DEFAULT_GLOBAL_SKILLS[0];
  const saveInput = globalSkillToSaveInput(skill);
  assert.equal(saveInput.kind, 'skill');
  assert.equal(saveInput.scope, 'global');
  assert.equal(saveInput.name, skill.name);

  const fakeRow = {
    id: 'sk-ai-commit-01',
    kind: 'skill',
    scope: 'global',
    name: saveInput.name,
    description: saveInput.description,
    body: saveInput.body,
    skills: [],
    tools: {},
    path: '.aisurada/resources/global/skills/ai-commit/SKILL.md',
    precondition: 'f1:skill-token',
  };

  const reconstructed = resourceRowToGlobalSkill(fakeRow);
  assert.equal(reconstructed.id, 'sk-ai-commit-01');
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
