import { useState } from 'react';
import type { FormEvent } from 'react';
import { Modal } from '../Modal.tsx';
import type { AIEngine, GlobalResourceItem, GlobalSkill, ProcessAgent } from './types.ts';
import { AI_ENGINES, MODEL_PRESETS } from './types.ts';

interface Props {
  agent?: ProcessAgent | null;
  skills: GlobalSkill[];
  resources: GlobalResourceItem[];
  onSave: (agent: ProcessAgent) => void;
  onClose: () => void;
}

export function ProcessAgentModal({ agent, skills, resources, onSave, onClose }: Props) {
  const isEdit = !!agent;
  const [name, setName] = useState(agent?.name || '');
  const [role, setRole] = useState(agent?.role || '');
  const [description, setDescription] = useState(agent?.description || '');
  const [enabled, setEnabled] = useState(agent?.enabled ?? true);
  
  // AI 엔진 및 모델 세분화
  const initialEngine: AIEngine = agent?.engine || 'gemini';
  const [engine, setEngine] = useState<AIEngine>(initialEngine);
  const initialModel = agent?.model || MODEL_PRESETS[initialEngine][0];
  const isPreset = MODEL_PRESETS[initialEngine].includes(initialModel);
  const [selectedModel, setSelectedModel] = useState(isPreset ? initialModel : 'custom');
  const [customModelInput, setCustomModelInput] = useState(isPreset ? '' : initialModel);

  const [selectedSkills, setSelectedSkills] = useState<string[]>(agent?.skills || []);
  const [selectedResources, setSelectedResources] = useState<string[]>(agent?.resources || []);
  const [prompt, setPrompt] = useState(agent?.prompt || '');
  const [error, setError] = useState('');

  const handleEngineChange = (newEngine: AIEngine) => {
    setEngine(newEngine);
    const defaultModel = MODEL_PRESETS[newEngine][0];
    setSelectedModel(defaultModel);
    setCustomModelInput('');
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

    const updated: ProcessAgent = {
      id: agent?.id || `proc-${Date.now()}`,
      key: agent?.key || name.trim().toLowerCase().replace(/\s+/g, '-'),
      name: name.trim(),
      role: role.trim(),
      description: description.trim(),
      engine,
      model: finalModel,
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

        {/* AI 엔진 및 세부 모델 설정 */}
        <div style={{ display: 'grid', gap: '8px', padding: '12px', background: 'var(--surface-sunken, #f8f9fa)', borderRadius: '8px', border: '1px solid var(--border-subtle, #e5e7eb)' }}>
          <strong style={{ fontSize: '13px' }}>AI 실행 엔진 및 모델</strong>
          
          <div style={{ display: 'flex', gap: '8px', flexWrap: 'wrap' }}>
            {AI_ENGINES.map((eng) => (
              <button
                key={eng.id}
                type="button"
                className={`res-badge ${engine === eng.id ? 'res-badge-ref' : 'res-badge-off'}`}
                style={{ padding: '6px 14px', cursor: 'pointer', fontSize: '13px' }}
                onClick={() => handleEngineChange(eng.id)}
              >
                {engine === eng.id ? '● ' : '○ '} {eng.label} <small style={{ opacity: 0.7 }}>({eng.company})</small>
              </button>
            ))}
          </div>

          <div style={{ display: 'flex', gap: '10px', alignItems: 'center', marginTop: '6px', flexWrap: 'wrap' }}>
            <label style={{ display: 'flex', alignItems: 'center', gap: '6px', fontSize: '13px' }}>
              <span>세부 모델:</span>
              <select
                value={selectedModel}
                onChange={(e) => setSelectedModel(e.target.value)}
                style={{ minWidth: '180px', padding: '4px 8px' }}
              >
                {MODEL_PRESETS[engine].map((m) => (
                  <option key={m} value={m}>
                    {m === 'inherit' ? '기본 모델 (inherit)' : m}
                  </option>
                ))}
                <option value="custom">직접 입력...</option>
              </select>
            </label>

            {selectedModel === 'custom' && (
              <input
                type="text"
                value={customModelInput}
                onChange={(e) => setCustomModelInput(e.target.value)}
                placeholder="예: claude-3-7-sonnet-20250219"
                style={{ flex: 1, minWidth: '200px', padding: '4px 8px' }}
                required
              />
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
          <button type="submit" className="button-primary">
            {isEdit ? '수정 내용 저장' : '등록'}
          </button>
        </div>
      </form>
    </Modal>
  );
}
