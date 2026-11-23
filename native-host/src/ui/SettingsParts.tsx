// 설정 화면의 공통 부품: 줄(SettingsRow), 스위치(SettingsSwitch), 상태 줄(StatusLine), 그룹(SettingsGroup), 패널 머리(SettingsPane)
import type { ReactNode } from "react";

export function SettingsPane({
  title,
  desc,
  children,
}: {
  title: string;
  desc?: string;
  children: ReactNode;
}) {
  return (
    <>
      <header className="set-pane-head">
        <h3>{title}</h3>
        {desc && <p>{desc}</p>}
      </header>
      {children}
    </>
  );
}

export function SettingsGroup({
  title,
  children,
  className = "",
}: {
  title?: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className="set-group">
      {title && <h4>{title}</h4>}
      <div className={`set-card ${className}`.trim()}>{children}</div>
    </div>
  );
}

export function SettingsRow({
  title,
  desc,
  settingKey,
  className = "",
  control,
  children,
  as = "label",
}: {
  title: ReactNode;
  desc?: ReactNode;
  settingKey?: string;
  className?: string;
  control?: ReactNode;
  children?: ReactNode;
  as?: "label" | "div";
}) {
  const Tag = as;
  return (
    <Tag
      className={`set-row ${className}`.trim()}
      data-setting={settingKey}
    >
      <span className="set-row-label">
        <strong>{title}</strong>
        {desc && <small>{desc}</small>}
      </span>
      {control}
      {children}
    </Tag>
  );
}

export function SettingsSwitch({
  checked,
  onChange,
  disabled = false,
  className = "",
  ariaLabel,
}: {
  checked: boolean;
  onChange: (checked: boolean) => void;
  disabled?: boolean;
  className?: string;
  ariaLabel?: string;
}) {
  return (
    <input
      type="checkbox"
      role="switch"
      className={`set-toggle ${className}`.trim()}
      checked={checked}
      disabled={disabled}
      aria-label={ariaLabel}
      onChange={(e) => onChange(e.target.checked)}
    />
  );
}

export function StatusLine({
  status,
  text,
  desc,
  className = "",
  role = "status",
}: {
  status: "ok" | "off" | "warn";
  text: ReactNode;
  desc?: ReactNode;
  className?: string;
  role?: string;
}) {
  return (
    <div className={`set-status-line ${className}`.trim()} role={role}>
      <div className="set-status-head">
        <span className={`set-dot set-dot-${status}`} aria-hidden="true" />
        <strong className="set-status-main">{text}</strong>
      </div>
      {desc && <p className="set-status-desc">{desc}</p>}
    </div>
  );
}
