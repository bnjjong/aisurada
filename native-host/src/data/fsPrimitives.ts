import type { FileEntry, FilePort } from '../workspace/types.ts';

export async function readDirectory(files: FilePort, path: string, warn: (message: string, error?: unknown) => void): Promise<FileEntry[]> {
  try { return await files.exists(path) ? await files.readDir(path) : []; }
  catch (error) { warn(`폴더를 읽지 못했어요: ${path}`, error); return []; }
}

export async function atomicWrite(files: FilePort, path: string, text: string, uuid: () => string) {
  const temporary = `${path}.${uuid()}.tmp`;
  try { await files.writeTextFile(temporary, text); await files.rename(temporary, path); }
  catch (error) { try { await files.remove(temporary); } catch { /* preserve original error */ } throw error; }
}
