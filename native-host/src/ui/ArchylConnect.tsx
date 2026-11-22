import { useEffect, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { archylStatus, connectArchyl, disconnectArchyl, listArchylProjects } from '../editor/archyl.ts';
import type { ArchylProject } from '../editor/archyl.ts';
import type { RepoEntry } from './repoLinks.ts';

export function ArchylConnect() {
  const [connected, setConnected] = useState(false);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  useEffect(() => {
    if (isTauri()) void archylStatus().then(setConnected).catch(reason => setError(String(reason)));
  }, []);
  if (!isTauri()) return null;

  async function change(connect: boolean) {
    setBusy(true);
    setError('');
    try {
      await (connect ? connectArchyl() : disconnectArchyl());
      setConnected(connect);
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  return (
    <section className="res-section" aria-label="Archyl 연결">
      <h2>Archyl</h2>
      <p>{busy ? '브라우저에서 Archyl 연결을 완료해 주세요.' : connected ? '읽기 전용으로 연결되어 있어요.' : '개발 프로젝트의 C4 다이어그램을 노트에 불러와요.'}</p>
      <div className="res-toolbar">
        <button type="button" disabled={busy} onClick={() => void change(true)}>
          {connected ? '다시 연결' : '브라우저에서 Archyl 연결'}
        </button>
        {connected && <button type="button" disabled={busy} onClick={() => void change(false)}>연결 해제</button>}
      </div>
      {error && <p role="alert">{error}</p>}
    </section>
  );
}

export function ArchylProjectField({ value, onChange, repos }: {
  value: string;
  onChange(value: string): void;
  repos: RepoEntry[];
}) {
  const [projects, setProjects] = useState<ArchylProject[] | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState('');
  if (!isTauri()) return null;

  async function load() {
    setBusy(true);
    setError('');
    try {
      setProjects(await listArchylProjects());
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(false);
    }
  }

  const github = new Set(repos.map(repo => repo.github?.toLowerCase()).filter(Boolean));
  const matches = (project: ArchylProject) => project.repositories.some(repo => github.has(repo));
  const choices = [...(projects ?? [])].sort((a, b) => Number(matches(b)) - Number(matches(a)) || a.name.localeCompare(b.name));
  return (
    <div className="archyl-project-field">
      <label>
        Archyl 프로젝트
        <select value={value} onChange={event => onChange(event.target.value)}>
          <option value="">연결 안 함</option>
          {value && !choices.some(project => project.id === value) && <option value={value}>연결된 프로젝트 · {value}</option>}
          {choices.map(project => (
            <option value={project.id} key={project.id}>
              {project.name}{matches(project) ? ' · GitHub 저장소 일치' : ''}
            </option>
          ))}
        </select>
      </label>
      <button type="button" disabled={busy} onClick={() => void load()}>
        {busy ? '불러오는 중…' : 'Archyl 프로젝트 불러오기'}
      </button>
      {error && <p role="alert">{error}</p>}
      {!error && projects?.length === 0 && <p>볼 수 있는 Archyl 프로젝트가 없어요.</p>}
    </div>
  );
}
