// 스킬·에이전트 정의를 만들고 고치는 창. 글로벌 페이지와 프로젝트 페이지가 같이 쓴다.
// 저장은 엔진이 하고(저장 전 검사, f1 보호, 기록), 이 창은 값을 모으고 미리 알려 줄 만한 것만 먼저 막는다.
import { useState } from "react";
import { Confirm, Modal } from "./Modal.tsx";
import {
  BODY_MAX, DESCRIPTION_MAX, NAME_MAX, PERMISSIONS, emptyForm, explainError, formChanged, formFrom, formToInput, kindLabel,
  saveResource, scopeLabel, validateForm,
} from "./resourcesClient.ts";
import type { EngineCall, ResourceForm, ResourceKind, ResourceRow } from "./resourcesClient.ts";

export function ResourceFormModal({
  call,
  kind,
  project,
  existing,
  skillChoices,
  repoNames,
  close,
  saved,
}: {
  call: EngineCall;
  kind: ResourceKind;
  /** 프로젝트 자원이면 프로젝트 slug. 없으면 글로벌 자원이다 */
  project?: string;
  /** 고칠 자원(본문과 쓰기 전 토큰이 든 것). 없으면 새로 만든다 */
  existing?: ResourceRow;
  /** 에이전트가 쓸 스킬로 고를 수 있는 것들 */
  skillChoices: ResourceRow[];
  /** 프로젝트 에이전트가 고를 수 있는 저장소 이름 */
  repoNames: string[];
  close: () => void;
  saved: (row: ResourceRow) => void;
}) {
  const [form, setForm] = useState<ResourceForm>(() => (existing ? formFrom(existing) : emptyForm(kind)));
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [discard, setDiscard] = useState(false);
  const label = kindLabel(kind);
  const isAgent = kind === "agent";
  const set = <K extends keyof ResourceForm>(key: K, value: ResourceForm[K]) => setForm((f) => ({ ...f, [key]: value }));
  const requestClose = () => (formChanged(form, existing, kind) ? setDiscard(true) : close());

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    const problem = validateForm(form);
    if (problem) return setError(problem);
    setError("");
    setBusy(true);
    try {
      const row = await saveResource(call, formToInput(form, { project, existing }));
      saved(row);
    } catch (err) {
      setError(explainError(err));
      setBusy(false);
    }
  };

  const toggleSkill = (id: string) =>
    set("skills", form.skills.includes(id) ? form.skills.filter((s) => s !== id) : [...form.skills, id]);
  // 지금 고른 스킬이 선택지에서 사라졌어도(지웠거나 다른 범위) 보여 주고 뺄 수 있게 한다
  const missingChosen = form.skills.filter((id) => !skillChoices.some((s) => s.id === id));

  return (
    <Modal title={`${project ? "프로젝트" : "글로벌"} ${label} ${existing ? "편집" : "추가"}`} close={requestClose} className="ndf res-form">
      <form onSubmit={submit}>
        <label>
          이름
          <input
            aria-label={`${label} 이름`}
            required
            maxLength={NAME_MAX}
            value={form.name}
            onChange={(e) => set("name", e.target.value)}
          />
          <small className="res-hint">같은 범위에서 이름은 하나뿐이에요. 이름을 바꿔도 참조는 안 끊겨요.</small>
        </label>
        <label>
          설명
          <textarea
            aria-label={`${label} 설명`}
            rows={2}
            maxLength={DESCRIPTION_MAX}
            value={form.description}
            onChange={(e) => set("description", e.target.value)}
          />
          <small className="res-hint">
            {isAgent ? "이 에이전트가 어떤 역할인지" : "이 스킬을 언제 쓰는지"} 한 문단으로 적어요. AI가 이 글을 보고 쓸지 정해요.
          </small>
        </label>
        {isAgent && (
          <>
            <fieldset className="res-perm">
              <legend>권한</legend>
              {PERMISSIONS.map((p) => (
                <label key={p.id} className="res-perm-option">
                  <input type="radio" name="permission" value={p.id} checked={form.permission === p.id} onChange={() => set("permission", p.id)} />
                  <span>
                    <strong>{p.label}</strong>
                    <small>{p.hint}</small>
                  </span>
                </label>
              ))}
              <small className="res-hint">권한은 작업 실행기가 생긴 뒤부터 쓰여요. 지금은 정해 두기만 해요.</small>
            </fieldset>
            <fieldset className="res-skills">
              <legend>쓸 스킬</legend>
              {skillChoices.length === 0 && missingChosen.length === 0 && <p className="res-empty">고를 수 있는 스킬이 없어요.</p>}
              {skillChoices.map((s) => (
                <label key={s.id} className="res-check">
                  <input type="checkbox" checked={form.skills.includes(s.id)} onChange={() => toggleSkill(s.id)} />
                  <span>{scopeLabel(s)}</span>
                </label>
              ))}
              {missingChosen.map((id) => (
                <label key={id} className="res-check res-check-missing">
                  <input type="checkbox" checked onChange={() => toggleSkill(id)} />
                  <span>찾을 수 없는 스킬({id.slice(0, 10)}…) — 체크를 풀면 빠져요</span>
                </label>
              ))}
              <small className="res-hint">
                {project
                  ? "글로벌 스킬은 이 프로젝트가 참조해야 실제로 쓸 수 있어요."
                  : "글로벌 에이전트는 글로벌 스킬만 쓸 수 있어요."}
              </small>
            </fieldset>
            {project && (
              <label>
                쓸 저장소
                <select aria-label="쓸 저장소" value={form.repo} onChange={(e) => set("repo", e.target.value)}>
                  <option value="">정하지 않음</option>
                  {form.repo && !repoNames.includes(form.repo) && <option value={form.repo}>{form.repo} (프로젝트에 없음)</option>}
                  {repoNames.map((n) => (
                    <option key={n} value={n}>
                      {n}
                    </option>
                  ))}
                </select>
              </label>
            )}
          </>
        )}
        <label>
          지침
          <textarea
            aria-label={`${label} 지침`}
            className="res-body"
            rows={10}
            maxLength={BODY_MAX}
            value={form.body}
            onChange={(e) => set("body", e.target.value)}
          />
          <small className="res-hint">AI가 그대로 따르는 글이에요(마크다운).</small>
        </label>
        <details className="res-advanced" open={form.toolsText.trim() !== ""}>
          <summary>도구별 설정 (선택)</summary>
          <label>
            JSON
            <textarea
              aria-label="도구별 설정"
              className="res-tools"
              rows={5}
              spellCheck={false}
              placeholder={'{"claude": {"allowed-tools": ["Read"]}}'}
              value={form.toolsText}
              onChange={(e) => set("toolsText", e.target.value)}
            />
            <small className="res-hint">
              {label}은 어느 AI 도구에도 묶이지 않아요. 도구마다 다른 점만 여기에 적어요. 비워 두면 없는 것이고, 모르는 도구의 칸도 지우지 않고 보존해요.
            </small>
          </label>
        </details>
        {existing?.editedSession && (
          <p className="res-note">마지막으로 AI가 고쳤어요. 저장하기 전에 내용을 한 번 읽어 보세요.</p>
        )}
        {error && <p role="alert">{error}</p>}
        <footer>
          <button type="button" onClick={requestClose}>
            취소
          </button>
          <button type="submit" className="button-primary" disabled={busy || !form.name.trim()}>
            {existing ? "저장" : "만들기"}
          </button>
        </footer>
      </form>
      {discard && (
        <Confirm title="쓴 글을 버릴까요?" description="작성 중인 내용이 사라집니다." label="버리기" close={() => setDiscard(false)} accept={close} />
      )}
    </Modal>
  );
}
