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
    openPath: path => isTauri() ? invoke('open_external_path', { path }) : Promise.resolve(),
    openURL: url => openUrl(url),
    // 창 제목과 단추 글자는 묻는 쪽이 넘긴다. 예전에는 「첨부 열기」「열기」가 기본값이라, 다른 확인(편집 버리기)에도 그대로 보였다.
    confirm: (text, options) => isTauri() ? confirm(text, { title: '확인', okLabel: '확인', cancelLabel: '취소', ...options }) : Promise.resolve(window.confirm(text)),
    warn: async text => { if (isTauri()) await message(text, { title: 'aisurada', kind: 'warning' }); else window.alert(text); },
  };
}
