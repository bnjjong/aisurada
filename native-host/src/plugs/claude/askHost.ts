// The real host behind `AskHost`: the Tauri commands the drag question uses.
import { claudeSelfWrites } from '../../data/selfWrites.ts';
import { invoke, isTauri } from '@tauri-apps/api/core';
import { engineReply } from '../../workspace/engine.ts';
import { engineCommands } from '../../workspace/engineCommands.ts';
import type { AskClaudeArgs, AskAnswer, AskHost, EngineReply } from './askRun.ts';

/**
 * 3.8: the bridge never throws. A refused command (`Err(text)`) becomes `{ok: false, error: text}`; an empty text is
 * 「묻기 명령이 까닭 없이 실패했어요.」. Outside the app nothing is called. A throwing bridge once left the in-flight
 * record in place and a sound question became an abandoned one at the next start.
 */
export async function askClaudeBridge(args: AskClaudeArgs | Record<string, unknown>): Promise<AskAnswer> {
  if (!isTauri()) return { ok: false, answer: '', error: '이 화면에서는 AI에게 물을 수 없어요(앱에서만 돼요).', elapsed_ms: 0 };
  try {
    return await invoke<AskAnswer>('ask_claude', { ...args });
  } catch (error) {
    const text = typeof error === 'string' ? error : error instanceof Error ? error.message : '';
    return { ok: false, answer: '', error: text.trim() ? text : '묻기 명령이 까닭 없이 실패했어요.', elapsed_ms: 0 };
  }
}

/** Rust 엔진으로 옮긴 묻기·대화 명령 4개. 호스트가 알리면 Rust 엔진으로 가고, 아니면 예전처럼 Java 엔진으로 간다. */
const ROUTED = new Set(['get-ask', 'append-ask-turn', 'list-chats', 'append-chat-turn']);
const enginePort = { invoke: (command: string, args?: Record<string, unknown>) => invoke<any>(command, args), engineCommands };

export const tauriAskHost: AskHost = {
  native: isTauri(),
  engine: (argv) => (ROUTED.has(argv[0]) ? engineReply(enginePort, argv) : invoke<EngineReply>('sidecar_call', { argv })),
  askClaude: askClaudeBridge,
  askRuntime: () => invoke<string>('ask_runtime'),
  // 2.8: the same door as a body picture: the bytes are the IPC body, the folder and name ride in headers.
  writeImage: async (noteDir, name, bytes) => {
    await invoke('write_attachment_bytes', bytes, {
      headers: { 'x-ais-note-dir': encodeURIComponent(noteDir), 'x-ais-name': encodeURIComponent(name) },
    });
  },
  announceSelfWrite: (vault, relPath) => {
    claudeSelfWrites.record(vault, relPath);
    void invoke('announce_self_write', { root: vault, relPath }).catch(() => {});
  },
};
