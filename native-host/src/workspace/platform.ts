import { invoke, isTauri } from '@tauri-apps/api/core';
import { documentDir } from '@tauri-apps/api/path';
import { listen } from '@tauri-apps/api/event';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { getCurrent, onOpenUrl } from '@tauri-apps/plugin-deep-link';
import { confirm, open } from '@tauri-apps/plugin-dialog';
import * as fs from '@tauri-apps/plugin-fs';
import { sanitizeHtml } from '../lib/sanitize.ts';
import type { WorkspaceServices } from './types.ts';

export function workspaceServices(): WorkspaceServices {
  return {
    native: isTauri(), mac: /Mac|iPhone|iPad/.test(navigator.platform),
    windowLabel: isTauri() ? getCurrentWebviewWindow().label : 'main',
    sanitizeHTML: html => sanitizeHtml(html),
    host: {
      invoke: (command, args) => {
        if (!isTauri()) return Promise.reject(new Error('앱에서 워크스페이스를 열어 주세요.'));
        return invoke(command, args);
      },
      documentDir,
      chooseDirectory: () => open({ directory: true, multiple: false }),
      confirmLeave: message => confirm(message, { title: '지금 vault에 남은 글이 있어요', kind: 'warning',
        okLabel: '그래도 바꾸기', cancelLabel: '남겨 두기' }),
    },
    files: fs,
    storage: {
      getItem: key => window.localStorage.getItem(key),
      setItem: (key, value) => window.localStorage.setItem(key, value),
    },
    watch: (root, changed) => fs.watch(root, event => changed(event.paths), { recursive: true, delayMs: 400 }),
    noteWindowsChanged: changed => isTauri()
      ? listen<string[]>('wl://note-windows', event => changed(event.payload))
      : Promise.resolve(() => {}),
    selfWritesChanged: changed => isTauri()
      ? listen<{ root: string; relPath: string; from: string }>('wl://self-write', event => changed(event.payload))
      : Promise.resolve(() => {}),
    menu: received => isTauri()
      ? getCurrentWebviewWindow().listen<string>('wl://menu', event => received(event.payload))
      : Promise.resolve(() => {}),
    navigateRequested: received => isTauri()
      ? getCurrentWebviewWindow().listen<{ path: string; rail?: string | null; sub?: string | null }>('wl://navigate', event => received(event.payload))
      : Promise.resolve(() => {}),
    ...(isTauri() ? { deepLinks: { listen: onOpenUrl, current: getCurrent } } : {}),
    ...(import.meta.hot ? { developmentLinks: (received: (urls: string[]) => void) => {
      const hot = import.meta.hot!;
      const handler = (value: { url: string }) => received([value.url]);
      hot.on('aisurada:deeplink', handler);
      return () => hot.off('aisurada:deeplink', handler);
    } } : {}),
    // 15.8: the window's logical pixels, not rounded. innerWidth and screenX are integers.
    noteGeometryChanged: changed => {
      if (!isTauri()) return Promise.resolve(() => {});
      const window = getCurrentWindow();
      const report = async () => {
        const [position, size, scale] = await Promise.all([window.outerPosition(), window.innerSize(), window.scaleFactor()]);
        changed({ width: size.width / scale, height: size.height / scale, x: position.x / scale, y: position.y / scale });
      };
      return Promise.all([window.onMoved(() => { void report(); }), window.onResized(() => { void report(); })])
        .then(([moved, resized]) => () => { moved(); resized(); });
    },
    closeWindow: () => getCurrentWindow().close(),
    closeRequested: isTauri()
      ? handler => getCurrentWindow().onCloseRequested(event => { event.preventDefault(); handler(); })
      : () => Promise.resolve(() => {}),
    navigate: (path, replace) => { window.history[replace ? 'replaceState' : 'pushState']({}, '', path); },
    pending: { flushReports: async () => {}, flushEditors: async () => {}, waitForWrites: async () => true, unsavedNotes: () => 0 },
    now: () => new Date(), uuid: () => crypto.randomUUID(), sleep: ms => new Promise(resolve => setTimeout(resolve, ms)),
    warn: (message, error) => console.warn(message, error ?? ''),
  };
}
