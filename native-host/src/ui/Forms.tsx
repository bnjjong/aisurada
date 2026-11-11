import { useRef, useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import type { WorkspaceState } from "../workspace/types.ts";
import { asText } from "../workspace/types.ts";
import { Modal, Confirm } from "./Modal.tsx";
import { Editor } from "../editor/Editor.tsx";
import { TYPES, TYPE_NAMES, agendaRepoField } from "./model.ts";
export const EMPTY_TEMPLATE =
  "# [제목]\n\n한 줄 요약을 적어요.\n\n## 핵심 내용\n- 첫 번째 항목\n";
const HUES = [
  "blue",
  "violet",
  "teal",
  "orange",
  "pink",
  "green",
  "red",
  "amber",
  "lime",
  "cyan",
  "fuchsia",
  "brown",
];
const HUE_NAMES: Record<string, string> = {
  blue: "파랑",
  violet: "보라",
  teal: "청록",
  orange: "주황",
  pink: "분홍",
  green: "초록",
  red: "빨강",
  amber: "호박",
  lime: "연두",
  cyan: "하늘",
  fuchsia: "자홍",
  brown: "갈색",
};
export function Forms({
  state,
  controller,
}: {
  state: WorkspaceState;
  controller: WorkspaceController;
}) {
  const form = state.form;
  if (!form) return null;
  return form.kind === "note" ? (
    <NewNote
      key={"note:" + form.agenda}
      state={state}
      controller={controller}
      agenda={form.agenda}
    />
  ) : (
    <EntityForm
      key={form.kind + form.id + form.project}
      state={state}
      controller={controller}
      kind={form.kind}
      id={form.id}
      project={form.project}
    />
  );
}
function NewNote({
  state,
  controller,
  agenda: initial,
}: {
  state: WorkspaceState;
  controller: WorkspaceController;
  agenda?: string;
}) {
  const ws = state.workspace!,
    now = new Date();
  const [agenda, setAgenda] = useState(initial || ws.agendas[0]?.id || ""),
    [type, setType] = useState("plan"),
    [template, setTemplate] = useState(""),
    [title, setTitle] = useState(""),
    [date, setDate] = useState((state.form as any)?.date || ws.today),
    [time, setTime] = useState(now.toTimeString().slice(0, 5)),
    [tags, setTags] = useState(""),
    [body, setBody] = useState(""),
    [revision, setRevision] = useState(0),
    [busy, setBusy] = useState(false),
    [error, setError] = useState("");
  const bodyRef = useRef(body);
  bodyRef.current = body;
  const [confirmDiscard, setConfirmDiscard] = useState(false);
  const isDirty = Boolean(title.trim() || body.trim());
  const handleClose = () => {
    if (isDirty) setConfirmDiscard(true);
    else controller.form(null);
  };
  const choices = ws.templates.filter(
    (t) =>
      t.category !== "ritual" &&
      t.type === type &&
      (!t.format || t.format === "markdown"),
  );
  return (
    <Modal title="새 노트" close={handleClose} className="ndf">
      {!ws.agendas.length ? (
        <>
          <p>먼저 아젠다를 하나 만들어 주세요</p>
          <button onClick={() => controller.form({ kind: "agenda" })}>
            아젠다 만들기
          </button>
        </>
      ) : (
        <form
          onSubmit={async (e) => {
            e.preventDefault();
            if (!title.trim() || !agenda) return;
            setBusy(true);
            const r = await controller.data.createDoc({
              agendaId: agenda,
              type,
              title: title.trim(),
              body: bodyRef.current.replace(/\[제목\]/g, title.trim()),
              date,
              time,
              tags: tags
                .split(",")
                .map((t) => t.trim())
                .filter(Boolean),
              template: template || undefined,
            });
            setBusy(false);
            if (r.ok) {
              controller.form(null);
              controller.navigate(
                "/agenda/" +
                  encodeURIComponent(agenda) +
                  "?open=" +
                  encodeURIComponent(asText(r.value?.id)),
              );
            } else setError("노트를 만들지 못했어요.");
          }}
        >
          <div className="form-grid">
            <label>
              아젠다
              <select
                aria-label="아젠다"
                value={agenda}
                onChange={(e) => setAgenda(e.target.value)}
              >
                {ws.agendas.map((a) => (
                  <option key={a.id} value={a.id}>
                    {a.title}
                  </option>
                ))}
              </select>
            </label>
            <label>
              유형
              <select
                value={type}
                onChange={(e) => {
                  setType(e.target.value);
                  setTemplate("");
                }}
              >
                {TYPES.map((t) => (
                  <option key={t} value={t}>
                    {TYPE_NAMES[t]}
                  </option>
                ))}
              </select>
            </label>
            <label>
              템플릿
              <select
                value={template}
                onChange={(e) => {
                  setTemplate(e.target.value);
                  setBody(
                    asText(choices.find((t) => t.id === e.target.value)?.body),
                  );
                  setRevision((v) => v + 1);
                }}
              >
                <option value="">템플릿 없음</option>
                {choices.map((t) => (
                  <option key={asText(t.id)} value={asText(t.id)}>
                    {asText(t.name)}
                  </option>
                ))}
              </select>
            </label>
            <label>
              제목
              <input
                aria-label="새 노트 제목"
                required
                value={title}
                onChange={(e) => setTitle(e.target.value)}
              />
            </label>
          </div>
          <Editor
            key={revision}
            body={body}
            onChange={(value) => {
              bodyRef.current = value;
              setBody(value);
            }}
            features={{
              notes: ws.notes,
              currentId: "",
              showRecords: state.prefs.showRecords,
            }}
          />
          <div className="form-grid">
            <label>
              날짜
              <input
                type="date"
                value={date}
                onChange={(e) => setDate(e.target.value)}
              />
            </label>
            <label>
              시간
              <input
                type="time"
                value={time}
                onChange={(e) => setTime(e.target.value)}
              />
            </label>
            <label>
              태그
              <input
                value={tags}
                placeholder="쉼표로 구분"
                onChange={(e) => setTags(e.target.value)}
              />
            </label>
          </div>
          {error && <p role="alert">{error}</p>}
          <footer className="ndf-foot">
            <button type="button" onClick={handleClose}>
              취소
            </button>
            <button type="submit" disabled={busy || !title.trim() || !agenda}>
              노트 만들기
            </button>
          </footer>
        </form>
      )}
      {confirmDiscard && (
        <Confirm
          title="쓴 글을 버릴까요?"
          description="작성 중인 내용이 모두 사라집니다."
          label="버리기"
          close={() => setConfirmDiscard(false)}
          accept={() => controller.form(null)}
        />
      )}
    </Modal>
  );
}
function EntityForm({
  kind,
  id,
  project: initial,
  state,
  controller,
}: {
  kind: "agenda" | "project";
  id?: string;
  project?: string;
  state: WorkspaceState;
  controller: WorkspaceController;
}) {
  const ws = state.workspace!,
    old = (kind === "agenda" ? ws.agendas : ws.projects).find(
      (a) => a.id === id,
    ),
    [title, setTitle] = useState(asText(old?.title)),
    [hue, setHue] = useState(
      asText(old?.hue) ||
        [...HUES].sort(
          (a, b) =>
            ws.agendas.filter((x) => x.hue === a).length -
            ws.agendas.filter((x) => x.hue === b).length,
        )[0],
    ),
    [project, setProject] = useState(initial || asText(old?.project)),
    [repo, setRepo] = useState(asText(old?.repo)),
    [summary, setSummary] = useState(asText(old?.summary)),
    [error, setError] = useState(""),
    [confirm, setConfirm] = useState(false),
    [confirmDiscard, setConfirmDiscard] = useState(false),
    [busy, setBusy] = useState(false);
  const isDirty = Boolean(
    id
      ? title !== asText(old?.title) ||
        summary !== asText(old?.summary) ||
        repo !== asText(old?.repo) ||
        project !== (initial || asText(old?.project))
      : title.trim() || summary.trim() || repo.trim(),
  );
  const handleClose = () => {
    if (isDirty) setConfirmDiscard(true);
    else controller.form(null);
  };
  const label = kind === "agenda" ? "아젠다" : "프로젝트";
  return (
    <Modal
      title={`${label} ${id ? "편집" : "추가"}`}
      close={handleClose}
      className="ndf"
    >
      <form
        onSubmit={async (e) => {
          e.preventDefault();
          setBusy(true);
          const r = await controller.data.saveEntity(
            kind,
            {
              title,
              hue,
              summary,
              ...(kind === "agenda"
                ? {
                    ...agendaRepoField(asText(old?.repo), repo),
                    ...(!id || project !== asText(old?.project)
                      ? { project }
                      : {}),
                  }
                : {}),
            },
            id,
          );
          setBusy(false);
          if (r.ok) {
            controller.form(null);
            if (kind === "agenda" && !id)
              controller.navigate(
                "/agenda/" + encodeURIComponent(asText(r.value?.id)),
              );
          } else setError(`${label}를 저장하지 못했어요.`);
        }}
      >
        <label>
          이름
          <input
            aria-label={`${label} 이름`}
            required
            value={title}
            onChange={(e) => setTitle(e.target.value)}
          />
        </label>
        <label className="hue-field">
          색
          <div className="hue-selector">
            <span
              className="hue-dot"
              style={{ backgroundColor: hue }}
              aria-hidden="true"
            />
            <select value={hue} onChange={(e) => setHue(e.target.value)}>
              {HUES.map((h) => (
                <option key={h} value={h}>
                  {HUE_NAMES[h] || h}
                </option>
              ))}
            </select>
          </div>
        </label>
        {kind === "agenda" && (
          <>
            {!!ws.projects.length && (
              <label>
                프로젝트
                <select
                  value={project}
                  onChange={(e) => setProject(e.target.value)}
                >
                  <option value="">미분류</option>
                  {project && !ws.projects.some((p) => p.slug === project) && (
                    <option value={project}>{project}</option>
                  )}
                  {ws.projects.map((p) => (
                    <option key={asText(p.id)} value={asText(p.slug)}>
                      {asText(p.title)}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <label>
              저장소
              <input value={repo} onChange={(e) => setRepo(e.target.value)} />
            </label>
          </>
        )}
        <label>
          설명
          <textarea
            value={summary}
            onChange={(e) => setSummary(e.target.value)}
          />
        </label>
        {error && <p role="alert">{error}</p>}
        <footer>
          {id && (
            <button type="button" onClick={() => setConfirm(true)}>
              삭제
            </button>
          )}
          <button type="button" onClick={handleClose}>
            취소
          </button>
          <button type="submit" disabled={busy || !title.trim()}>
            {id ? "저장" : "만들기"}
          </button>
        </footer>
      </form>
      {confirmDiscard && (
        <Confirm
          title="쓴 글을 버릴까요?"
          description="작성 중인 내용이 사라집니다."
          label="버리기"
          close={() => setConfirmDiscard(false)}
          accept={() => controller.form(null)}
        />
      )}
      {confirm && (
        <Confirm
          title={`${label}를 삭제할까요?`}
          description={
            kind === "agenda"
              ? "아젠다의 노트도 휴지통으로 옮깁니다."
              : "소속 아젠다를 미분류로 옮깁니다."
          }
          close={() => setConfirm(false)}
          accept={async () => {
            const r = await controller.data.deleteEntity(kind, id!);
            if (r.ok) {
              controller.form(null);
              if (kind === "agenda") {
                controller.navigate("/calendar", true);
              }
            } else setError(r.reason || `${label}를 지우지 못했어요.`);
          }}
        />
      )}
    </Modal>
  );
}
