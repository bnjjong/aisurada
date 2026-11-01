import { createRoot } from 'react-dom/client';
import { App } from './App';
import './styles.css';

const root = document.getElementById('root');
if (!root) throw new Error('화면을 표시할 자리가 없어요.');
createRoot(root).render(<App />);
