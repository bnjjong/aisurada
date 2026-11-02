// The text one markdown inline run shows in the editor (9.5): code spans keep
// their content, backslash escapes and numeric entities resolve, and only
// paired emphasis marks drop. Pairing follows CommonMark delimiter runs, so `_`
// never opens or closes inside a word.

interface Piece { text: string; mark?: string; count?: number; length?: number; open?: boolean; close?: boolean; }

const ASCII_PUNCT = /[!-/:-@[-`{-~]/;
const isSpace = (char: string | undefined) => char === undefined || /\s/u.test(char);
const isPunct = (char: string | undefined) => char !== undefined && /[\p{P}\p{S}]/u.test(char);

function entity(match: string): string | null {
  const hex = /^&#[xX]([0-9a-fA-F]{1,6});$/.exec(match), dec = /^&#(\d{1,7});$/.exec(match);
  const code = hex ? parseInt(hex[1], 16) : dec ? Number(dec[1]) : NaN;
  return code > 0 && code <= 0x10ffff ? String.fromCodePoint(code) : null;
}

function tokenize(source: string): Piece[] {
  const pieces: Piece[] = [];
  const literal = (text: string) => { const last = pieces.at(-1); if (last && last.mark === undefined) last.text += text; else pieces.push({ text }); };
  for (let i = 0; i < source.length;) {
    const char = source[i];
    if (char === '\\' && ASCII_PUNCT.test(source[i + 1] || '')) { literal(source[i + 1]); i += 2; continue; }
    if (char === '`') {
      const run = /^`+/.exec(source.slice(i))![0], end = source.indexOf(run, i + run.length);
      let close = end;
      while (close !== -1 && source[close + run.length] === '`') close = source.indexOf(run, close + run.length + 1);
      if (close === -1) { literal(run); i += run.length; continue; }
      let code = source.slice(i + run.length, close);
      if (code.length > 1 && code.startsWith(' ') && code.endsWith(' ') && code.trim()) code = code.slice(1, -1);
      literal(code); i = close + run.length; continue;
    }
    if (char === '&') {
      const match = /^&#[xX]?[0-9a-fA-F]+;/.exec(source.slice(i));
      const value = match && entity(match[0]);
      if (value) { literal(value); i += match![0].length; continue; }
    }
    if (char === '[' || char === '!' && source[i + 1] === '[') {
      const match = /^!?\[([^\]]*)\]\(([^)\s]*)\)/.exec(source.slice(i));
      if (match) { literal(plainInline(match[1])); i += match[0].length; continue; }
    }
    if (char === '*' || char === '_' || char === '~' || char === '=') {
      const run = new RegExp(`^\\${char}+`).exec(source.slice(i))![0];
      const before = source[i - 1], after = source[i + run.length];
      const left = !isSpace(after) && (!isPunct(after) || isSpace(before) || isPunct(before));
      const right = !isSpace(before) && (!isPunct(before) || isSpace(after) || isPunct(after));
      const open = char === '_' ? left && (!right || isPunct(before)) : left;
      const close = char === '_' ? right && (!left || isPunct(after)) : right;
      pieces.push({ text: '', mark: char, count: run.length, length: run.length, open, close });
      i += run.length; continue;
    }
    literal(char); i++;
  }
  return pieces;
}

export function plainInline(source: string): string {
  const pieces = tokenize(source);
  for (let closer = 0; closer < pieces.length; closer++) {
    const end = pieces[closer];
    while (end.mark && end.close && end.count! > 0) {
      const pair = end.mark === '~' || end.mark === '=';
      // CommonMark's rule of three: a run that can both open and close does not
      // pair when the two run lengths add up to a multiple of three.
      const three = (start: Piece) => (start.open && start.close || end.open && end.close)
        && (start.length! + end.length!) % 3 === 0 && !(start.length! % 3 === 0 && end.length! % 3 === 0);
      let opener = closer - 1;
      while (opener >= 0 && !(pieces[opener].mark === end.mark && pieces[opener].open && pieces[opener].count! > 0
        && (pair ? pieces[opener].count === 2 && end.count === 2 : !three(pieces[opener])))) opener--;
      if (opener < 0) break;
      const start = pieces[opener], used = pair ? 2 : Math.min(start.count!, end.count!) >= 2 ? 2 : 1;
      start.count! -= used; end.count! -= used;
      // Marks left between a matched pair can no longer pair across it.
      for (let n = opener + 1; n < closer; n++) if (pieces[n].mark) { pieces[n].open = false; pieces[n].close = false; }
    }
  }
  return pieces.map(piece => piece.mark ? piece.mark.repeat(piece.count!) : piece.text).join('');
}
