import { invoke, isTauri } from '@tauri-apps/api/core';

/** 호스트에 한 번만 묻는다. 못 받으면 빈 집합이라서 모든 명령이 명령줄 길로 간다. */
let rustEngineCommands: Promise<ReadonlySet<string>> | undefined;
export const engineCommands = (): Promise<ReadonlySet<string>> => {
  if (!isTauri()) return Promise.resolve(new Set<string>());
  rustEngineCommands ??= invoke<string[]>('engine_commands').then(list => new Set(list), () => new Set<string>());
  return rustEngineCommands;
};
