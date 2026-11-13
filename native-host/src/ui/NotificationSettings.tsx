// 설정 화면의 「알림」. 채널(앱 안, macOS, Telegram 자리)과 소식별 켜고 끄기, 시험 알림을 보인다.
// 설정은 호스트가 파일로 보관하고(러너도 같은 설정을 따른다), 이 화면은 값을 바꿔 저장할 뿐이다.
// 앱 안 채널을 꺼도 알림은 알림 센터에 쌓인다. 집중 모드는 macOS가 처리한다: 집중 모드에서 aisurada를 허용하지 않으면 macOS가 배너를 막는다.
import { useEffect } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import { EVENT_KINDS, bannerMessage, refreshNotifications, saveSettings, sendTest, useNotifications, withSwitch } from "./notificationsStore.ts";
import type { EventKind, NotifySettings } from "./notificationsStore.ts";

export function NotificationSettingsView({
  settings,
  native,
  banner,
  error,
  onChange,
  onTest,
}: {
  settings: NotifySettings;
  /** 앱(호스트)이 있는지. 브라우저에서는 설정을 못 바꾼다 */
  native: boolean;
  /** 마지막 시험 알림의 배너 결과 글 */
  banner: string;
  error: string;
  onChange: (next: NotifySettings) => void;
  onTest: () => void;
}) {
  const toggle = (channelKey: "inApp" | "macos", value: boolean, event?: EventKind) => onChange(withSwitch(settings, channelKey, value, event));
  const message = bannerMessage(banner);
  return (
    <section className="ntf-settings" aria-label="알림 설정">
      <h3>알림</h3>
      {!native && <p className="ntf-note">알림 설정은 앱에서만 바꿀 수 있어요.</p>}
      <label className="ntf-row">
        <span>
          <strong>앱 안</strong>
          <small>토스트와 배지예요. 끄면 알림 센터(종)에만 쌓여요.</small>
        </span>
        <input type="checkbox" role="switch" className="ntf-switch" aria-label="앱 안 알림" disabled={!native} checked={settings.inApp} onChange={(e) => toggle("inApp", e.target.checked)} />
      </label>
      <label className="ntf-row">
        <span>
          <strong>macOS 알림센터</strong>
          <small>앱이 뒤에 있어도 배너로 알려요.</small>
        </span>
        <input type="checkbox" role="switch" className="ntf-switch" aria-label="macOS 알림센터" disabled={!native} checked={settings.macos} onChange={(e) => toggle("macos", e.target.checked)} />
      </label>
      <label className="ntf-row ntf-off">
        <span>
          <strong>Telegram</strong>
          <small>준비 중이에요.</small>
        </span>
        <input type="checkbox" role="switch" className="ntf-switch" aria-label="Telegram" disabled checked={false} readOnly />
      </label>
      <p className="ntf-note">
        macOS의 집중 모드는 macOS가 처리해요. 집중 모드에서 aisurada를 허용하지 않으면 배너가 막히고 알림 센터에 쌓여요. 허용 목록은 시스템 설정의 집중 모드에서 바꿔요.
      </p>
      <table className="ntf-table">
        <thead>
          <tr>
            <th scope="col">소식</th>
            <th scope="col">앱 안</th>
            <th scope="col">macOS</th>
          </tr>
        </thead>
        <tbody>
          {EVENT_KINDS.map((e) => (
            <tr key={e.id}>
              <td>
                <strong>{e.label}</strong>
                <small>{e.hint}</small>
              </td>
              <td>
                <input type="checkbox" role="switch" className="ntf-switch" aria-label={`${e.label} · 앱 안`} disabled={!native} checked={settings.events[e.id].inApp} onChange={(ev) => toggle("inApp", ev.target.checked, e.id)} />
              </td>
              <td>
                <input type="checkbox" role="switch" className="ntf-switch" aria-label={`${e.label} · macOS`} disabled={!native} checked={settings.events[e.id].macos} onChange={(ev) => toggle("macos", ev.target.checked, e.id)} />
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      <div className="ntf-test">
        <button type="button" disabled={!native} onClick={onTest}>
          시험 알림 보내기
        </button>
        <span className="ntf-hint">설정대로 토스트와 배너가 가는지 확인해요.</span>
      </div>
      {message && <p className="ntf-result" role="status">{message}</p>}
      {error && <p role="alert" className="ntf-error">{error}</p>}
    </section>
  );
}

export function NotificationSettings({ controller }: { controller: WorkspaceController }) {
  const n = useNotifications();
  const native = controller.isNative();
  const call = controller.callHost;
  // 설정을 열 때마다 호스트의 값을 새로 읽는다(다른 창에서 바꿨을 수 있다)
  useEffect(() => {
    if (native) void refreshNotifications(call);
  }, [native, call]);
  return (
    <NotificationSettingsView
      settings={n.settings}
      native={native}
      banner={n.lastBanner}
      error={n.error}
      onChange={(next) => void saveSettings(call, next)}
      onTest={() => void sendTest(call)}
    />
  );
}
