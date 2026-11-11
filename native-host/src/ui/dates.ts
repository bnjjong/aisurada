/**
 * 날짜 및 시간 표기를 위한 공용 포맷 유틸리티
 */

function toDate(input: string | number | Date | null | undefined): Date | null {
  if (!input) return null;
  if (input instanceof Date) return isNaN(input.getTime()) ? null : input;
  const d = new Date(input);
  return isNaN(d.getTime()) ? null : d;
}

const pad = (n: number) => String(n).padStart(2, "0");

/**
 * 로컬 날짜(YYYY-MM-DD) 형식으로 반환합니다.
 */
export function formatDate(input: string | number | Date | null | undefined): string {
  const d = toDate(input);
  if (!d) return typeof input === "string" ? input : "";
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

/**
 * 로컬 날짜 및 시간(YYYY-MM-DD HH:mm) 형식으로 반환합니다.
 */
export function formatDateTime(input: string | number | Date | null | undefined): string {
  const d = toDate(input);
  if (!d) return typeof input === "string" ? input : "";
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
}

/**
 * 상대 시각(방금 전, N분 전, N시간 전, N일 전 등)으로 변환합니다.
 */
export function formatRelativeTime(input: string | number | Date | null | undefined): string {
  if (!input) return "";
  if (typeof input === "string" && input === "방금 전") return input;

  const d = toDate(input);
  if (!d) return typeof input === "string" ? input : "";

  const now = Date.now();
  const diffMs = now - d.getTime();

  // 미래 시간이거나 1분 미만
  if (diffMs < 60 * 1000) {
    return "방금 전";
  }

  const diffMinutes = Math.floor(diffMs / (60 * 1000));
  if (diffMinutes < 60) {
    return `${diffMinutes}분 전`;
  }

  const diffHours = Math.floor(diffMs / (60 * 60 * 1000));
  if (diffHours < 24) {
    return `${diffHours}시간 전`;
  }

  const diffDays = Math.floor(diffMs / (24 * 60 * 60 * 1000));
  if (diffDays < 7) {
    return `${diffDays}일 전`;
  }

  return formatDate(d);
}

/**
 * 한국어 받침 유무에 따른 올바른 조사를 반환합니다.
 * @param word 앞 단어
 * @param josa "와/과" | "을/를" | "이/가" | "은/는"
 */
export function withJosa(word: string, josa: "와/과" | "을/를" | "이/가" | "은/는"): string {
  if (!word) return "";
  const lastChar = word[word.length - 1];
  const code = lastChar.charCodeAt(0);

  // 한글 유니코드 음절 범위 (가: 0xAC00 ~ 힣: 0xD7A3)
  const isHangul = code >= 0xac00 && code <= 0xd7a3;
  const hasJongseong = isHangul ? (code - 0xac00) % 28 > 0 : false;

  switch (josa) {
    case "와/과":
      return `${word}${hasJongseong ? "과" : "와"}`;
    case "을/를":
      return `${word}${hasJongseong ? "을" : "를"}`;
    case "이/가":
      return `${word}${hasJongseong ? "이" : "가"}`;
    case "은/는":
      return `${word}${hasJongseong ? "은" : "는"}`;
  }
}
