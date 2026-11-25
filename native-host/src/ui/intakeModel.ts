export type IntakeResult = "created" | "commented" | "asked" | "ignored" | "pending";
export type IntakeVia = "desktop" | "scheduled" | "manual" | "direct";

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

export interface DirectIntakeSource {
  kind: string;
  postedAt?: number;
}

export interface IntakeEntry {
  id: string;
  source?: DirectIntakeSource;
  destination?: { conversation: string; message: string; thread?: string };
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

export type IntakeSource =
  Pick<IntakeEntry, "id" | "source" | "destination" | "url" | "messageTs" | "reporter" | "excerpt" | "taskOrigin" | "reactionAdded" | "reactions">
  & Partial<Pick<IntakeEntry, "manager" | "receivedAt">>;

export type IntakeAttentionReason = "question_limit" | "no_answer";
export const attentionLabel: Record<IntakeAttentionReason, string> = {
  question_limit: "질문 한도", no_answer: "답 없음",
};
interface IntakeAttention { reason: IntakeAttentionReason; at: number }

export interface IntakeRow {
  entry: IntakeEntry;
  current: IntakeProcessing;
  attention?: IntakeAttention;
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
  attention?: (IntakeAttention & { id: string; revision: string; url: string; excerpt?: string })[];
}

export const resultLabel: Record<IntakeResult, string> = {
  created: "새 태스크", commented: "재제보 댓글", asked: "되물음 대기", ignored: "무시", pending: "다시 처리 대기",
};
export const viaLabel: Record<IntakeVia, string> = { desktop: "데스크톱 대화", scheduled: "예약 확인", manual: "사람이 처리", direct: "직접 접수" };

export function intakeSourceLabel(entry: Pick<IntakeEntry, "source">) {
  if (!entry.source) return "Slack";
  if (entry.source.kind === "telegram") return "텔레그램";
  if (entry.source.kind === "desktop") return "데스크톱";
  return entry.source.kind;
}

export function intakePostedAt(entry: Pick<IntakeEntry, "source" | "messageTs">) {
  return entry.source ? entry.source.postedAt : slackTime(entry.messageTs);
}

export function intakeOriginalUrl(entry: Pick<IntakeEntry, "source" | "url">) {
  if (!entry.source) return slackLink(entry.url);
  try {
    const address = new URL(entry.url);
    return address.protocol === "https:" && !address.username && !address.password ? address.href : null;
  } catch {
    return null;
  }
}

export function intakeResultLabel(entry: IntakeEntry, event: IntakeProcessing) {
  return event.result === "pending" && entry.source && event.revision === entry.history[0]?.revision
    ? "접수됨" : resultLabel[event.result];
}
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

const slackPermalink = /^https:\/\/[a-z0-9][a-z0-9-]*\.slack\.com\/archives\/[CGD][A-Z0-9]{2,}\/p(\d{10})(\d{6})(?:\?[^#\s]+)?$/i;

export function slackLink(url: string) {
  return slackPermalink.test(url) ? url : null;
}

export function slackMessageTime(url: string) {
  const parts = slackPermalink.exec(url);
  return parts ? slackTime(`${parts[1]}.${parts[2]}`) : undefined;
}
