// 태스크를 만들고 고치는 창. 보드·목록 페이지와 상세 페이지가 같이 쓴다.
// 저장은 엔진이 하고(완료 기준 필수, f1 보호, 활동 기록), 이 창은 값을 모으고 먼저 알려 줄 만한 것만 막는다.
// 채팅 없이도 만들 수 있는 일반 양식이다. 완료 기준이 하나도 없으면 만들 수 없다(일이 끝났는지 이 기준으로 확인한다).
import { useState } from "react";
import { Confirm, Modal } from "./Modal.tsx";
import {
  BODY_MAX, CRITERION_MAX, PRIORITIES, TITLE_MAX, TYPES, createTask, emptyTaskForm, explainError, formFromTask, parseRefInput,
  taskFormChanged, updateTask, validateTaskForm,
} from "./tasksClient.ts";
import type { EngineCall, RepoChoice, TaskDetail, TaskForm, TaskPriority, TaskRef, TaskType } from "./tasksClient.ts";

export interface NoteChoice {
  id: string;
  title: string;
}

export function TaskFormModal({
  call,
  existing,
  initialTitle = "",
  repos,
  notes,
  close,
  saved,
}: {
  call: EngineCall;
  /** 고칠 태스크(쓰기 전 토큰이 든 것). 없으면 새로 만든다 */
  existing?: TaskDetail;
  /** 보드의 한 줄 입력에서 넘어온 요약 */
  initialTitle?: string;
  repos: RepoChoice[];
  notes: NoteChoice[];
  close: () => void;
  saved: (task: TaskDetail) => void;
}) {
  const [form, setForm] = useState<TaskForm>(() => {
    if (existing) return formFromTask(existing);
    const f = emptyTaskForm();
    f.title = initialTitle;
    f.criteria = [{ text: "", done: false }];
    // 저장소가 하나뿐이면 미리 골라 둔다
    if (repos.length === 1) {
      f.repo = repos[0].repo;
      f.project = repos[0].project;
    }
    return f;
  });
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [discard, setDiscard] = useState(false);
  const [refText, setRefText] = useState("");
  const [noteId, setNoteId] = useState("");
  const set = <K extends keyof TaskForm>(key: K, value: TaskForm[K]) => setForm((f) => ({ ...f, [key]: value }));
  const requestClose = () => (taskFormChanged(form, existing) ? setDiscard(true) : close());

  const submit = async (e: React.FormEvent) => {
    e.preventDefault();
    const problem = validateTaskForm(form);
    if (problem) return setError(problem);
    setError("");
    setBusy(true);
    try {
      const task = existing ? await updateTask(call, existing, cleaned(form)) : await createTask(call, cleaned(form));
      saved(task);
    } catch (err) {
      setError(explainError(err));
      setBusy(false);
    }
  };

  const pickRepo = (value: string) => {
    const choice = repos.find((r) => r.repo === value);
    setForm((f) => ({ ...f, repo: value, project: choice?.project ?? "" }));
  };
  const setCriterion = (i: number, text: string) => set("criteria", form.criteria.map((c, j) => (j === i ? { ...c, text } : c)));
  const addCriterion = () => set("criteria", [...form.criteria, { text: "", done: false }]);
  const removeCriterion = (i: number) => set("criteria", form.criteria.filter((_, j) => j !== i));
  const addRef = (ref: TaskRef) => {
    if (form.refs.some((r) => r.kind === ref.kind && r.target === ref.target)) return;
    set("refs", [...form.refs, ref]);
  };
  const addTypedRef = () => {
    const ref = parseRefInput(refText);
    if (!ref) return setError("GitHub 이슈·PR 주소나 owner/name#번호를 적어 주세요.");
    setError("");
    addRef(ref);
    setRefText("");
  };
  const addNote = () => {
    const note = notes.find((n) => n.id === noteId);
    if (!note) return;
    addRef({ kind: "note", target: note.id, title: note.title });
    setNoteId("");
  };
  const refName = (r: TaskRef) => (r.kind === "note" ? `노트 · ${r.title ?? r.target}` : `GitHub · ${r.target.replace("https://github.com/", "")}`);
  const unknownRepo = form.repo !== "" && !repos.some((r) => r.repo === form.repo);

  return (
    <Modal title={existing ? `${existing.key} 편집` : "태스크 만들기"} close={requestClose} className="ndf task-form">
      <form onSubmit={submit}>
        {!existing && <p className="task-hint">Draft로 시작해요. 만든 뒤 상태 메뉴에서 Ready For Develop으로 바꾸면 러너가 집어 갈 수 있어요.</p>}
        <div className="form-grid">
          <label>
            저장소 *
            {repos.length > 0 ? (
              <select aria-label="저장소" required value={form.repo} onChange={(e) => pickRepo(e.target.value)}>
                <option value="">저장소를 골라 주세요</option>
                {unknownRepo && <option value={form.repo}>{form.repo}</option>}
                {repos.map((r) => (
                  <option key={r.repo} value={r.repo}>
                    {r.label}
                  </option>
                ))}
              </select>
            ) : (
              <input aria-label="저장소" required placeholder="owner/name" value={form.repo} onChange={(e) => set("repo", e.target.value)} />
            )}
          </label>
          <label>
            유형 *
            <select aria-label="유형" value={form.type} onChange={(e) => set("type", e.target.value as TaskType)}>
              {TYPES.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.label}
                </option>
              ))}
            </select>
          </label>
        </div>
        <label>
          요약 *
          <input aria-label="요약" required maxLength={TITLE_MAX} value={form.title} onChange={(e) => set("title", e.target.value)} />
        </label>
        <label>
          설명
          <textarea
            aria-label="설명"
            rows={5}
            maxLength={BODY_MAX}
            placeholder="무엇이 문제이고 어떻게 되면 좋은지 적어요(마크다운)."
            value={form.body}
            onChange={(e) => set("body", e.target.value)}
          />
        </label>
        <fieldset className="task-criteria">
          <legend>완료 기준 *</legend>
          {form.criteria.map((c, i) => (
            <div key={i} className="task-criterion-row">
              <input
                aria-label={`완료 기준 ${i + 1}`}
                maxLength={CRITERION_MAX}
                placeholder="예: 지운 뒤 자원 폴더가 남지 않는다"
                value={c.text}
                onChange={(e) => setCriterion(i, e.target.value)}
              />
              {c.done && <span className="task-checked">확인됨</span>}
              <button type="button" aria-label={`완료 기준 ${i + 1} 지우기`} onClick={() => removeCriterion(i)}>
                ×
              </button>
            </div>
          ))}
          <button type="button" className="task-add" onClick={addCriterion}>
            + 항목 추가
          </button>
          <small className="task-hint">하나 이상 써야 만들 수 있어요. 일이 끝났는지 이 기준으로 확인해요.</small>
        </fieldset>
        <div className="form-grid">
          <label>
            우선순위
            <select aria-label="우선순위" value={form.priority} onChange={(e) => set("priority", e.target.value as TaskPriority)}>
              {PRIORITIES.map((p) => (
                <option key={p.id} value={p.id}>
                  {p.label}
                </option>
              ))}
            </select>
          </label>
          <label>
            기준 브랜치
            <input aria-label="기준 브랜치" placeholder="비우면 저장소의 기본 브랜치" value={form.base} onChange={(e) => set("base", e.target.value)} />
          </label>
        </div>
        <label>
          라벨
          <input aria-label="라벨" placeholder="쉼표로 나눠요. 예: 엔진, 휴지통" value={form.labelsText} onChange={(e) => set("labelsText", e.target.value)} />
        </label>
        <fieldset className="task-refs">
          <legend>연결한 자원</legend>
          {form.refs.length > 0 && (
            <ul className="task-chips">
              {form.refs.map((r) => (
                <li key={`${r.kind}:${r.target}`} className="task-chip">
                  <span>{refName(r)}</span>
                  <button type="button" aria-label={`${refName(r)} 연결 끊기`} onClick={() => set("refs", form.refs.filter((x) => x !== r))}>
                    ×
                  </button>
                </li>
              ))}
            </ul>
          )}
          <div className="task-ref-add">
            <select aria-label="노트 고르기" value={noteId} onChange={(e) => setNoteId(e.target.value)}>
              <option value="">노트에서 고르기</option>
              {notes.map((n) => (
                <option key={n.id} value={n.id}>
                  {n.title || n.id}
                </option>
              ))}
            </select>
            <button type="button" disabled={!noteId} onClick={addNote}>
              노트 연결
            </button>
          </div>
          <div className="task-ref-add">
            <input
              aria-label="GitHub 이슈나 PR"
              placeholder="GitHub 이슈·PR 주소 또는 owner/name#53"
              value={refText}
              onChange={(e) => setRefText(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  addTypedRef();
                }
              }}
            />
            <button type="button" disabled={!refText.trim()} onClick={addTypedRef}>
              GitHub 연결
            </button>
          </div>
          <small className="task-hint">에이전트는 여기 연결한 노트와 GitHub 글을 읽어요.</small>
        </fieldset>
        {error && <p role="alert" className="task-error">{error}</p>}
        <footer>
          <button type="button" onClick={requestClose}>
            취소
          </button>
          <button type="submit" className="button-primary" disabled={busy}>
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

/** 빈 완료 기준 줄은 빼고 보낸다. */
function cleaned(form: TaskForm): TaskForm {
  return { ...form, criteria: form.criteria.filter((c) => c.text.trim() !== "") };
}
