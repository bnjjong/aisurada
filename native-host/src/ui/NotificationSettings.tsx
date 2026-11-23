// 설정 화면의 「알림」. 채널(앱 안, macOS, Telegram 자리)과 소식별 켜고 끄기, 시험 알림을 보인다.
// 설정은 호스트가 파일로 보관하고(러너도 같은 설정을 따른다), 이 화면은 값을 바꿔 저장할 뿐이다.
// 앱 안 채널을 꺼도 알림은 알림 센터에 쌓인다. 집중 모드는 macOS가 처리한다: 집중 모드에서 aisurada를 허용하지 않으면 macOS가 배너를 막는다.
import { useEffect } from "react";
import type { WorkspaceController } from "../workspace/controller.ts";
import { EVENT_KINDS, bannerMessage, refreshNotifications, saveSettings, sendTest, useNotifications, withSwitch } from "./notificationsStore.ts";
import type { EventKind, NotifySettings } from "./notificationsStore.ts";
import { SettingsGroup, SettingsPane } from "./SettingsParts.tsx";

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
    <div className="ntf-settings" aria-label="알림 설정">
      {!native && <p className="ntf-note">알림 설정은 앱에서만 바꿀 수 있어요.</p>}

      <SettingsGroup title="받는 곳">
        <label className="ntf-row" data-setting="ntf-inapp">
          <span>
            <strong>앱 안</strong>
            <small>토스트와 배지예요. 끄면 알림 센터(종)에만 쌓여요.</small>
          </span>
          <input
            type="checkbox"
            role="switch"
            className="ntf-switch set-toggle"
            aria-label="앱 안 알림"
            disabled={!native}
            checked={settings.inApp}
            onChange={(e) => toggle("inApp", e.target.checked)}
          />
        </label>
        <label className="ntf-row" data-setting="ntf-macos">
          <span>
            <strong>macOS 알림센터</strong>
            <small>앱이 뒤에 있어도 배너로 알려요.</small>
          </span>
          <input
            type="checkbox"
            role="switch"
            className="ntf-switch set-toggle"
            aria-label="macOS 알림센터"
            disabled={!native}
            checked={settings.macos}
            onChange={(e) => toggle("macos", e.target.checked)}
          />
        </label>
        <label className="ntf-row ntf-off">
          <span>
            <strong>Telegram</strong>
            <small>준비 중이에요.</small>
          </span>
          <input
            type="checkbox"
            role="switch"
            className="ntf-switch set-toggle"
            aria-label="Telegram"
            disabled
            checked={false}
          />
        </label>
      </SettingsGroup>

      <details className="ntf-focus-details" data-setting="ntf-focus">
        <summary>집중 모드에서는?</summary>
        <p>
          집중 모드는 macOS가 처리해요. 집중 모드에서 aisurada를 허용하지 않으면 macOS가 배너를 막아요.
        </p>
      </details>

      <SettingsGroup title="소식별">
        <div data-setting="ntf-events">
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
                    <input
                      type="checkbox"
                      role="switch"
                      className="ntf-switch set-toggle"
                      aria-label={`${e.label} · 앱 안`}
                      disabled={!native}
                      checked={settings.events[e.id].inApp}
                      onChange={(ev) => toggle("inApp", ev.target.checked, e.id)}
                    />
                  </td>
                  <td>
                    <input
                      type="checkbox"
                      role="switch"
                      className="ntf-switch set-toggle"
                      aria-label={`${e.label} · macOS`}
                      disabled={!native}
                      checked={settings.events[e.id].macos}
                      onChange={(ev) => toggle("macos", ev.target.checked, e.id)}
                    />
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </SettingsGroup>

      <div className="ntf-test" data-setting="ntf-test">
        <button type="button" disabled={!native} onClick={onTest}>
          시험 알림 보내기
        </button>
        <span className="ntf-hint">설정대로 토스트와 배너가 가는지 확인해요.</span>
      </div>
      {banner && <p className="ntf-result" role="status">{message}</p>}
      {error && <p className="ntf-error" role="alert">{error}</p>}
    </div>
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
    <SettingsPane title="알림" desc="어디로, 어떤 소식을 알릴지 정해요. 꺼도 알림 센터(종)에는 쌓여요.">
      <NotificationSettingsView
        settings={n.settings}
        native={native}
        banner={n.lastBanner}
        error={n.error}
        onChange={(next) => void saveSettings(call, next)}
        onTest={() => void sendTest(call)}
      />
    </SettingsPane>
  );
}
