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

let activeSyncPromise: Promise<void> | null = null;
let needsAnotherSync = false;

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
    if (!Array.isArray(parsed) || !parsed.length) return DEFAULT_GLOBAL_RESOURCES;
    const timestamp = (value: unknown) => typeof value === 'string' && value.trim() ? value : undefined;
    const now = new Date().toISOString();
    const infras = (parsed as (GlobalResourceItem & { updatedAt?: unknown })[]).map(({ updatedAt, ...item }) => {
      const editedAt = timestamp(item.editedAt) ?? timestamp(updatedAt);
      const createdAt = timestamp(item.createdAt) ?? editedAt ?? now;
      return { ...item, createdAt, editedAt: editedAt ?? createdAt };
    });
    if (JSON.stringify(parsed) !== JSON.stringify(infras)) saveInfrasToStorage(infras);
    return infras;
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
export async function syncGlobalResources(call: EngineCall, force = false): Promise<void> {
  if (activeSyncPromise) {
    if (force) {
      needsAnotherSync = true;
    }
    await activeSyncPromise;
    if (!needsAnotherSync) {
      return;
    }
  }

  needsAnotherSync = false;
  const runSync = async () => {
    try {
      const { resources: globalRows } = await listResources(call, { global: true });
      const agentRows = globalRows.filter((r: ResourceRow) => r.kind === 'agent');
      const skillRows = globalRows.filter((r: ResourceRow) => r.kind === 'skill');

      // 1. 글로벌 기본 스킬 자동 시딩 (누락된 기본 스킬 각각 보충)
      const existingSkillNames = new Set(skillRows.map((r: ResourceRow) => r.name));
      for (const defaultSkill of DEFAULT_GLOBAL_SKILLS) {
        if (!existingSkillNames.has(defaultSkill.name)) {
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
      activeSyncPromise = null;
    }
  };

  activeSyncPromise = runSync();
  await activeSyncPromise;

  if (needsAnotherSync) {
    needsAnotherSync = false;
    return syncGlobalResources(call, false);
  }
}

// --------------------------- 에이전트 조작 (엔진 연동) ---------------------------

export async function saveProcessAgent(agent: ProcessAgent, call: EngineCall): Promise<void> {
  let targetId = agent.id;
  let f1 = agent.f1;

  // mock/시딩 id(proc-)인 경우 이미 디스크/스토어에 존재하는 동일 이름/키의 에이전트가 있는지 찾아 실 id와 f1 계승
  if (targetId.startsWith('proc-')) {
    const existing = storeState.agents.find(
      (a) => !a.id.startsWith('proc-') && (a.name === agent.name || a.key === agent.key)
    );
    if (existing) {
      targetId = existing.id;
      f1 = existing.f1;
    } else {
      try {
        const { resources } = await listResources(call, { global: true, kind: 'agent' });
        const matched = resources.find((r) => r.name === agent.name);
        if (matched) {
          targetId = matched.id;
          f1 = matched.precondition;
        }
      } catch {}
    }
  }

  // 실 id가 있으나 f1(precondition 토큰)이 비어있는 경우 getResource로 최신 토큰 확보
  if (targetId && !targetId.startsWith('proc-') && !f1) {
    try {
      const fresh = await getResource(call, targetId);
      f1 = fresh.precondition;
    } catch (err) {
      console.warn(`에이전트(${targetId}) 최신 토큰 조회 실패:`, err);
    }
  }

  // 유효한 글로벌 스킬만 유지 (엔진 validate_agent_links 오류 방지: 삭제되거나 존재하지 않는 스킬 ID 제거)
  let validSkills = agent.skills || [];
  if (storeState.skills.length > 0) {
    const activeSkillIds = new Set(storeState.skills.map((s) => s.id));
    validSkills = validSkills.filter((sId) => activeSkillIds.has(sId));
  }

  const agentToSave: ProcessAgent = {
    ...agent,
    id: targetId,
    f1,
    skills: validSkills,
  };

  const input = processAgentToSaveInput(agentToSave);
  const savedRow = await saveResource(call, input);

  // 저장 성공 시 즉각 storeState 반영 (UI 낙관적 즉시 업데이트)
  const updatedAgent: ProcessAgent = {
    ...agentToSave,
    id: savedRow.id || targetId,
    f1: savedRow.precondition || f1,
  };

  const exists = storeState.agents.some((a) => a.id === updatedAgent.id || a.key === updatedAgent.key);
  const nextAgents = exists
    ? storeState.agents.map((a) => (a.id === updatedAgent.id || a.key === updatedAgent.key ? updatedAgent : a))
    : [...storeState.agents, updatedAgent];

  storeState = {
    ...storeState,
    agents: nextAgents.sort((a, b) => a.order - b.order),
  };
  emitChange();

  // 최신 디스크 상태 전체 재동기화 강제 실행
  await syncGlobalResources(call, true);
}

export async function deleteProcessAgent(agentId: string, call: EngineCall): Promise<void> {
  await deleteResource(call, agentId);
  storeState = {
    ...storeState,
    agents: storeState.agents.filter((a) => a.id !== agentId),
  };
  emitChange();
  await syncGlobalResources(call, true);
}

// --------------------------- 스킬 조작 (엔진 연동) ---------------------------

export async function saveGlobalSkill(skill: GlobalSkill, call: EngineCall): Promise<void> {
  let targetId = skill.id;
  let f1 = skill.f1;

  if (targetId.startsWith('gskill-')) {
    const existing = storeState.skills.find(
      (s) => !s.id.startsWith('gskill-') && s.name === skill.name
    );
    if (existing) {
      targetId = existing.id;
      f1 = existing.f1;
    } else {
      try {
        const { resources } = await listResources(call, { global: true, kind: 'skill' });
        const matched = resources.find((r) => r.name === skill.name);
        if (matched) {
          targetId = matched.id;
          f1 = matched.precondition;
        }
      } catch {}
    }
  }

  if (targetId && !targetId.startsWith('gskill-') && !f1) {
    try {
      const fresh = await getResource(call, targetId);
      f1 = fresh.precondition;
    } catch (err) {
      console.warn(`스킬(${targetId}) 최신 토큰 조회 실패:`, err);
    }
  }

  const skillToSave: GlobalSkill = {
    ...skill,
    id: targetId,
    f1,
  };

  const input = globalSkillToSaveInput(skillToSave);
  const savedRow = await saveResource(call, input);

  const updatedSkill: GlobalSkill = {
    ...skillToSave,
    id: savedRow.id || targetId,
    createdAt: savedRow.createdAt,
    editedAt: savedRow.editedAt,
    f1: savedRow.precondition || f1,
  };

  const exists = storeState.skills.some((s) => s.id === updatedSkill.id || s.name === updatedSkill.name);
  const nextSkills = exists
    ? storeState.skills.map((s) => (s.id === updatedSkill.id || s.name === updatedSkill.name ? updatedSkill : s))
    : [...storeState.skills, updatedSkill];

  storeState = {
    ...storeState,
    skills: nextSkills,
  };
  emitChange();

  await syncGlobalResources(call, true);
}

export async function deleteGlobalSkill(skillId: string, call: EngineCall): Promise<void> {
  await deleteResource(call, skillId);
  storeState = {
    ...storeState,
    skills: storeState.skills.filter((s) => s.id !== skillId),
  };
  emitChange();
  await syncGlobalResources(call, true);
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
