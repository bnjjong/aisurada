import { createRoot } from 'react-dom/client';
import { App } from './App';
import './styles.css';

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
