// 프로젝트의 저장소 목록(`repos`)과 아젠다의 `projectRepo`를 다루는 순수 함수들. 화면과 분리해 시험한다.
// 엔진(`ProjectRepo`)의 검사 규칙과 같게 맞춰서, 엔진이 거절할 값을 폼에서 먼저 알려 준다.
import type { Row } from "../workspace/types.ts";

export interface RepoEntry {
  /** 프로젝트 안에서 저장소를 가리키는 이름. 아젠다의 `projectRepo`가 쓴다 */
  name: string;
  /** 이 컴퓨터의 로컬 폴더 절대 경로 */
  path?: string;
  /** GitHub 저장소 `owner/name` */
  github?: string;
  /** GitHub 저장소의 숫자 ID. 이름이 바뀌어도 같은 저장소를 알아본다 */
  githubId?: number;
}

export const MAX_REPOS = 20;
const GITHUB = /^[A-Za-z0-9](?:[A-Za-z0-9-]{0,38})\/[A-Za-z0-9._-]{1,100}$/;
const text = (value: unknown) => (typeof value === "string" ? value : "");

/**
 * 엔진이 돌려준 `repos` 배열을 항목 목록으로 읽는다. 모양이 틀린 항목은 건너뛴다.
 * 저장 직후 화면이 먼저 그려 두는 값은 JSON 글이라(엔진 응답이 오기 전) 글도 받는다.
 */
export function parseRepos(value: unknown): RepoEntry[] {
  if (typeof value === "string") {
    try {
      return parseRepos(JSON.parse(value));
    } catch {
      return [];
    }
  }
  if (!Array.isArray(value)) return [];
  const out: RepoEntry[] = [];
  for (const item of value) {
    if (!item || typeof item !== "object") continue;
    const row = item as Row;
    const name = text(row.name).trim();
    if (!name) continue;
    const entry: RepoEntry = { name };
    if (text(row.path).trim()) entry.path = text(row.path).trim();
    if (text(row.github).trim()) entry.github = text(row.github).trim();
    if (typeof row.githubId === "number" && Number.isFinite(row.githubId) && row.githubId > 0) entry.githubId = row.githubId;
    out.push(entry);
  }
  return out;
}

/** 폼에서 고친 항목을 정리한다: 글은 다듬고, 빈 값은 키째 빼고, githubId는 github가 있을 때만 둔다. */
export function cleanRepos(entries: RepoEntry[]): RepoEntry[] {
  return entries.map((entry) => {
    const clean: RepoEntry = { name: entry.name.trim() };
    if (entry.path?.trim()) clean.path = entry.path.trim();
    if (entry.github?.trim()) {
      clean.github = entry.github.trim();
      if (entry.githubId) clean.githubId = entry.githubId;
    }
    return clean;
  });
}

/** 엔진의 `repos=` 인자 값. 키 차례는 name, path, github, githubId. 빈 목록이면 빈 글이다(update에서는 지우는 뜻). */
export function reposJson(entries: RepoEntry[]): string {
  const clean = cleanRepos(entries);
  return clean.length ? JSON.stringify(clean.map(({ name, path, github, githubId }) => ({ name, path, github, githubId }))) : "";
}

export function sameRepos(a: RepoEntry[], b: RepoEntry[]): boolean {
  return reposJson(a) === reposJson(b);
}

/** 엔진이 거절할 값을 미리 알려 준다. 문제가 없으면 null이다. */
export function validateRepos(entries: RepoEntry[]): string | null {
  const clean = cleanRepos(entries);
  if (clean.length > MAX_REPOS) return `저장소는 ${MAX_REPOS}개까지 둘 수 있어요.`;
  const names = new Set<string>();
  for (let i = 0; i < clean.length; i++) {
    const entry = clean[i];
    const at = `${i + 1}번째 저장소`;
    if (!entry.name) return `${at}에 이름이 필요해요.`;
    if (entry.name.length > 50 || entry.name.includes("/")) return `${at}의 이름은 50자 안에서 빗금 없이 적어 주세요: ${entry.name}`;
    if (names.has(entry.name)) return `저장소 이름이 겹쳐요: ${entry.name}`;
    names.add(entry.name);
    if (entry.path && !entry.path.startsWith("/")) return `${at}의 폴더는 /로 시작하는 전체 경로여야 해요: ${entry.path}`;
    if (entry.github && !GITHUB.test(entry.github)) return `${at}의 GitHub 저장소는 owner/name 모양이어야 해요: ${entry.github}`;
    if (!entry.path && !entry.github) return `${at}에 폴더나 GitHub 저장소 가운데 하나는 있어야 해요.`;
  }
  return null;
}

/** GitHub `owner/name`에서 항목 이름 후보를 만든다. 이미 쓰는 이름과 겹치면 `-2`, `-3`을 붙인다. */
export function defaultRepoName(github: string, taken: Iterable<string>): string {
  const used = new Set(taken);
  const base = (github.split("/")[1] || github).replace(/[^A-Za-z0-9._-]/g, "-").slice(0, 44) || "repo";
  if (!used.has(base)) return base;
  for (let n = 2; n < 100; n++) if (!used.has(`${base}-${n}`)) return `${base}-${n}`;
  return base;
}

/** 항목이 가리키는 이 컴퓨터의 폴더. 폴더를 적지 않은 GitHub 항목은 앱이 받아 둘 자리(받기 전에도 같은 자리)다. */
export function localPathOf(entry: RepoEntry, cloneRoot?: string): string | undefined {
  if (entry.path) return entry.path;
  if (entry.github && cloneRoot) return `${cloneRoot.replace(/\/+$/, "")}/${entry.github}`;
  return undefined;
}

/**
 * 저장하고 나서 받아야 할 GitHub 항목. 폴더를 적은 항목은 그 폴더를 쓰니 받지 않는다.
 * 이전 목록에 같은 저장소(같은 이름·같은 `owner/name`·폴더 없음)가 이미 있었으면 그때 받았으니 건너뛴다.
 */
export function entriesToClone(next: RepoEntry[], previous: RepoEntry[] = []): RepoEntry[] {
  const seen = new Set(previous.filter((e) => !e.path && e.github).map((e) => `${e.name}\u0000${e.github}`));
  return cleanRepos(next).filter((e) => !e.path && e.github && !seen.has(`${e.name}\u0000${e.github}`));
}

/**
 * 프로젝트 저장 인자의 `repos` 칸. 새 프로젝트는 항목이 있을 때만 보내고, 고치는 프로젝트는 바뀌었을 때만 보낸다.
 * 항목을 모두 지웠으면 빈 글을 보내 엔진이 `repos:` 줄을 지우게 한다.
 */
export function reposField(previous: unknown, next: RepoEntry[], isNew: boolean): { repos?: string } {
  if (isNew) return next.length ? { repos: reposJson(next) } : {};
  return sameRepos(parseRepos(previous), next) ? {} : { repos: reposJson(next) };
}

/**
 * 아젠다 저장 인자의 `projectRepo` 칸. 새 아젠다는 고른 것이 있을 때만, 고치는 아젠다는 이전 값과 다를 때만 보낸다.
 * 프로젝트를 바꾸고 새 저장소를 따로 고르지 않았다면 보내지 않는다(엔진이 낡은 가리킴을 알아서 지운다).
 */
export function projectRepoField(previous: string, next: string, isNew: boolean): { projectRepo?: string } {
  if (isNew) return next ? { projectRepo: next } : {};
  return next === previous ? {} : { projectRepo: next };
}

export const githubUrl = (github: string) => `https://github.com/${github}`;

/** 항목을 한 줄로 보인다: `app · /Users/me/app`, `docs · me/docs`. */
export function repoLabel(entry: RepoEntry): string {
  return `${entry.name} · ${entry.path || entry.github || ""}`.trim();
}

export interface AgendaRepoView {
  /** 아젠다가 쓰는 로컬 폴더(세션 붙이기에 쓴다). 없으면 undefined */
  path?: string;
  /** 화면에 한 줄로 보일 글. 가리키는 저장소가 없으면 빈 글 */
  label: string;
  /** 프로젝트의 저장소 가운데 가리킨 항목. 낡은 이름이거나 안 가리키면 undefined */
  entry?: RepoEntry;
  /** `projectRepo`를 적었는데 프로젝트에 그 이름이 없다 */
  dangling: boolean;
}

/**
 * 아젠다가 쓰는 저장소를 푼다. `projectRepo`가 프로젝트의 항목을 가리키면 그것을, 아니면 옛 `repo`(로컬 경로 글)를 쓴다.
 * 프로젝트가 저장소를 지워 낡은 이름이 남으면 옛 `repo`로 물러나고 `dangling`으로 알린다.
 */
export function agendaRepoView(agenda: Row, projects: Row[], cloneRoot?: string): AgendaRepoView {
  const projectRepo = text(agenda.projectRepo).trim();
  const legacy = text(agenda.repo).trim();
  let dangling = false;
  if (projectRepo) {
    const project = projects.find((p) => p.slug === agenda.project);
    const entry = parseRepos(project?.repos).find((e) => e.name === projectRepo);
    if (entry) return { entry, path: localPathOf(entry, cloneRoot), label: repoLabel(entry), dangling: false };
    dangling = true;
  }
  return { path: legacy || undefined, label: legacy, dangling };
}
