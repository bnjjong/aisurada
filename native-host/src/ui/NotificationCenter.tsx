// 알림 센터(레일의 종 단추가 여는 목록)와 앱 안 토스트. 알림을 쌓고 보내는 일은 호스트가 하고(`notificationsStore.ts`),
// 이 화면은 목록을 보이고 읽음으로 바꾸고 태스크로 데려다 줄 뿐이다.
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { RefObject } from "react";
import { createPortal } from "react-dom";
import type { WorkspaceController } from "../workspace/controller.ts";
import { taskAddress } from "../navigation/routes.ts";
import { formatRelativeTime } from "./dates.ts";
import { dismissToast, kindLabel, markRead, useNotifications, visibleItems } from "./notificationsStore.ts";
import type { NotifyItem, NotifyKind } from "./notificationsStore.ts";

const GLYPH: Record<NotifyKind, string> = { question: "?", pr: "PR", failed: "!", done: "✓", test: "i" };

/** 알림 종류를 나타내는 동그라미. 색은 종류가 정한다(`notifications.css`의 `.ntf-ico-*`). */
function KindMark({ kind }: { kind: NotifyKind }) {
  return (
    <span className={`ntf-ico ntf-ico-${kind}`} role="img" aria-label={kindLabel(kind)}>
      {GLYPH[kind]}
    </span>
  );
}

/** 알림 한 줄에 붙는 단추 이름. 질문은 「답하기」, 그 밖의 태스크 알림은 「태스크 보기」. */
export const actionLabel = (item: Pick<NotifyItem, "kind" | "taskKey">): string | null =>
  !item.taskKey ? null : item.kind === "question" ? "답하기" : "태스크 보기";

export type CenterTab = "unread" | "all";

/** 알림 센터의 속(그리기만 한다). 위치와 열고 닫기는 {@link NotificationBell}이 맡는다. */
export function NotificationPanel({
  items,
  unread,
  tab,
  onTab,
  onOpen,
  onReadAll,
  onSettings,
}: {
  items: NotifyItem[];
  unread: number;
  tab: CenterTab;
  onTab: (tab: CenterTab) => void;
  onOpen: (item: NotifyItem) => void;
  onReadAll: () => void;
  onSettings: () => void;
}) {
  const shown = visibleItems(items, tab);
  return (
    <div className="ntf-panel" role="dialog" aria-label="알림 센터">
      <header>
        <h2>알림</h2>
        <button type="button" disabled={unread === 0} onClick={onReadAll}>
          모두 읽음
        </button>
        <button type="button" aria-label="알림 설정" onClick={onSettings}>
          설정
        </button>
      </header>
      <div className="ntf-tabs" role="tablist" aria-label="알림 보기">
        <button type="button" role="tab" aria-selected={tab === "unread"} onClick={() => onTab("unread")}>
          안 읽음 {unread}
        </button>
        <button type="button" role="tab" aria-selected={tab === "all"} onClick={() => onTab("all")}>
          전체
        </button>
      </div>
      {shown.length === 0 ? (
        <p className="ntf-empty">{tab === "unread" ? "안 읽은 알림이 없어요." : "아직 알림이 없어요."}</p>
      ) : (
        <ul className="ntf-list">
          {shown.map((item) => (
            <li key={item.id} className={`ntf-item${item.read ? "" : " is-new"}`} data-notification-id={item.id}>
              <KindMark kind={item.kind} />
              <div>
                <h3>
                  {!item.read && <span className="ntf-unread" role="img" aria-label="안 읽음" />}
                  {item.title}
                  <time dateTime={new Date(item.at).toISOString()}>{formatRelativeTime(item.at)}</time>
                </h3>
                {(item.body || item.taskKey) && (
                  <p>
                    {item.taskKey && <span className="ntf-key">{item.taskKey}</span>}
                    {item.taskKey && item.body ? " · " : ""}
                    {item.body}
                  </p>
                )}
                {actionLabel(item) && (
                  <button type="button" className={item.kind === "question" ? "button-primary" : ""} onClick={() => onOpen(item)}>
                    {actionLabel(item)}
                  </button>
                )}
                {!actionLabel(item) && !item.read && (
                  <button type="button" onClick={() => onOpen(item)}>
                    읽음으로
                  </button>
                )}
              </div>
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** 레일의 종 단추와 알림 센터. 단추에는 안 읽은 수가 붙는다. 센터는 레일(스크롤 상자) 밖 body에 그려서 잘리지 않게 한다. */
export function NotificationBell({ controller }: { controller: WorkspaceController }) {
  const n = useNotifications();
  const [open, setOpen] = useState(false);
  const [tab, setTab] = useState<CenterTab>("unread");
  const button = useRef<HTMLButtonElement>(null);
  const call = controller.callHost;

  const openItem = (item: NotifyItem) => {
    void markRead(call, [item.id]);
    if (item.taskKey) {
      setOpen(false);
      controller.navigate(taskAddress(item.taskKey));
    }
  };

  return (
    <>
      <button
        ref={button}
        type="button"
        className="rail-bell"
        aria-label={n.unread > 0 ? `알림 ${n.unread}개` : "알림"}
        aria-haspopup="dialog"
        aria-expanded={open}
        onClick={() => setOpen((o) => !o)}
      >
        <svg viewBox="0 0 16 16" width="16" height="16" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M4 11V7.5a4 4 0 0 1 8 0V11l1 1.5H3zM6.5 14h3" />
        </svg>
        {n.unread > 0 && <span className="rail-bell-dot">{n.unread > 99 ? "99+" : n.unread}</span>}
      </button>
      {open && (
        <Popover anchor={button} onClose={() => setOpen(false)}>
          <NotificationPanel
            items={n.items}
            unread={n.unread}
            tab={tab}
            onTab={setTab}
            onOpen={openItem}
            onReadAll={() => void markRead(call)}
            onSettings={() => {
              setOpen(false);
              controller.settings(true);
            }}
          />
        </Popover>
      )}
    </>
  );
}

/** 종 단추 아래에 뜨는 상자. 바깥을 누르거나 Esc를 누르면 닫는다. 단추를 다시 누르는 것은 바깥 누르기로 치지 않는다(단추가 스스로 여닫는다). */
function Popover({ anchor, onClose, children }: { anchor: RefObject<HTMLElement | null>; onClose: () => void; children: React.ReactNode }) {
  const box = useRef<HTMLDivElement>(null);
  const [place, setPlace] = useState<{ top: number; left: number } | null>(null);
  useLayoutEffect(() => {
    const rect = anchor.current?.getBoundingClientRect();
    if (rect) setPlace({ top: rect.bottom + 8, left: Math.max(8, Math.min(rect.left, window.innerWidth - 400)) });
  }, [anchor]);
  useEffect(() => {
    const outside = (e: MouseEvent) => {
      const target = e.target as Node;
      if (box.current?.contains(target) || anchor.current?.contains(target)) return;
      onClose();
    };
    const key = (e: KeyboardEvent) => {
      if (e.key === "Escape") onClose();
    };
    document.addEventListener("mousedown", outside);
    document.addEventListener("keydown", key);
    return () => {
      document.removeEventListener("mousedown", outside);
      document.removeEventListener("keydown", key);
    };
  }, [anchor, onClose]);
  if (!place) return null;
  return createPortal(
    <div className="ntf-pop" ref={box} style={{ top: place.top, left: place.left }}>
      {children}
    </div>,
    document.body,
  );
}

/** 앱 안 토스트. 설정이 허락한 새 알림만 뜨고 8초 뒤에 저절로 사라진다. */
export function NotificationToasts({ controller }: { controller: WorkspaceController }) {
  const n = useNotifications();
  if (n.toasts.length === 0) return null;
  return <ToastStack toasts={n.toasts.map((t) => t.item)} onDismiss={dismissToast} onOpen={(item) => {
    dismissToast(item.id);
    void markRead(controller.callHost, [item.id]);
    if (item.taskKey) controller.navigate(taskAddress(item.taskKey));
  }} />;
}

/** 토스트 묶음(그리기만 한다). */
export function ToastStack({ toasts, onDismiss, onOpen }: { toasts: NotifyItem[]; onDismiss: (id: string) => void; onOpen: (item: NotifyItem) => void }) {
  return (
    <div className="ntf-toasts" role="status" aria-live="polite">
      {toasts.map((item) => (
        <div key={item.id} className="ntf-toast" data-notification-id={item.id}>
          <KindMark kind={item.kind} />
          <div>
            <h3>{item.title}</h3>
            {(item.body || item.taskKey) && (
              <p>
                {item.taskKey && <span className="ntf-key">{item.taskKey}</span>}
                {item.taskKey && item.body ? " · " : ""}
                {item.body}
              </p>
            )}
            {actionLabel(item) && (
              <button type="button" className={item.kind === "question" ? "button-primary" : ""} onClick={() => onOpen(item)}>
                {actionLabel(item)}
              </button>
            )}
          </div>
          <button type="button" className="ntf-close" aria-label="닫기" onClick={() => onDismiss(item.id)}>
            ×
          </button>
        </div>
      ))}
    </div>
  );
}
