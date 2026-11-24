import { useCallback, useEffect, useMemo, useRef, useState, type ReactNode } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import type { WorkspaceController } from "../workspace/controller.ts";
import { taskAddress } from "../navigation/routes.ts";
import { channelId, channelLabel, resultLabel, slackEmoji, slackLink, slackTime, slackTimestamp, viaLabel, type ChannelVisibility, type IntakeChannel, type IntakeData, type IntakeEntry, type IntakeRow, type IntakeSchedule, type IntakeSource } from "./intakeModel.ts";
import { Modal } from "./Modal.tsx";
import { SettingsSwitch } from "./SettingsParts.tsx";
import { eventTime, managerError, useManagers, type ManagerItem } from "./managersStore.tsx";

const countLabels = ["오늘 접수", "새 태스크", "재제보 댓글", "되물음 대기", "무시"];
const filters = ["all", "asked", "created", "commented", "ignored", "pending"] as const;
type Filter = typeof filters[number];

const time = (millis: number) => new Date(millis).toLocaleString("ko-KR", {
  month: "numeric", day: "numeric", hour: "2-digit", minute: "2-digit", hour12: false,
});

export function SlackIntakeLink({ url, controller, children }: { url: string; controller: WorkspaceController; children: ReactNode }) {
  const target = slackLink(url);
  if (!target) return null;
  return <a href={target} target="_blank" rel="noreferrer" onClick={event => {
    if (!controller.isNative()) return;
    event.preventDefault();
    void openUrl(target).catch(() => {});
  }}>{children}</a>;
}

export function IntakeBadges({ item }: { item: ManagerItem }) {
  const intake = item.intake;
  if (!intake || !item.manager.enabled) return null;
  return <div className="mgr-actions">
    {intake.waiting > 0 && <span className="mgr-badge mgr-intake-warning">되물음 대기 {intake.waiting}</span>}
    {intake.stalledMinutes !== null && <span className="mgr-badge is-error">확인 {intake.stalledMinutes}분째 없음</span>}
    {intake.counts[0] > 0 && <span className="mgr-badge">오늘 접수 {intake.counts[0]}</span>}
  </div>;
}

function CursorTimes({ channel }: { channel: IntakeChannel }) {
  return <dl className="intake-cursor-times">
    <div><dt>읽은 곳</dt><dd>{time(slackTime(channel.cursor))}까지</dd></div>
    <div><dt>마지막 확인</dt><dd>{channel.lastCheckAt ? time(channel.lastCheckAt) : "확인 전"}
      {channel.lastCheckVia && ` · ${viaLabel[channel.lastCheckVia]}`}</dd></div>
    {channel.nextCheckAt != null && <div><dt>다음 예약 예상</dt><dd>{time(channel.nextCheckAt)}</dd></div>}
  </dl>;
}

export function IntakeConnectionCursor({ item }: { item: ManagerItem }) {
  const channels = item.intake?.channels;
  if (!channels?.length) return null;
  return <section className="mgr-panel intake-cursor-panel">
    <h2>커서</h2>
    {channels.map(channel => <div key={channel.id}>
      <strong>{channelLabel(channel)}</strong>
      <CursorTimes channel={channel} />
    </div>)}
  </section>;
}

function CursorCard({ channel, rows, onRewind, onPause, disabled }: {
  channel: IntakeChannel; rows: IntakeRow[]; onRewind(): void; onPause(): void; disabled: boolean;
}) {
  const latest = rows.filter(row => row.entry.channel === channel.id).slice(0, 8).reverse();
  return <section className="mgr-panel intake-cursor">
    <div className="intake-channel-icon" aria-hidden="true">#</div>
    <div className="intake-cursor-main">
      <div className="mgr-actions">
        <strong>{channelLabel(channel)}</strong>
        <span className="mgr-badge">{slackEmoji(channel.emoji)} 제보</span>
        <span className="mgr-badge">{channel.markProcessed ? "✅ 처리 표시" : "처리 표시 끔"}</span>
        {channel.paused ? <span className="mgr-badge mgr-intake-warning">멈춤</span>
          : channel.stalledMinutes != null ? <span className="mgr-badge is-error">확인 {channel.stalledMinutes}분째 없음</span>
          : channel.lastCheckAt != null && <span className="mgr-badge is-ok">정상</span>}
      </div>
      <CursorTimes channel={channel} />
      <div className="intake-cursor-track" aria-label="최근 제보의 처리 상태">
        {latest.map(({ entry, current }) => <span key={entry.id} title={`${time(slackTime(entry.messageTs))} · ${resultLabel[current.result]}`}
          className={`intake-dot is-${current.result}`} />)}
        <span className="intake-cursor-edge">← 읽은 곳</span>
      </div>
    </div>
    <div className="mgr-actions">
      <button type="button" disabled={disabled} onClick={onRewind}>되감기</button>
      <button type="button" disabled={disabled} onClick={onPause}>{channel.paused ? "다시 켜기" : "멈추기"}</button>
    </div>
  </section>;
}

function configureArgs(channel: IntakeChannel) {
  return { channel: channel.id, label: channel.label, visibility: channel.visibility ?? "unknown" as ChannelVisibility, emoji: channel.emoji, markProcessed: channel.markProcessed, paused: channel.paused,
    intervalMinutes: channel.intervalMinutes, schedule: channel.schedule ?? null };
}

type ChannelSettings = ReturnType<typeof configureArgs>;
type ChannelSettingsUpdate = Pick<ChannelSettings, "channel"> & Partial<ChannelSettings> & { createOnly?: boolean };

const visibilityLabel: Record<ChannelVisibility, string> = {
  public: "공개", private: "비공개", unknown: "공개 여부 미설정",
};

function RewindDialog({ channel, rows, close, save, busy, error }: {
  channel: IntakeChannel; rows: IntakeRow[]; close(): void; save(ts: string): void; busy: boolean; error: string;
}) {
  const [choice, setChoice] = useState<"today" | "last" | "date">("today");
  const [custom, setCustom] = useState("");
  const midnight = new Date();
  midnight.setHours(9, 0, 0, 0);
  const last = rows.filter(row => row.entry.channel === channel.id && row.entry.messageTs < channel.cursor)
    .map(row => row.entry.messageTs).sort().at(-1);
  const today = slackTimestamp(midnight.getTime());
  const customTime = new Date(custom).getTime();
  const target = choice === "today" ? today : choice === "last" ? last :
    Number.isFinite(customTime) ? slackTimestamp(customTime) : undefined;
  return <Modal title={`${channelLabel(channel)} 커서 되감기`} close={close} className="mgr-modal" defaultWidth={440}>
    <div className="mgr-modal-body">
      <p className="mgr-intro">선택한 위치 뒤에서 다음 확인을 시작해요. 이미 접수한 글은 원장에서 확인해 건너뛰어요.</p>
      <fieldset className="mgr-choices"><legend>읽기를 시작할 위치</legend>
        <label className="mgr-check"><input type="radio" checked={choice === "today"} onChange={() => setChoice("today")} />오늘 09:00부터</label>
        <label className="mgr-check"><input type="radio" disabled={!last} checked={choice === "last"} onChange={() => setChoice("last")} />
          마지막으로 접수한 글{last && `(${eventTime(slackTime(last))})`} 다음부터</label>
        <label className="mgr-check"><input type="radio" checked={choice === "date"} onChange={() => setChoice("date")} />직접 지정</label>
        {choice === "date" && <input aria-label="되감을 날짜와 시각" type="datetime-local" value={custom} onChange={event => setCustom(event.target.value)} />}
      </fieldset>
      <small>최근 7일 안에서 되감을 수 있어요.</small>
      {error && <p role="alert" className="mgr-error">{error}</p>}
    </div>
    <footer className="mgr-actions">
      <button type="button" onClick={close}>취소</button>
      <button type="button" className="primary" disabled={busy || !target || target >= channel.cursor}
        onClick={() => { if (target) save(target); }}>되감기</button>
    </footer>
  </Modal>;
}

function SlackReactions({ entry }: { entry: Pick<IntakeEntry, "reactions" | "reactionAdded"> }) {
  const reactions = entry.reactions ?? [];
  if (!reactions.length && !entry.reactionAdded) return null;
  return <div className="mgr-actions intake-reactions">
    {reactions.map(({ name, count }) => <span key={name} className="mgr-badge" title={name}>{slackEmoji(name)} {count}</span>)}
    {entry.reactionAdded && !reactions.some(reaction => reaction.name === "white_check_mark")
      && <span className="mgr-badge" title="Slack 처리 표시">✅</span>}
  </div>;
}

function EntryTimeline({ row }: { row: IntakeRow }) {
  const { entry, current } = row;
  const trigger = entry.reportReaction;
  const timeline: { id: string; at: number; content: ReactNode }[] = [
    { id: "posted", at: slackTime(entry.messageTs), content: <>Slack에 원글이 올라왔어요<small>{entry.reporter}</small></> },
  ];
  if (trigger?.at != null) timeline.push({
    id: "report-reaction", at: trigger.at, content: <>{slackEmoji(trigger.name)} 제보 반응{trigger.by && <small>{trigger.by}</small>}</>,
  });
  for (const event of entry.history) {
    if (event.checkedAt != null) timeline.push({
      id: `${event.revision}-checked`, at: event.checkedAt, content: <>매니저가 Slack 확인을 시작했어요<small>{viaLabel[event.via]}</small></>,
    });
    if (event.searchedAt != null) timeline.push({
      id: `${event.revision}-searched`, at: event.searchedAt, content: "비슷한 태스크와 노트를 검색했어요",
    });
    timeline.push({
      id: event.revision, at: event.at, content: <>
        <strong>{resultLabel[event.result]}</strong> · {viaLabel[event.via]}{event.decision && <small>{event.decision}</small>}
        {event !== current && event.question && <small>{event.question}</small>}
      </>,
    });
  }
  return <ol className="intake-timeline">{timeline.sort((a, b) => a.at - b.at).map(event =>
    <li key={event.id}><time>{time(event.at)}</time><span>{event.content}</span></li>
  )}</ol>;
}

function EntryDrawer({ row, channel, controller, close, action, busy, error }: {
  row: IntakeRow; channel?: IntakeChannel; controller: WorkspaceController; close(): void;
  action(result: "pending" | "ignored"): void; busy: boolean; error: string;
}) {
  const { entry, current } = row;
  const [copyStatus, setCopyStatus] = useState("");
  const copyQuestion = async () => {
    try {
      await navigator.clipboard.writeText(current.question);
      setCopyStatus("답글 초안을 복사했어요.");
    } catch {
      setCopyStatus("복사할 수 없어요. 위 초안의 문장을 선택해 주세요.");
    }
  };
  return <Modal title="원장 항목" close={close} className="mgr-modal intake-drawer" resizable={false}>
      <div className="intake-drawer-body">
      <span className={`mgr-badge intake-result is-${current.result}`}>{resultLabel[current.result]}</span>
      <h2>{entry.excerpt.split("\n")[0] || "Slack 제보"}</h2>
      <blockquote className="intake-quote">
        <strong>{entry.reporter} · {time(slackTime(entry.messageTs))} · {channel ? channelLabel(channel) : entry.channel}</strong>
        <p>{entry.excerpt || "인용한 글이 없어요."}</p>
        <SlackReactions entry={entry} />
        {entry.reportReaction?.by && entry.reportReaction.at == null &&
          <small>제보 반응 · {entry.reportReaction.by}</small>}
      </blockquote>
      <dl className="mgr-meta">
        <dt>원글 주소</dt><dd><code>{entry.url}</code><br /><SlackIntakeLink url={entry.url} controller={controller}>Slack에서 열기</SlackIntakeLink></dd>
        <dt>매니저</dt><dd>{entry.manager} · {viaLabel[current.via]}</dd>
        {current.decision && <><dt>판단</dt><dd>{current.decision}</dd></>}
        {current.question && <><dt>답글 초안</dt><dd>{current.question}
          <div className="mgr-actions intake-draft"><button type="button" onClick={() => void copyQuestion()}>초안 복사</button>
            {copyStatus && <small role="status">{copyStatus}</small>}</div></dd></>}
        {entry.taskKey && <><dt>태스크</dt><dd><button type="button" onClick={() => controller.navigate(taskAddress(entry.taskKey!))}>{entry.taskKey}</button></dd></>}
      </dl>
      <h3>처리 이력</h3>
      <EntryTimeline row={row} />
      {current.result === "asked" && <p className="mgr-warning">스레드에 답이 오면 다음 확인 때 매니저가 다시 검토해요.</p>}
      {error && <p role="alert" className="mgr-error">{error}</p>}
      <footer className="mgr-actions">
        <button type="button" className="primary" disabled={busy || current.result === "pending"} onClick={() => action("pending")}>다시 처리</button>
        <button type="button" disabled={busy || current.result === "ignored"} onClick={() => action("ignored")}>무시로 표시</button>
        <small className="intake-entry-id">원장 ID <code>{entry.id}</code></small>
      </footer>
      </div>
  </Modal>;
}

export function ManagerIntake({ item, controller, entryId, onConnection }: {
  item: ManagerItem; controller: WorkspaceController; entryId?: string; onConnection(): void;
}) {
  const { refresh } = useManagers();
  const [data, setData] = useState<IntakeData | null>(item.intake ?? null);
  const [filter, setFilter] = useState<Filter>("all");
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState(entryId);
  const [rewind, setRewind] = useState<IntakeChannel | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const name = item.manager.name;
  const channels = item.intake?.channels ?? data?.channels ?? [];
  const counts = item.intake?.counts ?? data?.counts;
  const rows = data?.rows ?? [];
  const rowsRevision = item.intake?.rowsRevision;
  const loadedRevision = useRef<string | undefined>(undefined);
  const request = useRef(0);
  const load = useCallback(async () => {
    const id = ++request.current;
    const value = await controller.callEngine(["manager", "intake", name]) as IntakeData;
    if (id === request.current) {
      loadedRevision.current = value.rowsRevision;
      setData(value);
      setError("");
    }
  }, [controller, name]);
  useEffect(() => {
    if (rowsRevision !== undefined && rowsRevision === loadedRevision.current) return;
    let active = true;
    void load().catch(reason => {
      if (active) setError(managerError(reason));
    });
    return () => { active = false; request.current++; };
  }, [load, rowsRevision]);
  useEffect(() => { if (entryId) setSelected(entryId); }, [entryId]);
  const execute = async (action: string, payload: object, after?: () => void) => {
    setBusy(true);
    setError("");
    try {
      await controller.callEngine(["manager", action, name, JSON.stringify(payload)]);
      await load();
      await refresh();
      after?.();
    } catch (reason) {
      setError(managerError(reason));
    } finally {
      setBusy(false);
    }
  };
  const channelById = useMemo(() => new Map(channels.map(channel => [channel.id, channel])), [channels]);
  const search = query.trim().toLowerCase();
  const shown = rows.filter(({ entry, current }) =>
    (filter === "all" || filter === current.result) &&
    (!search || [entry.excerpt, entry.reporter, entry.taskKey, entry.channel].some(value => value?.toLowerCase().includes(search))));
  const row = rows.find(item => item.entry.id === selected);
  return <div className="intake-stack">
    {!row && !rewind && error && <p role="alert" className="mgr-error">{error}</p>}
    {channels.map(channel => <CursorCard key={channel.id} channel={channel} rows={rows} disabled={busy}
      onRewind={() => { setError(""); setRewind(channel); }}
      onPause={() => void execute("intake-channel", { channel: channel.id, paused: !channel.paused })} />)}
    {data && !channels.length && <section className="mgr-panel mgr-between">
      <span>설정한 제보 채널이 없어요.</span><button type="button" onClick={onConnection}>제보 채널 설정</button>
    </section>}
    <dl className="mgr-counts intake-counts">{countLabels.map((label, index) => <div key={label}>
      <dt>{label}</dt><dd>{counts?.[index] ?? "—"}</dd>
    </div>)}</dl>
    <section className="mgr-panel">
      <div className="mgr-between intake-search">
        <nav className="mgr-chips" aria-label="접수 결과로 거르기">{filters.map(result =>
          <button type="button" key={result} aria-pressed={filter === result} onClick={() => setFilter(result)}>
            {result === "all" ? "전체" : resultLabel[result]} {result === "all" ? rows.length : rows.filter(row => row.current.result === result).length}
          </button>)}</nav>
        <button type="button" onClick={() => void load().catch(reason => setError(managerError(reason)))}>새로고침</button>
        <input type="search" value={query} aria-label="접수 원장 검색" placeholder="원글·제보자·태스크 키" onChange={event => setQuery(event.target.value)} />
      </div>
      <div className="mgr-table-wrap"><table className="mgr-table intake-table">
        <thead><tr><th scope="col">시각</th><th scope="col">원글</th><th scope="col">결과</th><th scope="col">처리</th></tr></thead>
        <tbody>{shown.map(({ entry, current, taskRepo, repeatCount }) => {
          const open = () => { setError(""); setSelected(entry.id); };
          return <tr key={entry.id} data-selected={selected === entry.id}
            onClick={() => { if (window.getSelection()?.type !== "Range") open(); }}>
            <td>{time(current.at)}</td>
            <td><button type="button" className="intake-row-open" onClick={event => { event.stopPropagation(); open(); }}
              aria-haspopup="dialog">{entry.excerpt || "Slack에서 받은 제보"}</button>
              <small>{entry.reporter} · {eventTime(slackTime(entry.messageTs))}{entry.reactionAdded && " · ✅"}</small></td>
            <td>
              <span className={`mgr-badge intake-result is-${current.result}`}>{resultLabel[current.result]}{entry.taskKey && ` · ${entry.taskKey}`}</span>
              {taskRepo && <small>{taskRepo}</small>}
              {!!repeatCount && <small>재제보 {repeatCount}건</small>}
              {current.decision && <small className="intake-row-decision" title={current.decision}>{current.decision}</small>}
            </td>
            <td>{current.via === "scheduled" && current.checkedAt != null
              ? `예약 ${eventTime(current.checkedAt)}` : viaLabel[current.via]}</td>
          </tr>;
        })}</tbody>
      </table></div>
      {!shown.length && <p className="mgr-intro">{!data ? "원장을 읽는 중이에요." : rows.length ? "검색 조건에 맞는 제보가 없어요." : "원장에 기록한 제보가 없어요."}</p>}
    </section>
    {row && <EntryDrawer row={row} channel={channelById.get(row.entry.channel)} controller={controller} close={() => { setSelected(undefined); setError(""); }}
      busy={busy} error={error} action={result => void execute("intake-review", { id: row.entry.id, revision: row.current.revision, result })} />}
    {rewind && <RewindDialog channel={rewind} rows={rows} close={() => { setRewind(null); setError(""); }} busy={busy} error={error}
      save={messageTs => void execute("intake-rewind", { channel: rewind.id, messageTs, revision: rewind.revision }, () => setRewind(null))} />}
  </div>;
}

function IntakeChannelRow({ channel, busy, save, edit, rewind }: {
  channel: IntakeChannel; busy: boolean; save(update: ChannelSettingsUpdate, done?: () => void): void;
  edit(): void; rewind(): void;
}) {
  const label = channelLabel(channel);
  const [draft, setDraft] = useState<string | null>(null);
  const emoji = (draft ?? channel.emoji).trim().replace(/^:+|:+$/g, "");
  return <div className="mgr-between intake-channel-row">
    <div className="intake-channel-name">
      <strong>{label}</strong>
      <small>{channel.id} · {visibilityLabel[channel.visibility ?? "unknown"]}
        {channel.paused && " · 멈춤"}</small>
      <small>{channel.schedule ? `예약 ${channel.intervalMinutes}분마다` : "예약 지연 알림 꺼짐"}</small>
    </div>
    <form className="mgr-actions intake-emoji-form" onSubmit={event => {
      event.preventDefault();
      if (emoji && emoji !== channel.emoji) save({ channel: channel.id, emoji }, () => setDraft(null));
    }}>
      <label className="mgr-label"><span>제보 이모지</span>
        <input aria-label={`${label} 제보 이모지`} value={draft ?? channel.emoji} disabled={busy}
          list="intake-emoji-names" maxLength={82} onChange={event => setDraft(event.target.value)} />
      </label>
      {emoji !== channel.emoji && <button type="submit" disabled={busy || !/^[a-z0-9_+-]{1,80}$/i.test(emoji)}>이모지 저장</button>}
    </form>
    <label className="mgr-check">
      <SettingsSwitch ariaLabel={`${label} ✅ 처리 표시`} checked={channel.markProcessed} disabled={busy}
        onChange={markProcessed => save({ channel: channel.id, markProcessed })} /> ✅ 처리 표시
    </label>
    <div className="mgr-actions">
      <button type="button" disabled={busy} onClick={edit}>설정</button>
      <button type="button" disabled={busy} onClick={() => save({ channel: channel.id, paused: !channel.paused })}>
        {channel.paused ? "다시 켜기" : "멈추기"}
      </button>
      <button type="button" disabled={busy} onClick={rewind}>되감기</button>
    </div>
  </div>;
}

export function IntakeConnection({ item, controller }: { item: ManagerItem; controller: WorkspaceController }) {
  const { refresh } = useManagers();
  const [channel, setChannel] = useState<IntakeChannel | null | undefined>();
  const [rewind, setRewind] = useState<{ channel: IntakeChannel; rows: IntakeRow[] } | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const configured = item.intake?.channels ?? [];
  const registered = item.manager.slackChannels.filter(id => channelId.test(id));
  const call = (action: string, payload: object) =>
    controller.callEngine(["manager", action, item.manager.name, JSON.stringify(payload)]);
  const run = async (action: () => Promise<void>) => {
    setBusy(true);
    setError("");
    try { await action(); }
    catch (reason) { setError(managerError(reason)); }
    finally { setBusy(false); }
  };
  return <section className="mgr-panel intake-stack">
    <header className="mgr-between"><h2>제보 채널</h2>
      <button type="button" onClick={() => setChannel(null)}>＋ 채널 추가</button>
    </header>
    {!rewind && (error || item.intakeIssue) && <p role="alert" className="mgr-error">{error || item.intakeIssue}</p>}
    <datalist id="intake-emoji-names">
      <option value="ticket">🎫</option><option value="beetle">🐞</option>
    </datalist>
    {configured.map(value => <IntakeChannelRow key={value.id} channel={value} busy={busy}
      save={(update, done) => void run(async () => {
        await call("intake-channel", update);
        await refresh();
        done?.();
      })}
      edit={() => setChannel(value)}
      rewind={() => void run(async () => {
        const data = await call("intake", { channel: value.id }) as IntakeData;
        const current = data.channels.find(channel => channel.id === value.id);
        if (current) setRewind({ channel: current, rows: data.rows });
      })} />)}
    {!configured.length && <p className="mgr-intro">Slack 채널 ID와 제보를 찾을 기준 이모지를 설정해 주세요.</p>}
    {channel !== undefined && <ChannelEditor value={channel} registered={registered} configured={configured} close={() => setChannel(undefined)}
      save={async payload => {
        if (Object.keys(payload).length > 1) {
          await call("intake-channel", payload);
          await refresh();
        }
        setChannel(undefined);
      }} />}
    {rewind && <RewindDialog channel={rewind.channel} rows={rewind.rows} close={() => { setRewind(null); setError(""); }} busy={busy} error={error}
      save={messageTs => void run(async () => {
        await call("intake-rewind", { channel: rewind.channel.id, messageTs, revision: rewind.channel.revision });
        await refresh();
        setRewind(null);
      })} />}
  </section>;
}

const weekdays = ["월", "화", "수", "목", "금", "토", "일"];
const defaultSchedule: IntakeSchedule = { days: [1, 2, 3, 4, 5], startMinute: 9 * 60, endMinute: 18 * 60 };

function scheduleTime(minutes: number) {
  const time = minutes % (24 * 60);
  return `${String(Math.floor(time / 60)).padStart(2, "0")}:${String(time % 60).padStart(2, "0")}`;
}

function minutesOfTime(value: string, end = false) {
  const [hour, minute] = value.split(":").map(Number);
  return end && hour === 0 && minute === 0 ? 24 * 60 : hour * 60 + minute;
}

function ChannelEditor({ value, registered, configured, close, save }: {
  value: IntakeChannel | null; registered: string[]; configured: IntakeChannel[]; close(): void;
  save(value: ChannelSettingsUpdate): Promise<void>;
}) {
  const [form, setForm] = useState(() => value ? configureArgs(value) : {
    channel: "", label: "", visibility: "unknown" as ChannelVisibility, emoji: "ticket", markProcessed: true, paused: false, intervalMinutes: 10, schedule: null,
  });
  const initial = useRef(form);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  const update = (patch: Partial<typeof form>) => setForm(current => ({ ...current, ...patch }));
  const channel = form.channel.trim().toUpperCase();
  const duplicate = !value && configured.some(item => item.id === channel);
  const schedule = form.schedule;
  const validSchedule = !schedule || (schedule.days.length > 0 && schedule.startMinute < schedule.endMinute);
  const updateSchedule = (patch: Partial<IntakeSchedule>) => {
    if (schedule) update({ schedule: { ...schedule, ...patch } });
  };
  return <Modal title={value ? "제보 채널 설정" : "제보 채널 추가"} close={close} className="mgr-modal" defaultWidth={480}>
    <form onSubmit={async event => {
      event.preventDefault();
      setBusy(true);
      setError("");
      try {
        const changes = Object.fromEntries(Object.entries(form).filter(([key, field]) =>
          field !== initial.current[key as keyof ChannelSettings]));
        await save(value ? { ...changes, channel } : { ...form, channel, createOnly: true });
      }
      catch (reason) { setError(managerError(reason)); }
      finally { setBusy(false); }
    }}>
      <div className="mgr-modal-body">
        <label className="mgr-label">Slack 채널 ID<input required value={form.channel} readOnly={!!value}
          list="intake-registered-channels" placeholder="C0123456789" onChange={event => update({ channel: event.target.value })} /></label>
        <datalist id="intake-registered-channels">{registered.map(id => <option key={id} value={id} />)}</datalist>
        <label className="mgr-label">채널 표시 이름<input value={form.label} maxLength={80} placeholder="#프로젝트-제보"
          onChange={event => update({ label: event.target.value })} /></label>
        <label className="mgr-label">채널 공개 여부
          <select value={form.visibility} onChange={event => update({ visibility: event.target.value as ChannelVisibility })}>
            {Object.entries(visibilityLabel).map(([key, label]) => <option key={key} value={key}>{label}</option>)}
          </select>
        </label>
        <div className="mgr-columns">
          <label className="mgr-label">제보 기준 이모지<input required value={form.emoji} placeholder="ticket"
            onChange={event => update({ emoji: event.target.value })} /></label>
          <label className="mgr-label">예약 확인 간격(분)<input required type="number" min={1} max={1440} value={form.intervalMinutes}
            onChange={event => update({ intervalMinutes: Number(event.target.value) })} /></label>
        </div>
        <label className="mgr-check"><input type="checkbox" checked={!!schedule}
          onChange={event => update({ schedule: event.target.checked ? defaultSchedule : null })} />예약 확인 지연 알리기</label>
        {schedule && <fieldset className="mgr-choices intake-schedule">
          <legend>예약이 실행되는 시간 · 이 기기의 시간대</legend>
          <div className="mgr-actions">{weekdays.map((label, index) => {
            const day = index + 1;
            const selected = schedule.days.includes(day);
            return <label className="mgr-check" key={label}><input type="checkbox" checked={selected}
              onChange={() => updateSchedule({ days: selected ? schedule.days.filter(value => value !== day) : [...schedule.days, day] })} />{label}</label>;
          })}</div>
          <div className="mgr-columns">
            <label className="mgr-label">시작<input required type="time" value={scheduleTime(schedule.startMinute)}
              onInput={event => {
                const value = event.currentTarget.value;
                if (value) updateSchedule({ startMinute: minutesOfTime(value) });
              }} /></label>
            <label className="mgr-label">종료<input required type="time" value={scheduleTime(schedule.endMinute)}
              onInput={event => {
                const value = event.currentTarget.value;
                if (value) updateSchedule({ endMinute: minutesOfTime(value, true) });
              }} /></label>
          </div>
          <small>종료를 00:00으로 설정하면 자정까지 확인해요.</small>
          {!validSchedule && <p className="mgr-warning">요일을 하나 이상 고르고 종료 시각을 시작 이후로 설정해 주세요.</p>}
        </fieldset>}
        <label className="mgr-check"><input type="checkbox" checked={form.markProcessed}
          onChange={event => update({ markProcessed: event.target.checked })} />처리한 원글에 ✅ 달기</label>
        <label className="mgr-check"><input type="checkbox" checked={form.paused}
          onChange={event => update({ paused: event.target.checked })} />채널 확인 멈추기</label>
        {duplicate && <p className="mgr-warning">이미 설정한 채널이에요.</p>}
        {error && <p role="alert" className="mgr-error">{error}</p>}
      </div>
      <footer className="mgr-actions">
        <button type="button" onClick={close}>취소</button>
        <button type="submit" className="primary" disabled={busy || duplicate || !channelId.test(channel) || !validSchedule}>저장</button>
      </footer>
    </form>
  </Modal>;
}

export function TaskSlackSources({ entries, firstUrl, controller, children }: {
  entries: IntakeSource[]; firstUrl?: string; controller: WorkspaceController; children?: ReactNode;
}) {
  const first = entries.find(entry => entry.taskOrigin === "created");
  const reports = entries.filter(entry => entry.taskOrigin === "commented");
  const total = reports.length + (first || firstUrl ? 1 : 0);
  if (!total) return null;
  return <section className="task-sec intake-source-panel" aria-label="출처">
    <header className="mgr-between"><h2>출처</h2><span className="mgr-badge">제보 {total}번</span></header>
    {!first && firstUrl && <blockquote className="intake-quote">
      <strong>처음</strong><p><SlackIntakeLink url={firstUrl} controller={controller}>Slack 원글 열기</SlackIntakeLink></p>
    </blockquote>}
    {[...(first ? [first] : []), ...reports].map(entry => <blockquote className="intake-quote" key={entry.id}>
      <strong>{entry.taskOrigin === "created" ? "처음" : "재제보"} · {entry.reporter} · {time(slackTime(entry.messageTs))}</strong>
      {entry.excerpt && <p>{entry.excerpt}</p>}
      <span><SlackIntakeLink url={entry.url} controller={controller}>Slack에서 열기</SlackIntakeLink>{entry.reactionAdded && " · ✅"}</span>
    </blockquote>)}
    {children}
  </section>;
}
