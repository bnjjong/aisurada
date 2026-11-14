export interface KeyInput {
  key: string;
  code: string;
  metaKey: boolean;
  ctrlKey: boolean;
  shiftKey: boolean;
  altKey: boolean;
  defaultPrevented: boolean;
  preventDefault(): void;
}
export type Shortcut =
  | { kind: 'search' }
  | { kind: 'close' }
  | { kind: 'terminal' }
  | { kind: 'cycle'; direction: -1 | 1 }
  | { kind: 'index'; index: number };

/**
 * 화면 배율 단축키: Cmd/Ctrl과 +(1: 키움), -(-1: 줄임), 0(0: 기본 배율). 아니면 null.
 * 자판 배열이 달라도 되도록 키 이름과 물리 키 이름(code)을 모두 본다. +는 Shift와 함께 눌러도 된다.
 */
export function zoomShortcutFor(event: KeyInput): 1 | -1 | 0 | null {
  if (event.defaultPrevented || event.altKey || !(event.metaKey || event.ctrlKey)) return null;
  if (event.key === '=' || event.key === '+' || event.code === 'Equal' || event.code === 'NumpadAdd') return 1;
  if (event.key === '-' || event.key === '_' || event.code === 'Minus' || event.code === 'NumpadSubtract') return -1;
  if (!event.shiftKey && (event.key === '0' || event.code === 'Digit0' || event.code === 'Numpad0')) return 0;
  return null;
}

export function shortcutFor(event: KeyInput, _mac: boolean, native: boolean): Shortcut | null {
  // 4.3: the modifier is Cmd or Ctrl, whatever the operating system. Ctrl+K and Ctrl+2 work on macOS too.
  if (event.defaultPrevented || event.altKey || !(event.metaKey || event.ctrlKey)) return null;
  if (event.key === 'k' || event.key === 'K') return { kind: 'search' };
  if (!native && event.shiftKey && (event.key === 't' || event.key === 'T' || event.code === 'KeyT')) return { kind: 'terminal' };
  if (!native && !event.shiftKey && event.key.toLowerCase() === 'w') return { kind: 'close' };
  if (event.shiftKey && event.code === 'BracketLeft') return { kind: 'cycle', direction: -1 };
  if (event.shiftKey && event.code === 'BracketRight') return { kind: 'cycle', direction: 1 };
  const digit = /^(?:Digit|Numpad)([1-9])$/.exec(event.code);
  if (!event.shiftKey && digit) return { kind: 'index', index: Number(digit[1]) === 9 ? -1 : Number(digit[1]) - 1 };
  return null;
}
