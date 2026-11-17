import { useSyncExternalStore } from 'react';
import type { GlobalResourceItem, GlobalResourcesData, GlobalSkill, ProcessAgent } from './types.ts';
import { DEFAULT_GLOBAL_RESOURCES, DEFAULT_GLOBAL_SKILLS, DEFAULT_PROCESS_AGENTS } from './types.ts';

const STORAGE_KEY = 'aisurada.globalResources.v1';

const listeners = new Set<() => void>();

function getInitialData(): GlobalResourcesData {
  if (typeof window === 'undefined' || !window.localStorage) {
    return {
      agents: DEFAULT_PROCESS_AGENTS,
      skills: DEFAULT_GLOBAL_SKILLS,
      resources: DEFAULT_GLOBAL_RESOURCES,
      version: 1,
    };
  }

  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      const initial: GlobalResourcesData = {
        agents: DEFAULT_PROCESS_AGENTS,
        skills: DEFAULT_GLOBAL_SKILLS,
        resources: DEFAULT_GLOBAL_RESOURCES,
        version: 1,
      };
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(initial));
      return initial;
    }
    const parsed = JSON.parse(raw) as Partial<GlobalResourcesData>;
    return {
      agents: Array.isArray(parsed.agents) && parsed.agents.length > 0 ? parsed.agents : DEFAULT_PROCESS_AGENTS,
      skills: Array.isArray(parsed.skills) ? parsed.skills : DEFAULT_GLOBAL_SKILLS,
      resources: Array.isArray(parsed.resources) ? parsed.resources : DEFAULT_GLOBAL_RESOURCES,
      version: parsed.version || 1,
    };
  } catch (err) {
    console.error('글로벌 리소스 데이터를 불러오지 못했습니다:', err);
    return {
      agents: DEFAULT_PROCESS_AGENTS,
      skills: DEFAULT_GLOBAL_SKILLS,
      resources: DEFAULT_GLOBAL_RESOURCES,
      version: 1,
    };
  }
}

let storeState: GlobalResourcesData = getInitialData();

function emitChange() {
  for (const listener of listeners) {
    listener();
  }
}

function persist(next: GlobalResourcesData) {
  storeState = next;
  if (typeof window !== 'undefined' && window.localStorage) {
    try {
      window.localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    } catch (err) {
      console.error('글로벌 리소스를 저장하지 못했습니다:', err);
    }
  }
  emitChange();
}

export function getGlobalResourcesSnapshot(): GlobalResourcesData {
  return storeState;
}

export function subscribeGlobalResources(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function useGlobalResources(): GlobalResourcesData {
  return useSyncExternalStore(subscribeGlobalResources, getGlobalResourcesSnapshot, getGlobalResourcesSnapshot);
}

// --------------------------- 에이전트 조작 ---------------------------
export function saveProcessAgent(agent: ProcessAgent): void {
  const current = getGlobalResourcesSnapshot();
  const exists = current.agents.some((a) => a.id === agent.id);
  const nextAgents = exists
    ? current.agents.map((a) => (a.id === agent.id ? agent : a))
    : [...current.agents, agent];
  persist({ ...current, agents: nextAgents });
}

export function deleteProcessAgent(agentId: string): void {
  const current = getGlobalResourcesSnapshot();
  persist({ ...current, agents: current.agents.filter((a) => a.id !== agentId) });
}

// --------------------------- 스킬 조작 ---------------------------
export function saveGlobalSkill(skill: GlobalSkill): void {
  const current = getGlobalResourcesSnapshot();
  const exists = current.skills.some((s) => s.id === skill.id);
  const updatedSkill: GlobalSkill = {
    ...skill,
    updatedAt: new Date().toISOString(),
    createdAt: skill.createdAt || new Date().toISOString(),
  };
  const nextSkills = exists
    ? current.skills.map((s) => (s.id === skill.id ? updatedSkill : s))
    : [...current.skills, updatedSkill];
  persist({ ...current, skills: nextSkills });
}

export function deleteGlobalSkill(skillId: string): void {
  const current = getGlobalResourcesSnapshot();
  persist({ ...current, skills: current.skills.filter((s) => s.id !== skillId) });
}

// --------------------------- 리소스 조작 ---------------------------
export function saveGlobalResourceItem(item: GlobalResourceItem): void {
  const current = getGlobalResourcesSnapshot();
  const exists = current.resources.some((r) => r.id === item.id);
  const updatedItem: GlobalResourceItem = {
    ...item,
    updatedAt: new Date().toISOString(),
    createdAt: item.createdAt || new Date().toISOString(),
  };
  const nextResources = exists
    ? current.resources.map((r) => (r.id === item.id ? updatedItem : r))
    : [...current.resources, updatedItem];
  persist({ ...current, resources: nextResources });
}

export function deleteGlobalResourceItem(resourceId: string): void {
  const current = getGlobalResourcesSnapshot();
  persist({ ...current, resources: current.resources.filter((r) => r.id !== resourceId) });
}

// --------------------------- 초기화 ---------------------------
export function resetGlobalResourcesToDefault(): void {
  const initial: GlobalResourcesData = {
    agents: DEFAULT_PROCESS_AGENTS,
    skills: DEFAULT_GLOBAL_SKILLS,
    resources: DEFAULT_GLOBAL_RESOURCES,
    version: 1,
  };
  persist(initial);
}
