import { useId } from "react";
import type { Note } from "../workspace/types.ts";
import { asText } from "../workspace/types.ts";

/**
 * 노트의 sessionAgent 줄에서 읽은 CLI 이름. 줄이 없거나 claude면 null이다.
 * Claude는 줄을 적지 않는 약속이라서(엔진의 SessionAgents), 읽는 쪽이 null을 Claude로 본다.
 */
export function sessionAgentId(value: unknown): string | null {
  const agent = asText(value).trim();
  return agent && agent !== "claude" ? agent : null;
}

/** 칩과 아이콘 설명에 쓰는 CLI 이름. 모르는 CLI는 줄에 적힌 이름 그대로 보인다. */
export function sessionAgentLabel(agent: string): string {
  if (agent === "claude") return "Claude Code";
  if (agent === "codex") return "Codex";
  if (agent === "gemini") return "Gemini CLI";
  return agent;
}

/** 노트에 흔적을 남긴 CLI와 그 역할. 만든 쪽이 먼저 오고, 같은 CLI가 만들고 고쳤으면 하나로 합친다. */
export interface NoteAgent {
  agent: string;
  role: "created" | "edited" | "both";
}

/**
 * 노트의 세션 기록으로 Claude, Gemini 같은 CLI가 이 노트에 관여했는지 가려냅니다.
 * 세션 id가 없으면 사람이 쓴 노트라서 빈 목록입니다. 목록 화면이 출처 아이콘을 그릴 때 씁니다.
 */
export function noteAgents(note: Note): NoteAgent[] {
  const createdSession = asText(note.sessionId).trim();
  const editedSession = asText(note.editedSessionId).trim();
  const created = createdSession ? (sessionAgentId(note.sessionAgent) ?? "claude") : null;
  // 같은 세션이면 만든 쪽 기록만 본다. NoteSessionChips와 같은 규칙이다.
  const edited =
    editedSession && editedSession !== createdSession
      ? (sessionAgentId(note.editedSessionAgent) ?? "claude")
      : null;

  if (created && edited && created === edited) return [{ agent: created, role: "both" }];
  if (created && edited) {
    return [
      { agent: created, role: "created" },
      { agent: edited, role: "edited" },
    ];
  }
  if (created) return [{ agent: created, role: "created" }];
  if (edited) return [{ agent: edited, role: "edited" }];
  return [];
}

const ROLE_TEXT: Record<NoteAgent["role"], string> = {
  created: "가 작성",
  edited: "가 편집",
  both: "가 작성·편집",
};

/** 마우스를 올렸을 때 보이는 설명이자 스크린 리더가 읽는 이름. */
export function noteAgentTitle({ agent, role }: NoteAgent): string {
  return `${sessionAgentLabel(agent)}${ROLE_TEXT[role]}`;
}

/** Claude의 불꽃 모양. 길고 짧은 살을 번갈아 그려 손으로 그린 듯한 느낌만 낸다. */
const CLAUDE_RAYS = Array.from({ length: 8 }, (_, i) => {
  const angle = (i * Math.PI) / 4;
  const inner = 2.4;
  const outer = i % 2 === 0 ? 7 : 5.6;
  const point = (r: number) =>
    `${(8 + Math.cos(angle) * r).toFixed(2)} ${(8 + Math.sin(angle) * r).toFixed(2)}`;
  return `M${point(inner)}L${point(outer)}`;
}).join("");

function AgentGlyph({ agent }: { agent: string }) {
  // useId는 :r1: 꼴이라 url(#...)에 쓰기 좋게 글자와 숫자만 남긴다.
  const gradient = `agent-mark-${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  if (agent === "claude") {
    return (
      <svg viewBox="0 0 16 16" width="12" height="12" fill="none" aria-hidden="true">
        <path d={CLAUDE_RAYS} stroke="#d97757" strokeWidth="1.9" strokeLinecap="round" />
      </svg>
    );
  }
  if (agent === "gemini") {
    return (
      <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
        <defs>
          <linearGradient id={gradient} x1="2" y1="14" x2="14" y2="2" gradientUnits="userSpaceOnUse">
            <stop offset="0" stopColor="#4285f4" />
            <stop offset=".55" stopColor="#9b72cb" />
            <stop offset="1" stopColor="#d96570" />
          </linearGradient>
        </defs>
        <path
          d="M8 1C8.5 5 11 7.5 15 8C11 8.5 8.5 11 8 15C7.5 11 5 8.5 1 8C5 7.5 7.5 5 8 1Z"
          fill={`url(#${gradient})`}
        />
      </svg>
    );
  }
  // Codex와 모르는 CLI는 상표 대신 터미널 프롬프트 모양을 글자색으로 그린다.
  return (
    <svg
      viewBox="0 0 16 16"
      width="12"
      height="12"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.5"
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
    >
      <rect x="1.8" y="3" width="12.4" height="10" rx="2" />
      <path d="M5 6.8l2 1.7-2 1.7M8.8 10.4h2.4" />
    </svg>
  );
}

/** 목록 줄에 붙이는 작은 출처 아이콘. Claude와 Gemini는 고유 모양과 색이고, 그 밖의 CLI는 터미널 모양이다. */
export function AgentMark({ info }: { info: NoteAgent }) {
  const title = noteAgentTitle(info);
  const known = info.agent === "claude" || info.agent === "gemini";
  return (
    <span
      className="agent-mark"
      data-agent={known ? info.agent : "other"}
      data-agent-role={info.role}
      role="img"
      aria-label={title}
      title={title}
    >
      <AgentGlyph agent={info.agent} />
    </span>
  );
}
