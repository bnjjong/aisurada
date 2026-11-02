import { createRoot } from 'react-dom/client';
import { App } from './App';
import './styles.css';

// Suppress benign WebKit ResizeObserver loop notifications from registering as unhandled errors
window.addEventListener('error', (event) => {
  if (
    event.message?.includes('ResizeObserver loop completed with undelivered notifications') ||
    event.message?.includes('ResizeObserver loop limit exceeded')
  ) {
    event.stopImmediatePropagation();
    event.preventDefault();
  }
});

const root = document.getElementById('root');
if (!root) throw new Error('화면을 표시할 자리가 없어요.');
createRoot(root).render(<App />);
