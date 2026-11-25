import { useEffect, useSyncExternalStore } from 'react';
import type { WorkspaceController } from '../../workspace/controller.ts';

export type ArchylProject = { id: string; name: string };
export type C4Element = {
  id: string;
  name: string;
  description: string;
  technology: string;
  level: number;
  parentId: string | null;
  kind: string;
  position: { x: number; y: number } | null;
  icon: string | null;
  color: string | null;
};
export type C4Relationship = {
  id: string | null;
  source: string;
  target: string;
  label: string;
  technology: string;
};
export type C4Model = { elements: C4Element[]; relationships: C4Relationship[] };
export type ArchylMessage = { role: 'user' | 'assistant'; text: string; warning?: string; updated?: boolean };
export type ArchylReply = { message: string; applied: number; warning: string | null; model: C4Model | null };
type Connection = { writable: boolean; projects: ArchylProject[] };
type Account = {
  connection: Connection | null;
  phase: 'idle' | 'checking' | 'login' | 'disconnect';
  error: string;
  checked: boolean;
  revision: number;
};

let account: Account = { connection: null, phase: 'idle', error: '', checked: false, revision: 0 };
let requestId = 0;
let cancelledLogin = 0;
const listeners = new Set<() => void>();
const subscribe = (listener: () => void) => { listeners.add(listener); return () => { listeners.delete(listener); }; };
const getAccount = () => account;
function setAccount(patch: Partial<Account>) {
  account = { ...account, ...patch };
  listeners.forEach(listener => listener());
}

export const errorText = (value: unknown) =>
  typeof value === 'string' ? value : value instanceof Error ? value.message : 'Archyl 요청을 처리하지 못했어요.';

async function change(
  phase: Exclude<Account['phase'], 'idle'>,
  invoke: () => Promise<Connection | null>,
) {
  if (account.phase !== 'idle') return;
  const id = ++requestId;
  setAccount({ phase, error: '' });
  try {
    const connection = await invoke();
    if (id === requestId) {
      setAccount({ connection, checked: true, revision: account.revision + (phase === 'checking' ? 0 : 1) });
    }
  } catch (error) {
    if (id === requestId && id !== cancelledLogin) {
      setAccount({ checked: true, error: errorText(error), ...(phase === 'checking' ? { connection: null } : {}) });
    }
  } finally {
    if (id === requestId) setAccount({ phase: 'idle' });
  }
}

export const reloadAccount = (host: WorkspaceController['callHost']) =>
  change('checking', () => host<Connection | null>('archyl_status'));
export const loginAccount = (host: WorkspaceController['callHost'], readOnly: boolean) =>
  change('login', () => host<Connection>('archyl_login', { readOnly }));
export const disconnectAccount = (host: WorkspaceController['callHost']) =>
  change('disconnect', async () => { await host('archyl_disconnect'); return null; });

export async function cancelLogin(host: WorkspaceController['callHost']) {
  if (account.phase !== 'login') return;
  const id = requestId;
  cancelledLogin = id;
  try { await host('archyl_cancel_login'); }
  catch (error) {
    if (id === requestId) {
      cancelledLogin = 0;
      setAccount({ error: errorText(error) });
    }
  }
}

export function useArchylAccount(controller: WorkspaceController) {
  const snapshot = useSyncExternalStore(subscribe, getAccount, getAccount);
  const native = controller.isNative();
  const host = controller.callHost;
  useEffect(() => { if (native) void reloadAccount(host); }, [host, native]);
  return snapshot;
}

export const getArchylModel = (host: WorkspaceController['callHost'], projectId: string) =>
  host<C4Model>('archyl_model', { projectId });
export const sendArchylRequest = (
  host: WorkspaceController['callHost'],
  projectId: string,
  selectedId: string | null,
  request: string,
  history: ArchylMessage[],
) => host<ArchylReply>('archyl_chat', { projectId, selectedId, request, history: history.slice(-8) });

export function readProjectLink(key: string): string {
  try { return localStorage.getItem(key) ?? ''; } catch { return ''; }
}

export function writeProjectLink(key: string, value: string): boolean {
  try {
    if (value) localStorage.setItem(key, value);
    else localStorage.removeItem(key);
    return true;
  } catch { return false; }
}
