import { useState } from 'react';
import type { FormEvent } from 'react';
import { EditPendingNotice } from '../ApprovalNotices.tsx';
import { openApprovalsDialog } from '../approvalsDialogStore.ts';
import { Modal } from '../Modal.tsx';
import type { GlobalSkill } from './types.ts';
import { skillFromForm } from './types.ts';

interface Props {
  skill?: GlobalSkill | null;
  onSave: (skill: GlobalSkill) => void;
  onClose: () => void;
}

export function GlobalSkillModal({ skill, onSave, onClose }: Props) {
  const isEdit = !!skill;
  // 승인을 기다리는 변경이 있으면 이 창의 값은 승인해 둔 옛 판이다. 모르고 저장하면 대기 중인 새 판을 덮어쓴다
  const awaitingApproval = isEdit && skill?.approval === 'dirty';
  const openApprovals = () => {
    onClose();
    openApprovalsDialog();
  };
  const [name, setName] = useState(skill?.name || '');
  const [description, setDescription] = useState(skill?.description || '');
  const [tagsText, setTagsText] = useState(skill?.tags.join(', ') || '');
  const [body, setBody] = useState(skill?.body || '');
  const [error, setError] = useState('');

  const handleSubmit = (e: FormEvent) => {
    e.preventDefault();
    if (!name.trim()) {
      setError('스킬 이름을 입력해 주세요.');
      return;
    }
    // 고치는 스킬은 쓰기 전 토큰(f1)을 이어받아야 엔진이 저장을 받는다
    const updated = skillFromForm(skill, { name, description, tagsText, body });
    onSave(updated);
    onClose();
  };

  return (
    <Modal
      title={isEdit ? `글로벌 스킬 수정: ${skill.name}` : '새 글로벌 스킬 등록'}
      close={onClose}
      wide
    >
      <form onSubmit={handleSubmit} className="res-form" style={{ display: 'grid', gap: '16px' }}>
        {error && <div className="res-error">{error}</div>}
        {awaitingApproval && <EditPendingNotice noun="스킬" field="본문" onOpen={openApprovals} />}

        <label style={{ display: 'grid', gap: '6px' }}>
          <strong>스킬 이름</strong>
          <input
            type="text"
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="예: ai-commit"
            required
          />
        </label>

        <label style={{ display: 'grid', gap: '6px' }}>
          <strong>스킬 설명</strong>
          <textarea
            rows={2}
            value={description}
            onChange={(e) => setDescription(e.target.value)}
            placeholder="이 스킬이 수행하는 일과 목적을 적어주세요."
            required
          />
        </label>

        <label style={{ display: 'grid', gap: '6px' }}>
          <strong>태그 (쉼표로 구분)</strong>
          <input
            type="text"
            value={tagsText}
            onChange={(e) => setTagsText(e.target.value)}
            placeholder="예: git, commit, test"
          />
        </label>

        <label style={{ display: 'grid', gap: '6px' }}>
          <strong>스킬 지침 본문 (SKILL.md)</strong>
          <textarea
            rows={8}
            value={body}
            onChange={(e) => setBody(e.target.value)}
            placeholder="# 스킬 실행 가이드&#10;구체적인 절차와 행동 지침을 마크다운으로 작성해 주세요."
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
