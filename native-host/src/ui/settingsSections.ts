// 설정 화면의 6개 묶음 정의와 색인 및 검색
import type { SettingsSection } from "../workspace/types.ts";
import type { IconName } from "./Icon.tsx";

export interface SectionMeta {
  id: SettingsSection;
  name: string;
  icon: IconName;
  desc: string;
}

export const SETTINGS_SECTIONS: readonly SectionMeta[] = [
  { id: "look", name: "모양", icon: "palette", desc: "앱이 어떻게 보이는지 정해요. 밝게·어둡게는 레일의 ◐ 단추가 그대로 맡아요." },
  { id: "fonts", name: "글꼴", icon: "type", desc: "앱과 터미널에 쓸 글꼴을 골라요. 고르면 바로 바뀌어요." },
  { id: "notes", name: "노트", icon: "note", desc: "노트 목록과 본문에 무엇을 보일지 정해요." },
  { id: "notifications", name: "알림", icon: "bell", desc: "어디로, 어떤 소식을 알릴지 정해요. 꺼도 알림 센터(종)에는 쌓여요." },
  { id: "ai", name: "AI", icon: "bot", desc: "Claude가 노트를 다루는 범위와 기다리는 시간을 정해요." },
  { id: "connections", name: "연결", icon: "chain", desc: "앱이 바깥 서비스에 닿는 연결이에요. 키는 이 Mac의 키체인에만 둬요." },
];

export interface SearchIndexItem {
  section: SettingsSection;
  key: string;
  label: string;
  keywords: string[];
}

export const SETTINGS_INDEX: readonly SearchIndexItem[] = [
  {
    section: "look",
    key: "skin",
    label: "화면 모양",
    keywords: ["스킨", "모양", "테마", "다크모드", "물성", "격자", "유리", "모던", "클래식"],
  },
  {
    section: "look",
    key: "scale",
    label: "화면 배율",
    keywords: ["배율", "크기", "줌", "확대", "축소", "글자 크기", "단축키"],
  },
  {
    section: "fonts",
    key: "ui-font",
    label: "앱 글꼴",
    keywords: ["글꼴", "폰트", "앱 폰트", "pretendard", "본문 글꼴"],
  },
  {
    section: "fonts",
    key: "terminal-font",
    label: "터미널 글꼴",
    keywords: ["터미널", "고정폭", "monospace", "d2coding", "콘솔"],
  },
  {
    section: "notes",
    key: "titleDatePrefix",
    label: "제목 앞 날짜",
    keywords: ["날짜", "제목 날짜", "접두사", "prefix", "목록"],
  },
  {
    section: "notes",
    key: "showRecordTier",
    label: "기록 노트",
    keywords: ["기록", "레코드", "일지", "아카이브", "검색 포함"],
  },
  {
    section: "notes",
    key: "mermaidShowSource",
    label: "다이어그램",
    keywords: ["다이어그램", "mermaid", "머메이드", "차트", "소스 코드"],
  },
  {
    section: "notifications",
    key: "ntf-inapp",
    label: "앱 안 알림",
    keywords: ["토스트", "배지", "인앱", "팝업", "알림"],
  },
  {
    section: "notifications",
    key: "ntf-macos",
    label: "macOS 알림센터",
    keywords: ["맥", "배너", "알림센터", "시스템 알림", "os"],
  },
  {
    section: "notifications",
    key: "ntf-focus",
    label: "집중 모드",
    keywords: ["집중 모드", "방해 금지", "조용히", "dnd", "무음"],
  },
  {
    section: "notifications",
    key: "ntf-events",
    label: "알릴 소식",
    keywords: ["소식", "이벤트", "러너", "마크다운", "깃허브", "이슈"],
  },
  {
    section: "notifications",
    key: "ntf-test",
    label: "시험 알림",
    keywords: ["테스트", "시험 알림", "알림 보내기"],
  },
  {
    section: "ai",
    key: "claude",
    label: "Claude Code 연결",
    keywords: ["claude", "클로드", "인공지능", "ai", "터미널"],
  },
  {
    section: "ai",
    key: "claude-write",
    label: "파일 쓰기 허용",
    keywords: ["파일 쓰기", "수정 권한", "편집", "write"],
  },
  {
    section: "ai",
    key: "ask-timeout",
    label: "응답 대기 시간",
    keywords: ["타임아웃", "대기 시간", "timeout", "응답 시간"],
  },
  {
    section: "connections",
    key: "github",
    label: "GitHub 연결",
    keywords: ["깃허브", "github", "저장소", "repo", "app id", "토큰"],
  },
  {
    section: "connections",
    key: "archyl",
    label: "Archyl 연결",
    keywords: ["archyl", "아킬", "엔진", "동기화", "원격"],
  },
];

/**
 * 주어진 검색어로 설정 인덱스를 검색한다.
 * claudeEnabled가 false이면 ai 섹션 항목은 결과에서 제외한다.
 * 최대 8개 결과를 반환한다.
 */
export function searchSettings(
  query: string,
  options: { claude: boolean },
): SearchIndexItem[] {
  const q = query.trim().toLowerCase();
  if (!q) return [];

  const sectionNameMap = new Map<SettingsSection, string>(
    SETTINGS_SECTIONS.map((s) => [s.id, s.name.toLowerCase()]),
  );

  return SETTINGS_INDEX.filter((item) => {
    if (!options.claude && item.section === "ai") return false;

    const labelMatch = item.label.toLowerCase().includes(q);
    const keywordMatch = item.keywords.some((k) => k.toLowerCase().includes(q));
    const sectionName = sectionNameMap.get(item.section) ?? "";
    const sectionMatch = sectionName.includes(q);

    return labelMatch || keywordMatch || sectionMatch;
  }).slice(0, 8);
}
