import { invoke, isTauri } from '@tauri-apps/api/core';
import { openUrl } from '@tauri-apps/plugin-opener';
import { confirm, message } from '@tauri-apps/plugin-dialog';
import type { EditorHost } from './context.ts';

export function editorHost(): EditorHost {
  return {
    native: isTauri(),
    writeImage: (directory, name, bytes) => invoke('write_attachment_bytes', bytes, {
      headers: { 'x-ais-note-dir': encodeURIComponent(directory), 'x-ais-name': encodeURIComponent(name) },
    }),
    openFile: path => invoke('open_in_default_app', { path }),
    openURL: url => openUrl(url),
    confirm: text => isTauri() ? confirm(text, { title: '첨부 열기', okLabel: '열기', cancelLabel: '취소' }) : Promise.resolve(window.confirm(text)),
    warn: async text => { if (isTauri()) await message(text, { title: 'aisurada', kind: 'warning' }); else window.alert(text); },
  };
}
