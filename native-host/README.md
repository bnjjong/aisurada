# aisurada 네이티브 호스트

2부 1~18장의 Tauri 2 호스트와 3부 React 화면이다. 2부 커맨드 29개, 이벤트 4개, `wlframe` 프로토콜을 구현했다. 화면은 제공된 13장까지 연결하여 workspace·리치 편집기·자동저장·충돌 처리와 노트 메타·댓글·연결, 캘린더와 목록, 검색·이력 복원·외부 Markdown·설정을 지원한다. [3부 구현 순서](../docs/rebuild-p3-plan.md)와 [11~13장 기록](../docs/rebuild-p3-chapters11-13-complete.md)을 참고한다. 14~18장과 부록 본문, 원본 가이드 스킬·검색 골든 파일이 없어 그 범위는 별도 판정한다. 4부 Rust 모듈 6개(cargo 기능 `claude`)는 기본으로 켜져 있고, `--no-default-features`로 끄면 4부 커맨드 9개가 빠진다.

```bash
bash build.sh
cd native-host
pnpm install --frozen-lockfile
pnpm tauri dev
```

Node.js 22.12 이상과 pnpm 10.33.4를 사용한다. `pnpm dev`는 Vite 서버만, `pnpm tauri dev`는 실제 앱까지 띄운다. `pnpm build`는 타입 검사 후 4부를 켠 화면을 `dist/`에 만들고 빌드 계약을 검사한다. 4부를 뺀 화면은 `pnpm build:noclaude`(`vite build --mode noclaude`)다. `pnpm preview`는 호스트 없이 화면을 보며 번들 CSP를 적용한다. 개발 서버 포트는 1420이며 충돌 시 실패한다.

macOS 앱은 `pnpm tauri build --debug --bundles app`으로 만든다. 산출물은 `src-tauri/target/debug/bundle/macos/aisurada.app`이다. 최적화 빌드는 `--debug`를 뺀다. 번들 빌드는 화면과 저장소의 Java jar도 새로 짓는다. 앱 설치나 launchd 등록은 하지 않는다.

검증은 저장소 루트에서 `bash test-native.sh`로 실행한다. Rust/Cargo, Node, pnpm, JDK 21, Xcode 도구가 필요하다. `Cargo.lock`과 `pnpm-lock.yaml`의 Tauri 2.11.2 호환 조합을 사용한다. 실제 macOS webview 점검은 다음과 같다.

```bash
cd native-host
pnpm test:acceptance              # 2부 호스트의 실제 WKWebView 점검
pnpm test:workspace               # 3부 부팅·전환·이관 검사 29개
pnpm test:workspace:acceptance    # 새 React 화면 + Java 엔진 + WKWebView 점검 9개
pnpm test:navigation              # 3부 주소·탭·스크롤 검사 25개
pnpm test:navigation:acceptance   # 실제 WKWebView의 주소·탭·새로고침 점검 11개
pnpm test:interaction             # 3부 메뉴·단축키·딥링크·개발 다리 검사 30개
pnpm test:interaction:acceptance  # WKWebView의 창별 이벤트·Markdown 진입 점검 11개
pnpm test:data                    # 엔진 argv·토큰·재조정·읽기 경합 검사 48개
pnpm test:data:acceptance         # Java 엔진·WKWebView·FSEvents·창 사이 쓰기 점검 12개
pnpm test:editor                  # 독립 왕복 71사례와 경계·저장 연결 검사
pnpm test:editor:acceptance       # 실제 WKWebView 왕복 바이트와 편집 기능 검사
pnpm test:views                   # 11~13장 검색·최근·날짜·배치·실패 복구 검사 22개
pnpm test:views:acceptance        # 실제 제품 화면·엔진·Git·WKWebView 통합 점검
pnpm test:autosave                # 자동저장·제목·충돌·복원·diff 독립 검사 44개
pnpm test:autosave:acceptance     # 실제 엔진·WKWebView·Git·미러/undo 검사 16개
```

3부 하네스도 별도 앱 식별자와 임시 HOME·vault·소켓을 사용하며 전역 Tauri 객체 없이 실행한다. `AISURADA_WORKSPACE_REPORT=/절대/경로.json`으로 결과를 남긴다. 편집기 검사는 DOM 선택·insertText·클립보드 이벤트와 실제 호스트 IPC를 사용한다. 실제 선택 대화 상자와 물리 IME 입력은 자동 조작하지 않는다.

`test:data:acceptance`는 제품의 controller·데이터 계층·플랫폼 어댑터와 알림 부품을 별도 시험 진입부로 컴파일한다. 실제 파일 쓰기와 엔진 응답을 확인하며 제품 권한 파일을 그대로 쓴다. 전체 편집 화면을 조작하는 시험과는 구분한다. 본문·메타 큐·토큰과 버퍼 분리·600ms 저장·4초 재시도·제목 커밋·충돌 선택·미러·이력 교체 포트를 연결했다. `test:views:acceptance`는 실제 제품 화면의 메타·댓글·하위 페이지·이력 복원·시각 변경·폼·사실층 충돌·템플릿·리포트·휴지통·외부 파일·노트 창 이동을 검증한다. 이력 목록과 폼은 11~13장 화면에 연결했다.

`test:interaction:acceptance`는 이벤트를 주입하려고 격리된 테스트 설정에서만 이벤트 송신 거절 두 개를 제외한다. 제품 권한 파일은 바꾸지 않는다. 실제 메뉴 클릭과 macOS의 URL·파일 연결 전달은 손 확인 범위이며 자동 점검과 구분한다.

개발 서버에 페이지가 붙어 있으면 `curl -X POST localhost:1420/__deeplink --data-raw 'aisurada://doc/<id>'`로 열린 main 창을 이동한다. `?deeplink=<인코딩한 주소>`도 개발 판에서 처음 한 번 받는다. 두 입구는 번들에서 제외하며 개발 다리는 `file://`를 받지 않는다.

이 하네스는 매 실행 별도 앱 식별자, 임시 HOME·vault·소켓·빈 ZDOTDIR를 만든다. 실제 Java 엔진, WKWebView, PTY를 통해 24개 페이지 점검과 창 자리 파일 2개 점검을 수행한다. 가짜 `claude`가 불리지 않는지도 확인한다. 성공하면 임시 자료를 지우고, 실패하면 진단 경로를 출력한다. `AISURADA_CHECK_REPORT=/절대/경로.json`으로 JSON 보고서를 남길 수 있다. 원본 저장소의 `scripts/parts/host.mjs`는 제공되지 않아 PDF 18장의 재료와 조건을 이 저장소에 구현했다.

`allow_workspace`는 main에서만 vault를 바꾸고, 노트 창이 있는 동안 다른 vault로 전환하지 못한다. 받은 경로 글자를 그대로 보관하고 파일·그림 범위는 누적한다. `vault_root`, `is_workspace({root})`, `create_workspace_dir({parent,name})`도 지원한다.

`sidecar_call({argv})`는 열린 vault로 `ais-cli.jar`를 호출한다. main 스레드 밖에서 두 출력을 동시에 읽고 120초를 넘으면 TERM→최대 3초→KILL로 직접 자식을 회수한다. `AISURADA_CLI_JAR`, `AISURADA_SIDECAR_JAVA`, `AISURADA_SIDECAR_JAR`로 경로를 지정한다. 원본 Gradle 경로와 이 저장소의 `dist/`를 개발 후보로 확인한다. 개발 판은 자원 폴더의 jar 사본을 쓰지 않는다. 개발 판과 빌드 폴더 아래 번들은 sidecar jar를 자동으로 넘기지 않는다.

`write_attachment_bytes`는 raw 바이트와 퍼센트 인코딩한 `x-wl-note-dir`, `x-wl-name` 헤더를 받는다. 그림 확장자 8개와 5,000,000바이트 상한을 적용하고 vault 상대 경로를 반환한다. `open_in_default_app({path})`는 현재 vault의 실재 경로만 열고 실행 파일을 거절한다. `announce_self_write({root,relPath})`는 유효한 상대 경로와 현재 vault 글자가 일치할 때 실제 호출 창 이름을 붙여 모든 창에 알린다.

`resolve_md_target({path,knownRoots})`는 vault 노트와 외부 Markdown을 구분한다. `read_external_md({path})`는 엄격한 UTF-8의 2 MiB 이하 파일을 읽는다. `allow_external_images({mdPath,srcs})`는 문서 폴더 안의 상대 그림을 파일별로 그림 범위에 연다.

`wlframe://localhost/절대/첨부/경로`는 `notes/**/attach/` 안의 허용 형식만 제공한다. HTML에는 높이·외부 링크 알림 스크립트를 붙이고 모든 응답에 고정 CSP·nosniff·no-store를 단다. 부모 화면의 sandbox와 링크 처리·높이 제한은 3부 편집기에 연결했다.

노트 창은 `open_note_window({docId,title,geometry})`로 열고 `note_windows`, `close_note_windows`, `destroy_current_window`, `show_in_main`으로 다룬다. `saved_note_windows`, `restore_note_windows({docs})`, `remember_note_window({geometry})`가 vault별 복원 목록을 관리한다. 창의 JS 닫기 요청 처리와 편집 글 저장은 3부의 몫이다.

모니터별 창 자리와 데스크톱을 기억한다. 창 이벤트 400밀리초 뒤에 저장하며, 파일은 앱 데이터 폴더의 `main-window`, `note-windows`, `window-layouts` 세 JSON이다. 디버그 빌드는 `.dev.json`을 쓰고 임시 파일 뒤 이름 바꾸기로 저장한다. yabai가 있으면 절대 경로로만 호출해 데스크톱을 기억·복원하며, 없어도 창은 열린다.

PTY 커맨드는 `pty_start`, `pty_write`, `pty_resize`, `pty_detach`, `pty_alive`, `pty_stats`다. 키마다 로그인 zsh를 두고 출력을 base64 채널로 보낸다. 2 MiB 재생 버퍼와 5분 분리 유예를 사용하며, 다시 붙으면 버퍼를 먼저 보낸다. 종료한 옛 셸의 읽기 스레드와 타이머가 같은 키의 새 셸을 지우지 못하게 인스턴스와 세대를 확인한다.

비밀 가림은 `secret_table_probe`, `secret_table_status`, `mask_scrollback`, `scan_text_shapes`로 제공한다. probe를 실제로 부를 때만 HOME의 `.zshrc` 관리 블록과 로그인 셸 환경을 읽는다. 값은 직렬화·Debug를 구현하지 않은 타입에만 보관한다. 실 이름 분류표 `src-tauri/secret-names.txt`는 Git에서 제외하며, 없으면 항목 없는 견본을 포함한다. 견본 빌드는 표를 뜨기 전 가리기를 거절한다. 테스트는 지어낸 이름과 값만 사용한다.

원본 `wl-agent/*.mjs`와 4부 소스는 제공되지 않아 포함하지 않았다. 4부 커맨드 9개는 Tauri의 `Command … not found`로 거절된다. 번들 자원은 이 저장소의 `dist/`에 맞췄으며 원본 아이콘 대신 임시 SVG 아이콘을 사용한다. 장별 구현·검증·손 확인 범위는 [9장](../docs/rebuild-p2-chapter09-complete.md)부터 [18장](../docs/rebuild-p2-chapter18-complete.md)까지의 완료 문서와 [부록 기록](../docs/rebuild-p2-appendices-complete.md)에 적었다.

외부 검증 사진에서 확인한 원본 받아들임·합치기 결과, 동작 차이 네 건과 실제 앱 조작으로 확인할 일곱 항목은 [2부 피드백 확인 기록](../docs/rebuild-p2-review-2026-10-31.md)에 정리했다.
