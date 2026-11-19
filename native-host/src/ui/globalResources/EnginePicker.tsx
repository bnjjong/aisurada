import type { ReactNode } from 'react';
import { AgentGlyph } from '../AgentMark.tsx';
import type { AIEngine, ReasoningEffort } from './types.ts';
import { AI_ENGINES, ENGINE_MODELS, effortInfosFor } from './types.ts';

/**
 * 에이전트 편집 창의 「AI 실행 엔진 및 모델」 영역을 이루는 부품들(모양은 engine-picker.css).
 * 상태와 저장은 ProcessAgentModal이 쥐고, 여기는 값을 받아 그리기만 한다.
 */

export type Tier = 1 | 2 | 3;

/** 「Sonnet 5.5 (권장, 코딩…)」을 이름과 설명으로 나눈다. 괄호가 없으면 설명은 빈 글이다. */
export function splitLabel(label: string): { name: string; desc: string } {
  const m = /^(.*?)\s*\((.*)\)\s*$/.exec(label);
  if (!m) return { name: label, desc: '' };
  return { name: m[1], desc: m[2].replace(/,\s*/g, ' · ') };
}

/** 목록에 있는 모델이면 짧은 이름, 직접 입력한 모델이면 적은 그대로. */
export function modelName(engine: AIEngine, model: string): string {
  const found = ENGINE_MODELS[engine].find((m) => m.id === model);
  return found ? splitLabel(found.label).name : model;
}

const engineLabel = (engine: AIEngine) => AI_ENGINES.find((e) => e.id === engine)?.label ?? engine;
const engineCompany = (engine: AIEngine) => AI_ENGINES.find((e) => e.id === engine)?.company ?? '';

const icon = { viewBox: '0 0 16 16', fill: 'none', stroke: 'currentColor', strokeWidth: 1.6, strokeLinecap: 'round', strokeLinejoin: 'round', 'aria-hidden': true } as const;

function CheckIcon() {
  return (
    <svg {...icon} width="12" height="12" stroke="currentColor" strokeWidth={2.4}>
      <path d="M3.5 8.5l3 3 6-7" />
    </svg>
  );
}
function LockIcon() {
  return (
    <svg {...icon} width="11" height="11">
      <rect x="3" y="7" width="10" height="7" rx="2" />
      <path d="M5.5 7V5.2a2.5 2.5 0 015 0V7" />
    </svg>
  );
}
function ChevronIcon() {
  return (
    <svg {...icon} width="14" height="14">
      <path d="M4 6l4 4 4-4" />
    </svg>
  );
}
function ChipIcon() {
  return (
    <svg {...icon} width="15" height="15" strokeWidth={1.5}>
      <rect x="3.5" y="3.5" width="9" height="9" rx="2" />
      <rect x="6.2" y="6.2" width="3.6" height="3.6" rx=".8" />
      <path d="M6 1.5v2M10 1.5v2M6 12.5v2M10 12.5v2M1.5 6h2M1.5 10h2M12.5 6h2M12.5 10h2" />
    </svg>
  );
}
function DownIcon() {
  return (
    <svg {...icon} width="12" height="12">
      <path d="M8 2.5v10M4 9l4 4 4-4" />
    </svg>
  );
}
function ArrowIcon() {
  return (
    <svg {...icon} width="12" height="12">
      <path d="M2 8h11M9 4l4 4-4 4" />
    </svg>
  );
}
/** 제목 앞의 반짝이 아이콘. */
export function SparkIcon() {
  return (
    <svg viewBox="0 0 16 16" width="15" height="15" fill="currentColor" aria-hidden="true">
      <path d="M8 1.2C8.4 5 10.6 7.4 14.8 8C10.6 8.6 8.4 11 8 14.8C7.6 11 5.4 8.6 1.2 8C5.4 7.4 7.6 5 8 1.2Z" />
    </svg>
  );
}

/** 순위별 설정 상세를 접고 펴는 단추. [locked]이면 접을 수 없다(채워야 하는 칸이 펼쳐져 있어야 해서). */
export function EngineFold({
  open,
  locked,
  controls,
  onToggle,
}: {
  open: boolean;
  locked: boolean;
  /** 접고 펴는 영역의 id */
  controls: string;
  onToggle: () => void;
}) {
  return (
    <button
      type="button"
      className="eng-fold"
      aria-expanded={open}
      aria-controls={controls}
      disabled={locked}
      title={locked ? '직접 입력한 모델 이름을 채우면 접을 수 있습니다' : undefined}
      onClick={onToggle}
    >
      {open ? '설정 접기' : '설정 펼치기'}
      <ChevronIcon />
    </button>
  );
}

/**
 * 엔진 로고를 둥근 네모 타일에 담는다. 모양은 노트 목록의 출처 아이콘(AgentMark)과 같다.
 * [bare]이면 타일 없이 아이콘만 [size] 안에 가득 그린다(순서 요약처럼 좁은 줄에서 쓴다).
 */
export function EngineLogo({ engine, size = 22, bare = false }: { engine: AIEngine; size?: number; bare?: boolean }) {
  return (
    <span
      className={`eng-logo eng-logo-${engine}${bare ? ' eng-logo-bare' : ''}`}
      style={{ width: size, height: size, borderRadius: Math.round(size * 0.28) }}
      aria-hidden="true"
    >
      <AgentGlyph agent={engine} size={bare ? size : Math.round(size * 0.58)} />
    </span>
  );
}

/** 추론 강도 막대 눈금: low 1칸에서 max 5칸까지. default는 점선, ultra는 5칸에 다른 빛깔. */
const EFFORT_LEVEL: Record<ReasoningEffort, number> = { default: 0, low: 1, medium: 2, high: 3, xhigh: 4, max: 5, ultra: 5 };

export function EffortMeter({ effort, compact = false }: { effort: ReasoningEffort; compact?: boolean }) {
  const level = EFFORT_LEVEL[effort] ?? 0;
  return (
    <span
      className={`eng-meter${level === 0 ? ' is-default' : ''}${effort === 'ultra' ? ' is-ultra' : ''}${compact ? ' is-compact' : ''}`}
      aria-hidden="true"
    >
      {[1, 2, 3, 4, 5].map((i) => (
        <i key={i} className={i <= level ? 'on' : undefined} style={{ height: compact ? 3 + i * 1.6 : 4 + i * 2 }} />
      ))}
    </span>
  );
}

/** 한 순위의 엔진을 고르는 로고 타일 셋. [usedBy]가 순위를 돌려주는 엔진은 그 순위가 이미 써서 못 고른다. */
export function EngineTiles({
  tier,
  selected,
  usedBy,
  onSelect,
}: {
  tier: Tier;
  selected: AIEngine;
  usedBy: (engine: AIEngine) => Tier | null;
  /** 없으면 고르는 칸이 아니라 정해진 값을 보여 주는 칸이다(3순위). */
  onSelect?: (engine: AIEngine) => void;
}) {
  return (
    <div className="eng-tiles" role="radiogroup" aria-label={`${tier}순위 엔진`}>
      {AI_ENGINES.map((eng) => {
        const by = usedBy(eng.id);
        const on = by === null && selected === eng.id;
        const cls = `eng-tile${on ? ' is-on' : ''}${by ? ' is-used' : ''}`;
        const body = (
          <>
            <EngineLogo engine={eng.id} size={36} />
            <span className="eng-tile-nm">
              <b>{eng.label}</b>
              <small>{eng.company}</small>
            </span>
            {by ? (
              <span className="eng-used-tag">
                <LockIcon />
                {by}순위 사용 중
              </span>
            ) : (
              <span className="eng-radio" aria-hidden="true">
                {on && <CheckIcon />}
              </span>
            )}
          </>
        );
        const common = {
          className: cls,
          role: 'radio' as const,
          'aria-checked': on,
          'data-engine': eng.id,
          'data-used-by': by ? `${by}순위` : undefined,
          title: by ? `${eng.label}는 ${by}순위로 선택되어 ${tier}순위로 중복 선택할 수 없습니다.` : undefined,
        };
        return onSelect ? (
          <button key={eng.id} type="button" disabled={by !== null} onClick={() => onSelect(eng.id)} {...common}>
            {body}
          </button>
        ) : (
          <div key={eng.id} aria-disabled="true" {...common}>
            {body}
          </div>
        );
      })}
    </div>
  );
}

/**
 * 한 순위의 세부 모델과 추론 강도 칸. 모양은 엔진 로고·모델 이름·설명을 한 줄에 그리고,
 * 실제로 값을 바꾸는 것은 그 위에 투명하게 덮은 select다(키보드와 접근성은 select가 맡는다).
 * 강도는 그 모델이 지원하는 값만 보인다.
 */
export function ModelEffortRow({
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
  tier: Tier;
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
  const picked = ENGINE_MODELS[engine].find((m) => m.id === selectedModel);
  const modelText = picked ? splitLabel(picked.label) : { name: '직접 입력', desc: customInput.trim() };
  const efforts = effortInfosFor(engine, resolvedModel);
  const currentEffort = efforts.find((e) => e.id === effort) ?? efforts[0];
  const effortText = splitLabel(currentEffort.label);
  return (
    <div className="eng-mrow">
      <div className="eng-fld eng-fld-model">
        <label>
          <span className="eng-lab">
            <ChipIcon />
            세부 모델
          </span>
          <span className="eng-sel">
            <EngineLogo engine={engine} size={22} />
            <span className="eng-sel-txt">
              <b>{modelText.name}</b>
              {modelText.desc && <em>{modelText.desc}</em>}
            </span>
            <ChevronIcon />
            <select className="eng-native" aria-label={`${tier}순위 세부 모델`} value={selectedModel} onChange={(e) => onSelectModel(e.target.value)}>
              {ENGINE_MODELS[engine].map((m) => (
                <option key={m.id} value={m.id}>
                  {m.label}
                </option>
              ))}
              <option value="custom">직접 입력...</option>
            </select>
          </span>
        </label>
        {selectedModel === 'custom' && (
          <input
            className="eng-custom"
            type="text"
            value={customInput}
            onChange={(e) => onCustomInput(e.target.value)}
            placeholder={placeholder}
            aria-label={`${tier}순위 모델 이름`}
            required
          />
        )}
      </div>

      <div className="eng-fld eng-fld-effort">
        <label>
          <span className="eng-lab">추론 강도 (Effort)</span>
          <span className="eng-sel">
            <EffortMeter effort={currentEffort.id} />
            <span className="eng-sel-txt">
              <b>{effortText.name}</b>
              {effortText.desc && <em>{effortText.desc}</em>}
            </span>
            <ChevronIcon />
            <select
              className="eng-native"
              aria-label={`${tier}순위 추론 강도 (Effort)`}
              value={currentEffort.id}
              onChange={(e) => onEffort(e.target.value as ReasoningEffort)}
            >
              {efforts.map((eff) => (
                <option key={eff.id} value={eff.id}>
                  {eff.label}
                </option>
              ))}
            </select>
          </span>
        </label>
      </div>
    </div>
  );
}

/** 순위 카드. 왼쪽에 번호 점과 세로선을 두고, 다음 순위와의 사이에 「토큰이 소진되면」 안내를 넣는다. */
export function TierCard({
  tier,
  title,
  tag,
  desc,
  head,
  off = false,
  last = false,
  children,
}: {
  tier: Tier;
  title: string;
  tag: string;
  desc?: ReactNode;
  head?: ReactNode;
  off?: boolean;
  last?: boolean;
  children?: ReactNode;
}) {
  return (
    <>
      <section className={`eng-tier${off ? ' is-off' : ''}`} data-card={tier}>
        <div className="eng-rail">
          <span className="eng-dot">{tier}</span>
          {!last && <span className="eng-ln" />}
        </div>
        <div className="eng-card">
          <div className="eng-top">
            <span className="eng-ttl">
              <b>{title}</b>
              <span className="eng-tag">{tag}</span>
            </span>
            {head}
          </div>
          {desc && <p className="eng-desc">{desc}</p>}
          {children}
        </div>
      </section>
      {!last && (
        <div className="eng-gap">
          <span className="eng-gap-rail" />
          <span className="eng-gap-txt">
            <DownIcon />
            토큰이 소진되면 다음 순위가 이어받습니다
          </span>
        </div>
      )}
    </>
  );
}

/** 3순위를 켜고 끄는 스위치. 체크박스를 스위치 모양으로 그린 것이라 키보드와 스크린 리더는 체크박스 그대로다. */
export function TierSwitch({ checked, onChange }: { checked: boolean; onChange: (checked: boolean) => void }) {
  return (
    <label className="eng-switch">
      <span>3순위 대체 모델 사용</span>
      <input type="checkbox" role="switch" checked={checked} onChange={(e) => onChange(e.target.checked)} />
      <span className="eng-sw" aria-hidden="true">
        <i />
      </span>
    </label>
  );
}

/** 3순위가 꺼져 있을 때 켜면 어느 엔진이 이어받는지 미리 보여 준다. */
export function ThirdPreview({ engine }: { engine: AIEngine }) {
  return (
    <div className="eng-preview">
      <EngineLogo engine={engine} size={28} />
      <span>
        켜면 남은 엔진인 <b>{engineLabel(engine)}</b>({engineCompany(engine)})가 3순위가 됩니다. 1순위와 2순위가 모두 소진됐을 때만 투입됩니다.
      </span>
    </div>
  );
}

export interface ChainStep {
  engine: AIEngine;
  model: string;
  effort: ReasoningEffort;
}

/**
 * 실행 순서 요약. 어느 AI가 어떤 모델·강도로 어떤 순서로 이어받는지 한 줄에 보인다.
 * [compact]이면 편집 창 맨 위와 같은 그림을 상자와 안내 글 없이 작게 그려, 에이전트 목록 카드에 넣을 수 있다.
 */
export function FallbackChain({ steps, thirdOff, compact = false }: { steps: ChainStep[]; thirdOff: boolean; compact?: boolean }) {
  return (
    <div className={`eng-chain${compact ? ' is-compact' : ''}`} role="group" aria-label="실행 순서 요약">
      {steps.map((step, i) => {
        const tier = (i + 1) as Tier;
        return (
          <span key={tier} className="eng-chain-item">
            {i > 0 && (
              <span className="eng-ar" aria-hidden="true">
                <span className="eng-ar-line" />
                {!compact && <small>토큰 소진 시</small>}
                <ArrowIcon />
              </span>
            )}
            <span
              className="eng-ch"
              data-step={tier}
              title={`${tier}순위 · ${engineLabel(step.engine)} ${modelName(step.engine, step.model)} · 추론 강도 ${step.effort}${i > 0 ? ` (앞 순위의 토큰이 소진되면 전환)` : ''}`}
            >
              <i className="eng-n">{tier}</i>
              <EngineLogo engine={step.engine} size={compact ? 15 : 20} bare />
              <b>{modelName(step.engine, step.model)}</b>
              <span className="eng-ep">
                <EffortMeter effort={step.effort} compact={compact} />
                {step.effort}
              </span>
            </span>
          </span>
        );
      })}
      {thirdOff && (
        <span className="eng-chain-item">
          <span className="eng-ar" aria-hidden="true">
            <span className="eng-ar-line" />
            <small>3순위를 켜면</small>
            <ArrowIcon />
          </span>
          <span className="eng-ch eng-ch-off">
            <i className="eng-n">3</i>
            <span>3순위 없음</span>
          </span>
        </span>
      )}
    </div>
  );
}
