import { useState } from 'react';
import type { FormEvent } from 'react';
import { Editor } from '../../editor/Editor.tsx';
import { EditPendingNotice } from '../ApprovalNotices.tsx';
import { openApprovalsDialog } from '../approvalsDialogStore.ts';
import { Modal } from '../Modal.tsx';
import type { AIEngine, GlobalResourceItem, GlobalSkill, ProcessAgent, ReasoningEffort } from './types.ts';
import { AI_ENGINES, ENGINE_MODELS, MODEL_PRESETS, clampEffort, effortInfosFor, remainingEngine } from './types.ts';

interface Props {
  agent?: ProcessAgent | null;
  skills: GlobalSkill[];
  resources: GlobalResourceItem[];
  onSave: (agent: ProcessAgent) => Promise<void> | void;
  onClose: () => void;
}

/** 2순위·3순위 카드가 같이 쓰는 한 줄: 세부 모델, 추론 강도, 직접 입력 칸. 강도는 그 모델이 지원하는 값만 보인다. */
function TierModelRow({
  tier,
  engine,
  selectedModel,
  onSelectModel,
  customInput,
  onCustomInput,
  resolvedModel,
  effort,
  onEffort,
  placeholder,
}: {
  tier: '2순위' | '3순위';
  engine: AIEngine;
  selectedModel: string;
  onSelectModel: (id: string) => void;
  customInput: string;
  onCustomInput: (value: string) => void;
  resolvedModel: string;
  effort: ReasoningEffort;
  onEffort: (effort: ReasoningEffort) => void;
  placeholder: string;
}) {
  return (
    <div style={{ display: 'flex', gap: '12px', alignItems: 'center', marginTop: '4px', flexWrap: 'wrap' }}>
      <label style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '13px' }}>
        <span>{tier} 세부 모델:</span>
        <select value={selectedModel} onChange={(e) => onSelectModel(e.target.value)} style={{ minWidth: '220px', padding: '4px 8px' }}>
          {ENGINE_MODELS[engine].map((m) => (
            <option key={m.id} value={m.id}>
              {m.label}
            </option>
          ))}
          <option value="custom">직접 입력...</option>
        </select>
      </label>

      <label style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '13px' }}>
        <span>{tier} 추론 강도 (Effort):</span>
        <select
          value={effort}
          onChange={(e) => onEffort(e.target.value as ReasoningEffort)}
          style={{ minWidth: '150px', padding: '4px 8px' }}
          title={`${tier} 대체 모델이 지원하는 Thinking / Reasoning 추론 강도 (Effort)`}
        >
          {effortInfosFor(engine, resolvedModel).map((eff) => (
            <option key={eff.id} value={eff.id}>
              {eff.label}
            </option>
          ))}
        </select>
      </label>

      {selectedModel === 'custom' && (
        <input
          type="text"
          value={customInput}
          onChange={(e) => onCustomInput(e.target.value)}
          placeholder={placeholder}
          style={{ flex: 1, minWidth: '200px', padding: '4px 8px' }}
          required
        />
      )}
    </div>
  );
}

export function ProcessAgentModal({ agent, skills, resources, onSave, onClose }: Props) {
  const isEdit = !!agent;
  // 승인을 기다리는 변경이 있으면 이 창의 값은 승인해 둔 옛 판이다. 모르고 저장하면 대기 중인 새 판을 덮어쓴다
  const awaitingApproval = isEdit && agent?.approval === 'dirty';
  const openApprovals = () => {
    onClose();
    openApprovalsDialog();
  };
  const [name, setName] = useState(agent?.name || '');
  const [role, setRole] = useState(agent?.role || '');
  const [description, setDescription] = useState(agent?.description || '');
  const [enabled, setEnabled] = useState(agent?.enabled ?? true);
  
  // 1순위 AI 엔진 및 모델
  const initialEngine: AIEngine = agent?.engine || 'gemini';
  const [engine, setEngine] = useState<AIEngine>(initialEngine);
  const initialModel = agent?.model || MODEL_PRESETS[initialEngine][0];
  const isPreset = MODEL_PRESETS[initialEngine].includes(initialModel);
  const [selectedModel, setSelectedModel] = useState(isPreset ? initialModel : 'custom');
  const [customModelInput, setCustomModelInput] = useState(isPreset ? '' : initialModel);
  const [effort, setEffort] = useState<ReasoningEffort>(agent?.effort || 'default');

  // 2순위 (토큰 소진 대비 폴백) AI 엔진 및 모델. 2순위는 늘 정한다(옛 파일에 없으면 기본값으로 채운다)
  const initialFallbackEngine: AIEngine =
    agent?.fallbackEngine && agent.fallbackEngine !== initialEngine
      ? agent.fallbackEngine
      : initialEngine === 'claude'
      ? 'gemini'
      : 'claude';
  const [fallbackEngine, setFallbackEngine] = useState<AIEngine>(initialFallbackEngine);
  const initialFallbackModel = agent?.fallbackModel || MODEL_PRESETS[initialFallbackEngine][0];
  const isFallbackPreset = MODEL_PRESETS[initialFallbackEngine].includes(initialFallbackModel);
  const [selectedFallbackModel, setSelectedFallbackModel] = useState(isFallbackPreset ? initialFallbackModel : 'custom');
  const [customFallbackModelInput, setCustomFallbackModelInput] = useState(isFallbackPreset ? '' : initialFallbackModel);
  const [fallbackEffort, setFallbackEffort] = useState<ReasoningEffort>(agent?.fallbackEffort || 'default');

  // 3순위 (선택). 1순위와 2순위가 모두 소진됐을 때 쓰고, 엔진은 둘이 쓰지 않는 나머지 하나로 정해진다
  const initialThirdEngine: AIEngine = remainingEngine(initialEngine, initialFallbackEngine) ?? AI_ENGINES[0].id;
  const [useThird, setUseThird] = useState<boolean>(!!agent?.thirdEngine && agent.thirdEngine === initialThirdEngine);
  const initialThirdModel = (agent?.thirdEngine === initialThirdEngine && agent?.thirdModel) || MODEL_PRESETS[initialThirdEngine][0];
  const isThirdPreset = MODEL_PRESETS[initialThirdEngine].includes(initialThirdModel);
  const [selectedThirdModel, setSelectedThirdModel] = useState(isThirdPreset ? initialThirdModel : 'custom');
  const [customThirdModelInput, setCustomThirdModelInput] = useState(isThirdPreset ? '' : initialThirdModel);
  const [thirdEffort, setThirdEffort] = useState<ReasoningEffort>(agent?.thirdEffort || 'default');

  // 유효한 글로벌 스킬 목록으로 초기화 (삭제되거나 존재하지 않는 스킬 ID 자동 제외)
  const availableSkillIds = new Set(skills.map((s) => s.id));
  const initialSkills = skills.length > 0
    ? (agent?.skills || []).filter((id) => availableSkillIds.has(id))
    : (agent?.skills || []);
  const [selectedSkills, setSelectedSkills] = useState<string[]>(initialSkills);
  const [selectedResources, setSelectedResources] = useState<string[]>(agent?.resources || []);
  const [prompt, setPrompt] = useState(agent?.prompt || '');
  const [promptViewMode, setPromptViewMode] = useState<'editor' | 'raw'>('editor');
  const [promptEditorKey, setPromptEditorKey] = useState(0);
  const [error, setError] = useState('');
  const [isSaving, setIsSaving] = useState(false);

  // 모델마다 고를 수 있는 추론 강도가 달라서, 지금 고른 모델에서 쓸 수 있는 값만 보여 준다.
  // 쓸 수 없는 값이 남아 있으면(모델을 바꾼 경우) default로 보이고 그대로 저장된다
  const resolvedModel = selectedModel === 'custom' ? customModelInput.trim() || 'inherit' : selectedModel;
  const resolvedFallbackModel = selectedFallbackModel === 'custom' ? customFallbackModelInput.trim() || 'inherit' : selectedFallbackModel;
  const shownEffort = clampEffort(engine, resolvedModel, effort);
  const shownFallbackEffort = clampEffort(fallbackEngine, resolvedFallbackModel, fallbackEffort);
  // 3순위 엔진은 1순위·2순위가 쓰지 않는 나머지 엔진 하나다
  const thirdEngine: AIEngine = remainingEngine(engine, fallbackEngine) ?? AI_ENGINES[0].id;
  const resolvedThirdModel = selectedThirdModel === 'custom' ? customThirdModelInput.trim() || 'inherit' : selectedThirdModel;
  const shownThirdEffort = clampEffort(thirdEngine, resolvedThirdModel, thirdEffort);

  // 1순위나 2순위 엔진이 바뀌면 3순위가 쓸 수 있는 엔진(나머지 하나)도 바뀐다. 달라지면 그 엔진의 기본 모델로 돌린다
  const syncThird = (nextEngine: AIEngine, nextFallback: AIEngine) => {
    const nextThird = remainingEngine(nextEngine, nextFallback) ?? AI_ENGINES[0].id;
    if (nextThird !== thirdEngine) {
      setSelectedThirdModel(MODEL_PRESETS[nextThird][0]);
      setCustomThirdModelInput('');
    }
  };

  const handleEngineChange = (newEngine: AIEngine) => {
    setEngine(newEngine);
    const defaultModel = MODEL_PRESETS[newEngine][0];
    setSelectedModel(defaultModel);
    setCustomModelInput('');

    // 1순위 엔진과 2순위 엔진이 같아지면 2순위 엔진을 남은 엔진 중 다른 하나로 자동 변경
    let nextFallback = fallbackEngine;
    if (fallbackEngine === newEngine) {
      const remainingEngines = AI_ENGINES.filter((e) => e.id !== newEngine);
      nextFallback = remainingEngines[0].id;
      setFallbackEngine(nextFallback);
      setSelectedFallbackModel(MODEL_PRESETS[nextFallback][0]);
      setCustomFallbackModelInput('');
    }
    syncThird(newEngine, nextFallback);
  };

  const handleFallbackEngineChange = (newFallback: AIEngine) => {
    if (newFallback === engine) return; // 1순위와 동일한 엔진은 선택 불가
    setFallbackEngine(newFallback);
    const defaultModel = MODEL_PRESETS[newFallback][0];
    setSelectedFallbackModel(defaultModel);
    setCustomFallbackModelInput('');
    syncThird(engine, newFallback);
  };

  const toggleSkill = (id: string) => {
    setSelectedSkills((prev) =>
      prev.includes(id) ? prev.filter((s) => s !== id) : [...prev, id]
    );
  };

  const toggleResource = (id: string) => {
    setSelectedResources((prev) =>
      prev.includes(id) ? prev.filter((r) => r !== id) : [...prev, id]
    );
  };

  const handleSubmit = async (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setError('에이전트 이름을 입력해 주세요.');
      return;
    }
    setError('');
    setIsSaving(true);
    try {
      const finalModel = selectedModel === 'custom' ? (customModelInput.trim() || 'inherit') : selectedModel;
      const finalFallbackModel =
        selectedFallbackModel === 'custom' ? (customFallbackModelInput.trim() || 'inherit') : selectedFallbackModel;
      const finalThirdModel = selectedThirdModel === 'custom' ? (customThirdModelInput.trim() || 'inherit') : selectedThirdModel;

      const updated: ProcessAgent = {
        id: agent?.id || `proc-${Date.now()}`,
        key: agent?.key || name.trim().toLowerCase().replace(/\s+/g, '-'),
        name: name.trim(),
        role: role.trim(),
        description: description.trim(),
        engine,
        model: finalModel,
        effort: clampEffort(engine, finalModel, effort),
        // 2순위는 늘 저장하고(1순위와 같은 엔진은 고를 수 없다), 3순위는 켠 경우에만 저장한다
        fallbackEngine,
        fallbackModel: finalFallbackModel,
        fallbackEffort: clampEffort(fallbackEngine, finalFallbackModel, fallbackEffort),
        thirdEngine: useThird ? thirdEngine : undefined,
        thirdModel: useThird ? finalThirdModel : undefined,
        thirdEffort: useThird ? clampEffort(thirdEngine, finalThirdModel, thirdEffort) : undefined,
        enabled,
        skills: skills.length > 0 ? selectedSkills.filter((id) => availableSkillIds.has(id)) : selectedSkills,
        resources: selectedResources,
        prompt: prompt.trim(),
        order: agent?.order ?? 99,
        f1: agent?.f1,
      };
      await onSave(updated);
      onClose();
    } catch (err: any) {
      console.error('에이전트 저장 오류:', err);
      setError(err?.message || '저장 중 오류가 발생했습니다.');
    } finally {
      setIsSaving(false);
    }
  };

  return (
    <Modal
      title={isEdit ? `에이전트 설정 수정: ${agent.name}` : '새 프로세스 에이전트 등록'}
      close={onClose}
      wide
    >
      <form onSubmit={handleSubmit} className="res-form" style={{ display: 'grid', gap: '16px' }}>
        {error && <div className="res-error">{error}</div>}
        {awaitingApproval && <EditPendingNotice noun="에이전트" field="지침" onOpen={openApprovals} />}

        <label style={{ display: 'grid', gap: '6px' }}>
          <strong>에이전트 이름</strong>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="예: 티켓 에이전트"
            required
          />
        </label>

        <label style={{ display: 'grid', gap: '6px' }}>
          <strong>역할 요약</strong>
          <input
            type="text"
            value={role}
            onChange={(e) => setRole(e.target.value)}
            placeholder="예: 요구사항 확인 및 티켓 관리"
          />
        </label>

        <label style={{ display: 'grid', gap: '6px' }}>
          <strong>상세 설명</strong>
          <textarea
            rows={2}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="에이전트의 책임과 역할을 상세히 기술해 주세요."
          />
        </label>

        {/* AI 엔진 및 세부 모델 설정 (1순위 & 2순위 폴백) */}
        <div
          style={{
            display: 'grid',
            gap: '12px',
            padding: '14px',
            background: 'var(--surface-sunken, #f8f9fa)',
            borderRadius: '8px',
            border: '1px solid var(--border-subtle, #e5e7eb)',
          }}
        >
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <strong style={{ fontSize: '13px' }}>AI 실행 엔진 및 모델</strong>
            <span style={{ fontSize: '11px', color: 'var(--muted)' }}>토큰 부족 시 2순위, 3순위 모델 순서로 자동 대체</span>
          </div>

          {/* 1순위 모델 카드 */}
          <div
            style={{
              display: 'grid',
              gap: '8px',
              padding: '10px 12px',
              background: 'var(--surface, #ffffff)',
              borderRadius: '6px',
              border: '1px solid var(--line, #e2e8f0)',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
              <span
                style={{
                  fontSize: '11px',
                  fontWeight: 600,
                  padding: '2px 6px',
                  borderRadius: '4px',
                  background: 'var(--accent, #2563eb)',
                  color: '#ffffff',
                }}
              >
                1순위
              </span>
              <span style={{ fontSize: '13px', fontWeight: 600 }}>기본 실행 엔진</span>
            </div>

            <div style={{ display: 'flex', gap: '8px', flexWrap: 'wrap' }}>
              {AI_ENGINES.map((eng) => (
                <button
                  key={eng.id}
                  type="button"
                  className={`res-badge ${engine === eng.id ? 'res-badge-ref' : 'res-badge-off'}`}
                  style={{ padding: '6px 14px', cursor: 'pointer', fontSize: '13px' }}
                  onClick={() => handleEngineChange(eng.id)}
                >
                  {engine === eng.id ? '● ' : '○ '} {eng.label}{' '}
                  <small style={{ opacity: 0.7 }}>({eng.company})</small>
                </button>
              ))}
            </div>

            <div style={{ display: 'flex', gap: '12px', alignItems: 'center', marginTop: '4px', flexWrap: 'wrap' }}>
              <label style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '13px' }}>
                <span>세부 모델:</span>
                <select
                  value={selectedModel}
                  onChange={(e) => setSelectedModel(e.target.value)}
                  style={{ minWidth: '220px', padding: '4px 8px' }}
                >
                  {ENGINE_MODELS[engine].map((m) => (
                    <option key={m.id} value={m.id}>
                      {m.label}
                    </option>
                  ))}
                  <option value="custom">직접 입력...</option>
                </select>
              </label>

              <label style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '13px' }}>
                <span>추론 강도 (Effort):</span>
                <select
                  value={shownEffort}
                  onChange={(e) => setEffort(e.target.value as ReasoningEffort)}
                  style={{ minWidth: '150px', padding: '4px 8px' }}
                  title="이 모델이 지원하는 Thinking / Reasoning 추론 강도 (Effort)"
                >
                  {effortInfosFor(engine, resolvedModel).map((eff) => (
                    <option key={eff.id} value={eff.id}>
                      {eff.label}
                    </option>
                  ))}
                </select>
              </label>

              {selectedModel === 'custom' && (
                <input
                  type="text"
                  value={customModelInput}
                  onChange={(e) => setCustomModelInput(e.target.value)}
                  placeholder="예: claude-sonnet-5.5"
                  style={{ flex: 1, minWidth: '200px', padding: '4px 8px' }}
                  required
                />
              )}
            </div>
          </div>

          {/* 2순위 (폴백) 모델 카드: 늘 정한다 */}
          <div
            style={{
              display: 'grid',
              gap: '8px',
              padding: '10px 12px',
              background: 'var(--surface, #ffffff)',
              borderRadius: '6px',
              border: '1px dashed var(--accent, #2563eb)',
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
              <span
                style={{
                  fontSize: '11px',
                  fontWeight: 600,
                  padding: '2px 6px',
                  borderRadius: '4px',
                  background: 'var(--warning, #d97706)',
                  color: '#ffffff',
                }}
              >
                2순위
              </span>
              <span style={{ fontSize: '13px', fontWeight: 600 }}>토큰 고갈 대비 대체 모델 (폴백)</span>
              <span style={{ fontSize: '11px', color: 'var(--muted)' }}>필수</span>
            </div>

            <p style={{ margin: 0, fontSize: '11px', color: 'var(--muted)' }}>
              1순위 엔진의 토큰이 소진되었을 때 즉시 투입됩니다. 동일 공급자의 동시 토큰 고갈을 방지하기 위해 <b>1순위와 같은 엔진은 선택할 수 없습니다.</b>
            </p>

            <div style={{ display: 'flex', gap: '8px', flexWrap: 'wrap' }}>
              {AI_ENGINES.map((eng) => {
                const isFirstChoice = eng.id === engine;
                const isSelected = fallbackEngine === eng.id;
                return (
                  <button
                    key={eng.id}
                    type="button"
                    disabled={isFirstChoice}
                    className={`res-badge ${isSelected ? 'res-badge-ref' : 'res-badge-off'}`}
                    style={{
                      padding: '6px 14px',
                      cursor: isFirstChoice ? 'not-allowed' : 'pointer',
                      fontSize: '13px',
                      opacity: isFirstChoice ? 0.35 : 1,
                      textDecoration: isFirstChoice ? 'line-through' : 'none',
                    }}
                    onClick={() => handleFallbackEngineChange(eng.id)}
                    title={isFirstChoice ? `${eng.label}는 1순위로 선택되어 2순위로 중복 선택할 수 없습니다.` : undefined}
                  >
                    {isFirstChoice ? '✕ ' : isSelected ? '● ' : '○ '}
                    {eng.label}
                    {isFirstChoice ? (
                      <small style={{ color: 'var(--err, #dc2626)', marginLeft: '4px' }}>(선택 불가)</small>
                    ) : (
                      <small style={{ opacity: 0.7 }}> ({eng.company})</small>
                    )}
                  </button>
                );
              })}
            </div>

            <TierModelRow
              tier="2순위"
              engine={fallbackEngine}
              selectedModel={selectedFallbackModel}
              onSelectModel={setSelectedFallbackModel}
              customInput={customFallbackModelInput}
              onCustomInput={setCustomFallbackModelInput}
              resolvedModel={resolvedFallbackModel}
              effort={shownFallbackEffort}
              onEffort={setFallbackEffort}
              placeholder="예: gemini-3.8-flash"
            />
          </div>

          {/* 3순위 모델 카드: 선택. 1순위와 2순위가 모두 소진됐을 때만 쓴다 */}
          <div
            style={{
              display: 'grid',
              gap: '8px',
              padding: '10px 12px',
              background: useThird ? 'var(--surface, #ffffff)' : 'transparent',
              borderRadius: '6px',
              border: useThird ? '1px dashed var(--accent, #2563eb)' : '1px dashed var(--line, #e2e8f0)',
              opacity: useThird ? 1 : 0.7,
            }}
          >
            <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: '6px' }}>
                <span
                  style={{
                    fontSize: '11px',
                    fontWeight: 600,
                    padding: '2px 6px',
                    borderRadius: '4px',
                    background: useThird ? 'var(--warning, #d97706)' : 'var(--muted, #94a3b8)',
                    color: '#ffffff',
                  }}
                >
                  3순위
                </span>
                <span style={{ fontSize: '13px', fontWeight: 600 }}>마지막 대체 모델</span>
                <span style={{ fontSize: '11px', color: 'var(--muted)' }}>선택</span>
              </div>
              <label style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '12px', cursor: 'pointer' }}>
                <input type="checkbox" checked={useThird} onChange={(e) => setUseThird(e.target.checked)} />
                <span>3순위 대체 모델 사용</span>
              </label>
            </div>

            {useThird && (
              <>
                <p style={{ margin: 0, fontSize: '11px', color: 'var(--muted)' }}>
                  1순위와 2순위가 모두 소진되었을 때 투입됩니다. 이미 쓰는 두 엔진은 선택할 수 없어서 <b>남은 엔진 하나로 정해집니다.</b>
                </p>

                <div style={{ display: 'flex', gap: '8px', flexWrap: 'wrap' }}>
                  {AI_ENGINES.map((eng) => {
                    const usedBy = eng.id === engine ? '1순위' : eng.id === fallbackEngine ? '2순위' : null;
                    const isSelected = eng.id === thirdEngine;
                    return (
                      <button
                        key={eng.id}
                        type="button"
                        disabled={usedBy !== null}
                        className={`res-badge ${isSelected ? 'res-badge-ref' : 'res-badge-off'}`}
                        style={{
                          padding: '6px 14px',
                          cursor: usedBy ? 'not-allowed' : 'default',
                          fontSize: '13px',
                          opacity: usedBy ? 0.35 : 1,
                          textDecoration: usedBy ? 'line-through' : 'none',
                        }}
                        title={usedBy ? `${eng.label}는 ${usedBy}로 선택되어 3순위로 중복 선택할 수 없습니다.` : undefined}
                      >
                        {usedBy ? '✕ ' : '● '}
                        {eng.label}
                        {usedBy ? (
                          <small style={{ color: 'var(--err, #dc2626)', marginLeft: '4px' }}>(선택 불가)</small>
                        ) : (
                          <small style={{ opacity: 0.7 }}> ({eng.company})</small>
                        )}
                      </button>
                    );
                  })}
                </div>

                <TierModelRow
                  tier="3순위"
                  engine={thirdEngine}
                  selectedModel={selectedThirdModel}
                  onSelectModel={setSelectedThirdModel}
                  customInput={customThirdModelInput}
                  onCustomInput={setCustomThirdModelInput}
                  resolvedModel={resolvedThirdModel}
                  effort={shownThirdEffort}
                  onEffort={setThirdEffort}
                  placeholder="예: gpt-6.2-sol"
                />
              </>
            )}
          </div>
        </div>

        <div style={{ display: 'flex', gap: '20px', alignItems: 'center' }}>
          <label style={{ display: 'flex', alignItems: 'center', gap: '8px', cursor: 'pointer' }}>
            <input
              type="checkbox"
              checked={enabled}
              onChange={(e) => setEnabled(e.target.checked)}
            />
            <span>활성화 (프로세스 참여)</span>
          </label>
        </div>

        <div style={{ display: 'grid', gap: '8px' }}>
          <strong>연결할 글로벌 스킬</strong>
          {skills.length === 0 ? (
            <span style={{ fontSize: '13px', color: 'var(--muted)' }}>등록된 글로벌 스킬이 없습니다.</span>
          ) : (
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px' }}>
              {skills.map((s) => {
                const checked = selectedSkills.includes(s.id);
                return (
                  <button
                    key={s.id}
                    type="button"
                    onClick={() => toggleSkill(s.id)}
                    className={`res-badge ${checked ? 'res-badge-ref' : 'res-badge-off'}`}
                    style={{ cursor: 'pointer' }}
                  >
                    {checked ? '✓ ' : '+ '} {s.name}
                  </button>
                );
              })}
            </div>
          )}
        </div>

        <div style={{ display: 'grid', gap: '8px' }}>
          <strong>연동할 글로벌 리소스</strong>
          {resources.length === 0 ? (
            <span style={{ fontSize: '13px', color: 'var(--muted)' }}>등록된 글로벌 리소스가 없습니다.</span>
          ) : (
            <div style={{ display: 'flex', flexWrap: 'wrap', gap: '8px' }}>
              {resources.map((r) => {
                const checked = selectedResources.includes(r.id);
                return (
                  <button
                    key={r.id}
                    type="button"
                    onClick={() => toggleResource(r.id)}
                    className={`res-badge ${checked ? 'res-badge-ref' : 'res-badge-off'}`}
                    style={{ cursor: 'pointer' }}
                  >
                    {checked ? '✓ ' : '+ '} {r.name} {r.isMock && '(Mock)'}
                  </button>
                );
              })}
            </div>
          )}
        </div>

        <div style={{ display: 'grid', gap: '6px' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <strong>지침 및 시스템 프롬프트</strong>
            <div style={{ display: 'flex', gap: '4px' }}>
              <button
                type="button"
                className={`res-badge ${promptViewMode === 'editor' ? 'res-badge-ref' : 'res-badge-off'}`}
                style={{ fontSize: '11px', padding: '2px 8px', cursor: 'pointer' }}
                onClick={() => {
                  if (promptViewMode !== 'editor') {
                    setPromptEditorKey((k) => k + 1);
                    setPromptViewMode('editor');
                  }
                }}
              >
                노트 서식
              </button>
              <button
                type="button"
                className={`res-badge ${promptViewMode === 'raw' ? 'res-badge-ref' : 'res-badge-off'}`}
                style={{ fontSize: '11px', padding: '2px 8px', cursor: 'pointer' }}
                onClick={() => setPromptViewMode('raw')}
              >
                마크다운 원문
              </button>
            </div>
          </div>
          {promptViewMode === 'editor' ? (
            <div className="res-prompt-editor">
              <Editor
                key={`${agent?.id || 'new'}-${promptEditorKey}`}
                body={prompt}
                onChange={(text) => setPrompt(text)}
                label="지침 및 시스템 프롬프트"
              />
            </div>
          ) : (
            <textarea
              rows={8}
              value={prompt}
              onChange={(e) => setPrompt(e.target.value)}
              placeholder="에이전트 실행 시 주입할 기본 지침과 행동 원칙을 작성해 주세요."
              className="res-body"
            />
          )}
        </div>

        <div className="page-actions" style={{ marginTop: '8px', display: 'flex', justifyContent: 'flex-end', gap: '8px' }}>
          <button type="button" onClick={onClose} disabled={isSaving}>
            취소
          </button>
          {awaitingApproval && (
            <button type="button" className="button-primary" onClick={openApprovals} disabled={isSaving}>
              승인 대기함에서 보기
            </button>
          )}
          <button
            type="submit"
            className={awaitingApproval ? 'apv-overwrite' : 'button-primary'}
            disabled={isSaving}
          >
            {isSaving ? '저장 중...' : awaitingApproval ? '덮어쓰고 저장' : isEdit ? '수정 내용 저장' : '등록'}
          </button>
        </div>
      </form>
    </Modal>
  );
}
