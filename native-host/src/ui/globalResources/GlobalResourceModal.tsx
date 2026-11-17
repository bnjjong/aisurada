import { useState } from 'react';
import type { FormEvent } from 'react';
import { Modal } from '../Modal.tsx';
import type { GlobalResourceItem, ResourceType } from './types.ts';

interface Props {
  resource?: GlobalResourceItem | null;
  onSave: (item: GlobalResourceItem) => void;
  onClose: () => void;
}

export function GlobalResourceModal({ resource, onSave, onClose }: Props) {
  const isEdit = !!resource;
  const [name, setName] = useState(resource?.name || '');
  const [type, setType] = useState<ResourceType>(resource?.type || 'custom');
  const [isMock, setIsMock] = useState(resource?.isMock ?? false);
  const [endpoint, setEndpoint] = useState(resource?.endpoint || '');
  const [authType, setAuthType] = useState<'none' | 'token' | 'basic' | 'oauth'>(resource?.authType || 'token');
  const [status, setStatus] = useState<'active' | 'inactive' | 'mocking'>(resource?.status || 'active');
  const [description, setDescription] = useState(resource?.description || '');
  const [config, setConfig] = useState(resource?.config || '{\n  \n}');
  const [error, setError] = useState('');

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setError('리소스 이름을 입력해 주세요.');
      return;
    }
    const updated: GlobalResourceItem = {
      id: resource?.id || `gres-${Date.now()}`,
      name: name.trim(),
      type,
      isMock,
      endpoint: endpoint.trim(),
      authType,
      status: isMock ? 'mocking' : status,
      description: description.trim(),
      config: config.trim(),
      createdAt: resource?.createdAt || new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };
    onSave(updated);
    onClose();
  };

  return (
    <Modal
      title={isEdit ? `글로벌 리소스 수정: ${resource.name}` : '새 글로벌 리소스 등록'}
      close={onClose}
      wide
    >
      <form onSubmit={handleSubmit} className="res-form" style={{ display: 'grid', gap: '16px' }}>
        {error && <div className="res-error">{error}</div>}

        <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr', gap: '12px' }}>
          <label style={{ display: 'grid', gap: '6px' }}>
            <strong>리소스 이름</strong>
            <input
              type="text"
              value={name}
              onChange={(e) => setName(e.target.value)}
              placeholder="예: jira (mock)"
              required
            />
          </label>

          <label style={{ display: 'grid', gap: '6px' }}>
            <strong>리소스 유형</strong>
            <select value={type} onChange={(e) => setType(e.target.value as ResourceType)}>
              <option value="jira">Jira (이슈 트래커)</option>
              <option value="task">Aisurada Task (내장 러너)</option>
              <option value="github">GitHub (형상관리/PR)</option>
              <option value="database">Database (데이터베이스)</option>
              <option value="grafana">Grafana (모니터링)</option>
              <option value="custom">직접 지정 (Custom)</option>
            </select>
          </label>
        </div>

        <div style={{ display: 'flex', gap: '24px', alignItems: 'center' }}>
          <label style={{ display: 'flex', alignItems: 'center', gap: '8px', cursor: 'pointer' }}>
            <input
              type="checkbox"
              checked={isMock}
              onChange={(e) => {
                const nextMock = e.target.checked;
                setIsMock(nextMock);
                if (nextMock) setStatus('mocking');
              }}
            />
            <strong>Mock(가상 모드) 사용</strong>
            <span style={{ fontSize: '12px', color: 'var(--muted)' }}>(실제 외부와 직접 통신하지 않음)</span>
          </label>

          <label style={{ display: 'flex', alignItems: 'center', gap: '8px' }}>
            <span>상태:</span>
            <select
              value={status}
              disabled={isMock}
              onChange={(e) => setStatus(e.target.value as 'active' | 'inactive' | 'mocking')}
            >
              <option value="active">활성 (Active)</option>
              <option value="inactive">비활성 (Inactive)</option>
              <option value="mocking">Mocking (가상)</option>
            </select>
          </label>
        </div>

        <div style={{ display: 'grid', gridTemplateColumns: '2fr 1fr', gap: '12px' }}>
          <label style={{ display: 'grid', gap: '6px' }}>
            <strong>엔드포인트 URL / 주소</strong>
            <input
              type="text"
              value={endpoint}
              onChange={(e) => setEndpoint(e.target.value)}
              placeholder="예: https://jira.example.com 또는 localhost:5432"
            />
          </label>

          <label style={{ display: 'grid', gap: '6px' }}>
            <strong>인증 방식</strong>
            <select
              value={authType}
              onChange={(e) => setAuthType(e.target.value as 'none' | 'token' | 'basic' | 'oauth')}
            >
              <option value="none">인증 없음 (None)</option>
              <option value="token">API 토큰 (Token)</option>
              <option value="basic">계정/비밀번호 (Basic)</option>
              <option value="oauth">OAuth / SSO</option>
            </select>
          </label>
        </div>

        <label style={{ display: 'grid', gap: '6px' }}>
          <strong>설명</strong>
          <textarea
            rows={2}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="이 리소스의 용도와 연결 환경을 적어주세요."
          />
        </label>

        <label style={{ display: 'grid', gap: '6px' }}>
          <strong>설정 및 파라미터 (JSON)</strong>
          <textarea
            rows={5}
            value={config}
            onChange={(e) => setConfig(e.target.value)}
            placeholder="연동에 필요한 추가 설정값이나 메타데이터를 JSON 형태로 작성해 주세요."
            className="res-tools"
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
