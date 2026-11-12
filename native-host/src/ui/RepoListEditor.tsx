// 프로젝트 폼의 「저장소」 목록 편집기. 프로젝트는 저장소를 여러 개 둘 수 있고, 항목마다
// 이 컴퓨터의 폴더(path), GitHub 저장소(owner/name), 또는 둘 다 적는다. 폴더를 적으면 그 폴더를 쓰고(받지 않는다),
// GitHub만 적으면 저장할 때 앱 전용 폴더에 받아 둔다(받기 결과는 githubStore가 기억해 이 목록에 보인다).
import { useState } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import { cloneRepo, listGithubRepos, useGithub } from "./githubStore.ts";
import type { CloneState, GithubRepo } from "./githubStore.ts";
import { MAX_REPOS, defaultRepoName, localPathOf } from "./repoLinks.ts";
import type { RepoEntry } from "./repoLinks.ts";

type EditorHost = Pick<WorkspaceController, "callHost" | "pickDirectory">;

/** 항목 한 줄 아래에 보일 받기 상태 글. 보일 말이 없으면 빈 글이다. */
export function cloneLine(entry: RepoEntry, clone: CloneState | undefined, connected: boolean): string {
  if (entry.path || !entry.github) return "";
  if (clone?.phase === "cloning") return "받는 중이에요…";
  if (clone?.phase === "done") return clone.cloned ? `받았어요 · ${clone.path}` : `이미 받아 둔 폴더가 있어요 · ${clone.path}`;
  if (clone?.phase === "error") return `받지 못했어요 · ${clone.error ?? "이유를 알 수 없어요."}`;
  return connected ? "저장하면 앱이 받아 둬요." : "GitHub를 연결하면 저장할 때 받아 둬요. 설정에서 연결해 주세요.";
}

export function RepoListEditor({
  entries,
  onChange,
  host,
}: {
  entries: RepoEntry[];
  onChange: (next: RepoEntry[]) => void;
  host: EditorHost;
}) {
  const github = useGithub();
  const connected = !!github.status?.connected;
  const [picking, setPicking] = useState<number | null>(null);
  const [repos, setRepos] = useState<GithubRepo[] | null>(null);
  const [loadError, setLoadError] = useState("");
  const [query, setQuery] = useState("");

  const patch = (index: number, change: Partial<RepoEntry>) =>
    onChange(entries.map((entry, i) => (i === index ? { ...entry, ...change } : entry)));

  const openPicker = async (index: number) => {
    setPicking(index);
    setQuery("");
    setLoadError("");
    if (repos) return;
    try {
      const list = await listGithubRepos(host.callHost);
      setRepos(list.repositories);
      if (list.errors.length) setLoadError(list.errors.join(" / "));
    } catch (e) {
      setLoadError(typeof e === "string" ? e : e instanceof Error ? e.message : "저장소 목록을 불러오지 못했어요.");
    }
  };

  const pick = (index: number, repo: GithubRepo) => {
    const taken = entries.filter((_, i) => i !== index).map((e) => e.name);
    patch(index, {
      github: repo.fullName,
      githubId: repo.id,
      name: entries[index].name.trim() ? entries[index].name : defaultRepoName(repo.fullName, taken),
    });
    setPicking(null);
  };

  const shown = (repos ?? []).filter((r) => r.fullName.toLowerCase().includes(query.trim().toLowerCase())).slice(0, 50);

  return (
    <fieldset className="repo-editor">
      <legend>저장소</legend>
      <p className="repo-editor-hint">
        아젠다가 고를 수 있는 저장소예요. 이 컴퓨터의 폴더를 적으면 그 폴더를 쓰고, GitHub 저장소만 적으면 앱이 받아 둬요.
      </p>
      {entries.map((entry, index) => {
        const clone = entry.github ? github.clones[entry.github] : undefined;
        const line = cloneLine(entry, clone, connected);
        const where = localPathOf(entry, github.status?.cloneRoot);
        return (
          <div className="repo-row" key={index} data-testid="repo-row">
            <div className="repo-row-head">
              <input
                aria-label={`${index + 1}번째 저장소 이름`}
                placeholder="이름 (예: app)"
                value={entry.name}
                onChange={(e) => patch(index, { name: e.target.value })}
              />
              <button type="button" aria-label={`${index + 1}번째 저장소 빼기`} onClick={() => onChange(entries.filter((_, i) => i !== index))}>
                빼기
              </button>
            </div>
            <div className="repo-row-field">
              <input
                aria-label={`${index + 1}번째 저장소 폴더`}
                placeholder="이 컴퓨터의 폴더 (선택) 예: /Users/me/app"
                value={entry.path ?? ""}
                onChange={(e) => patch(index, { path: e.target.value })}
              />
              <button
                type="button"
                onClick={() => void host.pickDirectory().then((dir) => dir && patch(index, { path: dir }))}
              >
                폴더 고르기
              </button>
            </div>
            <div className="repo-row-field">
              <input
                aria-label={`${index + 1}번째 저장소 GitHub`}
                placeholder="GitHub 저장소 (선택) 예: owner/name"
                value={entry.github ?? ""}
                // 손으로 고치면 숫자 ID는 더는 믿을 수 없어 버린다. 목록에서 고를 때만 ID가 붙는다.
                onChange={(e) => patch(index, { github: e.target.value, githubId: undefined })}
              />
              <button type="button" disabled={!connected} title={connected ? "" : "설정에서 GitHub를 먼저 연결해 주세요."} onClick={() => void openPicker(index)}>
                내 저장소에서 고르기
              </button>
            </div>
            {picking === index && (
              <div className="repo-picker">
                <input
                  aria-label="저장소 찾기"
                  placeholder="이름으로 찾기"
                  value={query}
                  autoFocus
                  onChange={(e) => setQuery(e.target.value)}
                />
                {!repos && !loadError && <p>불러오는 중이에요…</p>}
                {loadError && <p role="alert">{loadError}</p>}
                {repos && !shown.length && <p>앱을 설치한 저장소 중에 맞는 것이 없어요.</p>}
                <ul>
                  {shown.map((repo) => (
                    <li key={repo.id}>
                      <button type="button" onClick={() => pick(index, repo)}>
                        {repo.fullName}
                        {repo.private ? " 🔒" : ""}
                      </button>
                    </li>
                  ))}
                </ul>
                <button type="button" onClick={() => setPicking(null)}>
                  닫기
                </button>
              </div>
            )}
            {(line || where) && (
              <p className="repo-row-note" role={clone?.phase === "error" ? "alert" : "status"}>
                {line || `쓰는 폴더 · ${where}`}
                {entry.github && !entry.path && connected && clone?.phase !== "cloning" && (
                  <>
                    {" "}
                    <button type="button" onClick={() => void cloneRepo(host.callHost, entry.github!)}>
                      지금 받기
                    </button>
                  </>
                )}
              </p>
            )}
          </div>
        );
      })}
      <button
        type="button"
        disabled={entries.length >= MAX_REPOS}
        onClick={() => onChange([...entries, { name: "" }])}
      >
        저장소 추가
      </button>
    </fieldset>
  );
}
