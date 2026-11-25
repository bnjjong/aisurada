/**
 * 네이티브 호스트(Tauri / Rust)가 노출하는 IPC 커맨드 타입 맵
 * 명령 이름(command), 전달 인자(args), 반환 결과(result)의 단일 진실 공급원(SSOT)이다.
 */

export interface HostCommandMap {
  // 앱 수명주기 및 윈도우 관리
  exit_app: { args?: Record<string, never>; result: void };
  begin_quit: { args?: Record<string, never>; result: void };
  cancel_quit: { args?: Record<string, never>; result: void };
  show_in_main: { args: { path: string; rail?: string | null; sub?: string | null }; result: void };
  open_note_window: { args: { docId: string; title?: string; geometry?: unknown }; result: void };
  note_windows: { args?: Record<string, never>; result: string[] };
  close_note_windows: { args?: Record<string, never>; result: void };
  destroy_current_window: { args?: Record<string, never>; result: void };
  saved_note_windows: { args?: Record<string, never>; result: string[] };
  restore_note_windows: { args: { docs: unknown[] }; result: string[] };
  remember_note_window: { args: { geometry: unknown }; result: void };

  // 워크스페이스 및 볼트 관리
  allow_workspace: { args: { path: string }; result: void };
  vault_root: { args?: Record<string, never>; result: string | null };
  is_workspace: { args: { root: string }; result: boolean };
  create_workspace_dir: { args: { parent: string; name: string }; result: string };

  // 엔진 및 사이드카
  engine_commands: { args?: Record<string, never>; result: string[] };
  engine_call: { args: { command: string; args?: unknown }; result: unknown };
  sidecar_call: { args: { argv: string[] }; result: unknown };

  // 파일 및 마크다운
  read_external_md: { args: { path: string }; result: string };
  allow_external_images: { args: { mdPath: string; srcs: string[] }; result: number };
  resolve_md_target: { args: { path: string; knownRoots: string[] }; result: unknown };
  open_in_default_app: { args: { path: string }; result: void };
  open_external_path: { args: { path: string }; result: void };
  write_attachment_bytes: { args?: unknown; result: void };
  read_drawio_attachment: { args: { noteDir: string; name: string }; result: ArrayBuffer | number[] };
  write_drawio_attachment: { args?: unknown; result: void };
  copy_dropped_attachment: { args: { noteDir: string; path: string }; result: unknown };
  announce_self_write: { args: { root: string; relPath: string }; result: void };

  // GitHub 연동
  github_status: { args?: Record<string, never>; result: unknown };
  github_connect: { args: { appId: string; pemPath: string }; result: unknown };
  github_disconnect: { args?: Record<string, never>; result: unknown };
  github_repositories: { args?: Record<string, never>; result: unknown };
  github_clone: { args: { fullName: string }; result: unknown };

  // 알림 센터
  notifications_list: { args?: Record<string, never>; result: unknown };
  notifications_push: { args: { item: unknown; expectedVault?: string }; result: unknown };
  notifications_mark_read: { args?: { ids?: string[] }; result: number };
  notifications_settings_get: { args?: Record<string, never>; result: unknown };
  notifications_settings_set: { args: { settings: unknown }; result: unknown };

  // 태스크 러너
  runner_status: { args?: Record<string, never>; result: unknown };
  runner_run: { args: { taskKey: string }; result: unknown };
  runner_stop: { args: { taskKey: string }; result: unknown };
  runner_answer: { args: { taskKey: string; text: string }; result: unknown };
  runner_settings_set: { args: { settings: unknown }; result: unknown };
  runner_runs: { args: { taskKey: string }; result: unknown[] };
  runner_recent_runs: { args?: { limit?: number }; result: unknown[] };

  // 매니저 및 웹훅
  manager_webhook_secret: { args: { name: string; action: string }; result: string };
  manager_test_webhook: { args: { name: string; url: string; channel: string }; result: { status: number; elapsed: number } };
  manager_send_status: { args: { taskKey: string }; result: unknown };

  // Archyl 및 C4
  archyl_status: { args?: Record<string, never>; result: unknown };
  archyl_login: { args: { readOnly: boolean }; result: unknown };
  archyl_cancel_login: { args?: Record<string, never>; result: unknown };
  archyl_disconnect: { args?: Record<string, never>; result: unknown };
  archyl_model: { args: { projectId: string }; result: unknown };
  archyl_chat: { args: unknown; result: unknown };
  c4_chat: { args: { project: string; selectedId: string | null; request: string; history: unknown[] }; result: unknown };

  // 터미널 PTY
  pty_start: { args: unknown; result: unknown };
  pty_write: { args: unknown; result: unknown };
  pty_resize: { args: unknown; result: unknown };
  pty_detach: { args: unknown; result: unknown };
  pty_alive: { args: unknown; result: unknown };
  pty_stats: { args: unknown; result: unknown };
  pty_start_session: { args: unknown; result: unknown };

  // 시크릿 마스킹
  secret_table_probe: { args?: Record<string, never>; result: unknown };
  secret_table_status: { args?: Record<string, never>; result: unknown };
  mask_scrollback: { args: { lines: string[] }; result: unknown };
  scan_text_shapes: { args: { text: string }; result: unknown };

  // AI 및 세션
  ask_claude: { args: unknown; result: unknown };
  ask_runtime: { args?: Record<string, never>; result: unknown };
  cancel_ask: { args: { sessionId: string; askId?: string }; result: boolean };
  list_claude_agents: { args?: Record<string, never>; result: unknown };
  alive_pids: { args?: { args?: unknown; pids?: number[] }; result: number[] };
  held_agent_sessions: { args?: { vault?: string }; result: string[] };
  release_agent_session: { args?: { vault?: string; sessionId?: string }; result: unknown };
  write_session_image: { args: unknown; result: unknown };
  start_dictation: { args: unknown; result: unknown };
}

export type HostCommandName = keyof HostCommandMap;

export type HostCommandArgs<K extends HostCommandName> = HostCommandMap[K]['args'];

export type HostCommandResult<K extends HostCommandName> = HostCommandMap[K]['result'];

export type OptionalArgsCommandName = {
  [K in HostCommandName]: [undefined] extends [HostCommandMap[K]['args']] ? K : never;
}[HostCommandName];

export type RequiredArgsCommandName = Exclude<HostCommandName, OptionalArgsCommandName>;

/**
 * 정형화된 호스트 IPC 호출 함수 인터페이스.
 * 등록된 명령에 대해서는 타입 추론 및 인자 검사가 엄격히 적용된다.
 */
export interface TypedHostCall {
  // 1. 인자가 없거나 생략 가능한 명령
  <T = never, K extends OptionalArgsCommandName = OptionalArgsCommandName>(
    command: K,
    args?: HostCommandMap[K]['args']
  ): Promise<[T] extends [never] ? HostCommandMap[K]['result'] : T>;

  // 2. 인자가 필수인 명령
  <T = never, K extends RequiredArgsCommandName = RequiredArgsCommandName>(
    command: K,
    args: HostCommandMap[K]['args']
  ): Promise<[T] extends [never] ? HostCommandMap[K]['result'] : T>;
}
