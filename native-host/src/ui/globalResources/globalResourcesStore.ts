import { useSyncExternalStore } from 'react';
import type { EngineCall, ResourceRow } from '../resourcesClient.ts';
import { deleteResource, getResource, listResources, saveResource } from '../resourcesClient.ts';
import type { GlobalResourceItem, GlobalResourcesData, GlobalSkill, ProcessAgent } from './types.ts';
import {
  DEFAULT_GLOBAL_RESOURCES,
  DEFAULT_GLOBAL_SKILLS,
  DEFAULT_PROCESS_AGENTS,
  globalSkillToSaveInput,
  processAgentToSaveInput,
  resourceRowToGlobalSkill,
  resourceRowToProcessAgent,
} from './types.ts';

const INFRAS_STORAGE_KEY = 'aisurada.globalInfras.v1';

const listeners = new Set<() => void>();

let storeState: GlobalResourcesData = {
  agents: DEFAULT_PROCESS_AGENTS,
  skills: DEFAULT_GLOBAL_SKILLS,
  resources: DEFAULT_GLOBAL_RESOURCES,
  version: 1,
};

let initializingPromise: Promise<void> | null = null;

function emitChange() {
  for (const listener of listeners) {
    listener();
  }
}

function loadInfrasFromStorage(): GlobalResourceItem[] {
  if (typeof window === 'undefined' || !window.localStorage) {
    return DEFAULT_GLOBAL_RESOURCES;
  }
  try {
    const raw = window.localStorage.getItem(INFRAS_STORAGE_KEY);
    if (!raw) {
      window.localStorage.setItem(INFRAS_STORAGE_KEY, JSON.stringify(DEFAULT_GLOBAL_RESOURCES));
      return DEFAULT_GLOBAL_RESOURCES;
    }
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed) && parsed.length > 0 ? parsed : DEFAULT_GLOBAL_RESOURCES;
  } catch {
    return DEFAULT_GLOBAL_RESOURCES;
  }
}

function saveInfrasToStorage(infras: GlobalResourceItem[]): void {
  if (typeof window !== 'undefined' && window.localStorage) {
    try {
      window.localStorage.setItem(INFRAS_STORAGE_KEY, JSON.stringify(infras));
    } catch (err) {
      console.error('인프라 리소스 저장 실패:', err);
    }
  }
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

/** 엔진에서 글로벌 자원들을 로드하고, 없을 경우 10대 프로세스 에이전트를 자동 시딩합니다 */
export async function syncGlobalResources(call: EngineCall): Promise<void> {
  if (initializingPromise) {
    return initializingPromise;
  }

  initializingPromise = (async () => {
    try {
      const { resources: globalRows } = await listResources(call, { global: true });
      const agentRows = globalRows.filter((r: ResourceRow) => r.kind === 'agent');
      const skillRows = globalRows.filter((r: ResourceRow) => r.kind === 'skill');

      // 1. 글로벌 스킬 자동 시딩 (없는 경우)
      if (skillRows.length === 0) {
        for (const defaultSkill of DEFAULT_GLOBAL_SKILLS) {
          try {
            await saveResource(call, globalSkillToSaveInput(defaultSkill));
          } catch (err) {
            console.warn(`기본 스킬 시딩 건너뜀 (${defaultSkill.name}):`, err);
          }
        }
      }

      // 2. 프로세스 에이전트 자동 시딩 (엔진에 글로벌 에이전트가 하나도 없을 때)
      if (agentRows.length === 0) {
        const { resources: refreshedSkills } = await listResources(call, { global: true, kind: 'skill' });
        const skillIdMap = new Map<string, string>();
        for (const s of refreshedSkills) {
          skillIdMap.set(s.name, s.id);
        }

        for (const defaultAgent of DEFAULT_PROCESS_AGENTS) {
          try {
            const mappedSkills = defaultAgent.skills
              .map((sId) => {
                const sName = sId.replace(/^gskill-/, '');
                return skillIdMap.get(sName) || sId;
              })
              .filter((id) => id.startsWith('sk'));

            const agentInput = processAgentToSaveInput({
              ...defaultAgent,
              skills: mappedSkills,
            });
            await saveResource(call, agentInput);
          } catch (err) {
            console.warn(`기본 프로세스 에이전트 시딩 건너뜀 (${defaultAgent.name}):`, err);
          }
        }
      }

      // 3. 최신 엔진 목록 다시 조회하여 상세 데이터(body 및 precondition) 채우기
      const { resources: refreshedGlobal } = await listResources(call, { global: true });
      const refreshedAgentRows = refreshedGlobal.filter((r: ResourceRow) => r.kind === 'agent');
      const refreshedSkillRows = refreshedGlobal.filter((r: ResourceRow) => r.kind === 'skill');

      const detailedAgents: ProcessAgent[] = await Promise.all(
        refreshedAgentRows.map(async (row: ResourceRow, idx: number) => {
          try {
            const detail = await getResource(call, row.id);
            return resourceRowToProcessAgent(detail, idx);
          } catch {
            return resourceRowToProcessAgent(row, idx);
          }
        })
      );

      const detailedSkills: GlobalSkill[] = await Promise.all(
        refreshedSkillRows.map(async (row: ResourceRow) => {
          try {
            const detail = await getResource(call, row.id);
            return resourceRowToGlobalSkill(detail);
          } catch {
            return resourceRowToGlobalSkill(row);
          }
        })
      );

      const infras = loadInfrasFromStorage();

      storeState = {
        agents: detailedAgents.sort((a, b) => a.order - b.order),
        skills: detailedSkills,
        resources: infras,
        version: 1,
      };
      emitChange();
    } catch (err) {
      console.error('글로벌 리소스 동기화 중 오류 발생:', err);
    } finally {
      initializingPromise = null;
    }
  })();

  return initializingPromise;
}

// --------------------------- 에이전트 조작 (엔진 연동) ---------------------------

export async function saveProcessAgent(agent: ProcessAgent, call: EngineCall): Promise<void> {
  const input = processAgentToSaveInput(agent);
  await saveResource(call, input);
  await syncGlobalResources(call);
}

export async function deleteProcessAgent(agentId: string, call: EngineCall): Promise<void> {
  await deleteResource(call, agentId);
  await syncGlobalResources(call);
}

// --------------------------- 스킬 조작 (엔진 연동) ---------------------------

export async function saveGlobalSkill(skill: GlobalSkill, call: EngineCall): Promise<void> {
  const input = globalSkillToSaveInput(skill);
  await saveResource(call, input);
  await syncGlobalResources(call);
}

export async function deleteGlobalSkill(skillId: string, call: EngineCall): Promise<void> {
  await deleteResource(call, skillId);
  await syncGlobalResources(call);
}

// --------------------------- 인프라 조작 ---------------------------

export function saveGlobalResourceItem(item: GlobalResourceItem): void {
  const current = getGlobalResourcesSnapshot();
  const exists = current.resources.some((r) => r.id === item.id);
  const next = exists
    ? current.resources.map((r) => (r.id === item.id ? item : r))
    : [...current.resources, item];
  saveInfrasToStorage(next);
  storeState = { ...current, resources: next };
  emitChange();
}

export function deleteGlobalResourceItem(itemId: string): void {
  const current = getGlobalResourcesSnapshot();
  const next = current.resources.filter((r) => r.id !== itemId);
  saveInfrasToStorage(next);
  storeState = { ...current, resources: next };
  emitChange();
}

export async function resetGlobalResourcesToDefault(call: EngineCall): Promise<void> {
  const current = getGlobalResourcesSnapshot();
  for (const a of current.agents) {
    try {
      await deleteResource(call, a.id);
    } catch {}
  }
  saveInfrasToStorage(DEFAULT_GLOBAL_RESOURCES);
  await syncGlobalResources(call);
}
