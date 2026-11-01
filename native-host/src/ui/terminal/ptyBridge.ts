// Host IPC bridge for Chapter 14 Terminal and Secret Masking.
import { invoke } from '@tauri-apps/api/core';
import { Channel } from '@tauri-apps/api/core';

export interface SecretTableStatus {
  ready: boolean;
  reason: string | null;
  asked: number;
  read: number;
  maskable: number;
  short: string[];
  unreadable: string[];
}

export interface MaskShape {
  kind: string;
  label: string;
  count: number;
}

export interface MaskedResult {
  text: string;
  hits: number;
  maskedValues: number;
  tableSize: number;
  short: string[];
  unreadable: string[];
  shapes: MaskShape[];
}

export interface StartShellResult {
  fellBack: boolean;
}

export async function ptyStart(
  key: string,
  cols: number,
  rows: number,
  onData: (chunk: string) => void
): Promise<StartShellResult> {
  const channel = new Channel<string>();
  channel.onmessage = onData;
  return invoke<StartShellResult>('pty_start', {
    key,
    cols,
    rows,
    onData: channel,
  });
}

export async function ptyWrite(key: string, data: string): Promise<void> {
  return invoke<void>('pty_write', { key, data });
}

export async function ptyResize(key: string, cols: number, rows: number): Promise<void> {
  return invoke<void>('pty_resize', { key, cols, rows });
}

export async function ptyDetach(key: string): Promise<void> {
  try {
    await invoke<void>('pty_detach', { key });
  } catch {
    // detach errors are silent
  }
}

export async function secretTableProbe(): Promise<SecretTableStatus | null> {
  try {
    return await invoke<SecretTableStatus>('secret_table_probe');
  } catch {
    return null;
  }
}

export async function secretTableStatus(): Promise<SecretTableStatus | null> {
  try {
    return await invoke<SecretTableStatus>('secret_table_status');
  } catch {
    return null;
  }
}

export async function maskScrollback(lines: string[]): Promise<MaskedResult> {
  return invoke<MaskedResult>('mask_scrollback', { lines });
}
