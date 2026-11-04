# aisurada

외부 설명서(1~4부)를 보고 다시 지은 vault 엔진(Java 21 + SnakeYAML)과 데스크톱 앱(Tauri 2 호스트 + React 19 화면)입니다. 이름은 `aisurada`, 접두는 `ais`입니다.

## 구성

| 부 | 담는 것 | 어디 |
|----|---------|------|
| 1부 | vault 엔진과 클라이언트 `ais-cli` | `src/aisurada/`, `test/aisurada/` |
| 2부 | Tauri 호스트 (창, PTY, 파일, 프로세스 실행) | `native-host/src-tauri/` |
| 3부 | 화면 (편집기, 목록, 검색, 이력, 터미널) | `native-host/src/` |
| 4부 | Claude 연동 (묻기, 세션 화면) | 엔진 `ask`·`chat`·`session`, 호스트 `ask.rs` 따위, 화면 `plugs/claude/` |

## 명령

- `./build.sh`: `dist/`에 jar 4개를 짓습니다.
- `bash test.sh`: 엔진 시험 전체
- `bash test-native.sh`: 화면·호스트 시험 전체
- `pnpm --dir native-host test:*:acceptance`: 실제 WKWebView 점검 (종류별)
- `pnpm --dir native-host docs:guide`: 개발자 가이드 HTML 다시 만들기

자세한 구조, 사용법, 시험 방법은 [`docs/developer-guide.md`](docs/developer-guide.md)에 있습니다.

## 공통 규칙

작업을 시작하기 전에 아래 규칙을 읽고, 작업을 마친 뒤에는 체크리스트를 점검합니다. 공통 규칙은 `CommonRule.md` 한 곳에서만 고칩니다.

@CommonRule.md
