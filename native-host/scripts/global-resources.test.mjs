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
} from '../src/ui/globalResources/types.ts';

test('AI_ENGINES, MODEL_PRESETS and REASONING_EFFORTS cover claude, codex and gemini', () => {
  const engineIds = AI_ENGINES.map((e) => e.id);
  assert.deepEqual(engineIds.sort(), ['claude', 'codex', 'gemini'].sort());

  assert.ok(MODEL_PRESETS.claude.includes('claude-sonnet-5.5'));
  assert.ok(MODEL_PRESETS.claude.includes('claude-fable-5.2'));
  assert.ok(MODEL_PRESETS.codex.includes('gpt-6.1-sol'));
  assert.ok(MODEL_PRESETS.codex.includes('gpt-6-astra'));
  assert.ok(MODEL_PRESETS.gemini.includes('gemini-3.8-flash'));
  assert.ok(MODEL_PRESETS.gemini.includes('gemini-4-carbon'));

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
