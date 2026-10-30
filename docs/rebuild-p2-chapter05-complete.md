# 2부 5장 완료 — sidecar_call

작성일: 2026-10-30

제공받은 PDF 28~31쪽을 기준으로 `sidecar_call`을 등록했다. 현재 vault가 없으면 `vault가 아직 열리지 않았어요`로 거절한다. vault 값은 호출 시점에 복사하고 잠금을 놓으며, jar 탐색과 Java 실행은 `spawn_blocking` 일꾼에서 처리한다. argv는 변경 없이 `<java> -jar <wl-cli.jar>` 뒤에 붙인다.

환경변수에 유효한 값이 있으면 해당 자리만 보며 없는 파일이라고 다른 후보로 물러서지 않는다. 개발 판에서는 자원 폴더의 낡은 jar 사본을 건너뛴다. 번들 판은 자원 폴더를 먼저 보고, Java는 환경변수·자원 runtime·PATH의 일반 파일·`/usr/bin/java` 차례다. 실행 권한은 탐색 단계에서 검사하지 않는다. 자식에게 현재 vault를 그대로 주고, sidecar jar를 못 찾으면 물려받은 값도 지운다. 찾은 Java를 새 환경변수로 심지 않는다.

이 저장소의 Java 산출물은 원본 Gradle 모듈과 달리 `dist/`에 있다. 문서의 원본 후보 순서 뒤에, `build.sh`와 `src/withlog`가 확인되는 작업 폴더의 소스 트리에서만 `dist/` 후보를 추가했다. 번들의 자원 이름은 앞 장과 같다. 개발 판과 `target/{debug,release}/bundle` 아래 번들에서는 sidecar jar를 자동으로 자식에게 넘기지 않으며, 명시한 환경변수는 이 경우에도 우선한다.

결과는 `status`, `stdout`, `stderr` 차례이며 stdout/stderr는 UTF-8 손실 해독만 한다. 실행한 명령의 오류는 결과로 보존하고, jar 탐색이나 프로세스 기동 실패는 거절한다. 120초 상한은 `status: -1`, 빈 stdout과 문서의 고정 stderr를 반환한다. 테스트에서는 내부 상한을 짧게 주어 이 갈래를 검증했으며 실제로 120초를 기다리지는 않았다.

`bash test-native.sh`의 Rust 테스트 34개와 설정 테스트 3개가 통과했다. 별도 프로세스에서 Tauri MockRuntime invoke → 실제 Java wl-cli → 임시 serve 엔진을 호출했다. 열기 전 오류, 목록·상세, WL-404/409/412, 실제 노트 쓰기와 부모 Claude 세션이 새지 않는지를 확인했다. HOME과 소켓, vault 및 엔진 프로세스는 모두 테스트 전용이다. 테스트 목록의 ignored IPC probe는 이 격리 프로세스에서 자동 실행하는 진입점이며, 부모 테스트가 그 성공 여부를 확인한다.

jar·Java 후보, 개발/번들 구분, build 경로 마디, 명시한 경로의 실패, 디렉터리 후보, argv와 환경 보존, 깨진 UTF-8, 신호 종료와 시간 초과도 검증했다. macOS `.app` 번들 빌드가 통과했다. Finder에서 실행한 실제 webview와 launchd 설치는 이번 검증에서 사용하지 않았다.
