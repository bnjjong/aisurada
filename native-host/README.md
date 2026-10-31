# withlog 네이티브 호스트

2부 1~18장의 Tauri 2 호스트다. 2부 커맨드 29개, 이벤트 4개, `wlframe` 프로토콜을 구현했다. Java 엔진은 별도로 빌드하고, 화면은 3부 연결 전의 최소 HTML이다. 4부 Rust 모듈은 포함하지 않으며 `--no-default-features`로 빌드할 수 있다.

```bash
bash build.sh
cd native-host
pnpm install --frozen-lockfile
pnpm tauri dev
```

macOS 앱은 `pnpm tauri build --debug --bundles app`으로 만든다. 산출물은 `src-tauri/target/debug/bundle/macos/withlog.app`이다. 최적화 빌드는 `--debug`를 뺀다. 번들 빌드는 저장소의 Java jar도 새로 짓는다. 앱 설치나 launchd 등록은 하지 않는다.

검증은 저장소 루트에서 `bash test-native.sh`로 실행한다. Rust/Cargo, Node, pnpm, JDK 21, Xcode 도구가 필요하다. `Cargo.lock`과 `pnpm-lock.yaml`의 Tauri 2.11.2 호환 조합을 사용한다. 실제 macOS webview 점검은 다음과 같다.

```bash
cd native-host
pnpm test:acceptance
```

이 하네스는 매 실행 별도 앱 식별자, 임시 HOME·vault·소켓·빈 ZDOTDIR를 만든다. 실제 Java 엔진, WKWebView, PTY를 통해 24개 페이지 점검과 창 자리 파일 2개 점검을 수행한다. 가짜 `claude`가 불리지 않는지도 확인한다. 성공하면 임시 자료를 지우고, 실패하면 진단 경로를 출력한다. `WITHLOG_CHECK_REPORT=/절대/경로.json`으로 JSON 보고서를 남길 수 있다. 원본 저장소의 `scripts/parts/host.mjs`는 제공되지 않아 PDF 18장의 재료와 조건을 이 저장소에 구현했다.

`allow_workspace`는 main에서만 vault를 바꾸고, 노트 창이 있는 동안 다른 vault로 전환하지 못한다. 받은 경로 글자를 그대로 보관하고 파일·그림 범위는 누적한다. `vault_root`, `is_workspace({root})`, `create_workspace_dir({parent,name})`도 지원한다.

`sidecar_call({argv})`는 열린 vault로 `wl-cli.jar`를 호출한다. main 스레드 밖에서 두 출력을 동시에 읽고 120초를 넘으면 TERM→최대 3초→KILL로 직접 자식을 회수한다. `WITHLOG_CLI_JAR`, `WITHLOG_SIDECAR_JAVA`, `WITHLOG_SIDECAR_JAR`로 경로를 지정한다. 원본 Gradle 경로와 이 저장소의 `dist/`를 개발 후보로 확인한다. 개발 판은 자원 폴더의 jar 사본을 쓰지 않는다. 개발 판과 빌드 폴더 아래 번들은 sidecar jar를 자동으로 넘기지 않는다.

`write_attachment_bytes`는 raw 바이트와 퍼센트 인코딩한 `x-wl-note-dir`, `x-wl-name` 헤더를 받는다. 그림 확장자 8개와 5,000,000바이트 상한을 적용하고 vault 상대 경로를 반환한다. `open_in_default_app({path})`는 현재 vault의 실재 경로만 열고 실행 파일을 거절한다. `announce_self_write({root,relPath})`는 유효한 상대 경로와 현재 vault 글자가 일치할 때 실제 호출 창 이름을 붙여 모든 창에 알린다.

`resolve_md_target({path,knownRoots})`는 vault 노트와 외부 Markdown을 구분한다. `read_external_md({path})`는 엄격한 UTF-8의 2 MiB 이하 파일을 읽는다. `allow_external_images({mdPath,srcs})`는 문서 폴더 안의 상대 그림을 파일별로 그림 범위에 연다.

`wlframe://localhost/절대/첨부/경로`는 `notes/**/attach/` 안의 허용 형식만 제공한다. HTML에는 높이·외부 링크 알림 스크립트를 붙이고 모든 응답에 고정 CSP·nosniff·no-store를 단다. 부모 화면의 sandbox와 링크 처리·높이 제한은 3부에서 연결한다.

노트 창은 `open_note_window({docId,title,geometry})`로 열고 `note_windows`, `close_note_windows`, `destroy_current_window`, `show_in_main`으로 다룬다. `saved_note_windows`, `restore_note_windows({docs})`, `remember_note_window({geometry})`가 vault별 복원 목록을 관리한다. 창의 JS 닫기 요청 처리와 편집 글 저장은 3부의 몫이다.

모니터별 창 자리와 데스크톱을 기억한다. 창 이벤트 400밀리초 뒤에 저장하며, 파일은 앱 데이터 폴더의 `main-window`, `note-windows`, `window-layouts` 세 JSON이다. 디버그 빌드는 `.dev.json`을 쓰고 임시 파일 뒤 이름 바꾸기로 저장한다. yabai가 있으면 절대 경로로만 호출해 데스크톱을 기억·복원하며, 없어도 창은 열린다.

PTY 커맨드는 `pty_start`, `pty_write`, `pty_resize`, `pty_detach`, `pty_alive`, `pty_stats`다. 키마다 로그인 zsh를 두고 출력을 base64 채널로 보낸다. 2 MiB 재생 버퍼와 5분 분리 유예를 사용하며, 다시 붙으면 버퍼를 먼저 보낸다. 종료한 옛 셸의 읽기 스레드와 타이머가 같은 키의 새 셸을 지우지 못하게 인스턴스와 세대를 확인한다.

비밀 가림은 `secret_table_probe`, `secret_table_status`, `mask_scrollback`, `scan_text_shapes`로 제공한다. probe를 실제로 부를 때만 HOME의 `.zshrc` 관리 블록과 로그인 셸 환경을 읽는다. 값은 직렬화·Debug를 구현하지 않은 타입에만 보관한다. 실 이름 분류표 `src-tauri/secret-names.txt`는 Git에서 제외하며, 없으면 항목 없는 견본을 포함한다. 견본 빌드는 표를 뜨기 전 가리기를 거절한다. 테스트는 지어낸 이름과 값만 사용한다.

원본 `wl-agent/*.mjs`와 4부 소스는 제공되지 않아 포함하지 않았다. 4부 커맨드 9개는 Tauri의 `Command … not found`로 거절된다. 번들 자원은 이 저장소의 `dist/`에 맞췄으며 원본 아이콘 대신 임시 SVG 아이콘을 사용한다. 장별 구현·검증·손 확인 범위는 [9장](../docs/rebuild-p2-chapter09-complete.md)부터 [18장](../docs/rebuild-p2-chapter18-complete.md)까지의 완료 문서와 [부록 기록](../docs/rebuild-p2-appendices-complete.md)에 적었다.

외부 검증 사진에서 확인한 원본 받아들임·합치기 결과, 동작 차이 네 건과 실제 앱 조작으로 확인할 일곱 항목은 [2부 피드백 확인 기록](../docs/rebuild-p2-review-2026-10-31.md)에 정리했다.
