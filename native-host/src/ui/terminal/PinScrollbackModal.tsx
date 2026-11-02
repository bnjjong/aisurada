// Pinned note modal according to Chapter 14 (14.9, 14.10, 14.12, C32).
import React, { useState, useEffect, useRef } from 'react';
import type { MaskedResult, SecretTableStatus } from './ptyBridge.ts';
import { secretTableProbe, secretTableStatus, maskScrollback } from './ptyBridge.ts';
import { sliceLogicalLines, formatPinnedNoteBody } from './logicalLines.ts';
import type { Agenda } from '../../workspace/types.ts';

interface PinScrollbackModalProps {
  logicalLines: string[];
  selectionIndices?: [number, number];
  hadTui: boolean;
  agendas: Agenda[];
  onClose: () => void;
  onCreateDoc: (agendaSlug: string, title: string, body: string) => Promise<{ ok: boolean; id?: string; error?: string }>;
  onNavigate: (path: string) => void;
}

export function PinScrollbackModal({
  logicalLines,
  selectionIndices,
  hadTui,
  agendas,
  onClose,
  onCreateDoc,
  onNavigate,
}: PinScrollbackModalProps): React.JSX.Element {
  const linesSnapshot = useRef(logicalLines).current;
  const [range, setRange] = useState<'100' | '500' | '2000' | '10000' | 'selection'>('500');
  const now = new Date();
  const hh = String(now.getHours()).padStart(2, '0');
  const mm = String(now.getMinutes()).padStart(2, '0');
  const defaultTitle = `터미널 기록 ${hh}:${mm}`;

  const [title, setTitle] = useState(defaultTitle);
  const [agendaSlug, setAgendaSlug] = useState<string>(agendas[0]?.slug || '');
  const [tableStatus, setTableStatus] = useState<SecretTableStatus | null>(null);
  const [masked, setMasked] = useState<MaskedResult | null>(null);
  const [loading, setLoading] = useState(true);
  const [errorMsg, setErrorMsg] = useState<string | null>(null);
  const [submitting, setSubmitting] = useState(false);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') onClose();
    };
    window.addEventListener('keydown', handleKeyDown);
    return () => window.removeEventListener('keydown', handleKeyDown);
  }, [onClose]);

  // Initial probe and mask using snapshot lines
  useEffect(() => {
    let active = true;
    async function init() {
      setLoading(true);
      const status = await secretTableStatus();
      if (!active) return;
      setTableStatus(status);

      const sliced = sliceLogicalLines(linesSnapshot, '500', selectionIndices);
      try {
        const res = await maskScrollback(sliced);
        if (active) {
          setMasked(res);
          setErrorMsg(null);
        }
      } catch (err: any) {
        if (active) {
          setErrorMsg(err?.message || String(err));
          setMasked(null);
        }
      } finally {
        if (active) setLoading(false);
      }
    }
    init();
    return () => { active = false; };
  }, [linesSnapshot, selectionIndices]);

  async function handleRangeChange(newRange: '100' | '500' | '2000' | '10000' | 'selection') {
    setRange(newRange);
    setLoading(true);
    const sliced = sliceLogicalLines(linesSnapshot, newRange, selectionIndices);
    try {
      const res = await maskScrollback(sliced);
      setMasked(res);
      setErrorMsg(null);
    } catch (err: any) {
      setErrorMsg(err?.message || String(err));
      setMasked(null);
    } finally {
      setLoading(false);
    }
  }

  async function handleReprobe() {
    setLoading(true);
    const status = await secretTableProbe();
    setTableStatus(status);
    const sliced = sliceLogicalLines(linesSnapshot, range, selectionIndices);
    try {
      const res = await maskScrollback(sliced);
      setMasked(res);
      setErrorMsg(null);
    } catch (err: any) {
      setErrorMsg(err?.message || String(err));
      setMasked(null);
    } finally {
      setLoading(false);
    }
  }

  const currentLines = sliceLogicalLines(linesSnapshot, range, selectionIndices);
  const shapesSum = masked ? masked.shapes.reduce((sum, s) => sum + s.count, 0) : 0;

  const currentBody = masked ? formatPinnedNoteBody({
    title,
    linesSent: currentLines.length,
    masked,
    hadTui,
  }) : '';

  const bodyBytes = new TextEncoder().encode(currentBody).length;
  const canSubmit = !!masked && bodyBytes <= 300000 && !!agendaSlug && !loading && !submitting && title.trim().length > 0;

  async function handleSubmit() {
    if (!canSubmit || !masked) return;
    setSubmitting(true);
    setErrorMsg(null);
    try {
      const res = await onCreateDoc(agendaSlug, title.trim(), currentBody);
      if (res.ok && res.id) {
        onClose();
        onNavigate(`/doc/${res.id}`);
      } else {
        // C32: If create-doc fails, keep modal open and display error inline
        setErrorMsg(res.error || '노트 생성에 실패했어요.');
      }
    } catch (err: any) {
      setErrorMsg(err?.message || '노트 생성에 실패했어요.');
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <div className="modal-backdrop" onClick={onClose} style={{
      position: 'fixed', inset: 0, background: 'rgba(0,0,0,0.5)', zIndex: 1000,
      display: 'flex', alignItems: 'center', justifyContent: 'center'
    }}>
      <div className="modal-box pin-modal" onClick={e => e.stopPropagation()} style={{
        background: 'var(--bg-primary, #fff)', borderRadius: 8, padding: 24, width: 560, maxWidth: '90vw', maxHeight: '85vh', overflowY: 'auto'
      }}>
        <div className="modal-head" style={{ marginBottom: 16 }}>
          <h3 style={{ margin: 0, fontSize: 18 }}>터미널 스크롤백 굳히기</h3>
          <p style={{ margin: '4px 0 0', color: 'var(--text-muted, #888)', fontSize: 13 }}>
            스크롤백을 값 일치 마스킹을 거쳐 기록 노트로 저장합니다.
          </p>
        </div>

        {/* Range Selection */}
        <div className="pin-ranges" style={{ display: 'flex', gap: 6, marginBottom: 12, flexWrap: 'wrap' }}>
          {(['100', '500', '2000', '10000'] as const).map(r => (
            <button
              key={r}
              type="button"
              disabled={loading || submitting}
              className={`btn btn-sm ${range === r ? 'btn-active' : ''}`}
              onClick={() => handleRangeChange(r)}
            >
              마지막 {r === '10000' ? '10,000' : r === '2000' ? '2,000' : r}줄
            </button>
          ))}
          <button
            type="button"
            disabled={loading || submitting || hadTui || !selectionIndices}
            className={`btn btn-sm ${range === 'selection' ? 'btn-active' : ''}`}
            onClick={() => handleRangeChange('selection')}
          >
            고른 부분 (줄 단위)
          </button>
        </div>

        <div className="pin-hint" style={{ fontSize: 13, color: 'var(--text-secondary, #666)', marginBottom: 12 }}>
          실제로 굳힐 줄은 {currentLines.length}줄이에요. 빈 꼬리는 빼고 세요.
        </div>

        {/* Secret Table Status */}
        <div className="pin-status" style={{ fontSize: 13, background: 'var(--bg-secondary, #f5f5f5)', padding: 10, borderRadius: 6, marginBottom: 12 }}>
          {tableStatus?.ready ? (
            <div>가릴 수 있는 값 {tableStatus.maskable}종. 물어본 이름 {tableStatus.asked}중 {tableStatus.read}개를 읽었어요.</div>
          ) : (
            <div>
              표가 없어 굳힐 수 없어요. {tableStatus?.reason ? tableStatus.reason : '표를 아직 안 떴어요.'}
              <button type="button" onClick={handleReprobe} disabled={loading} style={{ marginLeft: 8 }} className="btn btn-xs">
                표 다시 뜨기
              </button>
            </div>
          )}
        </div>

        {/* Mask Result Summary */}
        {masked && (
          <div className="pin-summary" style={{ marginBottom: 14 }}>
            <table style={{ width: '100%', fontSize: 13, borderCollapse: 'collapse', textAlign: 'left' }}>
              <tbody>
                <tr><th style={{ padding: '4px 8px' }}>가린 자리</th><td style={{ padding: '4px 8px' }}>{masked.hits}</td></tr>
                <tr><th style={{ padding: '4px 8px' }}>가린 값 종수</th><td style={{ padding: '4px 8px' }}>{masked.maskedValues}</td></tr>
                <tr><th style={{ padding: '4px 8px' }}>표에 있던 값</th><td style={{ padding: '4px 8px' }}>{masked.tableSize}종</td></tr>
                <tr><th style={{ padding: '4px 8px' }}>모양으로 걸린 것</th><td style={{ padding: '4px 8px' }}>{shapesSum}건</td></tr>
              </tbody>
            </table>
          </div>
        )}

        {/* Warnings & Notices */}
        {masked && (
          <div className="pin-warn" style={{ fontSize: 12, display: 'flex', flexDirection: 'column', gap: 6, marginBottom: 14 }}>
            {masked.short.length > 0 && (
              <div style={{ color: '#d97706' }}>
                짧아서 못 지킨 이름 {masked.short.length}개: {masked.short.join(', ')}
              </div>
            )}
            {masked.unreadable.length > 0 && (
              <div style={{ color: '#dc2626' }}>
                값을 못 읽은 이름 {masked.unreadable.length}개: {masked.unreadable.join(', ')}
              </div>
            )}
            {shapesSum > 0 ? (
              <div style={{ color: '#dc2626' }}>
                표에 없는데 비밀처럼 생긴 것이 {shapesSum}건이에요. 안 가려졌어요.
              </div>
            ) : (
              <div style={{ color: '#059669' }}>
                모양 검사를 지났고 걸린 것이 0건이에요. 채택한 패턴 밖의 모양은 못 봐요.
              </div>
            )}
            {hadTui && (
              <div style={{ color: '#d97706' }}>
                굳힐 때 TUI가 떠 있었어요. 여기 담기는 것은 그 아래 쌓인 기록이라 보던 화면과 달라요.
              </div>
            )}
          </div>
        )}

        {errorMsg && (
          <div className="pin-error" style={{ color: '#dc2626', fontSize: 13, marginBottom: 12 }}>
            {errorMsg}
          </div>
        )}

        {/* Form: Title and Agenda */}
        <div className="form-group" style={{ marginBottom: 12 }}>
          <label style={{ display: 'block', fontSize: 13, marginBottom: 4 }}>제목</label>
          <input
            type="text"
            className="ndf-input input"
            value={title}
            onChange={e => setTitle(e.target.value)}
            style={{ width: '100%', boxSizing: 'border-box' }}
          />
        </div>

        <div className="form-group ndf-select" style={{ marginBottom: 16 }}>
          <label style={{ display: 'block', fontSize: 13, marginBottom: 4 }}>아젠다</label>
          <select
            value={agendaSlug}
            onChange={e => setAgendaSlug(e.target.value)}
            style={{ width: '100%', padding: '6px 8px' }}
          >
            {agendas.map(ag => (
              <option key={ag.slug} value={ag.slug}>{ag.title || ag.slug}</option>
            ))}
          </select>
        </div>

        {/* Footer */}
        <div className="ndf-foot" style={{ display: 'flex', justifyContent: 'flex-end', gap: 8 }}>
          <button type="button" className="btn btn-secondary" onClick={onClose} disabled={submitting}>
            닫기
          </button>
          <button
            type="button"
            className="btn btn-primary"
            onClick={handleSubmit}
            disabled={!canSubmit}
          >
            {submitting ? '저장 중…' : '굳히기'}
          </button>
        </div>
      </div>
    </div>
  );
}
