import { useState } from 'react';
import type { WorkspaceController } from '../../workspace/controller.ts';
import { Confirm } from '../Modal.tsx';
import { Page } from '../OtherPages.tsx';
import { GlobalResourceModal } from './GlobalResourceModal.tsx';
import { GlobalSkillModal } from './GlobalSkillModal.tsx';
import { ProcessAgentModal } from './ProcessAgentModal.tsx';
import {
  deleteGlobalResourceItem,
  deleteGlobalSkill,
  deleteProcessAgent,
  resetGlobalResourcesToDefault,
  saveGlobalResourceItem,
  saveGlobalSkill,
  saveProcessAgent,
  useGlobalResources,
} from './globalResourcesStore.ts';
import type { GlobalResourceItem, GlobalSkill, ProcessAgent } from './types.ts';

type ActiveTab = 'agents' | 'skills' | 'resources';

export function GlobalResourcesPage({ controller: _controller }: { controller: WorkspaceController }) {
  const data = useGlobalResources();
  const [activeTab, setActiveTab] = useState<ActiveTab>('agents');
  const [search, setSearch] = useState('');

  // 모달 상태
  const [editingAgent, setEditingAgent] = useState<ProcessAgent | null>(null);
  const [isNewAgent, setIsNewAgent] = useState(false);

  const [editingSkill, setEditingSkill] = useState<GlobalSkill | null>(null);
  const [isNewSkill, setIsNewSkill] = useState(false);

  const [editingResource, setEditingResource] = useState<GlobalResourceItem | null>(null);
  const [isNewResource, setIsNewResource] = useState(false);

  // 삭제 확인 모달 상태
  const [deletingItem, setDeletingItem] = useState<{
    type: 'agent' | 'skill' | 'resource';
    id: string;
    name: string;
  } | null>(null);

  // 초기화 확인 모달
  const [confirmReset, setConfirmReset] = useState(false);

  const handleConfirmDelete = () => {
    if (!deletingItem) return;
    if (deletingItem.type === 'agent') {
      deleteProcessAgent(deletingItem.id);
    } else if (deletingItem.type === 'skill') {
      deleteGlobalSkill(deletingItem.id);
    } else if (deletingItem.type === 'resource') {
      deleteGlobalResourceItem(deletingItem.id);
    }
    setDeletingItem(null);
  };

  // 필터링
  const lowerSearch = search.toLowerCase();
  const filteredAgents = data.agents.filter(
    (a) =>
      a.name.toLowerCase().includes(lowerSearch) ||
      a.role.toLowerCase().includes(lowerSearch) ||
      a.description.toLowerCase().includes(lowerSearch)
  );
  const filteredSkills = data.skills.filter(
    (s) =>
      s.name.toLowerCase().includes(lowerSearch) ||
      s.description.toLowerCase().includes(lowerSearch) ||
      s.tags.some((t) => t.toLowerCase().includes(lowerSearch))
  );
  const filteredResources = data.resources.filter(
    (r) =>
      r.name.toLowerCase().includes(lowerSearch) ||
      r.type.toLowerCase().includes(lowerSearch) ||
      r.description.toLowerCase().includes(lowerSearch)
  );

  return (
    <Page title="글로벌 리소스 정의" className="res-page">
      <p className="res-intro">
        공통 개발 프로세스 에이전트, 글로벌 스킬, 외부 연동 리소스를 등록하고 관리해요.
        외부 시스템과 <b>직접 연동하지 않고</b> 정의와 설정값을 관리하며 로컬에 안전하게 영속화돼요.
      </p>

      {/* 상단 탭 전환 & 툴바 */}
      <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', marginBottom: '16px', flexWrap: 'wrap', gap: '12px' }}>
        <div role="tablist" style={{ display: 'flex', gap: '4px', background: 'var(--surface-sunken, #f0f0f0)', padding: '4px', borderRadius: '8px' }}>
          <button
            type="button"
            role="tab"
            aria-selected={activeTab === 'agents'}
            className={activeTab === 'agents' ? 'button-primary' : ''}
            style={{ padding: '6px 14px', fontSize: '13px', borderRadius: '6px' }}
            onClick={() => setActiveTab('agents')}
          >
            개발 프로세스 에이전트 ({data.agents.length})
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={activeTab === 'skills'}
            className={activeTab === 'skills' ? 'button-primary' : ''}
            style={{ padding: '6px 14px', fontSize: '13px', borderRadius: '6px' }}
            onClick={() => setActiveTab('skills')}
          >
            글로벌 스킬 ({data.skills.length})
          </button>
          <button
            type="button"
            role="tab"
            aria-selected={activeTab === 'resources'}
            className={activeTab === 'resources' ? 'button-primary' : ''}
            style={{ padding: '6px 14px', fontSize: '13px', borderRadius: '6px' }}
            onClick={() => setActiveTab('resources')}
          >
            글로벌 리소스 연동 ({data.resources.length})
          </button>
        </div>

        <div style={{ display: 'flex', gap: '8px', alignItems: 'center' }}>
          <input
            type="search"
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder="이름 또는 설명 검색..."
            style={{ padding: '6px 10px', fontSize: '13px', borderRadius: '6px', border: '1px solid var(--line)' }}
          />
          <button
            type="button"
            onClick={() => setConfirmReset(true)}
            title="기본 10대 에이전트 및 샘플 리소스로 초기화"
            style={{ padding: '6px 10px', fontSize: '12px' }}
          >
            기본값 초기화
          </button>
        </div>
      </div>

      {/* ---------------- 1. 프로세스 에이전트 탭 ---------------- */}
      {activeTab === 'agents' && (
        <section className="res-section" aria-label="프로세스 에이전트">
          <header className="res-section-head">
            <div>
              <h2>공통 개발 프로세스 에이전트</h2>
              <span style={{ fontSize: '12px', color: 'var(--muted)' }}>
                오케스트레이터를 비롯한 10대 핵심 전문 에이전트의 역할과 지침을 관리합니다.
              </span>
            </div>
            <button
              type="button"
              className="button-primary"
              onClick={() => {
                setEditingAgent(null);
                setIsNewAgent(true);
              }}
            >
              새 프로세스 에이전트 추가
            </button>
          </header>

          {filteredAgents.length === 0 ? (
            <p className="res-empty">등록된 에이전트가 없거나 검색 결과가 없습니다.</p>
          ) : (
            <ul className="res-list">
              {filteredAgents.map((agent) => {
                const assignedSkillNames = agent.skills
                  .map((sId) => data.skills.find((s) => s.id === sId)?.name)
                  .filter(Boolean);
                const assignedResourceNames = agent.resources
                  .map((rId) => data.resources.find((r) => r.id === rId)?.name)
                  .filter(Boolean);

                return (
                  <li key={agent.id} className="res-item" style={{ opacity: agent.enabled ? 1 : 0.65 }}>
                    <div className="res-main">
                      <p className="res-title">
                        <strong className="res-name">{agent.name}</strong>
                        <span className="res-kind">모델: {agent.model}</span>
                        {!agent.enabled && <span className="res-kind" style={{ color: 'var(--err)' }}>비활성</span>}
                      </p>
                      {agent.role && <p className="res-desc" style={{ fontWeight: 500, color: 'var(--ink)' }}>{agent.role}</p>}
                      {agent.description && <p className="res-desc">{agent.description}</p>}
                      <div className="res-meta" style={{ display: 'flex', flexWrap: 'wrap', gap: '8px', marginTop: '6px' }}>
                        {assignedSkillNames.length > 0 && (
                          <span>스킬: {assignedSkillNames.join(', ')}</span>
                        )}
                        {assignedResourceNames.length > 0 && (
                          <span>연동: {assignedResourceNames.join(', ')}</span>
                        )}
                      </div>
                    </div>
                    <div className="res-actions">
                      <button
                        type="button"
                        onClick={() => {
                          setEditingAgent(agent);
                          setIsNewAgent(false);
                        }}
                      >
                        편집
                      </button>
                      <button
                        type="button"
                        onClick={() =>
                          setDeletingItem({
                            type: 'agent',
                            id: agent.id,
                            name: agent.name,
                          })
                        }
                      >
                        삭제
                      </button>
                    </div>
                  </li>
                );
              })}
            </ul>
          )}
        </section>
      )}

      {/* ---------------- 2. 글로벌 스킬 탭 ---------------- */}
      {activeTab === 'skills' && (
        <section className="res-section" aria-label="글로벌 스킬">
          <header className="res-section-head">
            <div>
              <h2>글로벌 스킬 N개 등록 및 관리</h2>
              <span style={{ fontSize: '12px', color: 'var(--muted)' }}>
                모든 에이전트가 공유하여 활용할 수 있는 글로벌 스킬을 자유롭게 등록합니다.
              </span>
            </div>
            <button
              type="button"
              className="button-primary"
              onClick={() => {
                setEditingSkill(null);
                setIsNewSkill(true);
              }}
            >
              새 글로벌 스킬 추가
            </button>
          </header>

          {filteredSkills.length === 0 ? (
            <p className="res-empty">등록된 글로벌 스킬이 없거나 검색 결과가 없습니다.</p>
          ) : (
            <ul className="res-list">
              {filteredSkills.map((skill) => (
                <li key={skill.id} className="res-item">
                  <div className="res-main">
                    <p className="res-title">
                      <strong className="res-name">{skill.name}</strong>
                      <span className="res-kind">Skill</span>
                      {skill.tags.map((tag) => (
                        <span key={tag} className="res-badge res-badge-off">
                          #{tag}
                        </span>
                      ))}
                    </p>
                    {skill.description && <p className="res-desc">{skill.description}</p>}
                    {skill.body && (
                      <p className="res-meta" style={{ fontFamily: 'var(--font-mono, monospace)', whiteSpace: 'pre-line' }}>
                        {skill.body.slice(0, 120)}
                        {skill.body.length > 120 ? '...' : ''}
                      </p>
                    )}
                  </div>
                  <div className="res-actions">
                    <button
                      type="button"
                      onClick={() => {
                        setEditingSkill(skill);
                        setIsNewSkill(false);
                      }}
                    >
                      편집
                    </button>
                    <button
                      type="button"
                      onClick={() =>
                        setDeletingItem({
                          type: 'skill',
                          id: skill.id,
                          name: skill.name,
                        })
                      }
                    >
                      삭제
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}

      {/* ---------------- 3. 글로벌 리소스 탭 ---------------- */}
      {activeTab === 'resources' && (
        <section className="res-section" aria-label="글로벌 리소스">
          <header className="res-section-head">
            <div>
              <h2>글로벌 리소스 (인프라 / 도구 연동)</h2>
              <span style={{ fontSize: '12px', color: 'var(--muted)' }}>
                Jira(Mock), Aisurada Task, GitHub, Database, Grafana(Mock) 등 외부 도구 설정을 정의합니다.
              </span>
            </div>
            <button
              type="button"
              className="button-primary"
              onClick={() => {
                setEditingResource(null);
                setIsNewResource(true);
              }}
            >
              새 글로벌 리소스 추가
            </button>
          </header>

          {filteredResources.length === 0 ? (
            <p className="res-empty">등록된 글로벌 리소스가 없거나 검색 결과가 없습니다.</p>
          ) : (
            <ul className="res-list">
              {filteredResources.map((res) => (
                <li key={res.id} className="res-item">
                  <div className="res-main">
                    <p className="res-title">
                      <strong className="res-name">{res.name}</strong>
                      <span className="res-kind" style={{ textTransform: 'uppercase' }}>
                        {res.type}
                      </span>
                      {res.isMock && (
                        <span className="res-badge" style={{ background: 'var(--soft)', color: 'var(--ink)' }}>
                          Mock (가상)
                        </span>
                      )}
                      <span
                        className="res-badge"
                        style={{
                          background:
                            res.status === 'active'
                              ? 'var(--ok-bg)'
                              : res.status === 'mocking'
                              ? 'var(--warn-bg, #fff3cd)'
                              : 'var(--err-bg, #f8d7da)',
                          color:
                            res.status === 'active'
                              ? 'var(--ok)'
                              : res.status === 'mocking'
                              ? 'var(--warn, #856404)'
                              : 'var(--err, #721c24)',
                        }}
                      >
                        {res.status}
                      </span>
                    </p>
                    {res.description && <p className="res-desc">{res.description}</p>}
                    <div className="res-meta" style={{ display: 'flex', gap: '16px', marginTop: '6px' }}>
                      {res.endpoint && <span>엔드포인트: {res.endpoint}</span>}
                      <span>인증: {res.authType}</span>
                    </div>
                  </div>
                  <div className="res-actions">
                    <button
                      type="button"
                      onClick={() => {
                        setEditingResource(res);
                        setIsNewResource(false);
                      }}
                    >
                      편집
                    </button>
                    <button
                      type="button"
                      onClick={() =>
                        setDeletingItem({
                          type: 'resource',
                          id: res.id,
                          name: res.name,
                        })
                      }
                    >
                      삭제
                    </button>
                  </div>
                </li>
              ))}
            </ul>
          )}
        </section>
      )}

      {/* 모달 창들 */}
      {(isNewAgent || editingAgent) && (
        <ProcessAgentModal
          agent={editingAgent}
          skills={data.skills}
          resources={data.resources}
          onSave={(agent) => saveProcessAgent(agent)}
          onClose={() => {
            setEditingAgent(null);
            setIsNewAgent(false);
          }}
        />
      )}

      {(isNewSkill || editingSkill) && (
        <GlobalSkillModal
          skill={editingSkill}
          onSave={(skill) => saveGlobalSkill(skill)}
          onClose={() => {
            setEditingSkill(null);
            setIsNewSkill(false);
          }}
        />
      )}

      {(isNewResource || editingResource) && (
        <GlobalResourceModal
          resource={editingResource}
          onSave={(item) => saveGlobalResourceItem(item)}
          onClose={() => {
            setEditingResource(null);
            setIsNewResource(false);
          }}
        />
      )}

      {/* 삭제 확인 모달 */}
      {deletingItem && (
        <Confirm
          title={`${deletingItem.name} 항목을 삭제할까요?`}
          description="삭제하면 목록에서 즉시 제거되며 로컬 스토리지에 반영됩니다."
          label="삭제하기"
          close={() => setDeletingItem(null)}
          accept={handleConfirmDelete}
        />
      )}

      {/* 기본값 초기화 모달 */}
      {confirmReset && (
        <Confirm
          title="기본 데이터로 초기화할까요?"
          description="현재 등록된 내용이 10대 프로세스 에이전트 및 기본 글로벌 리소스(Jira mock, Aisurada task, GitHub, Database, Grafana mock 등)의 기본 사양으로 재설정됩니다."
          label="기본값 복원"
          close={() => setConfirmReset(false)}
          accept={() => {
            resetGlobalResourcesToDefault();
            setConfirmReset(false);
          }}
        />
      )}
    </Page>
  );
}
