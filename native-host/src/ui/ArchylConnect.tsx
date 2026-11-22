import { useEffect, useState } from 'react';
import { isTauri } from '@tauri-apps/api/core';
import { archylConnection, connectArchyl, disconnectArchyl, listArchylProjects } from '../editor/archyl.ts';
import type { ArchylConnection, ArchylConnectionRequest, ArchylProject } from '../editor/archyl.ts';
import type { RepoEntry } from './repoLinks.ts';

export function ArchylConnect() {
  const [connection, setConnection] = useState<ArchylConnection | null>(null);
  const [apiKey, setApiKey] = useState('');
  const [pending, setPending] = useState('');
  const [error, setError] = useState('');
  useEffect(() => {
    if (isTauri()) void archylConnection().then(setConnection).catch(reason => setError(String(reason)));
  }, []);
  if (!isTauri()) return null;

  async function change(request?: ArchylConnectionRequest) {
    setPending(!request ? 'Archyl 연결을 해제하는 중…'
      : request.method === 'oauth' && !request.reuse ? '브라우저에서 Archyl 연결을 완료해 주세요.'
      : 'Archyl 프로젝트를 조회해 연결을 확인하는 중…');
    setError('');
    try {
      await (request ? connectArchyl(request) : disconnectArchyl());
      setApiKey('');
      setConnection(await archylConnection());
    } catch (reason) {
      setError(String(reason));
    } finally {
      setPending('');
    }
  }

  const busy = Boolean(pending);
  const method = connection?.method;
  return (
    <section className="res-section" aria-label="Archyl 연결">
      <h2>Archyl</h2>
      <p role="status">{pending || (method === 'api_key' ? '저장한 API 키를 사용해요.'
        : method === 'oauth' ? '저장한 브라우저 연결을 사용해요.'
        : '요소와 관계를 추가하려면 Archyl에서 읽기와 쓰기 권한이 있는 키를 발급해 주세요.')}</p>
      <form className="res-toolbar" onSubmit={event => {
        event.preventDefault();
        if (!busy && apiKey.trim()) void change({ method: 'api_key', key: apiKey });
      }}>
        <label className="archyl-api-key-field">
          API 키
          <input type="password" autoComplete="off" spellCheck={false} value={apiKey}
            onChange={event => setApiKey(event.target.value)} disabled={busy} />
        </label>
        <button type="submit" disabled={busy || !apiKey.trim()}>
          {method === 'api_key' ? 'API 키 바꾸기' : 'API 키로 연결'}
        </button>
        {connection?.savedApiKey && method !== 'api_key' && (
          <button type="button" disabled={busy} onClick={() => void change({ method: 'api_key' })}>
            저장한 API 키 사용
          </button>
        )}
      </form>
      <div className="res-toolbar">
        {connection?.savedOauth && method !== 'oauth' && (
          <button type="button" disabled={busy} onClick={() => void change({ method: 'oauth', reuse: true })}>
            저장한 브라우저 연결 사용
          </button>
        )}
        <button type="button" disabled={busy} onClick={() => void change({ method: 'oauth', reuse: false })}>
          {method === 'oauth' ? '브라우저에서 다시 연결' : '브라우저에서 연결'}
        </button>
        {method && <button type="button" disabled={busy} onClick={() => void change()}>연결 해제</button>}
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
