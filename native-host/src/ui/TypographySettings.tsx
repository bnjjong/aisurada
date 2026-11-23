// 설정 화면의 「글꼴과 크기」. 앱 글꼴·터미널 글꼴 입력을 보인다.
// 값은 컨트롤러의 preference()로 저장하고, 앱에 거는 일(CSS 변수, 웹뷰 확대)은 App이 상태를 보고 한다.
import { useEffect, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import { fontProblem, normalizeFont, previewFamily } from "../workspace/typography.ts";
import type { Typography } from "../workspace/typography.ts";
import { filterAvailableFonts } from "./fontAvailability.ts";

/** 글꼴 입력 칸에서 목록으로 보여 주는 이름들. 설치되어 있어야 보이고, 목록에 없는 이름도 직접 적을 수 있다. */
export const UI_FONT_SUGGESTIONS = [
  "Apple SD Gothic Neo",
  "AppleMyungjo",
  "Noto Sans KR",
  "Pretendard",
  "Nanum Gothic",
  "Nanum Myeongjo",
  "Helvetica Neue",
  "OpenDyslexic",
];
export const TERMINAL_FONT_SUGGESTIONS = [
  "D2Coding",
  "Menlo",
  "Monaco",
  "SF Mono",
  "JetBrains Mono",
  "Fira Code",
  "Hack",
  "MesloLGS NF",
];

/**
 * 글꼴 이름 한 줄 입력과 추천 칩, 미리보기. 치는 동안에는 잠깐 멈췄을 때, 또는 칸을 떠나거나 Enter를 눌렀을 때 저장한다.
 */
function FontField({
  id,
  label,
  hint,
  placeholder,
  suggestions,
  availableFonts,
  value,
  mono,
  sample,
  onCommit,
}: {
  id: string;
  label: string;
  hint: string;
  placeholder: string;
  suggestions: readonly string[];
  availableFonts?: readonly string[];
  value: string;
  mono?: boolean;
  sample: string;
  onCommit: (font: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  const [chips, setChips] = useState<string[]>(() =>
    availableFonts ? [...availableFonts] : [],
  );

  useEffect(() => {
    if (availableFonts) {
      setChips([...availableFonts]);
      return;
    }
    setChips(filterAvailableFonts(suggestions));
  }, [suggestions, availableFonts]);

  // 저장된 값이 바뀌면 입력 칸도 맞춘다.
  useEffect(() => {
    setDraft((current) => (normalizeFont(current) === value ? current : value));
  }, [value]);

  const problem = fontProblem(draft.trim());
  const commit = () => {
    if (!problem && normalizeFont(draft) !== value) onCommit(normalizeFont(draft));
  };

  useEffect(() => {
    if (problem || normalizeFont(draft) === value) return;
    const timer = setTimeout(() => onCommit(normalizeFont(draft)), 500);
    return () => clearTimeout(timer);
  }, [draft]);

  const shown = problem ? value : normalizeFont(draft);

  const handleChip = (fontName: string) => {
    setDraft(fontName);
    onCommit(fontName);
  };

  return (
    <div className="typo-row">
      <label htmlFor={id}>
        <strong>{label}</strong>
      </label>
      <small>{hint}</small>
      <div className="typo-field">
        <input
          id={id}
          type="text"
          list={`${id}-list`}
          value={draft}
          placeholder={placeholder}
          spellCheck={false}
          autoComplete="off"
          aria-invalid={problem ? true : undefined}
          onChange={(e) => setDraft(e.target.value)}
          onBlur={commit}
          onKeyDown={(e) => {
            if (e.key === "Enter") {
              e.preventDefault();
              commit();
            }
          }}
        />
        <datalist id={`${id}-list`}>
          {suggestions.map((name) => (
            <option key={name} value={name} />
          ))}
        </datalist>
        <button
          type="button"
          disabled={!draft && !value}
          onClick={() => {
            setDraft("");
            onCommit("");
          }}
        >
          기본값
        </button>
      </div>

      {/* 추천 글꼴 칩 목록: 기본(빈 값) + 설치된 추천 글꼴들 */}
      <div className="set-chip-row" role="group" aria-label={`${label} 추천 글꼴`}>
        <button
          type="button"
          className="set-chip"
          role="button"
          aria-pressed={shown === "" ? "true" : "false"}
          onClick={() => handleChip("")}
        >
          기본
        </button>
        {chips.map((name) => {
          const isPressed = shown === name;
          return (
            <button
              key={name}
              type="button"
              className="set-chip"
              role="button"
              aria-pressed={isPressed ? "true" : "false"}
              onClick={() => handleChip(name)}
            >
              {name}
            </button>
          );
        })}
      </div>

      {problem && (
        <p className="typo-problem" role="alert">
          {problem}
        </p>
      )}
      <p
        className={mono ? "typo-preview typo-preview-mono" : "typo-preview"}
        style={{ fontFamily: mono ? previewFamily.terminal(shown) : previewFamily.ui(shown) }}
      >
        <span className="typo-preview-label">미리보기</span>
        {sample}
      </p>
    </div>
  );
}

export function TypographySettings({
  controller,
  typography,
  availableFonts,
  availableUiFonts,
  availableTerminalFonts,
}: {
  controller: Pick<WorkspaceController, "preference">;
  typography: Typography;
  availableFonts?: readonly string[];
  availableUiFonts?: readonly string[];
  availableTerminalFonts?: readonly string[];
}) {
  const uiAvailable = availableUiFonts ?? availableFonts;
  const termAvailable = availableTerminalFonts ?? availableFonts;

  return (
    <div className="set-typo">
      <div className="set-group" data-setting="ui-font">
        <h4>앱 글꼴</h4>
        <div className="set-card" style={{ padding: "16px 18px" }}>
          <FontField
            id="typo-ui-font"
            label="앱 글꼴"
            hint="앱 전체에 쓸 글꼴이에요. 설치된 글꼴 이름이나 쉼표로 이은 글꼴 목록(Pretendard, sans-serif)을 적어요. 고른 글꼴에 없는 글자는 기본 글꼴이 채워요."
            placeholder="Pretendard 또는 글꼴 목록 (비우면 기본 글꼴)"
            suggestions={UI_FONT_SUGGESTIONS}
            availableFonts={uiAvailable}
            value={typography.uiFont}
            sample="다람쥐 헌 쳇바퀴에 타고파. The quick brown fox jumps over the lazy dog. 0123456789"
            onCommit={(font) => controller.preference({ uiFont: font })}
          />
        </div>
      </div>
      <div className="set-group" data-setting="terminal-font">
        <h4>터미널 글꼴</h4>
        <div className="set-card" style={{ padding: "16px 18px" }}>
          <FontField
            id="typo-terminal-font"
            label="터미널 글꼴"
            hint="터미널에 쓸 고정폭 글꼴이에요. 고른 글꼴에 한글이 없으면 D2Coding이 채워요. 열려 있는 터미널도 바로 바뀌어요."
            placeholder="MesloLGS NF 또는 글꼴 목록 (비우면 D2Coding)"
            suggestions={TERMINAL_FONT_SUGGESTIONS}
            availableFonts={termAvailable}
            value={typography.terminalFont}
            mono
            sample="~/project git:main ❯ ls -la  한글 폭 확인"
            onCommit={(font) => controller.preference({ terminalFont: font })}
          />
        </div>
      </div>
    </div>
  );
}
