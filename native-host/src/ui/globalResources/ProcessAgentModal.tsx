import { useState } from 'react';
import type { FormEvent } from 'react';
import { EditPendingNotice } from '../ApprovalNotices.tsx';
import { openApprovalsDialog } from '../approvalsDialogStore.ts';
import { Modal } from '../Modal.tsx';
import type { AIEngine, GlobalResourceItem, GlobalSkill, ProcessAgent, ReasoningEffort } from './types.ts';
import { AI_ENGINES, ENGINE_MODELS, MODEL_PRESETS, clampEffort, effortInfosFor } from './types.ts';

interface Props {
  agent?: ProcessAgent | null;
  skills: GlobalSkill[];
  resources: GlobalResourceItem[];
  onSave: (agent: ProcessAgent) => void;
  onClose: () => void;
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

  // 2순위 (토큰 소진 대비 폴백) AI 엔진 및 모델
  const [useFallback, setUseFallback] = useState<boolean>(
    agent ? (agent.fallbackEngine != null && agent.fallbackEngine !== initialEngine) : true
  );
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

  const [selectedSkills, setSelectedSkills] = useState<string[]>(agent?.skills || []);
  const [selectedResources, setSelectedResources] = useState<string[]>(agent?.resources || []);
  const [prompt, setPrompt] = useState(agent?.prompt || '');
  const [error, setError] = useState('');

  // 모델마다 고를 수 있는 추론 강도가 달라서, 지금 고른 모델에서 쓸 수 있는 값만 보여 준다.
  // 쓸 수 없는 값이 남아 있으면(모델을 바꾼 경우) default로 보이고 그대로 저장된다
  const resolvedModel = selectedModel === 'custom' ? customModelInput.trim() || 'inherit' : selectedModel;
  const resolvedFallbackModel = selectedFallbackModel === 'custom' ? customFallbackModelInput.trim() || 'inherit' : selectedFallbackModel;
  const shownEffort = clampEffort(engine, resolvedModel, effort);
  const shownFallbackEffort = clampEffort(fallbackEngine, resolvedFallbackModel, fallbackEffort);

  const handleEngineChange = (newEngine: AIEngine) => {
    setEngine(newEngine);
    const defaultModel = MODEL_PRESETS[newEngine][0];
    setSelectedModel(defaultModel);
    setCustomModelInput('');

    // 1순위 엔진과 2순위 엔진이 같아지면 2순위 엔진을 남은 엔진 중 다른 하나로 자동 변경
    if (fallbackEngine === newEngine) {
      const remainingEngines = AI_ENGINES.filter((e) => e.id !== newEngine);
      const nextFallback = remainingEngines[0].id;
      setFallbackEngine(nextFallback);
      setSelectedFallbackModel(MODEL_PRESETS[nextFallback][0]);
      setCustomFallbackModelInput('');
    }
  };

  const handleFallbackEngineChange = (newFallback: AIEngine) => {
    if (newFallback === engine) return; // 1순위와 동일한 엔진은 선택 불가
    setFallbackEngine(newFallback);
    const defaultModel = MODEL_PRESETS[newFallback][0];
    setSelectedFallbackModel(defaultModel);
    setCustomFallbackModelInput('');
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

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setError('에이전트 이름을 입력해 주세요.');
      return;
    }
    const finalModel = selectedModel === 'custom' ? (customModelInput.trim() || 'inherit') : selectedModel;
    const finalFallbackModel =
      selectedFallbackModel === 'custom' ? (customFallbackModelInput.trim() || 'inherit') : selectedFallbackModel;

    const updated: ProcessAgent = {
      id: agent?.id || `proc-${Date.now()}`,
      key: agent?.key || name.trim().toLowerCase().replace(/\s+/g, '-'),
      name: name.trim(),
      role: role.trim(),
      description: description.trim(),
      engine,
      model: finalModel,
      effort: clampEffort(engine, finalModel, effort),
      fallbackEngine: useFallback && fallbackEngine !== engine ? fallbackEngine : undefined,
      fallbackModel: useFallback && fallbackEngine !== engine ? finalFallbackModel : undefined,
      fallbackEffort: useFallback && fallbackEngine !== engine ? clampEffort(fallbackEngine, finalFallbackModel, fallbackEffort) : undefined,
      enabled,
      skills: selectedSkills,
      resources: selectedResources,
      prompt: prompt.trim(),
      order: agent?.order ?? 99,
      f1: agent?.f1,
    };
    onSave(updated);
    onClose();
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
            <span style={{ fontSize: '11px', color: 'var(--muted)' }}>토큰 부족 시 2순위 모델로 자동 대체</span>
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

          {/* 2순위 (폴백) 모델 카드 */}
          <div
            style={{
              display: 'grid',
              gap: '8px',
              padding: '10px 12px',
              background: useFallback ? 'var(--surface, #ffffff)' : 'transparent',
              borderRadius: '6px',
              border: useFallback ? '1px dashed var(--accent, #2563eb)' : '1px dashed var(--line, #e2e8f0)',
              opacity: useFallback ? 1 : 0.7,
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
                    background: useFallback ? 'var(--warning, #d97706)' : 'var(--muted, #94a3b8)',
                    color: '#ffffff',
                  }}
                >
                  2순위
                </span>
                <span style={{ fontSize: '13px', fontWeight: 600 }}>토큰 고갈 대비 대체 모델 (폴백)</span>
              </div>
              <label style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '12px', cursor: 'pointer' }}>
                <input
                  type="checkbox"
                  checked={useFallback}
                  onChange={(e) => setUseFallback(e.target.checked)}
                />
                <span>2순위 대체 모델 사용</span>
              </label>
            </div>

            {useFallback && (
              <>
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

                <div style={{ display: 'flex', gap: '12px', alignItems: 'center', marginTop: '4px', flexWrap: 'wrap' }}>
                  <label style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '13px' }}>
                    <span>2순위 세부 모델:</span>
                    <select
                      value={selectedFallbackModel}
                      onChange={(e) => setSelectedFallbackModel(e.target.value)}
                      style={{ minWidth: '220px', padding: '4px 8px' }}
                    >
                      {ENGINE_MODELS[fallbackEngine].map((m) => (
                        <option key={m.id} value={m.id}>
                          {m.label}
                        </option>
                      ))}
                      <option value="custom">직접 입력...</option>
                    </select>
                  </label>

                  <label style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '13px' }}>
                    <span>2순위 추론 강도 (Effort):</span>
                    <select
                      value={shownFallbackEffort}
                      onChange={(e) => setFallbackEffort(e.target.value as ReasoningEffort)}
                      style={{ minWidth: '150px', padding: '4px 8px' }}
                      title="2순위 대체 모델이 지원하는 Thinking / Reasoning 추론 강도 (Effort)"
                    >
                      {effortInfosFor(fallbackEngine, resolvedFallbackModel).map((eff) => (
                        <option key={eff.id} value={eff.id}>
                          {eff.label}
                        </option>
                      ))}
                    </select>
                  </label>

                  {selectedFallbackModel === 'custom' && (
                    <input
                      type="text"
                      value={customFallbackModelInput}
                      onChange={(e) => setCustomFallbackModelInput(e.target.value)}
                      placeholder="예: gemini-3.8-flash"
                      style={{ flex: 1, minWidth: '200px', padding: '4px 8px' }}
                      required
                    />
                  )}
                </div>
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

        <label style={{ display: 'grid', gap: '6px' }}>
          <strong>지침 및 시스템 프롬프트</strong>
          <textarea
            rows={5}
            value={prompt}
            onChange={(e) => setPrompt(e.target.value)}
            placeholder="에이전트 실행 시 주입할 기본 지침과 행동 원칙을 작성해 주세요."
            className="res-body"
          />
        </label>

        <div className="page-actions" style={{ marginTop: '8px', display: 'flex', justifyContent: 'flex-end', gap: '8px' }}>
          <button type="button" onClick={onClose}>
            취소
          </button>
          {awaitingApproval && (
            <button type="button" className="button-primary" onClick={openApprovals}>
              승인 대기함에서 보기
            </button>
          )}
          <button type="submit" className={awaitingApproval ? 'apv-overwrite' : 'button-primary'}>
            {awaitingApproval ? '덮어쓰고 저장' : isEdit ? '수정 내용 저장' : '등록'}
          </button>
        </div>
      </form>
    </Modal>
  );
}
