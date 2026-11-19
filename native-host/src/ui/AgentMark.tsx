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

/** OpenAI 로고의 윤곽선(24×24 칸). Codex 아이콘으로 쓴다. */
const OPENAI_KNOT =
  "M22.2819 9.8211a5.9847 5.9847 0 0 0-.5157-4.9108 6.0462 6.0462 0 0 0-6.5098-2.9A6.0651 6.0651 0 0 0 4.9807 4.1818a5.9847 5.9847 0 0 0-3.9977 2.9 6.0462 6.0462 0 0 0 .7427 7.0966 5.98 5.98 0 0 0 .511 4.9107 6.051 6.051 0 0 0 6.5146 2.9001A5.9847 5.9847 0 0 0 13.2599 24a6.0557 6.0557 0 0 0 5.7718-4.2058 5.9894 5.9894 0 0 0 3.9977-2.9001 6.0557 6.0557 0 0 0-.7475-7.0729zm-9.022 12.6081a4.4755 4.4755 0 0 1-2.8764-1.0408l.1419-.0804 4.7783-2.7582a.7948.7948 0 0 0 .3927-.6813v-6.7369l2.02 1.1686a.071.071 0 0 1 .038.052v5.5826a4.504 4.504 0 0 1-4.4945 4.4944zm-9.6607-4.1254a4.4708 4.4708 0 0 1-.5346-3.0137l.142.0852 4.783 2.7582a.7712.7712 0 0 0 .7806 0l5.8428-3.3685v2.3324a.0804.0804 0 0 1-.0332.0615L9.74 19.9502a4.4992 4.4992 0 0 1-6.1408-1.6464zM2.3408 7.8956a4.485 4.485 0 0 1 2.3655-1.9728V11.6a.7664.7664 0 0 0 .3879.6765l5.8144 3.3543-2.0201 1.1685a.0757.0757 0 0 1-.071 0l-4.8303-2.7865A4.504 4.504 0 0 1 2.3408 7.872zm16.5963 3.8558L13.1038 8.364 15.1192 7.2a.0757.0757 0 0 1 .071 0l4.8303 2.7913a4.4944 4.4944 0 0 1-.6765 8.1042v-5.6772a.79.79 0 0 0-.407-.667zm2.0107-3.0231l-.142-.0852-4.7735-2.7818a.7759.7759 0 0 0-.7854 0L9.409 9.2297V6.8974a.0662.0662 0 0 1 .0284-.0615l4.8303-2.7866a4.4992 4.4992 0 0 1 6.6802 4.66zM8.3065 12.863l-2.02-1.1638a.0804.0804 0 0 1-.038-.0567V6.0742a4.4992 4.4992 0 0 1 7.3757-3.4537l-.142.0805L8.704 5.459a.7948.7948 0 0 0-.3927.6813zm1.0976-2.3654l2.602-1.4998 2.6069 1.4998v2.9994l-2.5974 1.4997-2.6067-1.4997Z";

/**
 * 엔진 로고. 노트 목록의 출처 아이콘(12px)과 엔진 고르는 타일(그보다 큼)이 같은 모양을 쓰도록
 * 크기만 [size]로 받는다. 모양은 viewBox 안에서 그려서 크기와 무관하다.
 */
export function AgentGlyph({ agent, size = 12 }: { agent: string; size?: number }) {
  // useId는 :r1: 꼴이라 url(#...)에 쓰기 좋게 글자와 숫자만 남긴다.
  const gradient = `agent-mark-${useId().replace(/[^a-zA-Z0-9]/g, "")}`;
  if (agent === "claude") {
    return (
      <svg viewBox="0 0 16 16" width={size} height={size} fill="none" aria-hidden="true">
        <path d={CLAUDE_RAYS} stroke="#d97757" strokeWidth="1.9" strokeLinecap="round" />
      </svg>
    );
  }
  if (agent === "gemini") {
    return (
      <svg viewBox="0 0 16 16" width={size} height={size} aria-hidden="true">
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
  if (agent === "codex") {
    // Codex는 OpenAI의 제품이라 OpenAI 로고(여섯 가닥이 엮인 매듭)를 쓴다. 글자색으로 채워 어느 바탕에서도 읽힌다.
    return (
      <svg viewBox="0 0 24 24" width={size} height={size} fill="currentColor" aria-hidden="true">
        <path d={OPENAI_KNOT} />
      </svg>
    );
  }
  // 모르는 CLI는 상표 대신 터미널 프롬프트 모양을 글자색으로 그린다.
  return (
    <svg
      viewBox="0 0 16 16"
      width={size}
      height={size}
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

/** 목록 줄에 붙이는 작은 출처 아이콘. Claude와 Gemini는 고유 모양과 색, Codex는 OpenAI 로고이고, 그 밖의 CLI는 터미널 모양이다. */
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

/** [note]에 흔적을 남긴 CLI마다 [AgentMark]를 나란히 그린다. 세션 기록이 없는 노트는 아무것도 그리지 않는다. */
export function AgentMarks({ note }: { note: Note }) {
  return (
    <>
      {noteAgents(note).map((info) => (
        <AgentMark key={info.role} info={info} />
      ))}
    </>
  );
}
