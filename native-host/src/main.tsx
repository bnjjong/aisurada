import { createRoot } from 'react-dom/client';
import { App } from './App';
import './styles.css';
import './tasks.css';
import './runner.css';
import './notifications.css';
import './editor-insert.css';
import './skin-tactile.css';
import './skin-grid.css';
import './skin-glass.css';
import './skin-modern.css';
import './rail-night.css';
import { readSkin } from './workspace/storage.ts';

// 첫 화면부터 고른 모양으로 그린다. 저장소를 못 읽으면 기본 모양을 쓴다.
try { document.documentElement.dataset.skin = readSkin(window.localStorage); } catch { document.documentElement.dataset.skin = 'tactile'; }

// Suppress benign WebKit ResizeObserver loop notifications from registering as unhandled errors
const isResizeObserverError = (msg: unknown) =>
  typeof msg === 'string' &&
  (msg.includes('ResizeObserver loop completed with undelivered notifications') ||
    msg.includes('ResizeObserver loop limit exceeded'));

window.addEventListener(
  'error',
  (event) => {
    if (isResizeObserverError(event.message)) {
      event.stopImmediatePropagation();
      event.preventDefault();
    }
  },
  true,
);

const prevOnError = window.onerror;
window.onerror = function (msg, ...args) {
  if (isResizeObserverError(msg)) return true;
  if (prevOnError) return prevOnError.apply(this, [msg, ...args] as any);
  return false;
};

const root = document.getElementById('root');
if (!root) throw new Error('화면을 표시할 자리가 없어요.');
createRoot(root).render(<App />);
