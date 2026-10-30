# withlog 네이티브 호스트

2부 1~8장의 Tauri 2 호스트 기반이다. Java 엔진과 별도 빌드하며, 화면은 3부를 연결하기 전의 최소 HTML이다.

```bash
bash build.sh
cd native-host
pnpm install --frozen-lockfile
pnpm tauri dev
```

macOS 앱 번들을 만들려면 `pnpm tauri build --debug --bundles app`을 사용한다. 배포용 최적화 빌드는 `--debug`를 뺀다. 빌드 전 명령이 저장소의 Java jar를 새로 짓는다. 산출물은 `src-tauri/target/debug/bundle/macos/withlog.app`이다. 사용자 앱 설치나 launchd 등록은 하지 않는다.

검증은 저장소 루트에서 `bash test-native.sh`로 실행한다. Rust/Cargo, Node, pnpm, JDK 21과 Xcode 도구가 필요하다. `Cargo.lock`과 `pnpm-lock.yaml`을 함께 사용한다. Tauri 2.11.2가 최신 간접 런타임·매크로와 호환되지 않아 문서에 맞는 조합을 잠갔다.

환경 정리는 스레드를 만들기 전에 실행한다. Claude와 cmux의 표식이 있을 때만 주입된 변수를 지우며 사용자 설정은 보존한다. main 페이지의 시작·완료 알림 모두에서 등록된 자식 프로세스를 회수한다. 노트 창 알림은 회수하지 않는다. main 닫기 요청은 종료 플래그를 세우고 노트 창에 닫기를 요청한다. Cmd+Q나 종료 신호를 별도 창 닫기 요청으로 바꾸지 않는다.

현재 식별자·스킴·파일 연결·자원·CSP·플러그인·권한과 창 기동/이벤트, 공통 IPC 규약 및 vault 커맨드 네 개를 구현했다. `allow_workspace`, `vault_root`, `is_workspace`, `create_workspace_dir`를 invoke로 부를 수 있다. main만 vault를 바꾸며 노트 창이 있는 동안 다른 vault로 전환하지 못한다. vault는 받은 글자 그대로 보관하고 범위는 누적된다. 기본 상태는 비어 있다.

바이트 본문과 퍼센트 인코딩 헤더, base64 채널, 이벤트 대상과 페이로드의 공통 코드는 후속 커맨드가 사용한다. 나머지 커맨드, PTY, 파일 프로토콜 응답, 모니터별 레이아웃과 노트 창 복원 파일은 후속 장에서 연결한다. `wlframe`은 등록돼 있으나 10장 구현 전까지 파일을 내주지 않고 404를 반환한다.

`sidecar_call({argv})`은 열린 vault로 실제 `wl-cli.jar`를 호출한다. argv와 종료 코드·출력을 보존하며 작업은 main 스레드 밖에서 돈다. `WITHLOG_CLI_JAR`, `WITHLOG_SIDECAR_JAVA`, `WITHLOG_SIDECAR_JAR`로 경로를 지정할 수 있다. 환경변수가 없으면 문서의 탐색 순서를 쓰며, 원본 Gradle 자리 외에 이 소스 트리의 `dist/`도 개발 후보로 확인한다. 개발 판은 자원 폴더의 jar 사본을 쓰지 않는다. sidecar jar는 개발 판과 빌드 폴더 아래 번들에서 자동으로 넘기지 않으므로, 설치된 엔진이 없으면 `WITHLOG_SIDECAR_JAR`로 명시한다.

자식은 출력 두 개를 동시에 읽으며 120초 안에 끝나지 않으면 TERM→최대 3초→KILL로 직접 자식만 회수한다. `bash test-native.sh`는 격리된 HOME·소켓·vault에서 실제 Java 엔진과 IPC 왕복도 실행한다. 테스트의 ignored IPC probe는 이 격리 실행이 사용하는 내부 진입점이다.

`write_attachment_bytes`는 raw 본문 또는 숫자 배열과 `x-wl-note-dir`, `x-wl-name` 헤더로 그림을 받아 attach/에 쓴다. 8개 그림 확장자와 5,000,000바이트 상한을 적용한다. `open_in_default_app({path})`는 현재 vault의 실재 경로만 열고 실행 가능한 파일은 거절한다. 직접 덮어쓰기와 심볼릭 링크 관련 경계는 [7장 기록](../docs/rebuild-p2-chapter07-complete.md)에 적었다.

`announce_self_write({root, relPath})`는 현재 vault 글자가 일치할 때만 실제 호출 창 이름표를 붙여 모든 창에 `wl://self-write`를 보낸다. 상대 경로 문법과 UTF-8 2,048바이트 상한을 검사하며 파일이 있는지는 보지 않는다. 화면의 2초 기록은 3부 연결 범위다.

원본 저장소의 자원 경로를 이 저장소의 `dist/`에 맞춰 변경했다. jar 두 개의 번들 내 이름은 그대로다. 제공되지 않은 `wl-agent/*.mjs`는 번들에 추가하지 않았다. 원본 아이콘 대신 코드로 만든 임시 SVG 아이콘과 그 변환본을 사용한다. 실 비밀 이름 표는 추적하지 않으며, 없으면 항목 없는 견본을 포함한다.
