# Rust 엔진 이전 4단계 4d — 세션 기록 읽기 명령 3개

## 요약

**결론:** 에이전트가 남긴 대화 기록을 읽는 명령 3개(`read_session`, `session_titles`, `recent_sessions`)를 Rust 엔진으로 옮겼다. **Claude, Codex, Gemini CLI, Antigravity** 네 가지 기록 형식을 모두 옮겼다(사용자가 정했다). Java 엔진과 합성 홈에서 **응답과 폴더 전체 바이트가 2,291건 모두 같다**(앞 묶음 포함, 978단계, 이 가운데 세션은 50단계). 읽기 명령이라 앱이 바로 Rust로 읽는다. 실제 창 확인은 아직 못 했다([손 확인 안내](rust-engine-hand-check.md)).

전체 계획은 [4단계 계획](rust-engine-step4-plan.md)에 있고, 이 기록은 [4c 문답·대화 기록](rust-engine-step4c-ask-chat.md)에 이어진다.

## 1. 하는 일

기록은 vault 밖 사용자 홈 아래에 있고 읽기만 한다. 세션 id로 Claude 기록을 못 찾았을 때만 Codex, Gemini CLI, Antigravity를 차례로 찾고, 찾으면 응답에 `agent`(Antigravity는 `client: "antigravity"`도)를 더한다.

| 명령 | 하는 일 |
| --- | --- |
| `read_session` | 세션 하나의 회차(사용자·어시스턴트·도구)를 쪽으로 나눠 읽는다. 끝 자리(`before`)에서 거꾸로 도구가 아닌 회차를 `limit`개(기본 100, 최대 500) 센다. 글은 2,000자까지 싣고 `chars`는 전체 길이다. 주소(`urls=true`), 노트 id가 나온 회차(`note`), 글쓴이 노트(`author`, Claude만), 실행 상태(모델, 노력, 맥락 토큰, 할당량), 할 일, 아직 끝나지 않은 하위 에이전트를 함께 낸다 |
| `session_titles` | 세션 id 여럿(200개까지)의 제목, 브랜치, 처음 작업 폴더. Claude는 기록 파일의 머리·꼬리 64KB 창에서 `custom-title`, 폴더의 `custom-title.json`, `ai-title`, `gitBranch`, `cwd`를 규칙 순서대로 찾고, 다른 에이전트는 첫 사용자 글의 첫 줄을 제목으로 쓴다 |
| `recent_sessions` | 최근 `days`(1~30, 기본 7)일 안에 고쳐진 CLI 세션(머리에 `"entrypoint":"cli"`)을 최근 순으로 200개까지, 넘으면 `dropped`에 센다 |

| 형식 | 위치 | 읽는 법 |
| --- | --- | --- |
| Claude | `~/.claude/projects/<프로젝트>/<세션 id>.jsonl` | 줄마다 JSON. 사용자·어시스턴트 글, `tool_use`와 `tool_result`를 짝지어 걸린 시간과 실패를 낸다. `TaskCreate`·`TaskUpdate` 결과로 할 일 목록을 만든다 |
| Codex | `~/.codex/sessions/연/월/일/rollout-<시각>-<id>.jsonl` | 새 형식(`item_completed`)과 예전 형식(`user_message`, 함수 호출)을 한 번에 훑고 새 형식이 있으면 그것을 쓴다 |
| Gemini CLI | `~/.gemini/tmp/<프로젝트>/chats/session-<시각>-<id 앞 8자>.jsonl` | 첫 줄 머리의 `sessionId`를 맞대어 찾고, `$rewindTo`(되감기)와 CLI가 끼운 `<session_context>` 줄을 처리한다. 작업 폴더는 `~/.gemini/projects.json`을 거꾸로 찾는다 |
| Antigravity | `~/.gemini/antigravity-cli/conversations/<대화 id>.db` | SQLite의 `steps` 표를 시스템 `sqlite3`(읽기 전용)로 읽고, 단계 내용은 스키마 없이 protobuf 필드 번호를 짚어 읽는다(`session/protobuf.rs`) |

| 부품 | 자리 |
| --- | --- |
| 공용(회차 모양, 주소 모으기, 쪽 나누기, 줄 읽기) | `session/mod.rs` |
| 형식별 읽개 | `session/{claude,codex,gemini,antigravity,protobuf}.rs` |
| 명령 | `commands/session_commands.rs` |
| 화면 | `data/commands.ts`(`session_titles`의 자리 인자 여럿을 `ids` 배열로), `plugs/claude/askHost.ts`의 `engineReply`가 세션 화면, 세션 저장소, 이력 창이 부르는 세 명령을 Rust로 보낸다 |

기록 폴더는 환경 변수로 바꿀 수 있다: `AISURADA_CLAUDE_HOME`, `AISURADA_CODEX_HOME`, `AISURADA_GEMINI_HOME`(없으면 `HOME`). Java는 같은 이름의 시스템 속성(`aisurada.claude.home` 따위)이다. Antigravity의 `sqlite3` 자리는 `AISURADA_SQLITE_BIN`, `/usr/bin/sqlite3`, `PATH` 순서로 찾는다.

## 2. 결과

| 확인 | 내용 | 결과 |
| --- | --- | --- |
| Java와 맞대기 | 세션 50단계. 합성 홈에 네 형식의 기록을 만들고 두 엔진이 같은 글로 읽는지 견줬다. Claude(사용자·어시스턴트·도구 회차, 도구 결과 짝짓기와 걸린 시간·실패, `TaskCreate`·`TaskUpdate` 할 일, 아직 끝나지 않은 하위 에이전트, 주소 모으기와 로컬 주소 제외, 실행 상태와 할당량, 노트 언급과 글쓴이, 빈 줄·깨진 줄·`type` 없는 줄·CRLF·단독 CR, 2,000자 넘는 글과 이모지, 도구 인자 요약의 120자 자르기), 제목 규칙 다섯 가지와 폴더의 `custom-title.json`, 64KB를 넘는 파일의 머리·꼬리 창, 7일 밖 세션과 깨진 UTF-8 바이트, `recent_sessions`(CLI 입구만, 수정 시각 순, 기간), Codex 새·옛 형식, Gemini(주입된 줄, `$rewindTo`, 상태 없는 도구 호출, `projects.json`으로 작업 폴더), Antigravity(`sqlite3`로 만든 합성 DB: 사용자·모델·도구·MCP·빈 단계·시스템 단계), 쪽 나누기(`before`, `limit` 경계), 인자 오류 | 모든 단계 어긋남 0 |
| 비어 있지 않은 확인 | 합성 기록이 비어서 양쪽이 똑같이 빈 응답을 내는 일이 아닌지, 형식마다 회차가 일정 수 이상인지 Rust 사본에서 확인 | 5건 통과 |
| 앞 묶음 포함 | 쓰기 맞대기 전체 | **2,291건**(978단계, Java 성공 461단계) 어긋남 0, 합계 2,818건 |
| Rust 시험 | `cargo test -p ais-core` | 단위 97(protobuf 1 새로), 통합 24(새로 1: Claude·Codex·Gemini 기록 읽기와 인자 오류) 통과 |
| 정적 검사 | `cargo clippy --all-targets -D warnings`, `cargo fmt --check` | 통과 |
| 화면 시험 | `engine-route` 29개(1개 새로), `part4-ask-run` 17, `tasks-ui` 18, `resources-ui` 15, `approvals-ui` 15, `agent-session-chip` 15 등. `pnpm typecheck` | 모두 통과 |
| 호스트 시험 | 호스트 Rust 코드는 바꾸지 않았다 | 돌리지 않음 |

## 3. Java와 다르게 둔 것

- **깨진 UTF-8.** 읽을 때 대체 글자(U+FFFD)로 바꾸는 규칙은 같지만, 한 번에 몇 바이트를 한 글자로 바꾸는지는 Rust 표준 라이브러리 규칙을 따른다. 끊어진 다바이트 글자가 이어진 경우의 글자 수가 Java와 다를 수 있다.
- **UTF-16 자르기.** 글을 2,000자(UTF-16 단위)에서 자를 때 이모지가 경계에 걸리면 Java는 반쪽 글자를 남기고 Rust는 그 글자를 통째로 뺀다.
- **시각 읽기.** Claude 기록의 시각은 흔한 ISO 모양(`Z` 또는 `+09:00`)만 읽는다. Java `Instant.parse`가 받는 드문 모양은 읽을 수 없는 시각으로 본다.
- **정수 읽기.** 토큰 수, 마감 시각 같은 정수 칸은 ASCII 숫자만 정수로 읽는다.
- **`sqlite3` 호출.** 같은 방식으로(읽기 전용, 실패하면 immutable로 한 번 더) 부르지만, 주소 부호화와 시간 제한 처리는 Rust 방식이다. 해당 프로그램이 없으면 Antigravity 세션만 읽지 못하고 오류 글이 같다.
- **Codex 기록 찾기.** 같은 세션 id의 기록이 여러 날 폴더에 있으면 Java는 폴더를 도는 순서의 첫 것을, Rust는 이름순의 첫 것을 쓴다.
- 최근 세션은 현재 시각과 파일 수정 시각을 견주는 명령이라, 시험은 파일의 수정 시각을 현재 기준으로 맞춰 두 엔진에 같게 줬다.

## 4. 확인하지 못한 것과 남은 일

- **실제 앱 창에서 확인하지 못했다.** 손 확인 점검표에 세션 항목을 더했다(이슈 #54, #61). 실제 Claude·Codex·Gemini·Antigravity 기록은 쓰지 않았고 모두 합성 기록으로 견줬다(개인 대화가 시험 자료에 들어가지 않게 했다).
- 수십 MB짜리 실제 기록의 속도는 재지 않았다(이슈 #57). Rust는 파일을 통째로 읽어 줄로 나눈다.
- 다음은 4e(노트 잔여 명령 8개)와 5단계(JVM 제거)다.
