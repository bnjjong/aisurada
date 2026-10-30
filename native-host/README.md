# withlog 네이티브 호스트

2부 1~4장의 Tauri 2 호스트 기반이다. Java 엔진과 별도 빌드하며, 화면은 3부를 연결하기 전의 최소 HTML이다.

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

원본 저장소의 자원 경로를 이 저장소의 `dist/`에 맞춰 변경했다. jar 두 개의 번들 내 이름은 그대로다. 제공되지 않은 `wl-agent/*.mjs`는 번들에 추가하지 않았다. 원본 아이콘 대신 코드로 만든 임시 SVG 아이콘과 그 변환본을 사용한다. 실 비밀 이름 표는 추적하지 않으며, 없으면 항목 없는 견본을 포함한다.
