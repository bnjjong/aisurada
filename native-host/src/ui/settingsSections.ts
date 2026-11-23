// 설정 화면의 6개 묶음 정의와 색인
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
