// 프로젝트 폼의 「저장소」 옆 「왜 필요해요?」가 여는 안내 팝업. 글보다 그림으로 알려 준다.
// 큰 그림 3장(왜, 어떻게, 안전)을 한 장씩 넘겨 보고, 장마다 제목과 설명은 한 줄만 둔다.
// 그림에 적힌 말은 지금 구현된 동작 그대로다. 아직 없는 기능(앱 안 개발)은 점선으로 「앞으로」라고 밝힌다.
// 동작(보관 위치, 받는 규칙, GitHub에 하는 일)이 바뀌면 그림과 글을 같이 고친다.
import { useState } from "react";
import type { ReactNode } from "react";
import { Modal } from "./Modal.tsx";

/** 받는 폴더를 글로 보인다. 연결 상태를 아직 모르면 앱 전용 폴더라고만 한다. */
export function cloneFolderText(cloneRoot?: string): string {
  return cloneRoot ? `${cloneRoot.replace(/\/+$/, "")}/<소유자>/<이름>` : "앱 전용 폴더(~/Library/Application Support/aisurada/repos/<소유자>/<이름>)";
}

// ---------------------------------------------------------------------------
// 그림 부품: 24x24 칸에 그린 선 아이콘. 색은 CSS 클래스(.rg-*)가 앱의 색 변수에서 가져와 화면 모양(스킨)을 따른다.
// ---------------------------------------------------------------------------

type IconName = "folder" | "branch" | "terminal" | "lock" | "eye" | "file" | "cloud" | "key";

const ICONS: Record<IconName, ReactNode> = {
  folder: <path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />,
  branch: (
    <>
      <line x1="6" y1="3" x2="6" y2="15" />
      <circle cx="18" cy="6" r="3" />
      <circle cx="6" cy="18" r="3" />
      <path d="M18 9a9 9 0 0 1-9 9" />
    </>
  ),
  terminal: (
    <>
      <polyline points="4 17 10 11 4 5" />
      <line x1="12" y1="19" x2="20" y2="19" />
    </>
  ),
  lock: (
    <>
      <rect x="5" y="11" width="14" height="10" rx="2" />
      <path d="M8 11V7a4 4 0 0 1 8 0v4" />
    </>
  ),
  eye: (
    <>
      <path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7S2 12 2 12z" />
      <circle cx="12" cy="12" r="3" />
    </>
  ),
  file: (
    <>
      <path d="M6 3h8l4 4v14H6z" />
      <path d="M14 3v4h4" />
      <line x1="9" y1="13" x2="15" y2="13" />
      <line x1="9" y1="17" x2="15" y2="17" />
    </>
  ),
  cloud: <path d="M7 18a4 4 0 0 1-.5-7.97A6 6 0 0 1 18 9.5 4.5 4.5 0 0 1 17.5 18z" />,
  key: (
    <>
      <circle cx="8" cy="15" r="4" />
      <path d="M11 12l9-9" />
      <path d="M16 7l3 3" />
    </>
  ),
};

function Icon({ name, x, y, size = 24, accent = false }: { name: IconName; x: number; y: number; size?: number; accent?: boolean }) {
  return (
    <g transform={`translate(${x} ${y}) scale(${size / 24})`} className={accent ? "rg-line rg-line-accent" : "rg-line"}>
      {ICONS[name]}
    </g>
  );
}

/** 앱 아이콘: 둥근 네모에 ais. */
function AppIcon({ x, y, size = 34 }: { x: number; y: number; size?: number }) {
  return (
    <g transform={`translate(${x} ${y}) scale(${size / 24})`}>
      <rect x="1" y="1" width="22" height="22" rx="6" className="rg-app" />
      <text x="12" y="15.4" textAnchor="middle" fontSize="9" fontWeight="700" className="rg-app-t">
        ais
      </text>
    </g>
  );
}

/** 저장소 한 칸(알약). `on`이면 아젠다가 고른 것으로 강조한다. */
function Chip({ x, y, w = 122, icon, label, on = false }: { x: number; y: number; w?: number; icon: IconName; label: string; on?: boolean }) {
  return (
    <g>
      <rect className={on ? "rg-chip rg-chip-on" : "rg-chip"} x={x} y={y} width={w} height="34" rx="10" />
      <Icon name={icon} x={x + 10} y={y + 8} size={18} accent={on} />
      <text className="rg-t" x={x + 38} y={y + 22}>
        {label}
      </text>
    </g>
  );
}

function Defs() {
  return (
    <defs>
      <marker id="rg-arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse">
        <path d="M0 0 L10 5 L0 10 z" className="rg-fill" />
      </marker>
    </defs>
  );
}

// ---------------------------------------------------------------------------
// 그림 3장
// ---------------------------------------------------------------------------

/** 1장. 프로젝트의 저장소 목록에서 아젠다가 하나를 고르면, 그 폴더에서 연 세션이 그 아젠다에 자동으로 묶인다. */
function SceneWhy() {
  const sessions = [
    { y: 48, label: "Claude Code" },
    { y: 100, label: "터미널" },
    { y: 152, label: "Codex" },
  ];
  return (
    <svg
      className="rg"
      viewBox="0 0 520 240"
      role="img"
      aria-label="프로젝트가 저장소 목록을 가지고, 아젠다가 그중 하나를 고르면, 그 저장소 폴더에서 연 세션이 그 아젠다에 자동으로 묶인다. 앞으로는 앱 안에서 개발도 할 수 있게 된다"
    >
      <Defs />
      <rect className="rg-card" x="8" y="16" width="150" height="208" rx="14" />
      <text className="rg-t" x="83" y="42" textAnchor="middle">
        프로젝트
      </text>
      <text className="rg-s" x="83" y="60" textAnchor="middle">
        저장소 목록
      </text>
      <Chip x={22} y={76} icon="folder" label="app" on />
      <Chip x={22} y={122} icon="branch" label="docs" />
      <rect className="rg-dash" x="22" y="168" width="122" height="34" rx="10" />
      <text className="rg-s" x="83" y="190" textAnchor="middle">
        + 저장소 추가
      </text>

      <path className="rg-line" d="M146 93 C172 93 180 109 206 109" markerEnd="url(#rg-arrow)" />
      <text className="rg-s" x="177" y="84" textAnchor="middle">
        고름
      </text>

      <rect className="rg-card" x="198" y="36" width="120" height="168" rx="14" />
      <text className="rg-t" x="258" y="62" textAnchor="middle">
        아젠다
      </text>
      <Chip x={208} y={92} w={100} icon="folder" label="app" on />
      <rect className="rg-chip" x="212" y="146" width="92" height="8" rx="4" />
      <rect className="rg-chip" x="212" y="162" width="70" height="8" rx="4" />
      <rect className="rg-chip" x="212" y="178" width="80" height="8" rx="4" />
      <text className="rg-s" x="258" y="198" textAnchor="middle">
        노트 · 세션
      </text>

      <path className="rg-line" d="M320 120 H366" markerEnd="url(#rg-arrow)" />
      <text className="rg-s" x="345" y="108" textAnchor="middle">
        자동 묶임
      </text>
      <path className="rg-line rg-line-soft" d="M372 68 V172 M372 68 H382 M372 120 H382 M372 172 H382" />
      {sessions.map((s) => (
        <g key={s.label}>
          <rect className="rg-card" x="384" y={s.y} width="128" height="40" rx="12" />
          <Icon name="terminal" x={394} y={s.y + 10} size={20} />
          <text className="rg-t" x="422" y={s.y + 25} fontSize="12">
            {s.label}
          </text>
        </g>
      ))}
      <rect className="rg-dash" x="384" y="204" width="128" height="26" rx="13" />
      <text className="rg-s" x="448" y="221" textAnchor="middle">
        앞으로: 앱 안 개발
      </text>
    </svg>
  );
}

/** 2장. 이 컴퓨터의 폴더는 그대로 쓰고, GitHub 저장소는 앱이 앱 전용 폴더에 받아 둔다. */
function SceneHow() {
  return (
    <svg
      className="rg"
      viewBox="0 0 520 240"
      role="img"
      aria-label="두 가지 방법이 있다. 이 컴퓨터의 폴더는 복사하지 않고 그대로 쓴다. GitHub 저장소는 GitHub 연결이 있으면 앱이 앱 전용 폴더에 받아 둔다"
    >
      <Defs />
      <rect className="rg-card" x="8" y="20" width="180" height="84" rx="14" />
      <Icon name="folder" x={24} y={45} size={34} />
      <text className="rg-t" x="70" y="56">
        내 컴퓨터 폴더
      </text>
      <text className="rg-s" x="70" y="76">
        /Users/me/app
      </text>
      <path className="rg-line" d="M192 62 H328" markerEnd="url(#rg-arrow)" />
      <text className="rg-t" x="260" y="50" textAnchor="middle" fontSize="12">
        그대로 사용
      </text>
      <rect className="rg-pill rg-pill-ok" x="214" y="72" width="92" height="22" rx="11" />
      <text className="rg-pill-t rg-pill-ok-t" x="260" y="87" textAnchor="middle">
        복사 안 함
      </text>
      <rect className="rg-card" x="332" y="20" width="180" height="84" rx="14" />
      <AppIcon x={348} y={45} />
      <text className="rg-t" x="392" y="56">
        aisurada 앱
      </text>
      <text className="rg-s" x="392" y="76">
        세션·화면에 연결
      </text>

      <text className="rg-s" x="260" y="124" textAnchor="middle">
        또는
      </text>

      <rect className="rg-card" x="8" y="136" width="180" height="84" rx="14" />
      <Icon name="cloud" x={24} y={161} size={34} />
      <text className="rg-t" x="70" y="172">
        GitHub 저장소
      </text>
      <text className="rg-s" x="70" y="192">
        owner/name
      </text>
      <path className="rg-line" d="M192 178 H328" markerEnd="url(#rg-arrow)" />
      <text className="rg-t" x="260" y="166" textAnchor="middle" fontSize="12">
        앱이 받아 둠
      </text>
      <rect className="rg-pill" x="198" y="188" width="124" height="22" rx="11" />
      <Icon name="key" x={206} y={192} size={14} />
      <text className="rg-pill-t" x="268" y="203" textAnchor="middle">
        GitHub 연결 필요
      </text>
      <rect className="rg-card" x="332" y="136" width="180" height="84" rx="14" />
      <Icon name="folder" x={348} y={161} size={34} accent />
      <text className="rg-t" x="392" y="172">
        앱 전용 폴더
      </text>
      <text className="rg-s" x="392" y="192">
        …/repos/owner/name
      </text>
    </svg>
  );
}

/** 3장. 키는 키체인에만, 앱은 읽기와 받기만, 빼도 폴더는 그대로, 목록은 노트 폴더에 저장. */
function SceneSafe() {
  const tiles: { x: number; y: number; icon: IconName; title: string; sub: string; badge?: boolean }[] = [
    { x: 8, y: 10, icon: "lock", title: "개인 키는 키체인에만", sub: "노트 폴더에 안 남겨요" },
    { x: 264, y: 10, icon: "eye", title: "읽기와 받기만 해요", sub: "올리기·수정은 아직 없어요" },
    { x: 8, y: 126, icon: "folder", title: "빼도 폴더는 그대로", sub: "목록에서 빼도 안 지워져요", badge: true },
    { x: 264, y: 126, icon: "file", title: "목록은 노트 폴더에", sub: "프로젝트 파일에 저장돼요" },
  ];
  return (
    <svg
      className="rg"
      viewBox="0 0 520 240"
      role="img"
      aria-label="개인 키는 키체인에만 보관하고, 앱은 GitHub에서 읽고 받기만 하며, 목록에서 빼도 받아 둔 폴더는 지워지지 않고, 목록은 노트 폴더의 프로젝트 파일에 저장된다"
    >
      {tiles.map((t) => (
        <g key={t.title}>
          <rect className="rg-card" x={t.x} y={t.y} width="248" height="104" rx="14" />
          <circle className="rg-chip" cx={t.x + 44} cy={t.y + 52} r="28" />
          <Icon name={t.icon} x={t.x + 30} y={t.y + 38} size={28} />
          {t.badge && (
            <g>
              <circle className="rg-ok-fill" cx={t.x + 64} cy={t.y + 72} r="9" />
              <polyline className="rg-check" points={`${t.x + 59.5} ${t.y + 72} ${t.x + 62.5} ${t.y + 75} ${t.x + 68.5} ${t.y + 68.5}`} />
            </g>
          )}
          <text className="rg-t" x={t.x + 86} y={t.y + 48}>
            {t.title}
          </text>
          <text className="rg-s" x={t.x + 86} y={t.y + 68}>
            {t.sub}
          </text>
        </g>
      ))}
    </svg>
  );
}

const SCENES = [SceneWhy, SceneHow, SceneSafe];

/** 장마다 제목 한 줄과 설명 한 줄. 그림이 말하고, 글은 거드는 정도로만 둔다. */
export const GUIDE_STEPS = [
  { id: "why", title: "코드가 어디 있는지 알려 줘요", caption: "저장소를 적어 두면 아젠다와 세션이 저절로 이어져요." },
  { id: "how", title: "두 가지 중 편한 방법으로", caption: "둘 다 적으면 폴더를 먼저 써요." },
  { id: "safe", title: "안전하게 다뤄요", caption: "저장소는 선택이에요. 적지 않아도 노트는 그대로 써요." },
] as const;

/** 장 번호에 맞는 그림. 범위 밖이면 첫 장이다. */
export function RepoGuideScene({ step }: { step: number }) {
  const Scene = SCENES[step] ?? SCENES[0];
  return <Scene />;
}

export function RepoGuide({ close, cloneRoot, connected }: { close: () => void; cloneRoot?: string; connected: boolean }) {
  const [step, setStep] = useState(0);
  const last = GUIDE_STEPS.length - 1;
  const current = GUIDE_STEPS[step];
  return (
    <Modal title="프로젝트 저장소 안내" close={close} className="repo-guide">
      <div className="repo-guide-body">
        <h3 className="repo-guide-title" aria-live="polite">
          {current.title}
        </h3>
        <div className="repo-guide-scene">
          <RepoGuideScene step={step} />
        </div>
        <p className="repo-guide-caption">{current.caption}</p>
        {current.id === "how" && (
          <>
            <p className={connected ? "repo-guide-status repo-guide-status-ok" : "repo-guide-status"}>
              {connected ? "GitHub 연결됨" : "GitHub 연결 안 됨 · 설정에서 연결해 주세요"}
            </p>
            <p className="repo-guide-note">받는 폴더: {cloneFolderText(cloneRoot)}</p>
          </>
        )}
        <div className="repo-guide-nav">
          <button type="button" disabled={step === 0} onClick={() => setStep(step - 1)}>
            이전
          </button>
          <div className="repo-guide-dots" role="group" aria-label="안내 단계">
            {GUIDE_STEPS.map((s, i) => (
              <button
                key={s.id}
                type="button"
                className="repo-guide-dot"
                aria-label={`${i + 1}번째 안내: ${s.title}`}
                aria-current={i === step ? "step" : undefined}
                onClick={() => setStep(i)}
              />
            ))}
          </div>
          {step < last ? (
            <button type="button" className="button-primary" onClick={() => setStep(step + 1)}>
              다음
            </button>
          ) : (
            <button type="button" className="button-primary" onClick={close}>
              알겠어요
            </button>
          )}
        </div>
      </div>
    </Modal>
  );
}
