export type IntakeResult = "created" | "commented" | "asked" | "ignored" | "pending";
export type IntakeVia = "desktop" | "scheduled" | "manual";

export interface IntakeSchedule {
  days: number[];
  startMinute: number;
  endMinute: number;
}

export type ChannelVisibility = "public" | "private" | "unknown";

export interface IntakeChannel {
  id: string;
  label: string;
  visibility?: ChannelVisibility;
  emoji: string;
  markProcessed: boolean;
  paused: boolean;
  intervalMinutes: number;
  enabledAt: number;
  cursor: string;
  revision: string;
  lastCheckAt: number | null;
  lastCheckVia: IntakeVia | null;
  lastScheduledAt: number | null;
  schedule?: IntakeSchedule | null;
  nextCheckAt?: number | null;
  stalledMinutes?: number | null;
}

export interface IntakeProcessing {
  revision: string;
  result: IntakeResult;
  at: number;
  checkedAt?: number;
  searchedAt?: number;
  via: IntakeVia;
  decision: string;
  question: string;
}

export interface SlackReaction {
  name: string;
  count: number;
}

export interface ReportReaction {
  name: string;
  by?: string;
  at?: number;
}

export interface IntakeEntry {
  id: string;
  channel: string;
  messageTs: string;
  url: string;
  reporter: string;
  excerpt: string;
  manager: string;
  receivedAt: number;
  taskKey: string | null;
  taskOrigin: "created" | "commented" | null;
  reactionAdded: boolean;
  reactions?: SlackReaction[];
  reportReaction?: ReportReaction;
  history: IntakeProcessing[];
}

export type IntakeSource = Pick<IntakeEntry, "id" | "url" | "messageTs" | "reporter" | "excerpt" | "taskOrigin" | "reactionAdded">;

export interface IntakeRow {
  entry: IntakeEntry;
  current: IntakeProcessing;
  taskRepo?: string | null;
  repeatCount?: number | null;
}

export interface IntakeData {
  channels: IntakeChannel[];
  rows: IntakeRow[];
  rowsRevision?: string;
  counts: [number, number, number, number, number];
  waiting: number;
  stalledMinutes: number | null;
  asked: { id: string; at: number; revision: string; url: string }[];
}

export const resultLabel: Record<IntakeResult, string> = {
  created: "새 태스크", commented: "재제보 댓글", asked: "되물음 대기", ignored: "무시", pending: "다시 처리 대기",
};
export const viaLabel: Record<IntakeVia, string> = { desktop: "데스크톱 대화", scheduled: "예약 확인", manual: "사람이 처리" };
const slackEmojis = new Map([
  ["ticket", "🎫"], ["beetle", "🐞"], ["eyes", "👀"], ["white_check_mark", "✅"],
]);
export const slackEmoji = (name: string) => slackEmojis.get(name) ?? `:${name}:`;
export const channelLabel = (channel: IntakeChannel) => channel.label || channel.id;
export const channelId = /^[CGD][A-Z0-9]{2,79}$/;

export function slackTime(ts: string) {
  return Number(ts) * 1000;
}

export function slackTimestamp(milliseconds: number) {
  return `${Math.floor(milliseconds / 1000)}.${String((milliseconds % 1000) * 1000).padStart(6, "0")}`;
}

export function slackLink(url: string) {
  return /^https:\/\/[a-z0-9][a-z0-9-]*\.slack\.com\/archives\/[CGD][A-Z0-9]{2,}\/p\d{16}(?:\?[^#\s]+)?$/i.test(url) ? url : null;
}
