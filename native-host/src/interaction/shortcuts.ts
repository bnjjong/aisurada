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

export function shortcutFor(event: KeyInput, mac: boolean, native: boolean): Shortcut | null {
  if (event.defaultPrevented || event.altKey || !(mac ? event.metaKey : event.ctrlKey)) return null;
  if (event.key === 'k' || event.key === 'K') return { kind: 'search' };
  if (event.shiftKey && (event.key === 't' || event.key === 'T' || event.code === 'KeyT')) return { kind: 'terminal' };
  if (!native && !event.shiftKey && event.key.toLowerCase() === 'w') return { kind: 'close' };
  if (event.shiftKey && event.code === 'BracketLeft') return { kind: 'cycle', direction: -1 };
  if (event.shiftKey && event.code === 'BracketRight') return { kind: 'cycle', direction: 1 };
  const digit = /^(?:Digit|Numpad)([1-9])$/.exec(event.code);
  if (!event.shiftKey && digit) return { kind: 'index', index: Number(digit[1]) === 9 ? -1 : Number(digit[1]) - 1 };
  return null;
}
