# 3부 9단계: 14~17장 및 부록 완료 보고서

기준은 제공된 「withlog 3부 화면 설명서, 14장부터」(99쪽, 2026-11-01)의 14~17장과 부록 A~D 계약이다. 4부(Claude 연동) 없는 판의 화면 요구사항 및 플러그 규격을 구현하고 기존 Java 엔진 및 Tauri 호스트와 연결하여 전체 검증을 마쳤다.

---

## 1. 구현 내용

### 14장 터미널 화면 및 굳히기 (Terminal & Pin)
- **터미널 탭 및 라우팅**:
  - 범용 터미널 탭 ID `term:main` (`/terminal`), 세션 터미널 `term:<sessionId>` (`/terminal/:sessionId`).
  - 메뉴 `wl_open_terminal` 및 단축키 `Cmd+Shift+T` 연동.
  - 터미널 탭은 `withlog.tabs.v1`에 저장하지 않고 메모리에서만 유지.
- **터미널 칸 (`.term-host`)**:
  - vault가 열려 있는 동안 열린 터미널 탭마다 백그라운드에 1개씩 마운트 유지, 활성 탭 주소일 때만 보이고 아닐 때는 `.is-hidden` 토글.
  - `data-tab-key`, `data-testid="terminal-host"`.
  - 처음 보일 때 고정폭 글꼴(D2Coding 등 2,000ms 제한) 로드 후 `pty_start` 호출.
  - 키 입력 시 `pty_write`, 리사이즈 시 `pty_resize`, 탭 닫기/언마운트 시 `pty_detach` 호출.
  - 하단 막대 `.term-bar`에 「굳히기」 단추 및 4부 fallback 안내 문구(`.term-bar-note`) 배치.
- **스크롤백 굳히기 (`PinScrollbackModal`)**:
  - 논리 줄 복원 (`extractLogicalLines`): 터미널 버퍼 읽기, 줄바꿈 래핑 복원, 빈 끝 줄 제거.
  - 범위 단추 5개 (`.pin-ranges button`): 마지막 100/500/2000/10000줄, 고른 부분 (줄 단위 확장).
  - 호스트 `secret_table_status`, `secret_table_probe`, `mask_scrollback` 연동.
  - 가린 결과 요약 표 (`.pin-summary`), 힌트/경고(`.pin-hint`, `.pin-warn`), 안내문.
  - 본문 서식 (`formatPinnedNoteBody`): `# <제목>`, 요약 표, 콜아웃 5종(`[!warning]`, `[!danger]`, `[!success]`), 백틱 가변 펜스(최대 백틱 수 + 1, 최소 3개).
  - **C32 반영**: 문서 생성 실패 시 모달을 닫지 않고 실패 문구를 인라인으로 표시.

### 15장 노트 단독 창 (Note Window Lifecycle)
- **노트 창 식별 및 라우팅**:
  - `/note-window/:id` 진입 분기 (`windowContext`로 창 종류 확정).
  - 탭 바, 레일, 터미널, Cmd+K 제외, `vault_root`로 main 창 vault에 직접 연결.
  - 다른 화면 이동 링크 클릭 시 `show_in_main`으로 main 창에 위임하고 노트 창 주소 유지.
- **한 노트 한 창 원칙**:
  - `note_windows` 및 `wl://note-windows` 이벤트로 열린 창 목록 추적.
  - 미리보기, 링크, 탭 열기 시 이미 노트 창에 떠 있으면 해당 창을 앞으로 가져오고 본 창에서는 열지 않음.
- **닫기 라이프사이클 (15.9)**:
  - 닫기 시도(`close-requested`, Cmd+W `wl_close_tab`) 가로채기 -> 편집기 버퍼 flush -> 쓰기 큐 및 `settled` 대기 (10,000ms 한도).
  - 미저장 노트 수, `waitingAnswers()`, `unsavedAnswers()` 검사.
  - 미저장/충돌 시 확인 상자(`.note-window-dialog`, `<h2>창을 닫으면 사라지는 것이 있어요</h2>`, `.note-window-dialog-actions button` 「남겨 두기」/「그래도 닫기」) 표시.
  - 정상 또는 강제 닫기 시 감시 해제 후 `destroy_current_window` 호출.
- **창 위치 및 복원 (15.8, 15.10, 15.11)**:
  - 창 이동/크기 변경 시 400ms 디바운스 후 논리 픽셀을 반올림 없이 소수점 유지하여 `withlog.noteWindow.v1` 및 `remember_note_window`에 저장.
  - vault 전환 시 `close_note_windows` 호출 후 100ms 간격 최대 4,000ms 동안 0개가 될 때까지 대기.

### 16장 브라우저 저장소 (Storage Contracts)
- **`withlog.prefs.v1`**:
  - 21개 필드 전체 정의 및 기본값, 범위 보정/선택지 제한 로직 구현:
    - `railWidth` (200~420), `detailListWidth` (260~560), `docWidth` (640~1600), `noteRailWidth` (392~960), `previewWidth` (480~1600), `previewHeight` (320~2400), `askCardWidth` (280~720), `askCardHeight` (160~1600), `composeWidth` (240~720), `composeInputHeight` (80~1200), `askTimeoutSeconds` (600, 1200, 1800, 3600, 7200).
    - `weekLayout` ('swimlane' | 'stack'), `docSortMode` ('structure' | 'time'), `docSortDir` ('desc' | 'asc').
  - 버전 0 이관 (`askTimeoutSeconds: 1200` 처리) 및 버전 1 유지, 알 수 없는 필드 shallow merge 보존.
- **12개 저장소 키 규격 일치화**:
  - `withlog.tabs.v1`, `withlog.lastWorkspace.v1`, `withlog.workspaces.v1`, `withlog.workspaceHome.v1`, `withlog.recentNotes.v1:<vault>`, `withlog.noteWindow.v1`, `withlog.connMode.v3`, `withlog.connFilter.v2`, `withlog.reportEdits.v1`, `withlog.reportMemos.v1`, `withlog.templates.v1`.
  - `workspaceHome` 경로의 앞뒤 공백 및 끝 슬래시 제거, `~/Documents` 거부 및 `/` 시작 강제.

### 17장 4부 꽂이 5묶음 & 빈 구현 (Plugs & Stubs)
- **5묶음 15개 export 정의**:
  - `@claude-plug/state`: `CLAUDE_ENABLED = false`, `useAskState`, `useNoteRail`, `noteRailNow`, `waitingAnswers`, `unsavedAnswers`.
  - `@claude-plug/ask`: `useNoteAsk`, `NoteAskFrame`, `EditorAskPlugins`, `AskComposePopover`, `AskHoverCard`.
  - `@claude-plug/session`: `NoteSessionRail`, `SessionRoute`.
  - `@claude-plug/projects`: `RailProjects`.
  - `@claude-plug/terminal`: `planSessionShell`.
- **4부 없는 판 편집기/UI 약속**:
  - 편집기 루트에 `.lex-no-ask` 클래스 적용 (`CLAUDE_ENABLED ? '' : 'lex-no-ask'`).
  - 번들 assets 내 4부 커맨드 9개 0건 보장 (`check:screen` 통과).

---

## 2. 검증 결과

| 검사 항목 | 검사 대상 / 환경 | 결과 |
|---|---|---|
| TypeScript 타입 검사 (`pnpm typecheck`) | `tsc --noEmit` 전체 코드베이스 | 통과 (오류 0개) |
| 번들 규격 검사 (`pnpm check:screen`) | Web 번들 assets (인라인 스타일, 4부 커맨드 9종) | 통과 (인라인 스타일 0개, 4부 커맨드 0건) |
| 14~17장 신규 단위 테스트 (`test:chapters`) | 논리 줄, 백틱 펜스, 마스킹 본문, 저장소 21필드 클램핑, 플러그 스텁 | 9개 통과 |
| 기존 단위 테스트 전체 (`workspace`, `navigation`, `interaction`, `data`, `editor`, `autosave`, `views`, `contracts`) | Node 22 / tsx 테스트 러너 | 324개 전원 통과 |
| Rust 호스트 검사 (`cargo test`) | Tauri 커맨드, PTY, 비밀 표 마스킹, 레이아웃 등 | 65개 통과, 0개 실패 |
| 실제 macOS WKWebView 에디터 왕복 수용 검사 (`test:editor:acceptance`) | macOS WKWebView + Tauri + Java Engine | 83개 전원 통과 |
| 실제 macOS WKWebView 자동저장 수용 검사 (`test:autosave:acceptance`) | macOS WKWebView + Tauri + Java Engine | 16개 전원 통과 |
| 실제 macOS WKWebView 화면/뷰 수용 검사 (`test:views:acceptance`) | macOS WKWebView + Tauri + Java Engine | 25개 전원 통과 |
| 전체 네이티브 통합 스크립트 (`bash test-native.sh`) | Java Jar 빌드 + Web 번들 빌드 + 단위/수용/Rust 전체 | 통과 (Code 0) |

---

## 3. 관련 파일 링크

- **터미널 (14장)**:
  - [`native-host/src/ui/terminal/TerminalHost.tsx`](file:///Users/df/github/aisurada/native-host/src/ui/terminal/TerminalHost.tsx)
  - [`native-host/src/ui/terminal/PinScrollbackModal.tsx`](file:///Users/df/github/aisurada/native-host/src/ui/terminal/PinScrollbackModal.tsx)
  - [`native-host/src/ui/terminal/logicalLines.ts`](file:///Users/df/github/aisurada/native-host/src/ui/terminal/logicalLines.ts)
  - [`native-host/src/ui/terminal/ptyBridge.ts`](file:///Users/df/github/aisurada/native-host/src/ui/terminal/ptyBridge.ts)
- **노트 단독 창 (15장)**:
  - [`native-host/src/App.tsx`](file:///Users/df/github/aisurada/native-host/src/App.tsx)
  - [`native-host/src/workspace/controller.ts`](file:///Users/df/github/aisurada/native-host/src/workspace/controller.ts)
  - [`native-host/src/workspace/types.ts`](file:///Users/df/github/aisurada/native-host/src/workspace/types.ts)
  - [`native-host/src/workspace/platform.ts`](file:///Users/df/github/aisurada/native-host/src/workspace/platform.ts)
- **브라우저 저장소 (16장)**:
  - [`native-host/src/workspace/storage.ts`](file:///Users/df/github/aisurada/native-host/src/workspace/storage.ts)
- **4부 플러그 및 빈 구현 (17장)**:
  - [`native-host/src/plugs/empty/state.ts`](file:///Users/df/github/aisurada/native-host/src/plugs/empty/state.ts)
  - [`native-host/src/plugs/empty/ask.tsx`](file:///Users/df/github/aisurada/native-host/src/plugs/empty/ask.tsx)
  - [`native-host/src/plugs/empty/session.tsx`](file:///Users/df/github/aisurada/native-host/src/plugs/empty/session.tsx)
  - [`native-host/src/plugs/empty/projects.tsx`](file:///Users/df/github/aisurada/native-host/src/plugs/empty/projects.tsx)
  - [`native-host/src/plugs/empty/terminal.ts`](file:///Users/df/github/aisurada/native-host/src/plugs/empty/terminal.ts)
- **테스트**:
  - [`native-host/scripts/chapters14-17.test.mjs`](file:///Users/df/github/aisurada/native-host/scripts/chapters14-17.test.mjs)
  - [`native-host/scripts/setup-plugs.mjs`](file:///Users/df/github/aisurada/native-host/scripts/setup-plugs.mjs)
