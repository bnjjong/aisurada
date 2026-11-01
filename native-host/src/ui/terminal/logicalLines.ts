// Logical lines processing and pinned note markdown generator for Chapter 14.
import type { MaskedResult } from './ptyBridge.ts';

export interface PinnedNoteOptions {
  title: string;
  linesSent: number;
  masked: MaskedResult;
  hadTui: boolean;
}

export function extractLogicalLines(rawLines: string[]): string[] {
  const result: string[] = [];
  for (const line of rawLines) {
    // Strip trailing empty space that was never written to
    const trimmedRight = line.replace(/ +$/, '');
    result.push(trimmedRight);
  }
  // Remove trailing empty lines
  while (result.length > 0 && result[result.length - 1].trim() === '') {
    result.pop();
  }
  return result;
}

export function sliceLogicalLines(lines: string[], rangeMode: string, selectedIndices?: [number, number]): string[] {
  if (rangeMode === 'selection' && selectedIndices) {
    const [start, end] = selectedIndices;
    const clampedStart = Math.max(0, Math.min(start, lines.length - 1));
    const clampedEnd = Math.max(clampedStart, Math.min(end, lines.length - 1));
    return lines.slice(clampedStart, clampedEnd + 1);
  }
  const count = rangeMode === '100' ? 100
    : rangeMode === '2000' ? 2000
    : rangeMode === '10000' ? 10000
    : 500;
  return lines.slice(Math.max(0, lines.length - count));
}

export function calculateBacktickFence(content: string): string {
  const matches = content.match(/`+/g);
  let maxBackticks = 0;
  if (matches) {
    for (const m of matches) {
      if (m.length > maxBackticks) maxBackticks = m.length;
    }
  }
  return '`'.repeat(Math.max(3, maxBackticks + 1));
}

export function formatPinnedNoteBody(opts: PinnedNoteOptions): string {
  const { title, linesSent, masked, hadTui } = opts;
  const shapesSum = masked.shapes.reduce((sum, s) => sum + s.count, 0);

  const callouts: string[] = [];
  if (hadTui) {
    callouts.push(`>[!warning] 굳힐 때 TUI가 떠 있었습니다\n>굳힐 때 TUI가 떠 있었어요. 여기 담기는 것은 그 아래 쌓인 기록이라 보던 화면과 달라요.\n`);
  }
  if (masked.short && masked.short.length > 0) {
    const names = masked.short.map(n => `\`${n}\``).join(', ');
    callouts.push(`>[!warning] 짧아서 못 지킨 이름 ${masked.short.length}개\n>${names}\n>길이가 8자보다 짧아서 오탐을 막으려고 값 일치 검사에서 뺐어요.\n`);
  }
  if (masked.unreadable && masked.unreadable.length > 0) {
    const names = masked.unreadable.map(n => `\`${n}\``).join(', ');
    callouts.push(`>[!danger] 값을 못 읽은 이름 ${masked.unreadable.length}개\n>${names}\n>Keychain에서 값을 읽어 오지 못해 마스킹하지 못했어요.\n`);
  }
  if (shapesSum > 0) {
    const details = masked.shapes.filter(s => s.count > 0).map(s => `${s.label} ${s.count}건`).join(', ');
    callouts.push(`>[!danger] 표에 없는데 비밀처럼 생긴 것 ${shapesSum}건\n>${details}\n>Keychain 표에는 없지만 알려진 비밀 패턴과 일치하여 마스킹되지 않았어요.\n`);
  } else {
    callouts.push(`>[!success] 모양 검사에 걸린 것이 0건입니다\n>모양 검사를 지났고 걸린 것이 0건이에요. 채택한 패턴 밖의 모양은 보지 못해요.\n`);
  }

  const fence = calculateBacktickFence(masked.text);
  const cleanTitle = title.trim();

  let body = `# ${cleanTitle}\n\n`;
  body += `터미널 스크롤백을 굳힌 노트입니다. 값 일치 마스킹을 지났습니다.\n\n`;
  body += `| 무엇 | 값 |\n| --- | --- |\n`;
  body += `| 굳힌 논리 줄 | ${linesSent} |\n`;
  body += `| 가린 자리 | ${masked.hits} |\n`;
  body += `| 가린 값 종수 | ${masked.maskedValues} |\n`;
  body += `| 표에 있던 값 | ${masked.tableSize}종 |\n`;
  body += `| 모양으로 걸린 것 | ${shapesSum}건 |\n\n`;

  if (callouts.length > 0) {
    body += callouts.join('\n') + '\n';
  }

  body += `${fence}text\n${masked.text}\n${fence}\n`;
  return body;
}
