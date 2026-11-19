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

/** Claude 로고의 윤곽선(24×24 칸). 길이가 제각각인 살 열두 개가 뻗은 불꽃 모양이다. */
const CLAUDE_BURST =
  "m4.7144 15.9555 4.7174-2.6471.079-.2307-.079-.1275h-.2307l-.7893-.0486-2.6956-.0729-2.3375-.0971-2.2646-.1214-.5707-.1215-.5343-.7042.0546-.3522.4797-.3218.686.0608 1.5179.1032 2.2767.1578 1.6514.0972 2.4468.255h.3886l.0546-.1579-.1336-.0971-.1032-.0972L6.973 9.8356l-2.55-1.6879-1.3356-.9714-.7225-.4918-.3643-.4614-.1578-1.0078.6557-.7225.8803.0607.2246.0607.8925.686 1.9064 1.4754 2.4893 1.8336.3643.3035.1457-.1032.0182-.0728-.164-.2733-1.3539-2.4467-1.445-2.4893-.6435-1.032-.17-.6194c-.0607-.255-.1032-.4674-.1032-.7285L6.287.1335 6.6997 0l.9957.1336.419.3642.6192 1.4147 1.0018 2.2282 1.5543 3.0296.4553.8985.2429.8318.091.255h.1579v-.1457l.1275-1.706.2368-2.0947.2307-2.6957.0789-.7589.3764-.9107.7468-.4918.5828.2793.4797.686-.0668.4433-.2853 1.8517-.5586 2.9021-.3643 1.9429h.2125l.2429-.2429.9835-1.3053 1.6514-2.0643.7286-.8196.85-.9046.5464-.4311h1.0321l.759 1.1293-.34 1.1657-1.0625 1.3478-.8804 1.1414-1.2628 1.7-.7893 1.36.0729.1093.1882-.0183 2.8535-.607 1.5421-.2794 1.8396-.3157.8318.3886.091.3946-.3278.8075-1.967.4857-2.3072.4614-3.4364.8136-.0425.0304.0486.0607 1.5482.1457.6618.0364h1.621l3.0175.2247.7892.522.4736.6376-.079.4857-1.2142.6193-1.6393-.3886-3.825-.9107-1.3113-.3279h-.1822v.1093l1.0929 1.0686 2.0035 1.8092 2.5075 2.3314.1275.5768-.3218.4554-.34-.0486-2.2039-1.6575-.85-.7468-1.9246-1.621h-.1275v.17l.4432.6496 2.3436 3.5214.1214 1.0807-.17.3521-.6071.2125-.6679-.1214-1.3721-1.9246L14.38 17.959l-1.1414-1.9428-.1397.079-.674 7.2552-.3156.3703-.7286.2793-.6071-.4614-.3218-.7468.3218-1.4753.3886-1.9246.3157-1.53.2853-1.9004.17-.6314-.0121-.0425-.1397.0182-1.4328 1.9672-2.1796 2.9446-1.7243 1.8456-.4128.164-.7164-.3704.0667-.6618.4008-.5889 2.386-3.0357 1.4389-1.882.929-1.0868-.0062-.1579h-.0546l-6.3385 4.1164-1.1293.1457-.4857-.4554.0608-.7467.2307-.2429 1.9064-1.3114Z";

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
      <svg viewBox="0 0 24 24" width={size} height={size} fill="#d97757" aria-hidden="true">
        <path d={CLAUDE_BURST} />
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
