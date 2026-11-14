// 설정 화면의 「글꼴과 크기」. 화면 배율 단추 줄과 앱 글꼴·터미널 글꼴 입력을 보인다.
// 값은 컨트롤러의 preference()로 저장하고, 앱에 거는 일(CSS 변수, 웹뷰 확대)은 App이 상태를 보고 한다.
import { useEffect, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import { UI_SCALES, fontProblem, normalizeFont, previewFamily } from "../workspace/typography.ts";
import type { Typography } from "../workspace/typography.ts";

/** 글꼴 입력 칸에서 목록으로 보여 주는 이름들. 설치되어 있어야 보이고, 목록에 없는 이름도 직접 적을 수 있다. */
const UI_FONT_SUGGESTIONS = [
  "Apple SD Gothic Neo",
  "AppleMyungjo",
  "Noto Sans KR",
  "Pretendard",
  "Nanum Gothic",
  "Nanum Myeongjo",
  "Helvetica Neue",
  "OpenDyslexic",
];
const TERMINAL_FONT_SUGGESTIONS = ["D2Coding", "Menlo", "Monaco", "SF Mono", "JetBrains Mono", "Fira Code", "Hack", "MesloLGS NF"];

/**
 * 글꼴 이름 한 줄 입력과 미리보기. 치는 동안에는 잠깐 멈췄을 때, 또는 칸을 떠나거나 Enter를 눌렀을 때 저장한다.
 * 글자마다 저장하면 입력 중간의 이름(`Pret`)마다 글꼴이 바뀌어 보이기 때문이다.
 */
function FontField({
  id,
  label,
  hint,
  placeholder,
  suggestions,
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
  value: string;
  mono?: boolean;
  sample: string;
  onCommit: (font: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  // 저장된 값이 바뀌면(기본값 단추, 다른 창) 입력 칸도 맞춘다. 치고 있는 글과 같은 값이면 공백 하나까지 건드리지 않는다.
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
}: {
  controller: Pick<WorkspaceController, "preference">;
  typography: Typography;
}) {
  return (
    <fieldset className="set-typo">
      <legend>글꼴과 크기</legend>
      <div className="typo-row">
        <strong id="typo-scale-label">화면 크기</strong>
        <small>
          글자와 버튼을 앱 전체에서 함께 키우거나 줄여요. ⌘ + / ⌘ - / ⌘ 0 키로도 바꿀 수 있어요. 지금은 {typography.scale}%예요.
        </small>
        <div className="typo-scale" role="group" aria-labelledby="typo-scale-label">
          {UI_SCALES.map((scale) => (
            <button
              type="button"
              key={scale}
              aria-pressed={typography.scale === scale}
              onClick={() => controller.preference({ uiScale: scale })}
            >
              {scale}%
            </button>
          ))}
        </div>
      </div>
      <FontField
        id="typo-ui-font"
        label="앱 글꼴"
        hint="앱 전체에 쓸 글꼴이에요. 설치된 글꼴 이름이나 쉼표로 이은 글꼴 목록(Pretendard, sans-serif)을 적어요. 고른 글꼴에 없는 글자는 기본 글꼴이 채워요."
        placeholder="Pretendard 또는 글꼴 목록 (비우면 기본 글꼴)"
        suggestions={UI_FONT_SUGGESTIONS}
        value={typography.uiFont}
        sample="다람쥐 헌 쳇바퀴에 타고파. The quick brown fox jumps over the lazy dog. 0123456789"
        onCommit={(font) => controller.preference({ uiFont: font })}
      />
      <FontField
        id="typo-terminal-font"
        label="터미널 글꼴"
        hint="터미널에 쓸 고정폭 글꼴이에요. 고른 글꼴에 한글이 없으면 D2Coding이 채워요. 열려 있는 터미널도 바로 바뀌어요."
        placeholder="MesloLGS NF 또는 글꼴 목록 (비우면 D2Coding)"
        suggestions={TERMINAL_FONT_SUGGESTIONS}
        value={typography.terminalFont}
        mono
        sample="~/project git:main ❯ ls -la  한글 폭 확인"
        onCommit={(font) => controller.preference({ terminalFont: font })}
      />
    </fieldset>
  );
}
