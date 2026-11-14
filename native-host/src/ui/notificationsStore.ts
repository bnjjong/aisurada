// 알림의 화면 쪽 상태. 알림 센터, 토스트, 설정 화면이 같은 상태를 보도록 한 곳에 둔다.
// 알림을 쌓고 어디로 보낼지 정하는 일(앱 안, macOS 배너)은 모두 호스트(Rust, `notifications.rs`)가 한다.
// 이 모듈은 호스트 명령을 부르고, 호스트가 알려 주는 사건(`ais://notifications`)을 받아 기억하고 토스트로 띄울 뿐이다.
// 집중 모드는 macOS가 처리한다(앱이 상태를 읽지 않는다): 집중 모드에서 aisurada를 허용하지 않으면 OS가 배너를 막는다.
import { useSyncExternalStore } from "react";

export type HostCall = <T>(command: string, args?: Record<string, unknown>) => Promise<T>;

export type NotifyKind = "question" | "pr" | "failed" | "done" | "approval" | "test";
export type EventKind = Exclude<NotifyKind, "test">;

export interface NotifyItem {
  id: string;
  kind: NotifyKind;
  title: string;
  body: string;
  /** 눌렀을 때 열 태스크 키 */
  taskKey: string | null;
  /** 만든 시각(유닉스 밀리초) */
  at: number;
  read: boolean;
}

export interface Channel {
  inApp: boolean;
  macos: boolean;
}

export interface NotifySettings {
  /** 앱 안(토스트, 배지) 채널 */
  inApp: boolean;
  /** macOS 알림센터 채널 */
  macos: boolean;
  events: Record<EventKind, Channel>;
}

export interface Toast {
  id: string;
  item: NotifyItem;
}

/** 호스트가 알려 주는 사건. 목록이 바뀌었거나 새 알림이 왔다. */
export interface ChangedEvent {
  unread: number;
  /** 방금 온 알림. 읽음 표시만 바뀐 사건이면 null */
  pushed: unknown;
  /** 방금 온 알림을 앱 안 토스트로 띄울지(설정이 정한다) */
  toast: boolean;
}

export interface NotifySnapshot {
  items: NotifyItem[];
  unread: number;
  settings: NotifySettings;
  /** 호스트에서 한 번이라도 읽었는지 */
  loaded: boolean;
  /** 마지막 일이 낸 오류 글. 성공하면 빈 글 */
  error: string;
  toasts: Toast[];
  /** 마지막 시험 알림의 배너 결과(호스트가 준 글). 아직 안 보냈으면 빈 글 */
  lastBanner: string;
}

export const EVENT_KINDS: readonly { id: EventKind; label: string; hint: string }[] = [
  { id: "question", label: "질문이 도착했을 때", hint: "위 단계가 못 푼 질문만 와요. 답해야 일이 이어져요" },
  { id: "pr", label: "리뷰할 일이 생겼을 때", hint: "분석 보고가 올라왔어요(코드를 고치는 단계부터는 초안 PR도). 읽고 Done으로 보내거나 다시 맡겨요" },
  { id: "failed", label: "실패하거나 막혔을 때", hint: "한도를 넘었거나 에이전트가 멈췄어요" },
  { id: "done", label: "Done으로 바뀌었을 때", hint: "조용히 알려도 돼요" },
  { id: "approval", label: "스킬·에이전트 변경이 승인을 기다릴 때", hint: "AI가 쓴 변경은 사람이 승인해야 적용돼요. 늦으면 AI의 일이 막혀요" },
];

export const DEFAULT_SETTINGS: NotifySettings = {
  inApp: true,
  macos: true,
  events: {
    question: { inApp: true, macos: true },
    pr: { inApp: true, macos: true },
    failed: { inApp: true, macos: true },
    done: { inApp: true, macos: false },
    approval: { inApp: true, macos: true },
  },
};

export const MAX_ITEMS = 200;
export const MAX_TOASTS = 3;
/** 토스트가 저절로 사라지기까지(밀리초). */
export const TOAST_MS = 8000;

// ---------------------------------------------------------------- 읽기

const isRecord = (v: unknown): v is Record<string, unknown> => !!v && typeof v === "object" && !Array.isArray(v);
const KINDS = new Set<string>(["question", "pr", "failed", "done", "approval", "test"]);

/** 호스트 행을 알림으로 읽는다. 모양이 틀리면 null이다. */
export function toItem(v: unknown): NotifyItem | null {
  if (!isRecord(v)) return null;
  const { id, kind, title } = v;
  if (typeof id !== "string" || !id || typeof kind !== "string" || !KINDS.has(kind) || typeof title !== "string") return null;
  return {
    id,
    kind: kind as NotifyKind,
    title,
    body: typeof v.body === "string" ? v.body : "",
    taskKey: typeof v.taskKey === "string" && v.taskKey ? v.taskKey : null,
    at: typeof v.at === "number" && Number.isFinite(v.at) ? v.at : 0,
    read: v.read === true,
  };
}

const channel = (v: unknown, fallback: Channel): Channel =>
  isRecord(v) ? { inApp: typeof v.inApp === "boolean" ? v.inApp : fallback.inApp, macos: typeof v.macos === "boolean" ? v.macos : fallback.macos } : fallback;

/** 호스트 설정을 읽는다. 칸이 모자라거나 틀리면 그 칸만 기본값이다. */
export function toSettings(v: unknown): NotifySettings {
  const d = DEFAULT_SETTINGS;
  if (!isRecord(v)) return d;
  const events = isRecord(v.events) ? v.events : {};
  return {
    inApp: typeof v.inApp === "boolean" ? v.inApp : d.inApp,
    macos: typeof v.macos === "boolean" ? v.macos : d.macos,
    events: {
      question: channel(events.question, d.events.question),
      pr: channel(events.pr, d.events.pr),
      failed: channel(events.failed, d.events.failed),
      done: channel(events.done, d.events.done),
      approval: channel(events.approval, d.events.approval),
    },
  };
}

// ---------------------------------------------------------------- 상태

const EMPTY: NotifySnapshot = { items: [], unread: 0, settings: DEFAULT_SETTINGS, loaded: false, error: "", toasts: [], lastBanner: "" };
let snapshot: NotifySnapshot = EMPTY;
const listeners = new Set<() => void>();
const timers = new Map<string, ReturnType<typeof setTimeout>>();

function publish(patch: Partial<NotifySnapshot>) {
  snapshot = { ...snapshot, ...patch };
  for (const listener of [...listeners]) listener();
}

export const subscribeNotifications = (listener: () => void) => {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
};
export const getNotificationsSnapshot = () => snapshot;
export const useNotifications = (): NotifySnapshot => useSyncExternalStore(subscribeNotifications, getNotificationsSnapshot, getNotificationsSnapshot);

/** 시험이 서로 새지 않게 처음 상태로 돌린다. */
export function resetNotificationsStore() {
  for (const t of timers.values()) clearTimeout(t);
  timers.clear();
  snapshot = EMPTY;
}

const explain = (e: unknown): string => (e instanceof Error ? e.message : typeof e === "string" ? e : "알림을 처리하지 못했어요.");

/** 호스트에서 목록과 설정을 읽는다. 호스트가 없거나 vault가 없으면 오류 글만 남기고 비워 둔다. */
export async function refreshNotifications(call: HostCall): Promise<void> {
  try {
    const [list, settings] = await Promise.all([
      call<{ items?: unknown[]; unread?: number }>("notifications_list"),
      call<unknown>("notifications_settings_get"),
    ]);
    const items = (Array.isArray(list.items) ? list.items : []).map(toItem).filter((i): i is NotifyItem => i !== null);
    publish({ items, unread: typeof list.unread === "number" ? list.unread : items.filter((i) => !i.read).length, settings: toSettings(settings), loaded: true, error: "" });
  } catch (e) {
    publish({ loaded: true, error: explain(e) });
  }
}

/**
 * 호스트가 알려 준 사건을 반영한다. 새 알림은 목록 맨 앞에 두고(같은 id는 바꿔 끼운다), 설정이 토스트를 허락한 것만 토스트로 띄운다.
 * 토스트는 {@link MAX_TOASTS}개까지 두고 {@link TOAST_MS} 뒤에 저절로 사라진다. `showToasts`가 false면(노트 창) 토스트는 안 띄운다.
 */
export function applyChanged(event: ChangedEvent, showToasts = true): void {
  const pushed = toItem(event.pushed);
  const patch: Partial<NotifySnapshot> = { unread: typeof event.unread === "number" ? event.unread : snapshot.unread };
  if (pushed) {
    patch.items = [pushed, ...snapshot.items.filter((i) => i.id !== pushed.id)].slice(0, MAX_ITEMS);
    if (event.toast && showToasts) {
      const toast: Toast = { id: pushed.id, item: pushed };
      patch.toasts = [toast, ...snapshot.toasts.filter((t) => t.id !== toast.id)].slice(0, MAX_TOASTS);
      clearTimeout(timers.get(toast.id));
      timers.set(toast.id, setTimeout(() => dismissToast(toast.id), TOAST_MS));
    }
  }
  publish(patch);
}

export function dismissToast(id: string): void {
  clearTimeout(timers.get(id));
  timers.delete(id);
  if (snapshot.toasts.some((t) => t.id === id)) publish({ toasts: snapshot.toasts.filter((t) => t.id !== id) });
}

/** 읽음으로 바꾼다. `ids`를 안 주면 전부. */
export async function markRead(call: HostCall, ids?: string[]): Promise<void> {
  const target = ids ? new Set(ids) : null;
  const before = snapshot;
  const items = snapshot.items.map((i) => (!i.read && (!target || target.has(i.id)) ? { ...i, read: true } : i));
  publish({ items, unread: items.filter((i) => !i.read).length });
  try {
    const unread = await call<number>("notifications_mark_read", { ids: ids ?? null });
    if (typeof unread === "number") publish({ unread });
  } catch (e) {
    publish({ items: before.items, unread: before.unread, error: explain(e) });
  }
}

/** 설정을 바꾼다. 화면에 먼저 반영하고 호스트에 저장하며, 저장하지 못하면 되돌린다. */
export async function saveSettings(call: HostCall, next: NotifySettings): Promise<void> {
  const before = snapshot.settings;
  publish({ settings: next, error: "" });
  try {
    publish({ settings: toSettings(await call<unknown>("notifications_settings_set", { settings: next })) });
  } catch (e) {
    publish({ settings: before, error: explain(e) });
  }
}

/** 설정의 한 칸만 바꾼 사본. `event`가 없으면 채널 전체 스위치, 있으면 그 소식의 채널이다. */
export function withSwitch(s: NotifySettings, channelKey: "inApp" | "macos", value: boolean, event?: EventKind): NotifySettings {
  if (!event) return { ...s, [channelKey]: value };
  return { ...s, events: { ...s.events, [event]: { ...s.events[event], [channelKey]: value } } };
}

export interface PushResult {
  item: unknown;
  toast: boolean;
  banner: string;
  unread: number;
}

/** 시험 알림을 보낸다. 같은 길(쌓기, 토스트, 배너)을 지나므로 설정이 제대로 먹는지 이것으로 본다. 배너 결과 글을 `lastBanner`에 남긴다. */
export async function sendTest(call: HostCall): Promise<PushResult | null> {
  try {
    const out = await call<PushResult>("notifications_push", {
      item: { kind: "test", title: "시험 알림이에요", body: "이 알림이 보이면 알림이 잘 가고 있어요." },
    });
    publish({ lastBanner: typeof out?.banner === "string" ? out.banner : "", error: "" });
    return out;
  } catch (e) {
    publish({ error: explain(e), lastBanner: "" });
    return null;
  }
}

// ---------------------------------------------------------------- 표시

export const kindLabel = (k: NotifyKind): string =>
  ({ question: "질문", pr: "리뷰", failed: "실패", done: "Done", approval: "승인", test: "시험" })[k];

/** 시험 알림의 배너 결과를 사람이 읽을 글로 푼다. */
export function bannerMessage(banner: string): string {
  if (!banner) return "";
  if (banner === "off") return "macOS 알림이 꺼져 있어서 배너는 보내지 않았어요.";
  if (banner === "sent") return "macOS로 배너를 보냈어요. 배너가 안 보이면 시스템 설정의 알림에서 aisurada를 허용했는지, 집중 모드에서 허용했는지 확인해 주세요.";
  if (banner === "sent-fallback") return "앱의 알림 기능이 실패해서 다른 길(osascript)로 보냈어요. 배너는 Script Editor 이름으로 떠요.";
  return `macOS 배너를 보내지 못했어요. ${banner.replace(/^failed:\s*/, "")}`;
}

/** 안 읽은 알림을 앞에 두고 읽은 알림을 뒤에 둔 목록 모양(탭에 따라). */
export function visibleItems(items: NotifyItem[], tab: "unread" | "all"): NotifyItem[] {
  return tab === "unread" ? items.filter((i) => !i.read) : items;
}
