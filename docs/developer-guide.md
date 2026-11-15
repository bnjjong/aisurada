# aisurada 개발자 사용 가이드

이 문서는 저장소를 처음 받은 개발자가 빌드하고, 돌려 보고, 시험하고, 고치는 데 필요한 것을 모았다. 2026-11-15 판(`main` 브랜치, Java 엔진을 지운 뒤)을 기준으로 쓰고, 명령과 수치는 이 판에서 직접 확인한 것만 적었다. 읽어서만 확인한 것은 「확인 방법」을 함께 적었다.

> `docs/` 아래 기록은 이름을 바꾸기 전에 쓴 것이라 옛 이름이 그대로 적혀 있다. 읽다가 낯선 이름이 나오면 [§10의 대응표](#10-docs-읽는-법)로 바꿔 읽는다.

## 1. 구성 한눈에 보기

```
   화면(React, WKWebView)  ── Tauri IPC ──  호스트(Rust)  ── 같은 프로세스 안에서 호출 ──  엔진(Rust, ais-core) ── vault 폴더
        3부 · 4부 화면                       2부 · 4부 모듈                                 1부 · 4부 명령            (마크다운 + .aisurada/)
                                                                                                  ▲
                                              터미널·MCP 클라이언트 ── 실행 파일 ais-cli ──────────┘ (명령 1개를 처리하고 끝나는 프로세스)
```

| 부 | 무엇 | 어디 | 언어 |
|---|---|---|---|
| 1부 | vault를 읽고 쓰는 엔진 `ais-core`와 명령줄·MCP 서버 `ais-cli` | `engine-rs/crates/` | Rust |
| 2부 | Tauri 2 호스트(창, PTY, 파일, 프로세스 실행) | `native-host/src-tauri/` | Rust |
| 3부 | 화면(편집기, 목록, 검색, 이력, 설정, 터미널) | `native-host/src/` | React 19 + TypeScript |
| 4부 | Claude 연동(묻기, 세션 화면, 레일 대화) | 엔진 `ask`·`chat`·`session` 명령, 호스트 `ask.rs` 따위, 화면 `plugs/claude/` | 세 언어 모두 |

엔진은 색인 파일이 없다. 명령을 받을 때마다 vault 폴더를 훑는다. 화면은 호스트의 `engine_call`(명령 객체)이나 `sidecar_call`(명령줄 문자열 목록)로 같은 프로세스 안의 Rust 엔진을 부른다. 상주 엔진 프로세스도 소켓도 없다. 터미널이나 MCP 클라이언트(Claude, Codex 따위)는 실행 파일 `ais-cli`를 부른다.

예전 Java 엔진(`src/aisurada/`, SnakeYAML)은 저장소에서 지웠다. 태그 `java-engine-final`에 남아 있고, Rust 엔진과 답을 맞대는 기준(오라클)으로만 쓴다(§4).

## 2. 준비물

| 도구 | 버전 | 쓰임 |
|---|---|---|
| Rust / cargo | 안정판 | 엔진, `ais-cli`, 호스트 (확인한 판: 1.99.1) |
| Node.js | 22.12 이상 | 화면 빌드와 시험 (확인한 판: 26.9.0) |
| pnpm | 10.33.4 | `package.json`의 `packageManager`로 고정 |
| Xcode 명령줄 도구 | | Tauri 번들, WKWebView |
| git | | vault 자동 커밋과 이력이 실제 git을 부른다 |
| JDK | 21 이상 | 기준 jar를 지을 때만(`build-oracle.sh`, 맞대기 도구). 평소 빌드와 시험에는 필요 없다 |

macOS 전용이고 Apple Silicon만 지원한다. launchd(옛 사이드카 정리), WKWebView, AppKit(받아쓰기)을 쓰기 때문이다. 기준 jar를 지을 때 태그의 `build.sh`가 SnakeYAML 2.4를 Maven Central에서 받아 SHA-1을 확인한다. 저장소 `lib/`에 이미 받은 jar가 있으면(`lib/`는 git이 무시한다) 그것을 복사해 쓴다.

## 3. 처음 시작하기

```bash
# 1. 엔진과 클라이언트를 짓는다 (dist/ais-cli 하나가 생긴다)
./build.sh

# 2. 가장 작은 vault를 만든다 (임시 폴더에서 해 보자). vault는 환경 변수로 준다
export VAULT_DIR=$(mktemp -d)/vault && mkdir -p "$VAULT_DIR"
AISURADA_VAULT="$VAULT_DIR" dist/ais-cli init-workspace name=시험 'templates=[]'

# 3. 읽기와 쓰기 (아젠다 이름이 slug가 된다. 한글 이름이면 slug도 한글이다)
AISURADA_VAULT="$VAULT_DIR" dist/ais-cli create-agenda 'title=demo'
AISURADA_VAULT="$VAULT_DIR" dist/ais-cli create-doc agenda=demo type=free title=제목 body=본문
AISURADA_VAULT="$VAULT_DIR" dist/ais-cli list-docs limit=5
```

`AISURADA_VAULT`는 `export`하지 말고 명령 앞에 붙이는 편이 안전하다. `export`하면 같은 터미널에서 띄우는 앱도 그 값을 물려받아, 앱이 여는 vault 대신 이 vault를 쓴다.

`create-doc`은 `agenda=`, `type=`, `title=`, `body=`가 필요하다(`type=`을 빼면 `AIS-400`). 응답에는 `id`, `path`, 쓰기 전 토큰 `precondition`(`f1:…`)이 들어 있다.

`ais-cli`는 호출마다 엔진을 프로세스 안에서 돌리고 끝난다. 띄울 엔진도, 등록할 launchd 항목도 없다. 정리 끝낸 터미널에서 할 일은 `rm -r "$VAULT_DIR"`뿐이다.

산출물은 `dist/ais-cli` 하나다.

## 4. 저장소 구조

```
engine-rs/             엔진과 명령줄 (Rust 작업공간)
  crates/ais-core/     vault 규칙 라이브러리
  crates/ais-cli/      실행 파일 ais-cli (명령줄, MCP 서버, 등록, 진단)
  tools/compat/        기준 jar 짓기, 합성 vault 만들기
fixtures/              34장 받아들임 묶음과 실행기 (node)
native-host/
  src/                 화면 (React)
  src-tauri/           호스트 (Rust)
  scripts/             화면·호스트 시험, acceptance 하네스
docs/                  설명서 사본, 장별 완료 기록, 리뷰 기록 (§10)
build.sh               dist/ais-cli를 짓는다
test.sh                엔진 시험 전체 (cargo test)
test-native.sh         화면·호스트 시험 전체
```

### 엔진 (`engine-rs/`)

| 자리 | 내용 |
|---|---|
| `engine-rs/crates/ais-core/` | vault 규칙 라이브러리: `frontmatter`(노트 머리 YAML 읽기·쓰기), `vault`(아젠다·노트·후속 노트·프로젝트·템플릿·휴지통·코멘트·사실층 읽기와 훑기, 본문 글자 뽑기 `plain_text`, git 실행 `git`과 노트의 판 목록 `history`, 코멘트 id 겹침 검사 `comment_copy`), `kernel`(공용 커널: 호출자 `actor`, Java 글·수 규칙 `java`, 날짜·시각 `time`, id 모양 `ids`, 경로 가두기 `confine`, 값 객체 `doc_type`·`tier`(기록 층 가시성 정책)·`link_kind`·`task_link`·`hue`, 내용 확인 값 `token`. 다른 영역을 쓰지 않는다), `ports`(바깥에 바라는 일의 약속 `trait`: 시계 `Clock`, 난수 `IdSource`, 환경 변수 `EnvSource`, 쓰기 잠금 `VaultLock`, git `VersionControl`, sqlite3 `SqliteReader`와 묶음 `Ports`. `Engine::with_ports`로 바꿔 끼운다), `adapters`(포트를 해내는 바깥 코드: `system_clock`(시험 시계 `AISURADA_TEST_NOW`와 `TZ`), `queue_ids`(`AISURADA_TEST_RANDOM`), `process_env`, `flock_lock`(옛 `write/lock.rs`), `git_cli`, `sqlite_cli`. 시계·환경 변수·프로세스·시간대는 여기서만 직접 부른다), `write`(쓰기 공통 기반: `atomic`, `precondition`, `marker`, `apparatus`, `autocommit`, `pipeline`, `slug`, `agenda_file`, `project_file`, `template_file`, `warnings`, `note_front`, `meta_writer`, `snake_dump`), `task`(태스크), `resource`(스킬·에이전트 정의와 승인), `session`(에이전트 세션 읽기), `json`, `commands`(명령 실행기), `model`(응답 모양), `engine`(진입점 `Engine::call`) |
| `engine-rs/crates/ais-cli/` | `ais-cli`: `registry`·`spec_table`(이름과 명령 명세 표), `parse`(명령 표면)·`tool`(도구 표면) 인자 해석, `map`(엔진 호출로 바꿈), `legacy`(엔진 응답을 옛 응답 모양으로 바꿈), `output`(키를 정렬한 JSON 쓰기), `env`(vault 정하기, 세션 표식). `mcp`·`mcp_tools`(MCP 서버와 도구 표), `register`·`agent_cli`(`claude-mcp`·`codex-mcp`·`gemini-mcp`), `old_sidecar`·`doctor`·`install`·`migrate`(옛 사이드카 정리, 진단, 첫 실행 이주) |
| `engine-rs/crates/ais-cli/examples/` | `cli_compat`(기준 Java 단발 엔진과 명령줄 단위로 맞댐), `mcp_compat`(기준 Java MCP 서버와 JSON-RPC 대화를 맞댐) |
| `engine-rs/crates/ais-core/examples/` | `compat_check`(기준 Java 엔진과 읽기·쓰기 맞대기), `snake_check`(SnakeYAML 방출기 맞대기), `ais_call`(명령 하나를 불러 응답 보기), `yaml_check`·`yaml_fuzz_cases`(머리 읽기·쓰기 맞대기) |
| `engine-rs/crates/ais-core/tests/` | 합성 vault로 동작을 확인하는 통합 시험(Java 없이 돈다). `lock_interop.rs`는 `AISURADA_JAR`가 있을 때만 기준 `ais-engine.jar`와 쓰기 잠금을 맞대는 시험. 리팩토링 안전망: `architecture.rs`(모듈 사이 의존과 바깥 직접 호출을 `architecture-baseline.txt`에 박제), `serialization_golden.rs`(태스크·자원 기록장·승인 대기 변경·휴지통 목록의 파일 바이트를 `golden/serialization/`에 박제) |
| `engine-rs/crates/ais-cli/tests/` | `ais-cli` 프로세스·등록·정리·MCP 시험. 리팩토링 안전망: `golden.rs`(기준 엔진 없이 맞대기 재생), `command_tables.rs`(명령 정보를 따로 적은 표들의 교차 검사) |
| `engine-rs/golden/`, `engine-rs/tools/golden/` | 기준 Java 응답과 폴더 해시를 녹화한 테이프(`compat.ndjson`, `cli.ndjson`, `mcp.ndjson`)와 녹화·재생 스크립트 `run.sh` |
| `engine-rs/tools/compat/` | 기준 jar를 태그에서 짓는 `build-oracle.sh`, 명령 명세 표 생성기 `gen-spec-table.sh`(`DumpSpecs.java`), MCP 도구 표 생성기 `gen-mcp-tools.sh`(`DumpMcpTools.java`), MCP 맞대기용 Java 구동기 `DriveMcp.java`, 합성 vault 생성기 `make-fixture.sh`, 검색 시험 노트를 덧붙이는 `add-search-notes.sh`, git 저장소와 판 이력을 만드는 `add-git-history.sh`, 코멘트 id가 겹치는 노트를 더하는 `add-comment-clash.sh` |
| `engine-rs/tools/yaml-spike/` | 머리 읽기·쓰기를 Java 기준값과 맞대는 도구(`YamlGroundTruth.java`, `run.sh`, `cases.txt`) |

엔진을 Rust로 옮긴 결정과 단계는 [1단계 기록](rust-engine-step1-yaml.md), [2단계 새로고침 길 기록](rust-engine-step2-read.md), [2단계 노트 여는 길 기록](rust-engine-step2-open.md), [2단계 검색·판 목록 기록](rust-engine-step2-rest.md), [3단계 쓰기 이전 기록(W0~W4)](rust-engine-step3-w0.md), [4단계 계획](rust-engine-step4-plan.md), [4단계 4a 태스크](rust-engine-step4a-tasks.md), [4b 자원·승인](rust-engine-step4b-resources.md), [4c 문답·대화](rust-engine-step4c-ask-chat.md), [4d 세션 읽기](rust-engine-step4d-sessions.md), [4e 노트 도구](rust-engine-step4e-note-tools.md), [5단계 계획](rust-engine-step5-plan.md), [5a `ais-cli`](rust-engine-step5a-cli.md), [5b MCP·등록·정리](rust-engine-step5b-mcp.md), [5c 호스트·번들](rust-engine-step5c-host.md), [5d Java 지우기](rust-engine-step5d-java-removal.md)에 있다. 3단계의 나머지 기록은 [W1a](rust-engine-step3-w1a.md), [W2](rust-engine-step3-w2.md), [W3a](rust-engine-step3-w3a.md), [W3b](rust-engine-step3-w3b.md), [W3c](rust-engine-step3-w3c.md), [W4a](rust-engine-step3-w4a.md), [W4b](rust-engine-step3-w4b.md)다.

**앱의 호출 길.** 화면은 `workspace/engine.ts`의 `engineCall` 한 곳으로 엔진을 부른다. 호출 객체(`Call`, `data/commands.ts`의 `read.docs()` 따위)는 호스트의 `engine_commands`가 알린 명령이면 `engine_call`(`src-tauri/src/engine.rs`)로 앱 안의 Rust 엔진을 바로 부르고, 아니면 `toArgv`로 문자열 목록(argv)으로 풀어 명령줄 길(`sidecar_call`, `src-tauri/src/sidecar.rs`)로 간다. 문자열 목록과 러너·승인 알림(`call_blocking`)은 늘 명령줄 길이고, 이 길은 같은 프로세스 안에서 Rust `ais-cli`(`ais_cli::run`)를 부른다. 프로세스를 띄우지 않고, 엔진 안의 패닉은 종료 코드 1의 답으로 바꿔 앱이 죽지 않게 한다. AI 세션 표식(`CLAUDE_CODE_SESSION_ID` 따위)은 엔진에 넘기기 전에 지운다(#53). Rust 응답은 `engine.ts`의 어댑터가 기존 화면 모양으로 바꾼다. 앱을 열 때 vault를 `ais_core::write::autocommit::watch`로 지켜봐서, 앱이 닫힌 동안 단발 `ais-cli`가 남긴 자동 커밋 대기 기록(`.git/aisurada-pending.json`)을 이어받아 커밋한다. 시험과 브라우저의 가짜 호스트에는 `engineCommands`가 없어서 늘 명령줄 길이다. **첫 실행의 이주:** 정식 앱은 처음 뜰 때 한 번(`~/Library/Application Support/aisurada/rust-engine-migrated` 표시) 옛 사이드카(우리 레이블 `co.kcd.aisurada.sidecar.*`만)를 내리고 plist를 지우며, Java 시절의 jar를 가리키는 낡은 MCP 등록을 같은 모드로 다시 등록한다(`ais_cli::migrate`, 같은 일을 손으로 하려면 `ais-cli doctor --fix`). 개발 실행과 빌드 폴더의 번들은 기기 전체에 영향을 주는 이 일을 하지 않는다.

```bash
(cd engine-rs && cargo test -p ais-core)                                              # 엔진 단위·통합 시험
pnpm --dir native-host exec node --import tsx --test scripts/engine-route.test.mjs    # 화면의 라우팅과 어댑터
cargo test --manifest-path native-host/src-tauri/Cargo.toml --locked engine::         # 호스트 engine_call (실제 Tauri 호출 경로)
```

**기준(오라클)과 맞대기.** 맞대기는 같은 입력을 기준 Java 엔진과 Rust 엔진에 넣어 응답과 폴더 바이트를 견주는 일이다. 기준 jar는 저장소에 없고 태그 `java-engine-final`에서 짓는다. 태그가 없으면 `git fetch origin tag java-engine-final`로 먼저 받는다. `compat_check`는 읽기를 견줄 때 vault를 고치지 않고, 응답을 새 모양으로 바꿔 견주며, 어긋남이 0이어야 한다.

```bash
ORACLE="$(bash engine-rs/tools/compat/build-oracle.sh)"   # engine-rs/target/oracle/. 같은 태그 커밋이면 다시 짓지 않는다
engine-rs/tools/compat/make-fixture.sh <빈 폴더>        # 합성 vault(진짜 vault 내용 없음)
engine-rs/tools/compat/add-search-notes.sh <그 폴더>    # 검색 순위·서식·동점 시험 노트를 덧붙인다
engine-rs/tools/compat/add-git-history.sh <그 폴더>     # git 저장소와 판 이력(`history` 시험용). 작업 트리는 그대로 돌아온다
engine-rs/tools/compat/add-comment-clash.sh <그 폴더>    # 코멘트 id가 겹치는 노트(읽기는 뒤 노트를 빼고 쓰기·reindex는 실패)
(cd engine-rs && cargo run -q -p ais-core --example compat_check -- "$ORACLE/ais-engine.jar" <합성 vault> [다른 vault...])
# 쓰기 명령은 합성 vault의 사본 둘에 같은 명령열을 Java와 Rust로 돌려 응답과 폴더 전체 바이트를 견준다(vault 원본은 안 고친다)
(cd engine-rs && cargo run -q -p ais-core --example compat_check -- "$ORACLE/ais-engine.jar" <합성 vault> --write-fixture=<합성 vault>)
# 자동 커밋(commit_vault)은 vault를 고치므로 git 이력이 있는 합성 vault의 사본 둘에서만 맞댄다. 진짜 vault에는 주지 않는다
(cd engine-rs && cargo run -q -p ais-core --example compat_check -- "$ORACLE/ais-engine.jar" <합성 vault> --commit-fixture=<git 이력이 있는 합성 vault>)
# SnakeYAML 방출기(태스크·자원 파일)는 진짜 SnakeYAML 2.4가 쓴 사례와 맞댄다(자세한 절차는 tools/compat/SnakeCases.java 머리말)
(cd engine-rs && cargo run -q -p ais-core --example snake_check -- <SnakeCases가 쓴 사례 파일>)
AISURADA_JAR="$ORACLE/ais-engine.jar" cargo test --manifest-path engine-rs/Cargo.toml -p ais-core --test lock_interop   # Java 쓰기가 Rust 잠금을 기다리는지
```

Rust `ais-cli`를 기준 Java 단발 엔진과 명령줄 단위로 맞대려면 아래를 돌린다. 합성 vault의 사본 둘에 같은 명령줄을 돌려 종료 코드, 표준 오류, 표준 출력 JSON(키 차례는 무시), 폴더 전체 바이트를 견주며 어긋남이 0이어야 한다. 명령 명세 표(`spec_table.rs`)는 Java 엔진을 지운 뒤로 원본이라 직접 고친다. `engine-rs/tools/compat/gen-spec-table.sh`는 기준 표를 `engine-rs/target/oracle-tables/`에 뽑으므로 현재 표와 `diff`로 견주는 데 쓴다(5d에서 둘이 같음을 확인했다).

```bash
(cd engine-rs && cargo run -q -p ais-cli --example cli_compat -- "$ORACLE/ais-engine.jar" <합성 vault>)
(cd engine-rs && cargo run -q -p ais-cli --example cli_compat -- "$ORACLE/ais-engine.jar" <git 이력이 있는 합성 vault> --history)   # history·commit-vault까지
(cd engine-rs && cargo test -p ais-cli)                                                                                            # 단위·프로세스·등록·정리 시험
(cd engine-rs && cargo run -q -p ais-cli --example mcp_compat -- "$ORACLE")                                                        # 기준 MCP 서버와 대화 맞대기(기준 폴더에 ais-cli.jar와 ais-engine.jar)
```

MCP 도구 표(`mcp_tools.rs`)도 마찬가지로 직접 고치고, `engine-rs/tools/compat/gen-mcp-tools.sh`가 기준 표를 같은 자리에 뽑는다(현재 표와 같음을 확인했다).

**골든 시험: 기준 엔진 없이 맞대기.** 위 맞대기 셋(`compat_check`, `cli_compat`, `mcp_compat`)은 태그에서 기준 엔진을 지어야(JDK 21) 돈다. 어긋남 0을 확인한 순간의 기준(Java) 응답과 폴더 파일별 해시를 테이프(`engine-rs/golden/*.ndjson`)에 녹화해 두면, 같은 하네스를 Java 없이 다시 돌려 지금 Rust 엔진이 그때와 같은 답과 같은 폴더 바이트를 내는지 볼 수 있다. 리팩토링([계획](refactor-ddd-hexagonal-plan.md))이 동작을 바꾸지 않았다는 증거로 쓴다. 하네스는 시계와 난수를 고정해 결정적이라서 호출 순서가 곧 테이프의 열쇠이고, 재생 때 호출 인자가 같은지도 확인한다. 임시 폴더 경로는 자리표시자로 적고 재생 때 지금 경로로 되돌린다(`ais-core/examples/support/tape.rs`).

```bash
engine-rs/tools/golden/run.sh replay   # Java 없이 셋을 녹화본과 견준다(`cargo test -p ais-cli --test golden`이 같은 일을 한다)
engine-rs/tools/golden/run.sh record   # 기준 Java(JDK 21)와 어긋남 0일 때만 테이프를 새로 쓴다. 동작을 알고 바꿨을 때만 하고, 이유를 커밋 본문에 적는다
```

`record`는 어긋남이 하나라도 있으면 테이프를 쓰지 않는다(어긋난 응답을 정답으로 굳히지 않는다). 승인 대기 변경 파일(`.aisurada/pending/*.json`)은 맞대기가 키 차례를 같게 보므로 테이프도 읽은 값을 정렬해 센다. 이 파일의 바이트 자체는 `serialization_golden.rs`가 못박는다. 시험이 의존 방향을 지키는지(`architecture.rs`)와 명령 정보를 적은 표들이 어긋나지 않는지(`command_tables.rs`)도 Java 없이 돈다. 둘 다 지금 모양을 기준으로 박제해 새 위반이 생기거나 줄어도 실패하며, 기준을 새로 쓰는 법은 각 파일 머리말에 있다.

노트 머리 읽기·쓰기를 Java와 맞대려면 기준값은 노트 머리가 들어가므로 저장소 밖 폴더에 뽑는다.

```bash
engine-rs/tools/yaml-spike/run.sh <저장소 밖 폴더> --vault <vault 폴더>     # Java 기준값. vault는 읽기만 한다
(cd engine-rs && cargo run -q -p ais-core --example yaml_check -- <저장소 밖 폴더>/ground-truth.jsonl)
```

퍼징 사례는 `cargo run -q -p ais-core --example yaml_fuzz_cases -- <ground-truth.jsonl> <건수> <시드> > 사례.txt`로 만들고, `run.sh <다른 폴더> --cases 사례.txt`로 기준값을 다시 뽑아 같은 `yaml_check`에 넣는다. `yaml_check`의 「실패」 열이 0이어야 하고, 「경고」는 Rust가 Java보다 너그럽게 읽은 입력이다.

### 호스트 모듈 (`native-host/src-tauri/src/`)

`workspace.rs`(vault 허용·만들기), `github.rs`(GitHub App 연결, 아래 「GitHub App 연결」), `sidecar.rs`(`sidecar_call`), `files.rs`(그림 첨부 쓰기, 기본 앱 열기), `markdown.rs`(외부 마크다운), `notes.rs`(노트 창), `terminal.rs`(PTY), `secrets.rs`(비밀 가림), `menu.rs`, `layout.rs`(창 자리), `frame.rs`(`aisframe://`), `self_write.rs`, `process.rs`(자식 실행과 시간 상한), `ipc.rs`(이벤트 이름). `notifications.rs`(알림, 아래 「알림」), `approvals.rs`(승인 대기 알림, 아래 「승인 워크플로」). 4부는 `ask.rs`, `agents.rs`, `session_image.rs`, `dictation.rs`, `session_shell.rs`이고, 읽기 전용 러너 `runner/`(아래 「러너」)도 같이 cargo 기능 `claude` 아래에 있다.

### 화면 모듈 (`native-host/src/`)

`workspace/`(controller, 부팅, 저장 키, 엔진 다리), `data/`(vault 스토어, 쓰기 큐, 자기 쓰기), `editor/`(Lexical 편집기와 마크다운 왕복), `navigation/`(주소, 탭, 스크롤), `interaction/`(단축키, 링크), `ui/`(화면들, 터미널), `plugs/claude/`(4부 화면)와 `plugs/empty/`(4부 없는 판의 빈 구현). 화면 모양 CSS는 `src/styles.css`(기본 모양)와 `src/skin-*.css`(물성·격자·유리·모던)다(§7 「화면 모양」).

## 5. 엔진 쓰는 법

> 아래 기능 절의 「시험」 항목에 적힌 Java 시험 클래스(`AgentSessionTest`, `McpServerTest`, `ApprovalsTest`, `ResourcesTest`, `TasksTest`, `ProjectReposTest` 따위)는 Java 엔진과 함께 저장소에서 지웠다. 그 내용은 태그 `java-engine-final`의 `test/`에서 볼 수 있고, 같은 동작을 지키는 일은 지금 Rust 시험(`engine-rs/crates/*/tests/`)과 기준 맞대기(§4)가 맡는다.

### 세션 에이전트(Codex·Gemini) 확장

노트에 적는 세션 id는 원래 Claude Code 것만 있었다. 이 확장은 Codex와 Gemini CLI의 세션도 같은 자리에 적고, 노트 칩에서 터미널로 이어 열고, 두 CLI의 대화 기록을 세션 화면에서 읽게 한다. **외부 설명서에 없는 확장이다.** 설명서의 4.4(세션 표식), 1.7·1.10(머리말 키와 도장), 7장(`read-session`, `session-titles`)과 어긋나는 곳은 아래 표가 전부다. 이 표 밖에서는 설명서대로 동작해야 한다.

| 곳 | 설명서 | 확장 |
|---|---|---|
| 노트 머리말 | `sessionId`, `editedSessionId` | claude가 아닌 세션이면 바로 뒤에 `sessionAgent`, `editedSessionAgent`를 적는다. 줄이 없으면 claude라서 기존 노트와 Claude 호출의 파일은 바이트가 같다. |
| 소켓 표식(4.4) | 끝 2칸 `\0ais-session-id <id>` | claude가 아니면 그 뒤에 `\0ais-session-agent <이름>` 2칸이 더 온다. 인자 한도 64칸에는 이 2칸도 센다. |
| 세션 출처 | `CLAUDE_CODE_SESSION_ID`만 읽는다 | `SessionAgents`가 정한다. MCP 요청의 Codex `_meta.sessionId`(클라이언트 이름이 codex일 때만), `AISURADA_SESSION_ID`+`AISURADA_SESSION_AGENT`, 에이전트 고유 변수 차례다. 고유 변수가 둘 이상 보이면 기록하지 않는다. |
| `read-session`, `session-titles` | Claude 기록(`~/.claude/projects`)만 | Claude 기록이 없으면 Codex(`~/.codex/sessions`), Gemini CLI(`~/.gemini/tmp`), Antigravity(`~/.gemini/antigravity-cli/conversations`) 기록을 세션 id로 찾는다. Gemini CLI와 Antigravity는 둘 다 `agent`가 `gemini`다. 찾았을 때만 응답에 `agent`가 붙고, Antigravity 기록이면 `client: "antigravity"`도 붙는다(Gemini CLI와 Codex에는 `client` 키가 없다). |

- **기록 형식.** Codex는 `event_msg`의 `item_completed`(새 형식)가 있으면 그것만, 없으면 `user_message`/`agent_message`(예전 형식)를 읽는다(`CodexTranscript`). Gemini는 `chats/session-*.jsonl`의 `user`/`gemini` 줄에서 환경 설명 줄을 빼고 `toolCalls`를 풀며 `$rewindTo`를 지킨다. 작업 폴더는 `~/.gemini/projects.json`을 거꾸로 찾는다(`GeminiTranscript`). Antigravity(`agy`)는 대화 하나가 `<대화 id>.db`(SQLite)이고, `steps` 표의 단계 내용이 protobuf라서 `Protobuf`가 필요한 필드만 읽는다(`AntigravityTranscript`). 사용자 입력(`step_type` 14), 모델 답(15), 도구 호출(132)만 회차로 만들고, 시스템 알림(101)·이어쓰기 요약(23)·API 오류(17)와, 오래된 단계라 내용이 비워진 단계는 건너뛴다. 엔진에 SQLite 라이브러리가 없어서 시스템 `sqlite3`(`/usr/bin/sqlite3`, 다른 곳이면 `AISURADA_SQLITE_BIN`)를 `-readonly`로 부르고, 열리지 않으면 `immutable=1`로 한 번 더 연다. 도구 결과는 가져오지 않고 단계마다 앞부분만 SQL로 잘라 받는다. 홈 폴더는 시험용으로 `aisurada.codex.home`, `aisurada.gemini.home`로 바꿀 수 있고, `sqlite3` 자리는 `aisurada.sqlite.bin`으로 바꾼다.
- **래퍼.** Gemini CLI는 셸 도구와 MCP 서버에 세션 id를 주지 않는다. `bin/ais-gemini`가 실행 전에 UUID를 만들어 `--session-id`로 넘기고 같은 값을 변수로 내보낸다. Codex는 셸 도구에 `CODEX_SESSION_ID`/`CODEX_THREAD_ID`를 주고 MCP 호출 요청에 `_meta`를 실어서 래퍼가 필요 없지만, Claude 셸에서 띄우면 `CLAUDE_CODE_SESSION_ID`가 남아 어느 쪽인지 가릴 수 없다. `bin/ais-codex`가 그것을 지운다.
- **화면.** 칩은 세션을 연 CLI의 이름(`Claude Code`, `Codex`, `Gemini CLI`)을 늘 보인다. Claude는 파일에 `sessionAgent` 줄이 없어도 「Claude Code」로 보인다. 전체 노트 목록, 아젠다 페이지(`AgendaPage`)의 노트 목록, 캘린더 주 보기의 목록 모드(날짜 열·아젠다 레인)의 각 줄에도 같은 규칙으로 출처 아이콘이 붙는다(`AgentMark.tsx`의 `noteAgents`, 줄에 그리는 것은 `AgentMarks`). 만든 세션이 있으면 그 CLI, 고친 세션이 다른 CLI면 아이콘이 하나 더, 같은 CLI면 하나로 합친다. 세션 기록이 없는 노트에는 아이콘이 없다. Claude는 주황 불꽃, Gemini는 별, Codex와 모르는 CLI는 터미널 모양이고, 마우스를 올리면 「Gemini CLI가 작성」처럼 CLI와 역할이 보인다. 주·일 시간표 블록과 월 보기 칸, 아젠다 페이지의 히스토리 목록은 칸이 좁거나 줄 모양이 달라 붙이지 않았다. 한 줄 말줄임 칩(`mchip`)에서는 긴 제목에 잘리지 않게 아이콘을 글자 앞에 둔다. 네이티브 앱에서는 "터미널" 단추로 `/terminal/<세션 id>`를 연다. 터미널 계획(`terminal.ts`)은 `session-titles`가 알려 준 `agent`로 호스트에 `agent`를 넘기고, 호스트(`session_shell.rs`)는 `claude --resume`, `codex resume`, `gemini --resume` 가운데 고른다. Antigravity 대화는 노트에 Gemini와 같은 `gemini`로 적히지만 `gemini --resume`은 Gemini CLI의 세션(`~/.gemini/tmp/<프로젝트>/chats`)만 찾아서 "Invalid session identifier"로 실패한다. 그래서 엔진이 Antigravity 기록에만 `session-titles`와 `read-session` 응답에 `client: "antigravity"`를 더 붙이고(`AgentTranscripts.clientOf`), 터미널 계획은 `launchNameOf`로 `client`가 있으면 그것을 `agent`로 넘기며, 호스트는 `agy --conversation <대화 id>`로 연다. 세션 변수의 에이전트는 노트와 맞게 계속 `gemini`다. 이어 열기는 항상 세션의 작업 폴더(`session-titles`의 `cwd`)에서 하고, 그 폴더가 없으면 홈에서 열면서 알린다. 모르는 이름은 셸 앞에서 거절한다.
- **한계.** 세션 목록(프로젝트별 화면, `recent-sessions`)에는 Codex·Gemini 세션이 나오지 않는다. 노트 칩으로 들어간 세션 화면은 읽기 전용이고 입력창을 닫는다. Codex·Gemini·Antigravity 기록에서는 `author=`(작성 세션 대화를 묻기에 첨부)와 하위 에이전트·할 일 표시가 없다. Antigravity 기록은 `sqlite3`가 없으면 읽지 못한 것으로 보고한다(`session-titles`는 `found: false`). `FrontmatterWriter`가 정해진 필드만 쓰는 구조라서, `sessionAgent`를 모르는 옛 빌드가 노트를 다시 쓰면 그 줄이 지워질 수 있다(옛 빌드로 직접 확인하지는 않았다).
- **시험.** 엔진은 `AgentSessionTest`(파일 형식, 도장, 환경변수 해석, 세 CLI의 가짜 기록. Antigravity는 `sqlite3`로 진짜 모양의 SQLite와 protobuf를 지어 읽고, `sqlite3`가 없으면 그 부분만 건너뛴다)와 `McpServerTest`의 `sessionFromMeta`, 화면은 `chapter10.test.mjs`와 `agent-session-chip.test.mjs`다(목록 아이콘의 판정 `noteAgents`도 뒤쪽 시험이 함께 본다). 목록 아이콘의 모양은 가짜 목록 줄과 칩을 Edge headless로 그려 보았고, 실제 앱 창의 전체 노트·아젠다·캘린더 화면에서는 확인하지 않았다. 실제 Codex·Gemini 기록은 읽기 전용으로 엔진 명령에 먹여 확인했다. 실제 Antigravity 기록(1MB~72MB, 2236회차)은 `AgentTranscripts`를 자바로 직접 불러 읽기 전용으로 읽어 보았다(그 뒤 설치한 앱의 엔진 명령 `session-titles`, `read-session`으로도 읽기 전용으로 읽어 보았다). 실제 앱 창에서는 Antigravity 칩의 「터미널」 단추로 대화가 이어 열리는 것만 손으로 확인했고(2026-11-13), 나머지 칩 동작은 앱 창에서 확인하지 않았다.

### 세션 목록 저장소 (PROJECTS 패널은 없앴다)

왼쪽 레일에 있던 **PROJECTS 패널(Claude 세션 목록)을 없앴다.** 이름과 달리 프로젝트가 아니라 세션을 프로젝트·아젠다 밑에 늘어놓는 패널이었고, 세션을 아젠다에 붙이려면 아젠다의 저장소 폴더가 필요해서 비어 보이기 쉬웠다. 노트의 세션 칩, 노트 옆 세션 레일, 세션 화면(`/session/<id>`), 터미널로 이어 열기는 그대로다.

- **저장소는 남았다.** 세션 화면, 노트 옆 세션 레일, 터미널 이어 열기가 같이 쓰는 세션 목록 읽기(`list_claude_agents`와 엔진의 `session-titles`)는 `plugs/claude/sessionStore.ts`가 한다(옛 `projects.tsx`에서 옮겼다). 훅 이름은 `useProjectsList`에서 `useSessionList`로 바꿨다.
- **새로고침.** 패널이 하던 「창이 눈에 띄면 다시 읽기」는 목록을 실제로 쓰는 화면으로 옮겼다. `useSessionListSync`가 한 번도 못 읽었으면 읽고, 포커스와 보이게 됨에 다시 읽는다(세션 화면과 노트 옆 세션 레일이 쓴다). 터미널 이어 열기는 찾기 전에 직접 읽는다.
- **없앤 것.** `RailProjects`, 세션을 아젠다에 붙이는 규칙 `matchAgenda`와 그 시험, 4부 없는 판의 빈 구현 `plugs/empty/projects.tsx`, `styles.css`와 `rail-night.css`의 패널 전용 규칙.
- **일부러 남긴 것.** 저장 설정 `expandedSessionProjects`는 쓰는 곳이 없어졌지만 필드를 그대로 둔다. 16장이 설정을 정확히 21개로 정해 두었고 시험(`chapters14-17`)이 그 개수를 센다. 스킨 CSS(`skin-*.css`)의 공유 선택자 목록에 든 `.sess-toggle` 같은 이름도 해가 없어 그대로 두었다.
- **세션 화면의 입구.** 이제 노트의 세션 칩(「열기」로 노트 옆 레일을 열고 거기서 화면으로)뿐이다. 노트가 없는 세션에 들어갈 화면 안의 길은 없다.

### 프로젝트의 저장소와 아젠다의 `projectRepo`

프로젝트가 저장소(로컬 폴더, GitHub 저장소, 또는 둘 다)를 여러 개 가지고, 아젠다가 그중 하나를 이름으로 가리킨다. 나중에 GitHub App 연동과 앱 안의 개발 기능이 이 위에 올라간다. **외부 설명서에 없는 확장이다.** 설명서 21.5는 프로젝트 파일의 키를 4개로 못 박아 두었고(조건부 키 없음), 20.5는 아젠다의 키를 6개로 정했다.

- **프로젝트 파일.** 저장소가 있으면 `hue` 다음에 `repos:` 한 줄이 붙는다. 값은 한 줄짜리 JSON 배열이다: `repos: [{"name":"app","path":"/Users/me/app","github":"me/app","githubId":123}]`. 항목은 `name`(필수, 프로젝트 안에서 유일, 50자까지, 빗금 없음), `path`(절대 경로), `github`(`owner/name`), `githubId`(양의 정수, `github`와 함께)이고 `path`나 `github` 가운데 하나는 있어야 한다. 최대 20개. **저장소가 없으면 파일은 설명서의 4줄 그대로다.**
- **왜 한 줄인가.** 프로젝트와 아젠다 파일은 설명서 2.1의 줄 읽개로 읽는다. 이 읽개는 줄 맨 앞의 `키: 값`만 보고 들여쓴 줄은 건너뛰므로, 들여쓴 YAML 목록은 읽개에 안 보인다(`repos:`가 빈 값으로 읽힌다). JSON은 YAML이기도 해서 일반 YAML 도구도 읽는다.
- **아젠다 파일.** `project` 바로 뒤에 `projectRepo: <저장소 이름>`이 붙는다. 프로젝트가 없는 아젠다는 늘 없는 것으로 읽는다. 기존 `repo`(로컬 경로, 자유 텍스트)는 그대로이고 프로젝트가 없는 아젠다는 지금처럼 이것만 쓴다.
- **명령.** `create-project`와 `update-project`의 `repos=<JSON 배열>`(update는 통째로 바꾸고 빈 값이나 `[]`는 지움, 안 주면 그대로), `create-agenda`와 `update-agenda`의 `projectRepo=<이름>`. 틀린 값은 파일을 쓰기 전에 AIS-400으로 거절한다. `projectRepo`는 `project`가 있고 그 프로젝트의 `repos`에 그 이름이 있어야 한다. `update-agenda`로 프로젝트를 바꾸거나 미분류로 보내면 낡은 가리킴을 같이 지우고, 안 건드리면 그대로 둔다.
- **읽을 때는 너그럽다.** 손으로 고친 `repos:`에서 못 읽는 항목과 겹친 이름은 건너뛰고, 줄 전체가 JSON 배열이 아니면 없는 것이다. 프로젝트가 저장소를 지워도 아젠다 파일은 건드리지 않아 낡은 이름이 남을 수 있고, 그 판단은 쓰는 쪽(화면)이 한다.
- **응답.** `list-projects`와 `create/update-project`는 저장소가 있을 때만 `repos` 배열을, `list-agendas`와 `create/update-agenda`는 가리킬 때만 `projectRepo`를 싣는다. 없으면 설명서의 모양 그대로다.
- **로컬 폴더가 없는 GitHub 항목.** `path`가 없으면 앱이 정한 폴더(`~/Library/Application Support/aisurada/repos/<소유자>/<이름>`)에 받아 쓴다. 엔진은 이 폴더를 알지 못한다. 받는 일과 GitHub 연결은 호스트(아래 「GitHub App 연결」)가 하고, 폴더를 풀어 쓰는 일은 화면(`repoLinks.ts`)이 한다.
- **화면.** 프로젝트 폼(`Forms.tsx`의 `EntityForm`)에 저장소 목록 편집기(`RepoListEditor.tsx`)가 있다. 항목마다 이름, 폴더(「폴더 고르기」 대화상자), GitHub `owner/name`(연결했으면 「내 저장소에서 고르기」)을 적는다. 저장하기 전에 `validateRepos`가 엔진과 같은 규칙으로 막고, 바뀐 것만 `repos=`로 보낸다(`reposField`: 항목을 모두 지우면 빈 값을 보내 줄을 지운다). 아젠다 폼은 고른 프로젝트에 저장소가 있으면 「프로젝트 저장소」 선택을 보이고, 고르면 옛 「저장소」 글 칸은 숨긴다(옛 `repo` 값은 그대로 둔다). 프로젝트를 바꾸면 선택은 비워진다. 프로젝트가 없는 아젠다와 저장소가 없는 프로젝트의 아젠다는 옛 「저장소」 글 칸만 쓴다.
- **안내 팝업.** 편집기의 「왜 필요해요?」가 `RepoGuide.tsx`를 연다. 글이 아니라 **그림 3장을 한 장씩 넘겨 보는** 팝업이고(제목과 설명은 장마다 한 줄, 시험이 글자 수 상한을 지킨다), 그림은 인라인 SVG다. 1장은 프로젝트의 저장소 목록에서 아젠다가 하나를 고르면 아젠다 화면에 그 저장소가 표시되는 흐름과 점선의 「앞으로: 앱 안 개발」, 2장은 이 컴퓨터의 폴더(그대로 사용, 복사 안 함)와 GitHub 저장소(앱이 앱 전용 폴더에 받음, 연결 필요)의 두 방식과 지금 연결 상태·받는 폴더, 3장은 개인 키는 키체인에만, 읽기와 받기만 함, 빼도 폴더는 그대로, 목록은 프로젝트 파일에 저장이다. 색은 모두 앱의 색 변수(`.rg-*` 클래스)에서 와서 화면 모양과 어두운 모드를 따른다. **그림에 적힌 말은 이 문서의 동작 설명을 그대로 옮긴 것이라, 저장 위치나 받는 규칙이나 GitHub에 하는 일이 바뀌면 그림과 시험의 글(`repo-links.test.mjs`의 `GUIDE_WORDS`)을 같이 고친다.** 팝업은 `Modal`(포털)이라 서버 렌더 시험으로 열 수 없어, 시험은 그림 3장의 글과 설명, 글자 수 상한, 편집기의 단추만 본다. 모양은 앱 CSS를 입혀 따로 렌더해서 눈으로 확인한다.
- **아젠다가 쓰는 폴더.** `agendaRepoView`가 푼다: `projectRepo`가 프로젝트의 항목을 가리키면 그 `path`, 폴더 없는 GitHub 항목이면 `<받는 폴더>/<owner>/<name>`, 가리킨 이름이 프로젝트에서 사라졌으면 옛 `repo` 글로 물러나고 아젠다 화면이 「…는 프로젝트에서 빠졌어요」를 덧붙인다. 아젠다 화면의 저장소 글(`AgendaRepoLabel`)이 이 결과를 쓴다. 세션을 아젠다에 붙이는 규칙(`matchAgenda`)은 PROJECTS 패널과 함께 없앴다(아래 「세션 목록 저장소」).
- **한계.** `repos`와 `projectRepo`를 모르는 옛 빌드가 프로젝트나 아젠다를 다시 쓰면 그 줄이 지워진다(두 파일을 통째로 다시 쓰고 모르는 키는 버리는 지금 규칙).
- **시험.** `ProjectReposTest`(파일 형식, 응답, 틀린 값 거절, 손으로 고친 파일, 아젠다의 가리킴 규칙).

### 프로젝트 자원: 스킬과 에이전트 정의 (`resource/`)

프로젝트에서 AI에게 작업을 시키기 위한 바탕으로, **스킬**(`SKILL.md`가 든 폴더. AI에게 일하는 법을 알려 주는 지침)과 **에이전트 정의**(이름, 설명, 지침, 쓸 스킬, 권한을 적은 글 하나)를 vault에 보관하고 관리한다. **외부 설명서에 없는 확장이다.** 지금은 엔진의 명령줄 표면, MCP 도구, 화면(글로벌 페이지와 프로젝트 페이지)이 있고 작업 실행기는 아직 없다. 진행 계획은 이슈 [#53](https://github.com/bnjjong/aisurada/issues/53)(AI가 쓴 변경의 승인 워크플로우)과 같이 본다.

- **규칙 4개(사용자가 확정).** ① 스킬과 에이전트는 특정 AI 도구에 종속되지 않는다(중립한 형식이고, 도구별 차이는 선택 칸 `tools`에만 둔다). ② 자원은 글로벌과 프로젝트 두 범위로 관리한다. ③ 프로젝트는 글로벌 자원을 **ID로 참조**하고, 참조한 것만 그 프로젝트에서 쓸 수 있다. ④ 참조하지 않은 것은 쓰지 않는다.
- **위치.** `.aisurada/resources/` 아래다. 설명서가 정한 `notes/`와 `.aisurada/projects/` 등을 건드리지 않는 새 영역이고, 노트를 훑는 쓰기 명령과 기존 명령이 이 영역을 보지 않는다(시험으로 확인).
  `global/{skills/<slug>/SKILL.md, agents/<slug>.md}`(글로벌), `projects/<프로젝트 slug>/{skills/…, agents/…, refs.md}`(프로젝트 자원과 참조 목록), `.trash/<id>/`(지운 자원), `.history/<id>/`(고치기 전의 판). 범위는 파일 위치로 정해진다. 프로젝트 slug는 만든 뒤 바뀌지 않아서(`update-project`에 slug 인자가 없다) 프로젝트별 폴더가 어긋나지 않는다.
- **ID.** 노트·아젠다와 같은 형식(머리 + 36진수 시각 + `-` + 16진수 8자리)이고 머리가 종류를 알려 준다: 스킬 `sk…`, 에이전트 정의 `ap…`. ID는 머리말 안에 있어서 **이름을 바꿔도 참조가 안 끊긴다**(바꾸면 폴더·파일이 옮겨진다). 참조는 이름이 아니라 ID로 건다.
- **파일 모양.** `---` 머리말(YAML)과 지침 본문이다. 공통은 `id`, `name`, `description`, 에이전트만 `permission`(`read`, `edit`, `publish` 3단계, 모르는 값은 가장 약한 `read`로 읽는다), `skills`(스킬 ID 목록), `repo`(프로젝트 에이전트가 쓸 저장소 이름), 선택 칸 `tools`, 기록 칸 `editedAt`, `editedSession`, `editedAgent`다. 본문은 파일에서 끝에 줄바꿈 하나가 붙은 채 적히고 읽을 때 그 하나를 뗀다. 같은 값이면 늘 같은 바이트다.
- **도구별 선택 칸 `tools`.** `tools: {claude: {...}, codex: {...}}`처럼 소문자 도구 이름 아래에 둔다. 정의가 도구에 물들지 않게 안전장치 셋을 둔다. ① 중립 항목만으로 정의가 완전하다 ② 도구 칸은 없어도, 몰라도 무시된다 ③ 모르는 도구 칸도 읽고 다시 쓸 때 지우지 않고 보존한다(시험으로 고정). `tools={}`는 칸을 비운다.
- **이름과 범위.** 이름은 60자까지, `/`와 `\`와 제어 문자가 없고 slug(50자까지)를 만들 수 있어야 한다. 한 범위·한 종류 안에서는 slug가 하나뿐이다. **범위가 다르면 같은 이름이 공존한다**(ID로 구분하고 화면은 「글로벌 · 이름」, 「프로젝트 · 이름」으로 보인다). 스킬과 에이전트는 이름이 같아도 된다.
- **참조(`refs.md`).** 프로젝트 폴더의 `refs.md`에 줄마다 `<ID>  # <이름 힌트>`를 적는다. 엔진은 ID만 읽고 힌트는 읽는 사람을 위한 것이다(참조를 켜고 끌 때마다 지금 이름으로 새로 적는다). 프로젝트 파일이 아니라 따로 두는 까닭은, 설명서가 프로젝트 파일의 줄을 정해 두었고 그 파일을 모르는 옛 빌드가 다시 쓸 때 모르는 줄을 버리기 때문이다. 글로벌 자원만 참조한다(프로젝트 자원은 참조하지 않아도 쓸 수 있다). 참조한 글로벌 자원을 지우면(휴지통) 「깨진 참조」로 알리고, 되살리면 이어진다.
- **쓸 수 있는 자원을 계산하는 곳은 하나다.** `ResourceResolver.resolve`가 프로젝트 자원 + 참조한 글로벌 자원을 낸다(출처 `own`/`ref`). 깨진 참조(`broken`)와, 쓸 수 있는 에이전트가 필요로 하지만 쓸 수 없는 스킬(`missingSkills`, 글로벌 스킬이면 `canReference: true`라 참조해서 채울 수 있다)도 같이 낸다. 화면의 선택 목록, AI가 보는 목록, 실행 입력은 모두 이 결과만 써야 규칙 ④가 새지 않는다. **빠진 스킬은 스스로 쓸 수 있게 되지 않는다**(자동으로 참조하지 않는다). `set-resource-ref`의 `with-skills=true`가 에이전트를 참조할 때 그 에이전트의 글로벌 스킬도 함께 참조하게 해 준다.
- **에이전트 검사.** `skills`는 존재하는 스킬이어야 하고, 글로벌 에이전트는 글로벌 스킬만, 프로젝트 에이전트는 글로벌 스킬과 자기 프로젝트의 스킬을 쓴다. `repo`는 프로젝트 에이전트만 줄 수 있고 그 프로젝트의 저장소 이름이어야 한다(글로벌 에이전트에는 저장소 칸이 없다).
- **명령 15개**(명령줄과 MCP 도구 모두. 도구 이름은 밑줄 이름이고 `with-skills`는 `withSkills`다. 틀린 값은 쓰기 전에 AIS-400, 없는 것은 AIS-404). 앞의 8개는 자원을 다루고, 뒤의 7개는 승인 워크플로(아래)다. MCP에서는 읽기 6개(`list_resources`, `get_resource`, `usable_resources`, `list_resource_versions`, `list_resource_changes`, `get_resource_change`)가 기본 모드에도 보이고, 쓰기 4개(`save_resource`, `delete_resource`, `restore_resource`, `set_resource_ref`)는 노트와 같이 `--write`일 때만 보인다(설정의 「노트를 만들고 고치는 것도 허용」). 승인·거절·정책 변경(`approve-resource-change` 따위)은 MCP 도구가 아니다. 도구는 같은 `ResourceCommands`로 가고, 인자 해석(`ArgsParser`의 `RESOURCE_TOOLS`)이 JSON 인자를 명령줄과 같은 옵션 맵으로 바꾼다.

| 명령 | 하는 일 |
|---|---|
| `list-resources [scope=global\|project=<slug>] [kind=] [bodies=true] [trash=true]` | 관리 목록. 못 읽는 파일은 `skipped`에 이유와 함께 낸다. 글로벌이 먼저다 |
| `get-resource <id> [version=]` | 하나를 지침과 `precondition`(`f1`)과 함께 낸다. `version`을 주면 옛 판을 낸다(토큰은 늘 지금 파일의 것) |
| `save-resource kind= scope=\|project= name= description= …` | 새로 만든다. `id=<id> f1=<토큰>`을 주면 준 칸만 고친다 |
| `delete-resource <id>` / `restore-resource <id>` | 휴지통으로 보낸다(스킬은 폴더째, 누가 참조했는지 알려 준다) / 되살린다 |
| `set-resource-ref <프로젝트 slug> <글로벌 id> [remove=true] [with-skills=true]` | 참조를 켜고 끈다 |
| `usable-resources <프로젝트 slug> [bodies=true]` | 쓸 수 있는 자원 |
| `list-resource-versions <id>` | 고치기 전의 판 목록(새것이 앞) |

- **고치기 보호.** 고칠 때는 노트처럼 직전에 읽은 `f1`이 필요하다(없거나 어긋나면 AIS-412, 종료 코드 5). 실제로 달라지는 게 없으면 파일도 기록도 안 바꾼다. 종류·범위·ID는 못 바꾼다. 쓰기는 같은 폴더의 임시 파일에 쓰고 원자적으로 이름을 바꾼다.
- **기록과 되돌리기.** 고치면 `editedAt`이 남고, 세션 표식이 오면 `editedSession`(과 Claude가 아닐 때 `editedAgent`)이 남는다. 사람이 고치면 세션 칸이 없다. 고치기 전의 판은 `.history/<id>/`에 30판까지 남고, 옛 판의 내용으로 `save-resource`를 부르면 되돌려진다. 지운 것은 `.trash/<id>/`에 있고 `empty-trash`가 이 휴지통을 비우지 않는다(노트 휴지통과 따로다).
- **손으로 고친 파일.** 머리말이 없거나 id가 겹친 파일은 목록에서 건너뛰고 이유를 알린다. 나머지 자원은 그대로 읽는다.
- **지금의 한계.** ① 프로젝트를 지워도(`delete-project`) 그 프로젝트의 자원 폴더는 남는다. 함께 휴지통으로 옮기는 일은 아직 안 했다. ② 작업 실행기가 없다. ③ 러너가 에이전트 정의를 읽는 부분은 4단계에서 생기며, 그때 이 승인된 판(`usable-resources`와 `get-resource`)만 읽는다. 승인 화면은 아래 「승인 워크플로」의 「화면」에 있다. ④ MCP 서버로 쓰면 세션 표식이 넘어가 `editedSession`(과 `editedAgent`)이 남는 것을 시험(`McpServerTest`)으로 확인했다. 명령줄 `ais-cli`로 직접 부를 때 세션 표식이 넘어가는지는 확인하지 못했다.
#### 승인 워크플로(이슈 #53)

AI가 고친 스킬·에이전트 정의가 곧바로 쓰이지 않고, 사람이 승인한 판만 쓰이게 한다. 에이전트의 권한(`read`/`edit`/`publish`)과 참조가 AI 스스로에 의해 풀리지 않게 하려는 것이다. 엔진(이 절)과 화면, 알림 세 조각이다. 러너 연결은 4단계의 첫 작업이다.

- **AI의 통로 판별.** 세션 표식이 달린 요청이나 MCP 도구 이름(밑줄)으로 부른 요청은 AI다. 앱 화면은 명령 이름(하이픈)으로 세션 표식 없이 부르므로 사람이다. 사람이 낸 변경은 정책과 상관없이 바로 적용한다.
- **정책(`set-resource-policy`, 사람만).** 칸 4개가 각각 `auto`(AI가 써도 바로 적용)나 `approve`(승인 필요)다. 기본은 `project=auto`(프로젝트 자원의 내용), `global=approve`(글로벌 자원 변경), `refs=approve`(참조 끄기), `permission=approve`(권한 내림). **권한 올림, 도구 칸(`tools`) 변경, 참조 켜기는 정책과 상관없이 늘 승인이 필요하다**(`Approvals.Category`의 `fixed`). `tools`는 도구별 허용 도구 목록 같은 권한성 값이 들어갈 수 있어서 같이 묶었다. 되살리기는 예전 판이 없는 만들기로 본다.
- **대기 변경.** 승인이 필요하면 적용하지 않고 `.aisurada/pending/<pc…>.json`에 올린다(vault 안이라 이력에 남고 vault를 옮겨도 따라온다). 응답은 오류가 아니라 `queued: true`, `changeId`, 까닭(`reasons`), 안내 글이다. 같은 변경을 되풀이하면 새로 쌓지 않고 같은 `changeId`에 `duplicate: true`를 준다. 대기 중에는 `list-resources`, `get-resource`, `usable-resources`, MCP 조회 모두에 이전 판이 그대로 보인다.
- **승인·거절.** `approve-resource-change <id>`는 같은 명령 경로를 한 번 더 지나 적용하고(승인 때의 검사와 평소 검사가 갈라지지 않는다) 쓴 이는 올린 AI의 세션으로 남긴다. 만들기는 대기에 올릴 때 보인 ID로 만든다. 승인 전에 대상이 바뀌었으면(`f1`이 어긋났거나 이름이 겹침) 적용하지 않고 `stale`로 남기고 AIS-412/409를 낸다. `reject-resource-change <id> [reason=]`는 이유를 남긴다. 결정이 난 변경은 최근 200건까지 남는다. AI가 승인·거절·정책 변경·직접 수정 처리를 부르면 AIS-403이다(MCP 도구도 아니고, 세션 표식이 달린 `ais-cli`도 거절한다).
- **승인한 판의 지문(P2).** 승인할 때 파일 바이트의 SHA-256(`f1:…`)과 사본을 앱 폴더(`~/Library/Application Support/aisurada/approvals/<vault 해시>/`의 `ledger.json`, `copies/`)에 적는다. 읽는 쪽(`ResourceStore.scanServed`, `ResourceRefs.readApproved`, `ResourceResolver`)은 지금 파일의 지문이 기록과 맞을 때만 그 파일을 쓴다. 파일을 직접 고쳐 어긋나면(`dirty`) 승인해 둔 사본을 대신 쓰고 화면 목록에는 `approval: dirty`가 붙는다. 기록 없이 생긴 새 파일(`new`)은 아예 쓰지 않고 조회도 404다. 이런 것은 `list-resource-changes`의 `unapproved`에 이전·지금 판과 함께 나온다. 사람이 `save-resource`로 고치면 **승인한 판 위에** 얹어 쓰고 직접 고친 내용은 이어받지 않는다(AI가 사소한 변경으로 직접 고친 내용을 세탁하지 못하게 한다). `resolve-resource-edit <id|refs:<프로젝트>> action=approve|revert`가 직접 고친 것을 받아들이거나 승인한 판으로 되돌린다(새 파일이면 휴지통으로).
- **기준선과 깨진 기록장.** 기록장이 없는 vault는 처음 읽을 때 있는 자원과 참조 목록을 모두 승인한 것으로 받아들인다(이 기능 전에 만든 자원이 못 쓰이게 되지 않게). 기록장이 깨졌으면 비어 있는 것으로 보지 않고 AIS-500으로 멈춘다(모든 파일이 다시 승인된 것이 되지 않게).
- **명령.** `list-resource-changes [decided=<건수>]`(대기·승인 안 된 직접 수정·결정 난 것, `counts.pending`/`counts.unapproved`는 화면 배지용. `list-resources`도 `approvals` 칸에 같은 수를 낸다), `get-resource-change <id>`(이전 판 `before`, 새 판 `after`, 바뀐 칸 `changedFields`), `approve-resource-change`, `reject-resource-change`, `resolve-resource-edit`, `get-resource-policy`, `set-resource-policy`. 앞의 둘만 MCP 읽기 도구다. 새 명령 7개는 `AisCliMain.ENGINE_COMMANDS_APPROVALS`에도 있어야 앱이 `ais-cli`로 부를 수 있고, `TasksTest`가 레지스트리와 대조한다.
- **한계.** 지문도 앱 폴더에 있어서 앱 폴더를 쓸 수 있는 프로세스(셸을 마음대로 쓰는 AI)는 위조할 수 있다. 그것은 이 장치가 아니라 실행 격리(작업 폴더와 도구 허용 목록)의 몫이다. 이 장치가 막는 것은 프롬프트 주입에 속아 MCP나 `ais-cli`로 고치는 AI와 파일을 실수로 직접 고친 경우다. 기본 정책은 프로젝트 자원의 내용(지침 글)을 AI가 바로 고치게 둔다. 그 지침을 쓰는 에이전트의 권한은 승인된 값이지만, 지침 자체가 바뀌는 위험은 `project=approve`로 줄인다. 앱 폴더의 승인 기록은 vault를 옮기면 따라오지 않는다(옮긴 뒤 처음 읽을 때 기준선으로 다시 받아들인다).
- **화면.** 스킬·에이전트 페이지(`ui/ResourcesPage.tsx`) 맨 위에 「승인이 필요한 변경 N건」 배너와 「승인 대기함」 단추가 있고, 레일의 「스킬·에이전트」 메뉴에는 같은 수의 배지가 붙는다(`ui/approvalsStore.ts`가 수를 들고, 앱이 켜질 때·창이 눈에 띌 때·60초마다·승인 알림이 올 때 `list-resource-changes decided=0`으로 다시 읽는다. 엔진을 한 번 부를 때마다 자바가 뜨므로 길게 잡았다). 승인 대기함(`ui/ApprovalPanel.tsx`)은 탭 넷이다: **승인 대기**(변경마다 요약, 올린 AI 세션, 정책과 상관없이 늘 승인이 필요한 종류의 표시, 「이전 판과 비교」로 칸별 비교와 지침의 줄 단위 나란히 비교, 승인, 이유를 적는 거절), **직접 수정**(승인 안 된 직접 수정. 승인한 판과 지금 파일을 견주고 「이 내용을 승인」이나 「승인한 판으로 되돌리기」/새 파일이면 「지우기」), **지난 결정**(승인함·거절함·적용 못 함과 이유), **정책**(범위별 「바로 적용」/「승인 필요」 4개와 고정 종류 안내). 그리는 부분(`ApprovalsView`)과 엔진을 부르는 부분(`ApprovalsDialog`)을 나눠 서버 렌더로 시험한다. 비교의 계산(`fieldDiffs`, 줄 단위 최장 공통 부분열 `sideBySide`)은 `ui/approvalsClient.ts`에 있고, 줄 수의 곱이 400만을 넘으면 통째로 바뀐 것으로 본다. 승인 중 대상이 바뀌어 적용하지 못하면 엔진의 AIS-412가 「다른 곳에서 먼저 고쳤어요」로 풀려 보이고 그 변경은 「지난 결정」에 「적용 못 함」으로 남는다. 목록에서 직접 고쳐진 자원에는 「직접 고쳐짐」 표시(`approval: dirty`)가 붙는다. 프로젝트 페이지에는 아직 배너가 없다(전체 대기함은 스킬·에이전트 페이지 한 곳이다).
- **알림.** 호스트의 `approvals.rs`가 앱이 켜져 있는 동안 30초마다(처음은 8초 뒤) 엔진에 `list-resource-changes decided=0`을 묻고, 아직 알리지 않은 대기 변경마다 알림 센터에 종류 `approval`(화면 이름 「승인」)을 쌓는다. 알림 `id`가 `approval-<변경 id>`라 같은 변경은 한 번만 쌓인다. 한 번에 5개까지 쌓는다. 창이 닫혀 있어도 알리는 쪽이다. 설정의 소식이 4개에서 5개(「스킬·에이전트 변경이 승인을 기다릴 때」, 기본은 앱 안·macOS 모두 켬)가 됐고, 알림 센터의 「승인하러 가기」가 스킬·에이전트 페이지(`/resources`)로 데려다 준다. 승인 안 된 직접 수정은 알리지 않고 배지와 대기함의 「직접 수정」 탭으로만 보인다.
- **앱이 엔진을 부를 때 AI 세션 표식을 지운다.** 앱은 엔진을 같은 프로세스 안에서 `ais_cli::run`으로 부르고 앱의 환경변수를 넘긴다. 앱을 AI 세션의 터미널에서 띄웠다면 `CLAUDE_CODE_SESSION_ID` 같은 표식이 환경에 남아 있어서, 엔진이 화면의 저장과 승인을 AI의 것으로 보고 승인을 거절하거나 대기에 올릴 수 있다. 그래서 `sidecar.rs`의 `cli_env`가 `AISURADA_SESSION_ID`, `AISURADA_SESSION_AGENT`, `CLAUDE_CODE_SESSION_ID`, `CODEX_SESSION_ID`, `CODEX_THREAD_ID`를 지운다(`the_cli_environment_drops_ai_session_markers_and_names_the_vault`). 러너의 엔진 호출도 같은 길이다. AI 프로세스(claude, 터미널의 AI CLI)는 이 길을 거치지 않는다.
- **화면 시험.** `approvals-ui.test.mjs` 15건(응답 읽기, 정책 기본값, 엔진 인자, 배지·표시 글, 칸별 비교, 줄 단위 비교, 저장소와 지켜보기, 탭별 서버 렌더, 레일 배지와 페이지 배너)과 `notifications-ui.test.mjs`(소식 5개, 승인 알림의 이동). 호스트는 `approvals::tests` 2건, `notifications::tests`의 승인 소식, `sidecar::tests`의 세션 표식 삭제다. 승인 대기함은 진짜 컴포넌트를 서버 렌더하고 Edge headless로 밝은 모양과 어두운 모양에서 그려서 모양을 눈으로 확인했다. **실제 앱 창(WKWebView)에서 눌러 본 확인은 없다.**
- **시험.** `ApprovalsTest`(통로 판별, 정책별 동작, 고정 종류, 대기 중에는 이전 판이 보임, 승인·거절·stale·중복, AI의 승인 거절, 만들기·지우기·되살리기, 직접 수정 걸러내기와 받아들이기·되돌리기, 새 파일, 참조 목록 직접 수정, 기준선, 깨진 기록장)과 `McpServerTest`(진짜 MCP 서버와 진짜 `ais-cli`로 AI의 변경이 대기하고 사람의 `ais-cli`로 승인하는 길, 세션 표식이 달린 `ais-cli`의 승인 거절).

- **화면.** 글로벌 페이지(`/resources`, 왼쪽 메뉴의 「스킬·에이전트」)와 프로젝트 페이지(`/project/<slug>`, 레일 「에이전트」 탭의 「프로젝트 자원」 줄)가 있다. 글로벌 페이지는 공용 스킬·에이전트를 만들고 고치고 지우고(휴지통에서 되살림), 지울 때는 참조하던 프로젝트를 알려 준다. 프로젝트 페이지는 저장소와 아젠다를 보이고, 엔진의 `usable-resources`가 낸 「쓸 수 있는 자원」(출처 꼬리표 「프로젝트 자원」·「글로벌에서 참조」)과 깨진 참조, 에이전트가 필요로 하지만 못 쓰는 스킬(글로벌이면 「참조 추가」)을 보이며, 참조하지 않은 글로벌 자원은 「글로벌 자원 참조하기」에만 나온다. 에이전트를 참조할 때는 그 에이전트가 쓰는 글로벌 스킬을 보여 주고 「스킬도 함께 참조」(`with-skills`)나 「에이전트만 참조」를 고르게 한다(확인하는 것이 곧 명시적인 참조). 이름이 같아도 범위를 붙여 구분한다(`scopeLabel`: 「글로벌 · 이름」). 편집 창(`ResourceFormModal`)은 이름·설명·지침, 에이전트의 권한 3단계·쓸 스킬(글로벌 에이전트는 글로벌 스킬만, 프로젝트 에이전트는 자기 프로젝트의 스킬도)·쓸 저장소, 선택 칸 「도구별 설정」(JSON)을 받는다. 고칠 때는 바뀐 칸만 보내서 모르는 도구 칸이 보존되고(`formToInput`), 마지막에 AI가 고쳤으면(`editedSession`) 알려 준다. 화면은 엔진을 `controller.callEngine`으로 부르고(`ui/resourcesClient.ts`), 창이 다시 눈에 띄면 다시 읽어 AI나 다른 창이 고친 것을 따라간다. 프로젝트 자원의 휴지통은 프로젝트 페이지의 「지운 것」에 있다.
- **시험.** 화면은 `resources-ui.test.mjs`(순수 함수, 가짜 엔진으로 본 호출 인자, 서버 렌더로 본 페이지와 레일)다. 실제 창에서 눌러 본 확인은 없다. 엔진과 MCP는 `ResourcesTest`와 `McpServerTest`의 자원 도구 부분(진짜 엔진과 `--write` 서버로 만들기, `f1` 보호, 참조, 규칙 ④, 지우기·되살리기, 읽기 모드 거절, 세션 기록)이다. `ResourcesTest`(파일 모양, ID 머리, 도구 칸 보존, 같은 이름 규칙, 규칙 ④, 참조 규칙과 거절, 이름을 바꿔도 참조 유지, f1 보호, 기록, 이력과 되돌리기, 30판 상한, 에이전트 검사, 빠진 스킬, 지우기·되살리기, 손으로 망가뜨린 파일, 기존 명령과의 공존).

### 태스크 (`task/`)

사람이 일을 티켓으로 적어 두면 나중에 러너가 집어 에이전트에게 맡기는 단위다. **외부 설명서에 없는 확장이다.** 지금은 **1단계(태스크 저장, 명령, 상태 전이, 화면)** 까지이고 러너와 태스크 안 채팅은 아직 없다. 알림은 2단계로 따로 있다(아래 「알림」). 계획은 「태스크 시스템 구현 계획」 문서(5단계, 결정 12개)를 따른다.

- **위치.** `.aisurada/tasks/` 아래다. 노트를 훑는 명령과 기존 명령이 이 영역을 보지 않는다(시험으로 확인).
  `TSK-14.md`(태스크), `TSK-14/activity.jsonl`(활동, 한 줄에 사건 하나, 최근 500줄까지), `.trash/<id>/`(지운 태스크). 실행 기록(러너가 만드는 로그)은 vault가 아니라 앱 폴더에 둔다(결정 3).
- **키와 ID.** 키 `TSK-<번호>`는 엔진이 매기고 만든 뒤 바뀌지 않는다. 지운 번호(휴지통에 있는 것 포함)를 다시 쓰지 않는다. ID는 `tk…`(노트 `nd…`, 스킬 `sk…`와 같은 형식)이고 파일 안에 있다. 접두 `TSK`는 엔진 오류 코드(`AIS-…`)와 겹치지 않게 골랐다.
- **파일 모양.** `---` 머리말(YAML)과 설명 본문이다. 머리말은 스킬·에이전트 자원처럼 SnakeYAML로 읽고 쓰므로 `criteria`, `links`, `refs`를 들여쓴 목록으로 적는다(줄 읽개로 읽는 프로젝트·아젠다 파일과 다르다). 모르는 머리말 칸은 지우지 않고 보존한다(시험으로 고정). `key`, `id`, `title`, `type`, `priority`, `status`, `repo`, `project`, `baseBranch`, `labels`, `criteria`, `links`, `refs`, `createdAt`, `editedAt`, `editedSession`, `editedAgent` 순서로 쓴다. 유형은 `feature`·`bug`·`chore`, 우선순위는 `high`·`medium`·`low`다.
- **완료 기준은 필수다.** `create-task`는 `criteria`가 비면 AIS-400으로 거절한다. 일이 끝났는지 이 기준으로 확인하기 때문이다(사용자 결정, 2026-11-13). `repo`(`owner/name`)와 `title`도 필수다.
- **명령 9개.** MCP에는 읽기 도구 `list_tasks`, `get_task`만 열린다. 상태를 바꾸는 일은 사람과 러너만 한다.

| 명령 | 하는 일 |
|---|---|
| `create-task title= repo= criteria=[…] [type= priority= project= base= body= labels=[…] refs=[…]]` | Draft로 만든다. 응답에 `precondition`(`f1`)이 있다 |
| `get-task <키 또는 id> [activity=<개수>]` | 설명, 완료 기준, 자원, 연결, `readiness`(Ready 조건), `transitions`(사람이 바꿀 수 있는 곳과 막힌 까닭), 활동을 모두 낸다 |
| `list-tasks [status= type= repo= label= q= blocked=true bodies=true trash=true]` | 키가 큰 것이 앞이다. 못 읽는 파일은 `skipped`에 이유와 함께 낸다 |
| `update-task <키> f1=<토큰> [고칠 칸…]` | 노트처럼 직전에 읽은 `f1`이 필요하다(없거나 어긋나면 AIS-412). 같은 값이면 파일도 활동도 안 바꾼다. 목록 칸은 통째로 바뀐다 |
| `transition-task <키> to=<상태> [actor=human\|runner] [from=<기대하는 지금 상태>]` | 규칙과 조건을 검사하고 바꾼다. `from`이 지금 상태와 다르면 AIS-409 |
| `link-tasks <건 쪽> <받는 쪽> type=blocks\|relates\|duplicates [remove=true]` | 연결을 걸거나 푼다 |
| `add-task-comment <키> body=<댓글>` | 활동에 댓글을 남긴다 |
| `delete-task <키>` / `restore-task <키>` | 휴지통으로 보낸다(활동 기록도 함께) / 되살린다. `empty-trash`는 태스크 휴지통을 비우지 않는다 |

- **상태 전이(`TaskTransitions`).** 상태 8개는 `draft`, `ready-for-develop`, `in-progress`, `waiting-answer`, `in-review`, `done`, `failed`, `cancelled`다. 화면, 러너, 명령이 모두 `transition-task`를 거치므로 조건이 한 곳에서만 검사된다. 누가 바꾸는지는 `actor`(`human`이 기본, 러너는 `runner`)로 준다.

| 전이 | 누가 | 조건 |
|---|---|---|
| Draft → Ready For Develop | 사람 | 요약, 완료 기준 1개 이상, 저장소가 있어야 한다(`readiness`) |
| Ready For Develop → Draft | 사람 | |
| Ready For Develop → In Progress | 사람(「지금 실행」), 러너 | 다른 태스크에 막혀 있지 않아야 한다 |
| In Progress → Waiting Answer | 러너 | |
| Waiting Answer → In Progress | 사람, 러너 | |
| In Progress → In Review | 러너 | |
| In Review → Done | 사람, 러너 | 지금은 사람이 누른다(결정 1). 러너도 같은 조건으로 부를 수 있게 열어 두었다 |
| Ready·In Progress·Waiting·In Review → Failed | 러너 | |
| Draft·Ready·In Progress·Waiting·In Review → Cancelled | 사람 | |
| Failed·Cancelled → Draft | 사람 | |

  Done에서는 어디로도 갈 수 없다. 그 밖의 쌍은 AIS-409다. `actor`는 믿을 수 있는 인증이 아니라 규칙을 지키게 하는 표시다(이 컴퓨터 안의 약속이다).
- **연결.** 종류는 `blocks`(건 쪽이 받는 쪽을 막는다), `relates`(관련, 한 번이면 양쪽에 보인다), `duplicates`(건 쪽이 받는 쪽의 중복)다. 연결은 **건 쪽 파일에만** 적고 반대쪽(`blocked-by`, `duplicated-by`)은 읽을 때 계산한다. 두 파일이 어긋날 일이 없다. 자기 자신에게 못 걸고, 서로 막는 순환(`blocks`의 맞은편)은 거절한다. `blocks`로 막힌 태스크는 막은 태스크가 **Done**이 되기 전에는 Ready For Develop → In Progress로 못 간다(Failed·Cancelled는 풀지 않는다). 지운 태스크는 막지 않고, 되살리면 다시 막는다(결정 4).
- **기록.** 만들기, 상태 바뀜(`from`, `to`), 고침(바뀐 칸 이름), 연결, 댓글이 `activity.jsonl`에 시간순으로 남는다. 고치면 `editedAt`이 남고, 세션 표식이 오면 `editedSession`(과 Claude가 아닐 때 `editedAgent`)이 남는다. 완료 기준을 확인하면 확인한 시각과 쪽(`human`, `runner`)이 항목에 남고, 확인이 풀리면 기록이 지워진다.
- **쓰기 비용.** 쓰기 명령은 vault 전체 쓰기 잠금과 노트 색인 맞추기(`NoteIndex.reconciled`)를 거치므로 태스크를 쓸 때마다 노트 폴더를 훑는다. 호출 한 번은 노트 17개짜리 vault에서 0.1초 안팎이었다(Java 시절에 잰 값이고 JVM을 띄우는 시간을 포함한다). 노트가 아주 많은 vault의 값은 재지 않았다.
- **화면.** 레일의 「태스크」(`/tasks`)가 보드와 목록이고(`ui/TasksPage.tsx`), 태스크를 열면 `/task/<키>`가 상세다(`ui/TaskDetailPage.tsx`). 만들기와 편집은 한 양식이다(`ui/TaskFormModal.tsx`). 엔진을 부르는 얇은 층과 순수 계산(폼 값 → 인자, 거르기, 상태별 묶기, 한 줄 풀이)은 `ui/tasksClient.ts`에 모았고, 화면은 엔진을 `controller.callEngine`으로 부른다(호스트 커맨드를 더하지 않았다). 스타일은 `src/tasks.css`(모든 화면 모양의 토큰을 따르고, 토큰이 없으면 대체값으로 물러난다)다.
  - **보드.** 열 5개(Draft, Ready For Develop, In Progress, Waiting Answer, In Review)와 접어 둔 끝난 상태(Done, Failed, Cancelled)다. Draft 열 아래의 한 줄 입력은 Enter를 누르면 요약이 채워진 만들기 양식을 연다(완료 기준이 필수라서 바로 만들지 않는다). 거르기(검색, 저장소, 유형, 라벨, 막힌 것만)는 한 번에 받은 목록을 화면에서 거른다. 보드와 목록 가운데 고른 것은 `localStorage`의 `aisurada.tasks.view`에 둔다.
  - **상세.** Jira 이슈 화면 구조다. 가운데는 설명, 완료 기준(체크하면 `update-task`로 저장하고 확인한 시각과 쪽이 남는다), 연결된 태스크, 연결한 자원, 활동(전체·댓글·이력)이고, 오른쪽은 세부 정보와 Ready For Develop 조건이다. **상태 메뉴는 엔진이 알려 준 `transitions`를 그대로 보인다.** 사람이 바꿀 수 있는 곳만 나오고, 조건이 모자라거나 막혀 있으면 까닭과 함께 막혀 보인다. 화면은 이 규칙을 다시 만들지 않는다. 상태를 바꿀 때 지금 상태를 `from`으로 넘겨서, 그 사이에 다른 곳이 먼저 바꿨으면 엔진이 거절한다.
  - **새로 불러오기.** 창이 다시 눈에 띄면 다시 읽는다(러너나 다른 창이 바꾼 것을 따라간다).
  - **시험용 `preset`.** 두 페이지는 엔진 없이 처음 모양을 정하는 `preset` 속성을 받는다(앱은 주지 않는다). 서버 렌더 시험이 데이터가 든 보드와 상세를 그려 보는 데 쓴다.
- **시험.** `TasksTest`(만들기와 필수 값, 거절 문구, 고치기 보호와 모르는 칸 보존, 완료 기준 확인 기록, 번호, 휴지통, 상태 전이 전부, 연결과 막힘, 활동, 목록 거르기, 손으로 망가뜨린 파일, MCP 읽기 도구와 쓰기 도구 없음, 기존 명령과의 공존)와 `McpServerTest`의 도구 개수, 화면은 `tasks-ui.test.mjs`(순수 함수, 가짜 엔진으로 본 호출 인자, 서버 렌더로 본 보드·목록·상세·상태 메뉴와 레일)다. 서버 렌더에는 `document`가 없어서 설명(마크다운) 영역은 그리지 않는다. 데이터가 든 보드와 상세는 진짜 엔진으로 만든 임시 vault의 응답을 서버 렌더하고 Edge headless로 그려서 모양을 눈으로 확인했다(밝은 모양과 어두운 모양). 실제 앱 창(WKWebView)에서 눌러 본 확인은 없다.
- **`ais-cli`가 넘겨 주는 이름.** `ais-cli`는 `engine-rs/crates/ais-cli/src/registry.rs`의 목록(`ENGINE_COMMANDS_61`, 태스크 9개는 `ENGINE_COMMANDS_TASKS`, 승인 명령 7개는 `ENGINE_COMMANDS_APPROVALS`)에 든 이름만 엔진으로 넘긴다. 새 명령을 넣고 이 목록에 빠뜨리면 실제 앱에서 「알 수 없는 이름」(종료 코드 2)으로 거절된다. Java 시절에 태스크 명령이 이 목록에 빠져 있던 적이 있다(`5b38c33`). 엔진 시험과 화면 시험은 `ais-cli`를 거치지 않아 이것을 잡지 못하므로, 명령을 더하면 `ais-cli`로 한 번 불러 본다.
- **아직 없는 것.** 코드를 고치고 PR을 올리는 러너(4단계), 태스크 안 채팅(정의 에이전트, 3b), 질문을 위 단계 에이전트가 먼저 푸는 계층(5단계). 승인 워크플로우(이슈 #53)는 러너가 코드를 고치기 전에 필요하다. 읽기 전용 러너는 아래 「러너」에 있다.

### 알림 (`notifications.rs`)

태스크 시스템 2단계다. 알림 목록 저장, 채널 설정, macOS 배너를 호스트의 이 모듈 한 곳에서 맡는다. **외부 설명서에 없는 확장이다.** 러너(3단계)도 화면도 같은 `push`를 거치므로 「어디로 보낼지」가 한 곳에서만 정해진다. 알림을 만드는 쪽은 러너(질문, 보고, 실패), 승인 알림(`approvals.rs`, 아래 「승인 워크플로」의 「알림」)과 **설정의 「시험 알림 보내기」**다.

- **저장.** 목록은 vault마다 `~/Library/Application Support/aisurada/notifications/items-<vault 경로의 SHA-256 앞 8바이트>.json`에(최근 200개, 새것이 앞), 설정은 모든 vault가 같이 쓰는 `…/notifications/settings.json`에 둔다. vault 안에 두지 않는 까닭은 이 컴퓨터에서만 뜻이 있는 기록이고 vault의 git 이력을 부풀리지 않으려는 것이다. 파일이 없거나 망가졌으면 빈 목록, 기본 설정으로 읽는다(알림은 지워져도 큰일이 아니고 앱이 죽으면 안 된다). 쓸 때는 임시 파일에 쓰고 이름을 바꾼다.
- **알림 하나.** `id`, `kind`(`question`·`pr`·`failed`·`done`·`approval`·`test`), `title`(120자), `body`(400자), `taskKey`(`TSK-14`, 없을 수 있다), `at`(밀리초), `read`. `id`를 주면 같은 id가 이미 있을 때 새로 만들지 않는다(같은 일을 두 번 알리지 않으려는 것이다. 러너는 `run-<실행 id>-done` 같은 id를 준다).
- **설정과 보내는 곳.** 채널 스위치 2개(앱 안, macOS)와 소식 5개(질문, 리뷰할 일, 실패·막힘, Done, 스킬·에이전트 변경의 승인 대기)별 채널 스위치다. 기본은 모두 켜고 Done만 macOS를 끈다(조용히 알려도 된다). 승인 대기는 늦으면 AI의 일이 막혀서 둘 다 켠다. 옛 설정 파일에 승인 소식 칸이 없으면 켠 것으로 읽는다. 알림은 설정과 무관하게 **목록(알림 센터)에는 늘 쌓인다**. 설정은 토스트와 배너만 정한다: 토스트는 `앱 안 채널 && 그 소식의 앱 안`, 배너는 `macOS 채널 && 그 소식의 macOS`다. `test`는 채널 스위치만 따른다(`route`).
- **명령 5개.** `notifications_list`, `notifications_push`, `notifications_mark_read`, `notifications_settings_get`, `notifications_settings_set`. `push`는 쌓고, 설정에 따라 배너를 보내고, 화면에 사건(`ais://notifications`: `{unread, pushed, toast}`)을 알린 뒤 `{item, toast, banner, unread}`를 돌려준다. `banner`는 `off`, `sent`, `sent-fallback`, `failed: …`다. 열린 vault가 없으면 `push`는 거절하고 `list`는 빈 목록을 낸다.
- **macOS 배너.** `tauri-plugin-notification`(2.4.0. tauri 2.11.2와 맞는 가장 새 판. 2.5 이상은 tauri 2.12가 필요하다)으로 보내고, 그것이 실패하면 `osascript`(`display notification`)로 한 번 더 시도한다(이때 배너는 Script Editor 이름으로 뜬다). 배너를 누르면 앱이 앞으로 올 뿐 그 태스크로 가지는 않는다(플러그인이 macOS에서 누름 소식을 주지 않는다). 태스크로 가는 단추는 알림 센터와 토스트에 있다.
- **집중 모드는 macOS가 처리한다**(사용자 결정). 집중 모드에서 aisurada를 허용하지 않으면 macOS가 배너를 막고 알림 센터에 쌓는다. 앱이 집중 모드 상태를 읽어 토스트까지 숨기지는 않는다. 읽는 방법과 권한은 확인하지 못했다.
- **화면.** 레일 맨 위의 종 단추(안 읽은 수 배지)가 알림 센터를 연다(`ui/NotificationCenter.tsx`: 안 읽음·전체 탭, 모두 읽음, 질문은 「답하기」, 승인은 「승인하러 가기」, 그 밖에는 「태스크 보기」). 토스트는 메인 창 오른쪽 아래에 3개까지 뜨고 8초 뒤에 사라진다. 설정 화면의 「알림」(`ui/NotificationSettings.tsx`)이 채널(Telegram은 자리만), 소식별 스위치, 집중 모드 안내, 시험 알림을 보인다. 화면의 상태는 `ui/notificationsStore.ts`(GitHub 연결처럼 모듈 하나의 저장소)가 들고, 앱이 켜질 때 한 번 읽고 호스트의 사건을 받아 갱신한다. 노트 창은 목록만 따라가고 토스트는 안 띄운다. 스위치는 `.set-switch`(설정 화면 시험이 세는 이름)가 아니라 `.ntf-switch`이고, 모든 화면 모양에서 같게 보이도록 `:root[data-skin]`으로 덮었다.
- **시험.** 호스트는 `notifications::tests`(기본 설정, 가는 곳 계산, 설정 파일 읽기·쓰기와 모자란 칸 채우기, 알림 검사와 자르기, 쌓기·같은 id·200개 상한, 읽음, vault별 파일, AppleScript 글 탈출, JSON 이름) 10건이고, 실제 macOS 알림을 한 번 보내는 `real_banner_smoke`는 `cargo test -- --ignored real_banner_smoke`로만 돈다(이 컴퓨터에서 `sent`를 확인했다). 화면은 `notifications-ui.test.mjs`(저장소의 읽기·사건 반영·되돌림, 서버 렌더로 본 센터·토스트·설정·레일의 종)다. 알림 센터, 토스트, 설정 화면은 진짜 컴포넌트를 서버 렌더하고 Edge headless로 다섯 화면 모양(밝은 모양과 어두운 모양)에서 그려서 모양을 눈으로 확인했다.
- **확인하지 못한 것.** 실제 앱 창(WKWebView)에서 종·토스트·설정을 눌러 본 것, 번들한 앱에서 macOS 배너가 aisurada 이름과 아이콘으로 뜨는지(개발 실행은 터미널 이름으로 뜬다), 시스템 설정의 알림 허용 상태가 어떻게 보이는지. 설정의 「시험 알림 보내기」로 직접 확인할 수 있다.

### 러너 (`runner/`)

태스크 시스템 3단계(읽기 전용)다. **외부 설명서에 없는 확장이다.** Ready For Develop인 태스크를 호스트가 집어서 claude에게 저장소와 노트를 **읽게만** 하고, 조사 결과(`[보고]`)를 댓글로 남겨 In Review로 보내거나, 사람만 답할 수 있는 질문(`[질문]`)을 남겨 Waiting Answer로 보낸다. 코드를 고치고 PR을 올리는 일은 4단계다. 앱이 켜져 있는 동안에만 돈다.

| 파일 | 맡은 일 |
|---|---|
| `runner/mod.rs` | 언제 누구를 돌릴지: 15초마다 훑기, 동시 실행 수, 「지금 실행」·중지·답하기, 앱을 껐다 켰을 때 이어 가기, 앱을 닫을 때 멈추기, 화면 명령 6개 |
| `runner/job.rs` | 실행 하나(일곱 걸음)와 보고·질문·실패·중지의 끝맺음 |
| `runner/protocol.rs` | 태스크를 프롬프트로, claude의 마지막 답을 `[보고]`와 `[질문]`으로, claude를 읽기 전용으로 띄우는 인자, MCP 설정 |
| `runner/store.rs` | 설정과 실행 기록 파일 |
| `runner/host.rs` | 엔진(`ais-cli`)·claude·GitHub 저장소 사본·알림을 감싼 trait `Host`(시험은 가짜를 끼운다)와 읽기 전용 작업 폴더 |

cargo 기능 `claude` 아래에 있다. 이 기능이 없는 판에는 명령 6개가 없고(`Command … not found`) 화면은 러너 막대와 「실행」 영역을 그리지 않는다.

**일곱 걸음.** 걸음 이름은 실행 기록의 `step`과 화면의 단계 표시가 같이 쓴다.

| 걸음 | 하는 일 |
|---|---|
| 받기 | `get-task`로 태스크를 읽는다 |
| 집기 | `transition-task to=in-progress actor=runner from=ready-for-develop`. 막혀 있거나 그 사이 바뀌었으면 엔진이 거절하고, 러너는 그 태스크를 60초 쉰다 |
| 판단 | claude가 있는지, 저장소 이름이 있는지, 읽기 전용 MCP 설정(`claude-mcp config`에 이 vault를 박은 것)을 만든다 |
| 받아오기 | 앱 소유 사본(`~/Library/Application Support/aisurada/repos/<owner>/<name>`)을 `git fetch --prune`으로 최신으로 맞추고, 거기서 **분리된 HEAD의 읽기 전용 작업 폴더**(`…/runs/<vault 해시>/<TSK-n>/<실행 id>/work`)를 만든다. 사본의 브랜치와 작업 트리는 건드리지 않는다. 태스크에 기준 브랜치(`base`)가 있으면 그 브랜치, 없으면 원격의 기본 브랜치다 |
| 일하기 | `claude -p`를 작업 폴더에서 돌린다(아래). 사람의 답이 왔거나 앱을 다시 켠 뒤면 같은 세션을 `--resume`으로 잇는다 |
| 내보내기 | 보고면 `**분석 보고**` 댓글 + In Review + 알림(`pr`), 질문이면 `**질문**` 댓글 + Waiting Answer + 알림(`question`). 실패면 `**실행이 실패했어요**` 댓글 + Failed + 알림(`failed`) |
| 정리 | 끝났으면 작업 폴더를 `git worktree remove`로 지운다. 질문으로 멈춘 동안에는 남겨 둔다 |

**읽기 전용을 지키는 법.** claude에는 다음 인자를 준다(`protocol::claude_args`). 모두 실제 `claude` 2.1.322로 확인했다.

- `--permission-mode dontAsk --permission-prompts none`: 미리 허용한 도구만 쓰고 나머지는 묻지 않고 거절한다.
- `--allowedTools "Read(<작업 폴더>/**)" mcp__aisurada`: 파일 읽기와 노트 읽기만 허용한다. **`Read` 규칙에 경로를 주지 않으면 `/etc/hosts`까지 읽혔다.** 경로를 준 규칙 하나가 Read·Grep·Glob을 모두 그 폴더로 가둔다(`/etc`, `~/.ssh` 거절을 확인했다). 노트는 vault 전체를 읽을 수 있다(사용자 결정 2-나: 질문하기 전에 노트에서 먼저 찾는다).
- `--disallowedTools Bash Edit Write NotebookEdit WebFetch WebSearch Agent Task`: 셸, 쓰기, 웹, 하위 에이전트를 막는다.
- `--strict-mcp-config --mcp-config <JSON>`: 이 호출이 준 MCP 서버만 쓴다. 서버는 `ais-cli mcp`(`--write` 없음)라서 읽기 도구만 있고, `AISURADA_VAULT`로 이 실행의 vault에 못 박는다. 쓰기 모드 설정이 오면 거절한다.
- `--output-format json`: 결과 글, 세션 id, 토큰, 비용(`total_cost_usd`)을 한 번에 받는다. 모델은 설정(기본 `sonnet`)에서 정한다.

**상태와 사람이 할 일.** 상태 전이는 모두 엔진의 `transition-task`를 거친다(규칙은 한 곳).

| 지금 | 일어나는 일 | 사람이 할 일 |
|---|---|---|
| Ready For Develop | 자동으로 집기가 켜져 있으면 러너가 집고, 꺼져 있으면 「지금 실행」을 눌렀을 때 집는다 | 「지금 실행」 |
| In Progress | claude가 조사한다. 시간 제한 120분(기다린 시간은 세지 않는다) | 「중지」(Cancelled로 끝나고 Draft로 되돌릴 수 있다) |
| Waiting Answer | 질문을 기다린다. 질문은 최대 5개이고 다섯 번째 답 뒤에는 `[보고]`를 쓰게 한다 | 상세의 「러너의 질문」 칸에 답. 답은 `**답변**` 댓글로 남고 같은 세션이 이어진다 |
| In Review | 보고가 댓글로 올라와 있다 | 읽고 Done으로 보내거나 되돌린다 |
| Failed | 까닭이 댓글과 기록에 있다 | Draft로 되돌려 다시 Ready로 보낸다 |

세션 기록(`~/.claude/projects/…`)이 사라져 `--resume`이 실패하면(`No conversation found`) 새 세션을 열어 태스크와 댓글(질문과 답이 들어 있다)로 이어 간다. 작업 폴더가 사라졌으면 다시 만든다(읽기 전용이라 「초기화」 대신 다시 만든다).

**앱을 닫을 때(사용자 결정 7-나).** 일하는 실행이 있으면 종료 확인에 「러너가 일하는 태스크가 n개 있어요. 앱을 닫으면 멈췄다가 다음에 앱을 켤 때 이어서 해요」가 붙는다. 닫으면 claude를 멈추되(최대 8초 기다린다) 실행 기록은 `running`으로 두고, 다시 켜면 첫 훑기가 같은 세션을 이어 간다. 그 사이 태스크가 다른 상태로 옮겨졌으면(`cancelled` 등) 이어 가지 않고 `abandoned`로 닫고 작업 폴더를 지운다. 워크스페이스를 바꿀 때는 묻지 않는다(실행은 자기 vault로 계속 돈다).

**저장.** 모두 앱 지원 폴더(`~/Library/Application Support/aisurada/runs/`) 아래에 둔다. vault 안에 두지 않는 까닭은 이 컴퓨터에서만 뜻이 있는 기록(작업 폴더 경로, 세션 id)이라 vault의 git 이력을 부풀리지 않으려는 것이다(사용자 결정 3).

| 파일 | 내용 |
|---|---|
| `settings.json` | `autoPickup`(스스로 집기, **기본 꺼짐**), `concurrency`(동시 실행 1~6, 기본 3), `model`(기본 `sonnet`, 화면에서는 못 바꾸고 파일로만) |
| `<vault 해시>/<TSK-n>/<실행 id>/run.json` | 실행 기록: 단계(`running`·`waiting-answer`·`done`·`failed`·`cancelled`·`abandoned`), 걸음, claude 부른 횟수, 질문 수, 일한 시간, 토큰, 비용, 보고 요약, 오류, 세션 id |
| 같은 폴더의 `events.jsonl` | 걸음별 진행 기록(화면에는 마지막 80줄) |

**명령 6개와 사건.** 열린 vault가 없으면 모두 거절한다.

| 명령 | 하는 일 |
|---|---|
| `runner_status` | 설정, claude 유무, 일하는 스레드 수(`running`), 일하는 실행(`active`), 답을 기다리는 실행(`waiting`) |
| `runner_settings_set` | 설정을 저장한다(범위를 벗어나면 바로잡는다) |
| `runner_run` | 「지금 실행」. Ready가 아니거나 막혀 있거나 동시 한도에 닿았으면 까닭을 돌려준다 |
| `runner_stop` | 일하는 중이면 claude를 멈추고 Cancelled로, 답을 기다리는 중이면 바로 Cancelled로 |
| `runner_answer` | 답을 댓글로 남기고 In Progress로 옮긴 뒤 같은 세션을 잇는다(한도와 무관하게 사람이 누른 일이다) |
| `runner_runs` | 태스크 하나의 실행 기록과 가장 최근 실행의 진행 기록 |

러너가 태스크를 집거나 질문하거나 끝내면 사건 `ais://runner`(`{taskKey}`)를 보낸다. 화면(`ui/runnerStore.ts`)은 이때 상태를 다시 읽고 `version`을 올려서 태스크 목록과 상세가 다시 읽도록 한다.

**화면.** 태스크 페이지 위의 막대(`ui/RunnerBar.tsx`: 러너 상태 알약, 「Ready가 되면 스스로 집기」, 동시 실행 수), 보드 카드의 「▶ 지금 실행」·「■ 중지」·「러너 · 걸음」 표시, 상세의 「실행」 영역(`ui/RunnerPanel.tsx`: 러너의 질문과 답 칸, 일곱 걸음 표시와 걸린 시간, 지난 실행의 결과·시간·토큰·비용·진행 기록)이다. 「실행」은 활동 탭이 아니라 상세 위쪽의 따로 둔 영역이다.

**알림.** 보고는 종류 `pr`(화면 이름 「리뷰」, 설정의 「리뷰할 일이 생겼을 때」)로, 질문은 `question`, 실패는 `failed`로 쌓는다. `id`는 `run-<실행 id>-done`·`-q<번호>`·`-failed`라 같은 일을 두 번 알리지 않는다. 사람이 직접 중지한 것과 앱을 닫아 끊긴 것은 알리지 않는다.

**시험.**

- 호스트: `runner::` 30건. 보고·질문·답·중지·실패(종료 코드, 시간 초과, `is_error`)·질문 한도·세션 소실·작업 폴더 소실·앱 닫기와 다시 켜기 이어 가기·태스크가 옮겨졌을 때의 정리·우선순위와 동시 한도·막힘, 프롬프트와 claude 인자와 MCP 설정, 파일 저장소, 실제 git으로 만든 읽기 전용 작업 폴더를 가짜 엔진·가짜 claude로 돌린다.
- **진짜로 도는 시험** `runner::tests::real_cli_and_claude_report_end_to_end`(무시됨): 진짜 `ais-cli`와 진짜 `claude`(haiku)로 태스크 하나를 끝까지 돌려 In Review와 댓글을 확인한다. 토큰을 조금 쓴다. 격리 vault와 격리 HOME을 만들고 `AISURADA_VAULT`, `E2E_HOME`(격리 HOME), `E2E_CLI_BIN`(빌드한 `ais-cli` 실행 파일), 필요하면 `AISURADA_LOCK_DIR`를 주고 `cargo test real_cli_and_claude -- --ignored --nocapture`로 돌린다. 이 컴퓨터에서 작은 README 조사 하나가 10초, 약 0.007달러였다(Java 시절에 잰 값이다).
- 화면: `runner-ui.test.mjs` 13건(저장소의 읽기·사건·호출 인자, 서버 렌더로 본 막대·「실행」 영역 각 상태·보드 카드), `workspace.test.mjs`의 종료 확인 3건. 새 화면은 서버 렌더하고 Edge headless로 밝은 모양과 어두운 모양에서 그려 눈으로 확인했다.
- **확인하지 못한 것.** 키체인 자격으로 진짜 GitHub 저장소를 fetch하는 길(`refresh_clone`은 시험에서 가짜로 대신했다), 번들한 앱에서 `claude` 위치 찾기, 질문→답 왕복과 동시 실행 여러 개를 진짜 `claude`로 돌리는 것(가짜 대본으로만), 120분 제한, 앱을 닫을 때 진짜 `claude` 프로세스가 멈추는 것, 실제 앱 창(WKWebView)에서 눌러 본 것.
- **한계.** 코드를 고치지 못하고 PR도 없다(4단계). 질문은 위 단계 에이전트가 먼저 풀지 않고 바로 사람에게 간다(5단계). 훑는 주기는 15초라서 Ready로 보낸 뒤 집기까지 그만큼 걸릴 수 있다. 태스크 안 채팅(정의 에이전트)은 만들지 않았다(3b). 러너의 claude 세션은 4부의 세션 화면(`/session/<id>`)에 잇지 않았다.

### GitHub App 연결 (`github.rs`)

프로젝트의 GitHub 저장소(앞의 「프로젝트의 저장소와 아젠다의 `projectRepo`」)를 앱이 읽고 받아 오는 호스트 쪽 바탕이다. 사용자가 GitHub에서 직접 만든 GitHub App의 **App ID와 개인 키(PEM)** 로 **설치 토큰**을 받는 방식이고, 사용자 로그인(디바이스 플로우)은 쓰지 않는다. 활동은 앱(봇) 이름으로 남는다.

- **GitHub와 말하는 곳은 이 모듈뿐이다.** 화면의 CSP(`connect-src 'self' ipc:`)는 그대로고, 엔진과 에이전트(MCP)는 토큰을 볼 수 없다. HTTP는 `ureq`(rustls), RS256 서명은 `ring`, 키체인은 `security-framework`다. JWT는 base64url 세 조각이라 직접 만든다(`iat`는 60초 앞당기고 10분 안에 만료).
- **보관.** 개인 키(PEM)는 키체인(서비스 `co.kcd.aisurada.github-app`, 계정 `private-key`)에만 둔다. App ID, 앱 이름과 slug는 `~/Library/Application Support/aisurada/github-app.json`에 둔다. 설치 토큰은 설치 id별로 메모리에만 두고 만료 5분 전부터는 새로 받는다.
- **명령(`invoke`).** `github_status`(네트워크 없이 연결 여부), `github_connect(appId, pemPath)`(키는 파일 경로로만 받아 호스트 안에서 읽고 화면으로 돌려보내지 않는다. 키로 `GET /app`을 불러 App ID가 맞을 때만 저장한다), `github_disconnect`, `github_repositories`(설치된 모든 계정의 저장소. 설치 하나가 실패해도 나머지는 주고 실패는 `errors`에 적는다), `github_clone(fullName)`.
- **clone.** `~/Library/Application Support/aisurada/repos/<소유자>/<이름>`에 받는다. 폴더에 `.git`이 있으면 GitHub를 부르지 않고 그대로 쓴다. 이름은 폴더로 쓰기 전에 `owner/name` 모양(`..`, 빗금, 앞의 `-` 거절)을 검사한다. 토큰은 명령줄(`ps`에 보임)이 아니라 환경변수(`GIT_CONFIG_*`로 `http.extraheader`)로만 싣고, 키체인 자격 증명 도우미가 토큰을 저장하지 못하게 `credential.helper`를 비운다. 실패하거나 시간(10분)이 넘으면 반쯤 받은 폴더를 지운다. 이미 있는 로컬 폴더를 쓰려면 프로젝트 저장소 항목의 `path`를 쓴다. 화면은 `path`가 있는 항목은 받지 않는다(`entriesToClone`).
- **화면.** 모든 호출은 `githubStore.ts`가 한다. 이 모듈은 연결 상태(`github_status`가 주는 `connected`, 앱 이름, `cloneRoot`)와 저장소별 받기 상태(`cloning`·`done`·`error`)를 한 곳에 두고 설정, 프로젝트 폼, 아젠다 화면이 같이 본다. 같은 저장소를 받는 중에 다시 부르면 그 결과를 같이 기다린다. 앱이 켜질 때(`App.tsx`) 한 번 상태를 읽어 와서, 폴더 없는 GitHub 항목의 폴더를 처음부터 풀 수 있다.
  - **설정의 「GitHub 연결」**(`GithubConnect.tsx`). App ID를 적고 개인 키 파일(`.pem`)을 고르면 `github_connect`를 부른다. 키 내용은 화면에 오지 않고 경로만 넘어간다. 연결되면 입력을 비우고 키체인에 들어갔으니 파일을 지워도 된다고 알린다. 연결을 끊으면 키체인의 키와 앱 정보가 지워지고, 이미 받은 저장소 폴더는 그대로다.
  - **받기.** 프로젝트를 저장하고 나서 폴더 없는 새 GitHub 항목을 백그라운드로 받고, 끝나면 알림으로 결과(받음, 이미 있음, 실패 이유)를 알린다. 편집기의 각 항목에는 상태 글과 「지금 받기」가 있다. 연결이 없으면 호스트를 부르지 않고 설정에서 연결하라는 글을 보인다.
  - **시험.** `scripts/repo-links.test.mjs`(순수 함수, 가짜 호스트로 본 상태 흐름, 서버 렌더로 본 글과 단추). 실제 창에서 눌러 본 확인은 없다.
- **시험.** 기본 시험 9개는 가짜 GitHub 서버(로컬 TCP)와 로컬 git 저장소로 JWT 서명 검증, App ID 검사, 설치별 토큰 재사용과 만료 교체, 쪽 나누기 없는 목록, clone과 토큰 흔적 없음을 잰다. 개인 키는 시험 전용 일회용 키다. `#[ignore]` 시험 두 개는 실제 키체인(`keychain_round_trip`)과 실제 GitHub(`real_github_smoke`, 환경변수 `AISURADA_REAL_APP_ID`, `AISURADA_REAL_PEM_FILE`, `AISURADA_REAL_REPO`)를 쓰고 `cargo test -- --ignored`로만 돈다.
- **확인한 것과 못 한 것.** 실제 키체인의 저장·읽기·덮어쓰기·삭제와, 실제 GitHub의 연결·목록·clone을 확인했다. 확인한 저장소(`bnjjong/aisurada`)가 공개 저장소라서 **비공개 저장소를 토큰으로 받는 길은 실제로 증명하지 못했다.** 쓰기(push, PR)는 이 모듈에 없다. 설치 하나에 저장소가 100개를 넘는 쪽 나누기와 20쪽 상한, 호출 한도는 실제로 겪어 보지 못했다.

### 앱이 연 vault (`current-vault.json`)

에이전트의 MCP 서버를 등록할 때 `AISURADA_VAULT` 경로를 박아 넣으면, 손 확인이나 옛 설정의 vault가 그대로 남아 노트가 앱과 다른 vault에 쓰인다. 그래서 등록에는 vault를 박지 않고, 앱이 연 vault를 파일에 적어 둔다.

- **쓰는 쪽.** 호스트(`workspace.rs`의 `allow_workspace`)가 워크스페이스를 열 때마다 `~/Library/Application Support/aisurada/current-vault.json`에 `{"vault":"<경로>","updatedAt":<ms>}`를 적는다(임시 파일에 쓰고 이름을 바꾼다). 못 적어도 vault 열기는 막지 않는다. 호스트 시험은 진짜 HOME의 파일을 덮어쓰지 않게 이 쓰기를 건너뛴다(`cfg!(test)`).
- **읽는 쪽.** `ais-cli`(`engine-rs/crates/ais-cli/src/env.rs`의 `resolve_vault`)와 MCP 서버가 호출마다 읽는다. 환경변수가 비지 않았으면 그것이 이기고, 없으면 파일이다. 둘 다 없으면 `AIS-400 vault를 정하지 못했다…`다. 파일이 망가졌거나 절대 경로가 아니면 없는 것으로 본다.
- **엔진은 그대로다.** 엔진 자체(`ais-core`)는 늘 vault 경로를 인자로 받는다. 어느 vault인지 정하는 일은 `ais-cli`(`env.rs`)가 한다.
- **격리.** 파일은 HOME 아래라서 가짜 HOME으로 띄운 앱(손 확인, 받아들임 점검, 번들 검증)은 자기 HOME에 적고 진짜 파일을 안 건드린다.
- **한계.** 앱을 한 번도 안 열었으면 파일이 없다. 앱을 끄고 다른 vault로 `AISURADA_VAULT`를 준 `ais-cli`를 돌려도 파일은 그대로다(앱이 마지막으로 연 vault).

### 호출 방식

`ais-cli`는 명령 1개를 처리하고 끝나는 프로세스다. 엔진은 그 프로세스 안에서 돌고, 호출마다 vault를 새로 훑는다. 표준 입출력으로 오래 도는 것은 MCP 서버(`ais-cli mcp`)뿐이다. 앱은 같은 코드를 프로세스 안에서 직접 부른다(§4 「앱의 호출 길」). 예전의 serve 모드와 launchd 사이드카는 없다.

### 두 가지 표면

같은 일을 이름만 달리 부른다. 도구 표면은 밑줄 이름에 JSON 한 덩이, 명령 표면은 하이픈 이름에 `key=value`다.

```bash
AISURADA_VAULT=<vault> dist/ais-cli get_doc '{"id":"nddemo-00000001"}'   # 도구 표면
AISURADA_VAULT=<vault> dist/ais-cli get-doc nddemo-00000001              # 명령 표면
```

도구 38개와 명령 82개는 `engine-rs/crates/ais-cli/src/registry.rs`에 있다(`ais-cli`가 직접 하는 8개는 따로다). 도구는 모두 명령에 짝이 있지만, 아젠다·프로젝트·사실층·4부 따위는 명령 표면에만 있다. 이름 목록은 `dist/ais-cli --list`로 본다.

### 설정

설정은 환경 변수로만 준다. Java 시절의 명령줄 `--키=값`과 JVM `-D키=값`, 색인 폴더 `AISURADA_DB_HOME`은 없어졌다(색인이 없다).

| 환경 변수 | 쓰임 | 기본값 |
|---|---|---|
| `AISURADA_VAULT` | vault 폴더 | 없으면 앱이 연 vault(아래 「앱이 연 vault」) |
| `AISURADA_NOTES_SHARD_BY_MONTH` | 노트를 월별 폴더에 두는지 | 정하지 않음(디스크를 보고 판정) |
| `AISURADA_VAULT_AUTOCOMMIT_ENABLED` | vault 자동 커밋 | true |
| `AISURADA_VAULT_AUTOCOMMIT_IDLE_SECONDS` | 마지막 쓰기부터 커밋까지의 유휴 초 | 300 |

불리언은 `true/on/yes/1`과 `false/off/no/0`만 받는다. 본문을 덮기 전에 옛 바이트를 남기던 스냅숏 쓰기(`AISURADA_VAULT_SNAPSHOT_ENABLED`)는 Rust 이전 때 지웠다. 이미 남은 스냅숏은 `history`가 계속 읽는다.

그 밖의 환경 변수:

| 이름 | 쓰임 |
|---|---|
| `AISURADA_LOCK_DIR` | 쓰기 잠금 파일 폴더. 기본은 `~/Library/Application Support/aisurada/locks` |
| `AISURADA_APPROVALS_DIR` | 자원 승인 기록의 바탕 폴더(시험용). 기본은 `~/Library/Application Support/aisurada/approvals` |
| `AISURADA_SESSION_ID`, `AISURADA_SESSION_AGENT` | 이 호출을 보낸 에이전트 세션. 둘을 함께 줄 때만 쓴다 |
| `AISURADA_CLAUDE_BIN`, `AISURADA_CODEX_BIN`, `AISURADA_GEMINI_BIN`(또는 `AISURADA_ANTIGRAVITY_BIN`) | 등록 명령(`claude-mcp` 따위)과 호스트가 부를 CLI의 자리 |
| `AISURADA_CLAUDE_HOME`, `AISURADA_CODEX_HOME`, `AISURADA_GEMINI_HOME` | 세션 기록을 읽을 홈을 바꾼다(시험용) |
| `AISURADA_SQLITE_BIN` | Antigravity 대화 기록을 읽을 `sqlite3`의 자리. 기본은 `/usr/bin/sqlite3`, 없으면 `PATH` |
| `AISURADA_TEST_NOW`, `AISURADA_TEST_RANDOM` | 시험용 시계(유닉스 밀리초)와 id 무작위 조각. 주지 않으면 아무 일도 안 한다 |
| `AISURADA_CLI_BIN` | 호스트 시험과 점검이 부를 `ais-cli` 실행 파일(없으면 `engine-rs/target/debug/ais-cli`) |
| `AISURADA_ENGINE_TRACE` | 손 확인용. `1`이면 앱을 띄운 터미널에 `[engine] 명령 ok 걸린시간ms`를 한 줄씩 찍어 어느 명령이 `engine_call` 길로 갔는지 보인다 |
| `AISURADA_CALL_LOG` | 점검 하네스용. 앱이 엔진을 부른 호출을 이 파일에 한 줄씩 적는다(§8) |

### 종료 코드

| 코드 | 뜻 |
|---|---|
| 0 | 성공. stdout에 JSON 하나와 줄바꿈 1개 |
| 1 | 예상 밖 실패(인자 길이가 1 MiB를 넘는 경우 따위). 쓰기 명령이 vault 사전 훑기에서 서면 이것도 1 |
| 2 | vault를 못 찾았거나 모르는 명령 |
| 3 | `AIS-…` 코드가 붙은 실패. stderr 한 줄, stdout 비어 있음 |
| 5 | 선결 조건 토큰(`f1`)이 어긋나 아무것도 안 씀(`AIS-412`) |

Java 시절에 `ais-cli`만 내던 4(엔진에 못 닿음)는 엔진 프로세스가 없어서 사라졌다.

### vault의 모양

```
vault/
├── .aisurada/
│   ├── workspace.json          이게 있어야 쓰기 명령이 돈다 (init-workspace가 만든다)
│   ├── agendas/<slug>.md       아젠다 파일 (프론트매터 id, slug, title)
│   ├── projects/<slug>.md
│   ├── facts/<slug>.md         아젠다 사실층
│   ├── templates/{doc,ritual}/
│   └── trash/                  휴지통
└── notes/<아젠다 slug>/<노트 폴더>/
    ├── note.md                 프론트매터(id, agenda, type, title, created, tags …) + 본문
    ├── attach/                 글·그림 첨부
    ├── sub/<하위 노트>/note.md  하위 노트(재귀)
    ├── ask.json, chats/        4부 문답과 레일 대화
    └── …
```

HTML 노트와 템플릿은 `---` 대신 `<!--aisurada` … `-->` 머리를 쓴다.

프론트매터에는 소요 시간(`mins`)이 없다. 사용자 결정(2026-11-07)으로 지웠다. 새 노트는 `mins:` 줄을 쓰지 않고, 읽기 응답에도 `mins`가 없고, `create-doc`과 `update-doc`은 `mins=`를 「모르는 옵션」으로 거절한다. 옛 파일에 남은 `mins:` 줄은 읽을 때 무시하고, 그 노트를 다음에 쓰면 사라진다(그 쓰기에서 편집 도장도 새로 찍힌다). 캘린더 일 보기는 모든 일정을 같은 길이(겹침 계산 60분, 높이 고정 72px, 1분당 1.2px, 제목 3줄 말줄임, 겹치는 묶음별 동적 칸 수)로 다룬다.

### 쓰기 규칙 (고칠 때 지킬 것)

- 쓰기는 같은 폴더의 임시 파일에 쓴 뒤 **원자적 이름 바꾸기**로 둔다. 실패하면 다른 길로 다시 하지 않고 `AIS-500`으로 선다.
- `update_doc`, 메타 쓰기 3개, `update_sub_note`는 직전에 읽은 토큰 `f1`이 지금 파일 바이트와 맞아야 쓴다. 안 왔거나 어긋나면 `AIS-412`, 종료 코드 5.
- 편집 도장(`editedBy`, `editedAt`)은 파일 글이 실제로 달라질 때만 새로 찍는다.
- 쓰기 명령은 일하기 전에 vault를 훑고, 노트로 못 읽는 파일이 있으면 아무것도 안 쓰고 종료 코드 1로 선다.
- 자동 커밋은 쓸 때마다 대기 기록(`vault/.git/aisurada-pending.json`)을 남기고, 마지막 쓰기 뒤 유휴 시간이 지나거나, 계속 써서 유휴가 오지 않아도 처음 쓴 지 15분이 지나거나(`OVERDUE_MILLIS`), 작성자·도구 세션이 바뀌면 커밋한다. `.git`이 있는 `main` 브랜치에서만 돈다. 유휴 타이머는 앱과 MCP 서버만 돌리고(`ais_core::write::autocommit::watch`), 단발 `ais-cli`는 기록만 남기고 끝난다. 남은 기록은 다음에 도는 앱이나 MCP 서버가 이어받는다.

### 명령을 새로 넣는 길

1. `engine-rs/crates/ais-core/src/commands/`에 실행기를 만든다. 쓰기 공통 부품은 `write/`에 있다.
2. `engine.rs`의 명령 이름 표(쓰기면 `WRITE_COMMANDS`)에 넣는다.
3. `engine-rs/crates/ais-cli/src/registry.rs`(명령 이름 목록, 도구 표면이 있으면 `CLI_TOOL_NAMES`와 사용법 줄)와 `spec_table.rs`(인자 명세)에 넣는다. 빠뜨리면 `ais-cli`가 「알 수 없는 이름」으로 거절한다.
4. 시험을 `engine-rs/crates/ais-core/tests/`나 `crates/ais-cli/tests/`에 두고 `bash test.sh`로 돌려 본다(§8).
5. 설명서 장이 있으면 그 장의 바이트 예시와 맞는지 대조하고, 받아들임 묶음에 케이스를 더할 수 있으면 더한다(§8).

### MCP 서버 (`ais-cli mcp`)

Claude 같은 MCP 클라이언트가 노트를 도구로 쓰게 하는 모드다. 표준 입출력으로 한 줄에 JSON-RPC 메시지 하나씩 주고받고(`initialize`, `ping`, `tools/list`, `tools/call`), 입력이 닫히면 끝난다. 프로토콜은 `engine-rs/crates/ais-cli/src/mcp.rs`, 도구 정의는 `mcp_tools.rs`에 있다. 이것은 설명서 밖의 확장이다. 외부 설명서에는 MCP가 없고, 예전 Node MCP 서버는 C119에서 지웠다.

연결하는 법, 도구의 인자, 안전장치, 응답과 오류 예시는 [`mcp-guide.md`](mcp-guide.md)에 따로 정리했다. 이 절은 만드는 쪽 이야기만 적는다.

- **도구.** 이름은 엔진 도구 38개와 같다. 기본은 읽기 19개만 내놓고 `--write`를 주면 37개를 내놓는다(둘 다 `tools/list`로 직접 세어 확인했다). `purge_doc`(영구 삭제)는 어느 쪽에도 없다. 모델이 되돌릴 수 없는 삭제를 하지 못하게 하려는 것이다. 읽기와 쓰기의 구분은 `mcp_tools.rs`의 `write` 깃발에 있다.
- **호출 길.** 도구 호출은 단발 `ais-cli`와 같은 길(`ais_cli::run`)을 프로세스 하나에서 되풀이한다. 명령줄 길이 표준 출력·오류에 쓰는 글은 호출마다 모아서 돌려주므로 프로토콜 출력에 섞이지 않는다. 도구를 부를 때마다 자동 커밋을 지켜본다(`watch`). 유휴 타이머가 MCP 서버 안에서 돌기 때문이다.
- **vault.** 도구를 부를 때마다 정한다. `AISURADA_VAULT`가 있으면 그것, 없으면 앱이 지금 연 vault(위 「앱이 연 vault」)다. 서버 하나는 한 번에 vault 하나를 맡지만 호출마다 바뀔 수 있다.
- **스키마를 고칠 때.** `mcp_tools.rs`의 입력 스키마 키는 명령줄 인자 명세(`spec_table.rs`)가 받는 키와 같아야 한다. 도구의 인자를 바꾸면 두 표와 `mcp_compat`의 기대를 함께 맞춘다.
- **Claude에 연결.** `ais-cli claude-mcp status|add [--write]|remove|config`(`register.rs`)가 Claude Code에 서버를 등록한다. Codex와 Antigravity·Gemini CLI는 `codex-mcp`, `gemini-mcp`가 같은 방식으로 한다. 셋 다 등록에 vault를 박지 않는다. CLI마다 `--` 자리가 달라서(codex와 agy는 이름 바로 뒤, gemini는 실행 파일 경로 뒤) 인자 모양은 `crates/ais-cli/tests/register.rs`가 가짜 CLI로 못 박는다. Claude의 설정 파일을 직접 고치지 않고 `claude mcp add-json --scope user`만 부른다. 등록하는 실행 파일은 이 명령을 돌린 `ais-cli`라서, 앱에서 부르면 앱에 든 것이 등록된다. `config`는 등록하지 않고 Claude Desktop 설정에 붙여 넣을 `mcpServers` 조각만 출력한다. 설정 화면의 「Claude 연결」(`src/plugs/claude/connect.tsx`)은 `sidecar_call`로 이 명령을 부르기만 하므로 호스트 커맨드를 더하지 않았다(4부 9개 그대로). 4부를 끈 판에는 이 화면이 없다(`plugs/empty/connect.tsx`). 연결은 지금 연 워크스페이스 하나에 묶이고, 다른 워크스페이스에서 다시 연결하면 바뀐다. 이미 등록된 서버가 옛 Java jar를 가리키면 `doctor`가 낡았다고 알리고 `doctor --fix`가 같은 모드로 다시 등록한다.

시험은 `bash test.sh`에 들어 있다. `crates/ais-cli/tests/mcp_stdio.rs`는 진짜 `mcp` 프로세스와 표준 입출력으로 왕복하고, `tests/register.rs`는 가짜 에이전트 CLI로 `status`·`add`·`remove`·`config`를 잰다. 기준 Java MCP 서버와의 대화 맞대기는 `mcp_compat`(§4)다.

## 6. 옛 사이드카와 launchd — 건드리기 전에

Rust `ais-cli`는 launchd를 쓰지 않는다. 다만 Java 시절에 이 기기에서 `ais-cli`나 앱을 쓴 적이 있으면 vault마다 launchd에 올라간 옛 사이드카(레이블 `co.kcd.aisurada.sidecar.<키>`, `~/Library/LaunchAgents/…plist`)가 남아 있을 수 있다. 낡은 JVM이 계속 떠 있고 옛 엔진이 vault를 고칠 수 있으므로 걷어 낸다. 정식 앱은 새 판을 처음 열 때 한 번 하고(§4 「앱의 호출 길」), 손으로는 이렇게 한다.

```bash
dist/ais-cli doctor            # 어느 vault를 쓰는지, vault의 git 상태, 이 실행 파일, 에이전트에 등록된 MCP 서버, 옛 사이드카 흔적
dist/ais-cli sweep --dry-run   # 걷어 낼 것만 보여 준다
dist/ais-cli sweep             # 옛 사이드카를 내리고 plist, 상태 파일, 깔아 둔 jar, 로그를 지운다
dist/ais-cli doctor --fix      # 낡은(Java jar를 가리키는) MCP 등록을 같은 모드로 다시 등록한다
```

`sweep`은 레이블이 `co.kcd.aisurada.sidecar.`로 시작하는 항목만 내린다. vault, 잠금 파일, 다른 프로그램의 launchd 항목은 건드리지 않는다. 시험(`crates/ais-cli/tests/old_sidecar.rs`)은 임시 HOME과 가짜 `launchctl`(`AISURADA_LAUNCHCTL_CMD`)로만 돌아서 진짜 launchd는 쓴 적이 없다. 진짜 기기에서 `sweep`을 돌려 본 확인은 사용자의 손 확인(이슈 #54, #61)에 남아 있다.

앱을 격리해서 띄우는 전체 순서는 [`beta-hand-check.md`](beta-hand-check.md)의 §1에 있다.

## 7. 네이티브 호스트와 화면

### 개발 실행

```bash
(cd engine-rs && cargo build -p ais-cli)   # 개발용 번들의 bin/ais-cli 스크립트가 찾는 실행 파일
cd native-host
pnpm install --frozen-lockfile      # postinstall이 @claude-plug 꾸러미를 꽂는다
pnpm tauri dev                      # 실제 앱까지 (포트 1420, 충돌하면 실패)
pnpm dev                            # Vite 서버만
pnpm preview                        # 호스트 없이 번들 CSP로 화면만 본다
```

`pnpm tauri dev`는 README에 적힌 대로 쓰는 길이고 이 문서를 쓰면서 새로 돌려 보지는 않았다. 개발 서버에 페이지가 붙어 있으면 `curl -X POST localhost:1420/__deeplink --data-raw 'aisurada://doc/<id>'`로 열린 main 창을 이동할 수 있다(개발 판 전용, 번들에서는 빠진다).

앱은 엔진을 프로세스 안에서 직접 부르므로 엔진을 따로 띄우거나 등록할 것이 없다. 에이전트에 MCP 서버로 등록할 `ais-cli` 실행 파일은 환경 변수 `AISURADA_CLI_BIN`, 앱 번들의 `bin/ais-cli`, 저장소의 빌드 폴더(`engine-rs/target/{release,debug}/ais-cli`) 순으로 찾는다(`sidecar.rs`의 `cli_exe`). **개발 실행과 `target/…/bundle` 아래 번들은 기기 전체에 영향을 주는 첫 실행 이주(옛 사이드카 정리, MCP 재등록)를 하지 않는다**(`sidecar.rs`의 `under_build_tree`). 그래서 빌드 폴더의 앱을 진짜 vault로 띄워도 정식 앱이 해 둔 등록은 바뀌지 않는다. 그래도 앱은 진짜 HOME의 vault를 열 수 있으니 격리해서 띄우려면 [`beta-hand-check.md`](beta-hand-check.md)의 격리 실행법을 쓴다.

### 번들 만들기

```bash
cd native-host
pnpm tauri build --debug --bundles app   # 산출: src-tauri/target/debug/bundle/macos/aisurada.app
pnpm release:build                       # 정식 번들(릴리스 설정). 아래 「앱에 든 ais-cli」
```

`--debug`를 빼면 최적화 빌드다. `beforeBuildCommand`는 화면만 새로 짓는다(`pnpm build`). Rust `ais-cli`는 개발용 번들에 지어 넣지 않는다(아래). **설치나 launchd 등록은 하지 않는다.** 단 번들은 `.md`/`.markdown`의 기본 편집기로 등록되므로(`fileAssociations`) `open 문서.md`가 이 앱을 진짜 HOME으로 띄운다.

### 앱에 든 `ais-cli`

번들의 `Contents/Resources/bin/ais-cli`는 두 가지다.

- **개발용 번들**(`target/…/bundle`): `native-host/src-tauri/resources/ais-cli`(셸 스크립트)가 들어간다(`tauri.conf.json`의 `resources`). 저장소의 빌드 폴더(`engine-rs/target/{release,debug}/ais-cli`)나 `AISURADA_CLI_BIN`으로 넘겨 주고, 없으면 종료 코드 127로 선다. PATH가 비어 있어도(Finder나 launchd가 띄운 환경) 돌아야 하므로 셸 내장 기능만 쓰고, 외부 명령은 절대 경로로 부르는 `readlink` 하나뿐이다. 심볼릭 링크로 불러도 앱 안의 자리를 찾는다.
- **정식 번들**(`pnpm release:build`, 릴리스 설정 `tauri.release.conf.json`): `scripts/prepare-cli.mjs`가 `cargo build --release --locked -p ais-cli`로 지은 Rust 실행 파일을 `.release/bin/ais-cli`에 두고, 이것이 `bin/ais-cli`로 들어간다. 실행 파일 하나(약 4.5 MB)뿐이고 Java 실행 환경이나 jar는 싣지 않는다. 정식 번들은 Apple Silicon macOS에서만 짓는다.

호스트는 앱이 뜰 때 이 폴더를 `ask.rs`의 `set_tools_dir`로 알려 두고, `claude -p`를 띄울 때 PATH 맨 앞에 얹는다. 그래서 앱 안에서 묻는 Claude는 사용자가 래퍼를 만들지 않아도 `ais-cli`를 부를 수 있다. 앱 안 터미널(`pty_start_session`)의 PATH는 바꾸지 않았다. 사용자의 로그인 셸 환경을 따른다. 터미널에서 쓰려면 연동 가이드 2단계의 링크 한 줄이면 된다. 예전 가이드로 만든 `~/.local/bin/ais-cli`(Java `ais-cli.jar`를 돌리는 래퍼)가 있어도 앱이 띄우는 Claude에게는 번들 것이 먼저 잡힌다.

시험은 스크립트 `test:cli-script`(가짜 앱 폴더에서 진짜 셸 스크립트를 돌린다. PATH가 비었을 때, 링크로 불렀을 때, 개발용 빌드일 때, 실행 파일을 못 찾을 때 포함), PATH 얹기 `ask.rs`의 `bundled_tools_dir_comes_first_only_when_it_exists`, 실제 번들 `scripts/verify-release.mjs`(번들의 Rust `ais-cli`로 노트를 만들고 읽는지, 표준 입출력 MCP가 도는지, Java가 안 들었는지)다. 번들을 검증하는 데 쓴 임시 HOME과 vault는 만든 쪽이 지운다.

### 4부 스위치 2개

| 층 | 켠 판(기본) | 끈 판 |
|---|---|---|
| 엔진 | 항상 켜짐. 4부 명령 7개도 `ais-cli`에 있다 | 없음. 옛 `ais-sidecar-noclaude.jar`는 Java와 함께 지웠다 |
| 호스트 | cargo 기능 `claude` (기본) | `--no-default-features` (4부 커맨드 9개가 빠짐) |
| 화면 | `pnpm build`, `pnpm dev` | `pnpm build:noclaude` (`vite build --mode noclaude`) |

화면은 `@claude-plug/*` 별칭을 모드에 따라 `src/plugs/claude/` 또는 `src/plugs/empty/`로 돌린다. 빌드 뒤에는 `scripts/screen-contracts.mjs`가 번들을 검사한다. 끈 판에는 4부 커맨드 이름이 하나도 없어야 하고, 켠 판에는 하나 이상 있어야 한다.

### 화면 모양 (skin)

설정 → 「화면 모양」에서 **물성**(기본값), **격자**, **유리**, **모던**, **기본**(이전 모양) 중에서 고른다. 밝게·어둡게(`data-theme`, 레일의 ◐)와는 따로 고르므로 모든 모양에 밝게와 어둡게가 있다.

| 무엇 | 어디 |
|---|---|
| 저장 | `aisurada.prefs.v1`의 `state.skin`(`'tactile'`·`'grid'`·`'glass'`·`'modern'`·`'classic'`). 16장의 설정 21개에 들어가지 않는 **확장 값**이라, 고르기 전에는 `readPreferences` 결과에 나타나지 않는다. `readSkin`이 읽고, 없거나 모르는 값(없앤 `'page'` 포함)이면 물성이다 |
| 적용 | `<html data-skin>`. `main.tsx`가 첫 그리기 전에 걸고, `App.tsx`가 `state.skin`을 따라간다. 다른 창은 `storage` 이벤트와 포커스에서 `refreshTheme`으로 따라온다. 노트 창은 설정을 쓰지 않는다 |
| 기본 모양 | `src/styles.css`. 설정의 모양 고르기 카드(`.set-skin*`, `.skin-preview`)도 여기에 둔다. 카드의 미리보기 그림은 지금 켠 모양과 상관없이 각자의 모양을 그린다 |
| 물성 | `src/skin-tactile.css`. 눌리는 것은 떠 있고 쓰는 것은 파인 둥근 모양. 모든 규칙이 `:root[data-skin="tactile"]` 아래에 있어서 다른 모양에는 영향이 없다. 파일 맨 위에 다섯 규칙(깊이가 역할, 반경은 높이의 절반, 선 대신 면, 위에서 한 방향의 빛, 회색에도 색)과 토큰이 있다 |
| 격자 | `src/skin-grid.css`. 반경 2px, 그림자 없음, 네이비 먹선 상자. 켜짐·선택은 네이비로 채우고 빨강은 포커스·현재 위치·눌림에만 쓴다. 「모든 요소 반경 2px·그림자 없음」을 한 줄(`:root[data-skin="grid"] *`)로 덮고 원이 필요한 몇 곳만 되돌린다 |
| 유리 | `src/skin-glass.css`. 반투명 흰 면(`--surface`)에 흐림(`backdrop-filter`)을 건 패널, 배경에는 블루·시안·레드의 빛번짐(`:root`의 `radial-gradient`). 컨트롤 반경 12px, 패널 24px, 강조는 블루 유리구슬. 글자 색은 불투명하게 둬서 대비를 지킨다. `--surface`가 반투명이라 이 값을 불투명한 바탕으로 쓰는 새 요소는 `--glass-solid`를 쓴다 |
| 모던 | `src/skin-modern.css`. 1px 옅은 선, 곡률 8px(떠 있는 것은 12~14px), 검정에 가까운 네이비와 회색, 블루 하나는 포커스·선택에만. 그림자는 평소 1px과 떠 있는 것 두 가지뿐이고 탭은 아래 줄로 현재 위치를 말한다 |

**지킬 것.**

- 전역 규칙(`button`, `input` …)은 `:where()`로 특이도를 낮춰 썼다. 그래야 `styles.css`의 개별 클래스(`border: 0`, `background: transparent`)를 덮어쓰지 않는다.
- `styles.css`에서 `background: transparent`나 `border: 0`으로 맨살로 만든 버튼을 새로 더하면 네 스킨 파일(`skin-tactile.css`·`skin-grid.css`·`skin-glass.css`·`skin-modern.css`)의 「맨살 버튼」 선택자 목록에 모두 넣는다. 빠뜨리면 그 스킨에서 그 버튼만 상자나 알약, 유리 조각으로 보인다.
- 물성은 토큰(`--bg`, `--soft`, `--surface`, `--sunken`, `--accent` …)을 바꿔서 기본 모양의 규칙까지 같이 물든다. 기본 모양의 색을 `#rrggbb`로 박은 곳은 물성에서도 그 색이다.
- 설정 화면 인수 시험이 `.set-switch`를 3개로 센다. 모양 고르기 카드는 그래서 다른 클래스(`.set-skin-option`)를 쓴다.
- 격자는 모든 요소의 반경과 그림자를 한 줄로 덮는다. 설정 카드의 미리보기 그림은 그 규칙에 흔들리지 않도록 `styles.css`에서 반경·그림자에만 `!important`를 쓴다.
- 유리는 `backdrop-filter`를 큰 패널에만 건다. 버튼마다 걸면 느려진다. 맨살 버튼과 일반 버튼은 반투명 배경만 쓴다.

새 모양을 더하려면 `types.ts`의 `Skin`, `storage.ts`의 `SKINS`, `Settings.tsx`의 `SKIN_OPTIONS`, `src/skin-<이름>.css`와 `main.tsx`의 import, 미리보기 그림 규칙(`styles.css`)을 함께 고친다. 시험은 `scripts/chapters14-17.test.mjs`의 「화면 모양」과 `scripts/views-checks.js`의 설정 점검(다섯 모양을 차례로 눌러 `data-skin`과 저장 값을 잰다)이다.

### 글꼴과 화면 크기 (typography)

설정 → 「글꼴과 크기」에서 **화면 크기**(90·100·110·125·150·175%), **앱 글꼴**, **터미널 글꼴**을 고른다. 글꼴은 설치된 글꼴 이름 하나나 쉼표로 이은 CSS 글꼴 목록이고, 비우면(「기본값」) 기본 글꼴이다. 화면 크기는 ⌘ + / ⌘ - / ⌘ 0(Ctrl도 됨)으로도 바꾼다.

| 무엇 | 어디 |
|---|---|
| 저장 | `aisurada.prefs.v1`의 `state.uiScale`(위 여섯 값만), `state.uiFont`, `state.terminalFont`. 화면 모양(skin)처럼 16장의 설정 21개 밖 **확장 값**이라 고르기 전에는 `readPreferences` 결과에 나타나지 않는다. `workspace/typography.ts`의 `readTypography`가 읽고, 이상한 값은 항목별로 기본이다. 글꼴 글은 `; { } < > \`와 주석 표시, 200자 초과를 받지 않는다 |
| 앱 글꼴 | `styles.css`의 `--font-ui-base`(기본 목록)와 `--font-ui`(`:root`의 `font-family`가 읽는다). 고르면 `<html>`에 `--font-ui: "고른 글꼴", var(--font-ui-base)`를 덧씌우고, 비우면 지운다(`applyTypographyVars`). 쉼표·따옴표가 없는 이름은 한 글꼴로 보고 따옴표로 감싸고(`Source Han 2`처럼 공백·숫자가 있어도 되게), 끝에 기본 목록이 붙어서 고른 글꼴에 없는 글자(한글 등)는 기본 글꼴이 채운다. `main.tsx`가 첫 그리기 전에 걸고 `App.tsx`가 상태를 따라간다 |
| 터미널 글꼴 | `--font-terminal`(기본은 `var(--font-mono)`). `TerminalHost.tsx`가 이 변수를 읽는다(전에는 `--font-mono`). 바꾸면 `ais:typography` window 이벤트로 열려 있는 터미널이 `options.fontFamily`를 바꾸고 칸 수를 다시 잰다. 터미널이 쥔 글꼴은 `.term-screen`의 `data-font-family`에도 적는다(WebGL로 그리면 글꼴이 DOM에 안 드러나서 실제 창 점검이 이것을 읽는다). 노트의 코드 블록은 이 설정을 따르지 않는다(`--font-mono`와 고정 목록 그대로) |
| 화면 크기 | 웹뷰 확대다. `services.setZoom` → Tauri `getCurrentWebview().setZoom(배율 / 100)`(macOS는 WKWebView의 `pageZoom`). `core:webview:allow-set-webview-zoom` 권한이 `capabilities/main.json`과 `note.json`에 있다(`contracts.test.mjs`가 두 목록이 같은지 잰다). CSS `zoom`은 쓰지 않았다. 앱이 아닌 환경(브라우저, 시험)에서는 `setZoom`이 없어 글꼴만 바뀐다 |
| 단축키 | `interaction/shortcuts.ts`의 `zoomShortcutFor`(Cmd/Ctrl과 +·-·0, 키 이름과 물리 키 이름을 모두 본다). `controller.handleKey`가 탭 단축키보다 먼저, 어느 화면에서나 듣는다. 한 단계는 `stepScale`(맨 끝에서 멈춤, 0은 100%) |
| 창마다 | 메인 창은 바꾼 값을 저장소에 쓴다. 노트 창은 설정을 공유 저장소에 쓰지 않는 규칙(15.14) 때문에 단축키로 바꾼 배율이 그 창에만 남고(`controller`의 `localScale`), 글꼴은 저장소(`storage` 이벤트, 포커스)를 따라간다. 설정 화면은 메인 창에만 있다 |
| 화면 | `ui/TypographySettings.tsx`. 글꼴 입력은 500ms 멈췄을 때, 칸을 떠날 때, Enter에서 저장한다(글자마다 저장하면 입력 중간의 이름이 그대로 적용된다). 쓸 수 없는 글이면 오류 문구를 보이고 직전 값을 지킨다. 배율 단추는 일반 `button`에 `aria-pressed`라 스킨의 단추 모양을 따르고, 고른 단추의 색만 `:root:root`로 덮는다. 설정 인수 시험이 `.set-switch`를 3개로 세므로 그 클래스는 쓰지 않는다 |

시험은 `scripts/typography.test.mjs`(15건: 값 읽기·보정, 글꼴 목록 만들기, CSS 변수, 단축키, 컨트롤러의 메인·노트 창 동작, 설정 구역 렌더)이고 `test-native.sh`가 부른다. 실제 창 점검에도 두 건이 있다. `views-checks.js`의 `typography-zoom-font-and-shortcuts-in-the-real-webview`는 배율 단추와 ⌘ - / ⌘ = / ⌘ 0으로 창의 CSS 너비(`innerWidth`)가 배율만큼 줄고 늘어나는지(실제 웹뷰 확대), 앱 글꼴의 적용·저장·오류 문구·기본값을 재고, `navigation-checks.js`의 `terminal-font-and-zoom-follow-the-settings`는 열려 있는 터미널의 글꼴이 바뀌고 150%로 확대한 채 터미널 화면이 칸 안에 들어오는지 잰다.

### 좌측 메뉴(레일)의 두 탭: 노트와 에이전트

레일(`Rail.tsx`) 맨 위에 탭 둘이 있다. **노트** 탭은 노트 일(전체 노트, 캘린더, 즐겨찾기, 연결, 템플릿, 휴지통)과 프로젝트·아젠다 목록이고, **에이전트** 탭은 에이전트 일(태스크, 스킬·에이전트, 러너 한 줄, 프로젝트 자원)이다. 연동 가이드는 두 탭이 같이 쓰므로 탭 메뉴가 아니라 아래 줄의 「?」 단추다. 시안은 디자인 캔버스의 「02 물성 · 사이드바 탭 (노트 / 에이전트)」 보드다.

- **어느 탭이 켜질지는 화면 주소가 정한다**(`navigation/railTabs.ts`의 `railTabOf`). 노트 탭은 `/notes` `/agenda/…` `/calendar` `/favorites` `/connections` `/templates` `/trash` `/review`, 에이전트 탭은 `/tasks` `/task/…` `/resources` `/project/…`다. 연동 가이드·세션·터미널처럼 어느 쪽도 아닌 화면에서는 직전에 켜 둔 탭을 그대로 둔다. 그래서 알림의 「승인하러 가기」처럼 주소로 들어와도 맞는 탭이 켜지고 탭과 본문이 어긋나지 않는다.
- **탭을 누르면 그 탭에서 마지막으로 보던 화면으로 간다.** 처음 누르면 각 탭의 첫 화면(`/notes`, `/tasks`)이다. 이미 그 탭의 화면이면 아무것도 하지 않는다. 기억은 모듈 변수에 두므로 앱을 켜 둔 동안만 가고 저장하지 않는다(`rememberRailTab`, `railTabTarget`).
- **에이전트 탭 이름 옆 배지는 「내 차례」의 합**이다: 러너가 답을 기다리는 태스크 수(`runnerStore`의 `waiting`)와 승인이 필요한 변경 수(`approvalsStore`)를 더한다. 노트 탭을 보고 있어도 보인다. 태스크 메뉴에는 답 대기 수, 스킬·에이전트 메뉴에는 승인 수가 따로 붙는다.
- **러너 한 줄**(「러너 · 쉬는 중」 따위)은 에이전트 탭에만 있고 누르면 태스크 화면으로 간다. 러너가 없는 빌드(`supported` false)에서는 숨긴다. 색 단계는 태스크 화면의 러너 막대와 같은 `runnerTone`을 쓴다.
- **프로젝트 자원** 줄은 프로젝트마다 하나이고 누르면 프로젝트 페이지(`/project/<slug>`)다. 예전에 노트 탭 프로젝트 머리에 있던 「프로젝트 페이지」 단추는 여기로 옮겼다. 줄은 아젠다 줄(`.rail-agenda`)을 그대로 써서 모든 모양(skin)에서 같은 활성 표시를 갖는다. 프로젝트별 자원 개수는 아직 안 보인다(시안에는 있다).
- 탭 줄의 색은 레일 바탕(스킨마다 밤하늘 위에 `--ink`가 다시 정의된다)에서 읽히도록 `--ink`를 섞어 만들었다(`color-mix`). 그래서 시안의 물성 알약과 모양이 조금 다르다. 선택자를 `.workspace-rail .rail-tabs …`부터 걸어 스킨의 일반 버튼 규칙보다 앞서게 했다.
- 시험은 `scripts/rail-tabs.test.mjs` 7건(주소→탭 표, 기억과 누름 규칙, 노트·에이전트 탭의 메뉴와 줄, 연동 가이드, 배지 합)이고 `tasks-ui`, `resources-ui`가 새 위치를 본다. `views-checks.js`의 메뉴 순서는 노트 탭 여섯 개로 고쳤지만 실제 창에서 돌려 보지는 않았다. 실제 앱 창에서 탭을 눌러 본 확인은 없다.

### 후속 노트 탭

메인 창에서는 후속 노트(노트 폴더 아래 `sub/`에 든 하위 노트)가 노트 위에 뜨는 창이 아니라 **탭 바의 탭 하나**로 열린다. 상위 노트와 후속 노트를 탭으로 오가며 볼 수 있고, 앱을 껐다 켜도 남는다. 탭 바가 없는 노트 창(새 창으로 옮긴 노트)만 예전의 후속 노트 창(`interaction/Overlays.tsx`의 `SubNotePages`)을 그대로 쓴다.

- **주소와 탭 식별자.** 후속 노트는 (최상위 노트 id, 후속 id 사슬)로만 정해지므로 주소가 사슬 전체다: `/sub/<최상위 노트 id>/<후속 id>/<그 아래 후속 id>…`(`routes.ts`의 `subAddress`, `parseRoute`의 `kind: 'sub'`). 탭 식별자는 `sub:<최상위 노트 id>/<후속 id>/…`(`subTabId`, `parseSubTab`)다. `isNoteTab`은 최상위 노트 탭만 가리키고(후속 탭은 아니다), 저장할 탭은 `isStoredTab`, 탭이 기대는 최상위 노트는 `tabRootId`가 돌려준다.
- **여는 길은 `controller.openSubNotes(root, ids)` 하나다.** 후속 노트 목록의 행, 노트 안의 후속 노트 카드(`#sub:`), 「추가」, ⌘K 미리보기 안의 카드, 링크의 사슬(`receiveURLs`로 온 `.md` 파일 링크), 노트 창이 메인 창에 보낸 `show_in_main`의 `sub`가 모두 여기를 지난다. 메인 창에서는 `/sub/…`로 이동하는 것이 전부이고, 노트 창에서만 `state.subNotes`로 창을 연다. 최상위 노트가 새 창으로 옮겨져 있으면 탭을 열지 않고 그 창을 앞으로 가져온다.
- **탭 모델(`navigation/tabs.ts`).** 후속 탭은 같은 최상위 노트의 탭 무리(상위 노트 탭과 이미 열린 후속 탭들) **바로 뒤**에 놓인다(`withTab`). 무리가 없으면 상위 노트 탭을 앞에 함께 연다. 최상위 노트가 볼트에서 사라지면 후속 탭도 함께 걸러진다(`available`). 저장 형식은 그대로 `openIds` 문자열 배열(버전 1)이고 후속 탭 식별자가 섞여 들어간다. 사슬 중간의 노트가 지워졌어도 탭은 남고, 열면 「이 후속 노트를 찾을 수 없어요」와 「탭 닫기」가 보인다.
- **화면.** `navigation/SubNoteScreen.tsx`가 `holdSubChain`(`navigation/subNotes.ts`)으로 최상위 노트부터 사슬의 목록을 한 단계씩 읽은 뒤 일반 노트 화면(`NoteScreen`)을 `parent`·`root`와 함께 그린다. 그래서 본문 폭 조절, 세션 레일, 이력, 삭제가 최상위 노트와 같다. 후속 노트 화면의 정보줄 아래에는 「↰ 상위 노트 제목」 칩이 조상마다 하나씩 있어 누르면 그 탭으로 간다(옛 창의 이동 경로 자리). 레일(왼쪽 메뉴)은 노트 화면처럼 숨는다.
- **탭 이름.** 탭 바(`TabBar.tsx`)는 후속 탭에 「↳」 표시를 붙이고 제목만 보인다. 마우스를 올리면 「최상위 제목 › … › 이 제목」이 나온다. 이름을 알려면 목록을 읽어야 하므로 탭 바가 열린 후속 탭의 사슬을 미리 읽어 둔다(앱을 다시 켠 직후에도 이름이 보인다). 아직 못 읽었으면 「↳ 후속 노트」다.
- **닫기와 지우기.** 탭의 ×와 ⌘W는 일반 탭과 같다. 후속 노트를 지우면(`controller.popSubNote`) 그 탭과 그 아래 후속 탭들이 닫히고 상위 탭으로 돌아간다. 상위 탭이 닫혀 있으면 이웃 탭이다. Esc는 후속 탭을 닫지 않는다(노트 창의 후속 노트 창만 Esc로 한 단계씩 닫힌다). 노트를 새 창으로 옮기면 그 노트의 후속 탭도 메인 창에서 닫힌다.
- **시험.** `interaction.test.mjs`(열기·자리·저장·지우기·새 창 이동·링크 사슬, 노트 창의 후속 노트 창), `navigation.test.mjs`(주소 왕복, 탭 모델의 저장·걸러냄·자리), `sub-tabs-ui.test.mjs`(탭 이름·↳·설명 글, 최상위 노트가 없는 탭). 실제 창 점검 `interaction-checks.js`, `views-checks.js`, `editor-checks.js`, `part4-checks.js`의 후속 노트 사례를 창 대신 탭을 보도록 고쳤지만 **실제 앱 창(WKWebView)에서 돌려 보지는 않았다.**

### 좌측 메뉴(레일)의 프로젝트·아젠다 목록

레일의 목록(`Rail.tsx`)은 **프로젝트가 맨 위, 아젠다는 그 안쪽**으로 그린다. 아젠다는 프로젝트 없이도 만들 수 있어서, 프로젝트에 속하지 않은 것은 맨 아래 「미분류」 묶음(점선 폴더 아이콘)에 같은 모양으로 둔다.

- 제목은 프로젝트가 하나라도 있으면 「프로젝트」, 하나도 없으면 「아젠다」다. 프로젝트가 없으면 묶음 머리글도 없이 아젠다만 평평하게 나온다.
- 제목 오른쪽의 「＋」 하나가 `ActionMenu`를 열어 「새 프로젝트」와 「새 아젠다」 중에 고르게 한다(예전의 「프로젝트 ＋」, 「＋」 단추 둘을 합쳤다).
- 묶음 머리글은 접기 화살표, 폴더 아이콘, 이름, 아젠다 개수다. 마우스를 올리거나 키보드로 들어가면 개수 자리에 「프로젝트 편집」(연필)과 「이 프로젝트에 아젠다 추가」(＋) 아이콘이 나타난다. 아젠다 줄도 같은 연필 아이콘으로 「아젠다 편집」을 열고(예전의 `⋯`를 바꿨다), 개수는 프로젝트 줄과 같은 오른쪽 열에 선다. 접근성 이름(`… 프로젝트 편집`, `…에 아젠다 추가`, `… 아젠다 편집`)은 그대로다.
- 아젠다 줄은 `.rail-kids` 안에 들여 쓰고 프로젝트 줄의 폴더 아이콘 가운데 아래로 내린 세로선으로 묶는다. 접기 상태 저장(`collapsedProjects`)은 그대로다.
- 시안은 디자인 캔버스의 「02 물성 · 사이드바 계층」 보드다. 실제 앱 창에서 「＋」 메뉴를 눌러 본 확인은 없고, 실제 컴포넌트를 렌더링해 모양만 확인했다.

### 좌측 메뉴(레일) 폭

레일 오른쪽 가장자리의 손잡이(`RailResizer`, `role="separator"`)를 끌어서 폭을 바꾼다. 범위는 200~420px, 기본은 264px이고 설정 `railWidth`(16장 21개 중 하나)에 저장한다.

| 동작 | 결과 |
|---|---|
| 끌기 | 끄는 동안에는 `.workspace-screen`의 `--rail-width`만 바꾸고, 손을 떼면 한 번 저장한다 |
| 더블클릭 | 기본 폭(264)으로 |
| 손잡이에 포커스 후 ←/→ | 16px씩(Shift는 48px). Home/End는 최소/최대. 캘린더의 날짜 이동 단축키로 번지지 않게 이벤트를 멈춘다 |

- 폭은 `WorkspaceShell`이 `state.prefs.railWidth`를 `clampRailWidth`로 보정해 `--rail-width`로 건다. 범위 상수와 보정은 `ui/model.ts`(`RAIL_WIDTH_*`)에 있고 저장소의 보정(`storage.ts`)과 같아야 한다. 시험이 둘을 맞대어 본다.
- 손잡이는 레일 안이 아니라 `.workspace-screen` 위에 절대 위치로 얹는다. 레일은 스크롤 상자라서 안에 두면 내용과 같이 밀려 올라간다. 모양(skin)이 이 격자에 왼쪽 여백을 주면 그 값을 `--screen-pad`로 알려야 손잡이가 레일 가장자리에 맞는다(`skin-tactile.css`, `skin-glass.css`).
- 창 폭 750px 이하에서는 레일이 190px 고정이거나 위로 쌓이므로 손잡이를 숨긴다. 노트 화면처럼 레일이 없는 화면에는 손잡이도 없다.
- 설정 값이 있어도 화면에 안 이어진 것이 남아 있다. `detailListWidth`와 `docWidth`는 저장·보정만 되고 읽는 화면이 없다(아젠다 목록은 320px 고정). `railWidth`와 `noteRailWidth`(노트 옆 세션 레일)는 이어져 있다.
- 시험은 `scripts/chapters14-17.test.mjs`의 「레일 폭」(범위와 보정)과 `scripts/views-checks.js`의 `rail-resize-drag-keyboard-reset-and-persist`(끌기·범위·키보드·더블클릭·저장)다.

### 코드 블록 문법 색 (`editor/codeSyntax.ts`)

코드 블록의 언어별 색을 입히는 곳은 한 모듈이다. 문법을 토큰(키워드, 문자열, 주석 같은 조각)으로 나누는 일은 Prism이 하고, 편집기와 읽기 화면이 같은 모듈과 같은 CSS를 쓴다.

| 화면 | 칠하는 방법 |
|---|---|
| 편집기(`Editor.tsx`의 `CodeSyntaxPlugin`) | Lexical의 `registerCodeHighlighting`이 코드 노드를 입력할 때마다 다시 나눠 토큰 노드로 바꾼다. 색은 화면에만 있고 저장되는 Markdown은 그대로다 |
| 읽기 화면(`ReadMarkdown.tsx`), 외부 문서, 코멘트 | 정화가 끝난 DOM의 `pre > code.language-<이름>`을 `highlightCodeBlocks`가 칠한다 |
| Claude 응답 글(`plugs/claude/modelProse.tsx`) | 같은 `highlightCodeBlocks`를 쓴다 |

- **문법.** Lexical에 든 Prism에는 Kotlin, Bash, YAML, JSON, Groovy, Dockerfile, properties가 없어서 편집기의 언어 선택 목록(`tools.tsx`의 `languages`)에 있어도 색이 없었다. `codeSyntax.ts`가 이들과 TOML, JSX, TSX를 더 불러오고, 펜스에 흔히 쓰는 이름(`gradle`, `zsh`, `jsonc`, `c++`)을 별칭으로 잇는다. `prismjs`는 이 때문에 직접 의존성이다(1.30.0, Lexical이 쓰는 버전과 같다). 문법을 더하면 `codeSyntax.ts`에 `prism-<이름>.js`를 불러오되, 기대는 문법(Kotlin·Groovy는 clike, JSX는 markup·javascript, TSX는 JSX·typescript)을 먼저 불러온다.
- **언어를 적지 않은 블록은 칠하지 않는다.** Lexical은 기본값으로 이런 블록의 언어를 `javascript`로 바꾸는데, 그러면 저장할 때 펜스가 ```` ``` ````에서 ```` ```javascript ````로 바뀐다. 그래서 토크나이저의 `defaultLanguage`를 `null`로 둔다. 모르는 언어(`mermaid` 포함)도 그대로 둔다.
- **색은 CSS 클래스 `token <종류>`로 정한다.** Lexical의 `theme.codeHighlight`가 토큰 종류를 그대로 클래스로 바꾸고, Prism이 만드는 HTML도 같은 클래스를 단다. 색 값은 `styles.css`의 `--tok-*` 변수이고 밝기마다(라이트, OS 다크, `data-theme="dark"`) 값만 다르다. `light-dark()`는 macOS 13의 WebKit에 없어서 쓰지 않는다.
- **별칭 문제.** Prism은 토큰에 별칭을 달아(`assign-left`는 `variable`) HTML에 둘 다 클래스로 쓰지만, Lexical은 종류 이름 하나만 쓴다. 별칭으로 색이 정해지는 이름(`function-name`, `scalar`, `decorator` 등)도 `styles.css`에 규칙이 있어야 두 화면이 같게 칠해진다. 새 문법을 더했을 때 빠뜨리지 않도록 `code-syntax.test.mjs`가 문법에서 이름을 읽어 CSS와 맞대어 본다.
- **함께 켜지는 것.** `registerCodeHighlighting`은 색 입히기와 함께 Lexical의 코드 안 편집 기능(들여쓰기, 줄 이동 따위)도 등록한다. Tab은 `tools.tsx`의 기존 처리(우선순위 높음)가 먼저 받아 `\t`을 넣는다. 다른 키 동작은 앱 창에서 손으로 확인하지 않았다.
- **한계.** 펜스 이름이 대문자(`Kotlin`)이면 읽기 화면은 칠하는데 편집기는 칠하지 않는다(Lexical이 이름을 그대로 비교한다). 문자열 안의 `${…}`처럼 토큰 안에 든 토큰은 읽기 화면에서는 바깥 색을 물려받고 편집기에서는 안쪽 토큰 색을 쓴다. Swift의 `directive`처럼 별칭이 둘로 갈리는 몇몇 토큰은 두 화면의 색이 다를 수 있다.
- **시험.** `scripts/code-syntax.test.mjs`(`test-native.sh`가 부른다)가 문법 쪽(Kotlin 토큰, 선택 목록의 모든 언어에 문법이 있는지, 모르는 이름과 이스케이프, 별칭 규칙)과 헤드리스 편집기(색이 입혀지는지, 입력해도 색이 따라가는지, 색 때문에 저장되는 Markdown이 달라지지 않는지, 언어 없는 블록이 그대로인지)를 잰다. 실제 번들 조건(Vite, `safari15`)에서 편집기와 읽기 화면을 Edge headless로 그려 라이트·다크 모두 색이 나오는 것을 확인했다. 앱 창(WKWebView)과 Claude 응답 글에서는 확인하지 않았다.

### 계약을 건드릴 때

호스트 커맨드(2부 44개, cargo 기능 `claude` 아래 16개: 4부 10개와 러너 6개)와 이벤트 6개(`ais://menu`, `ais://navigate`, `ais://note-windows`, `ais://self-write`, `ais://notifications`, `ais://runner`), 번들 식별자, 권한 파일(`src-tauri/capabilities/main.json`·`note.json`)은 시험이 글자까지 잰다(`scripts/contracts.test.mjs`). 커맨드를 더하거나 권한을 바꾸면 이 시험과 `capabilities/`를 함께 고친다. `main`과 `note-*` 창의 권한은 일부러 다르다.

**설명서와 일부러 다르게 둔 것.** `create_workspace_dir({parent, name})`는 설명서대로라면 저장 위치(`parent`)가 없을 때 실패해야 하지만, 지금은 이름 검사를 모두 통과한 뒤 저장 위치를 만든다(`mkdir -p`). 새 워크스페이스 화면에서 없는 폴더를 적으면 「저장 위치를 찾을 수 없어요」로 막히던 것을 2026-11-04에 바꿨다. 저장 위치가 파일이면 여전히 실패하고, 잘못된 이름은 저장 위치를 만들기 전에 거절한다(`workspace.rs`의 `create_dir_makes_a_missing_parent_after_validating_the_name`).

## 8. 시험

### 한눈에

| 하는 일 | 명령 | 걸리는 시간 | 뭘 재나 |
|---|---|---|---|
| 엔진 전체 | `bash test.sh` | 컴파일이 캐시된 상태에서 약 70~105초(202건, 이 가운데 골든 재생이 약 1분) | `cargo test --workspace --locked`. 엔진과 `ais-cli`의 단위·통합 시험(임시 vault, 임시 HOME, 가짜 `launchctl`과 가짜 에이전트 CLI, 진짜 `ais-cli mcp` 프로세스). 잠금 폴더는 임시로 따로 쓴다. 리팩토링 안전망 11건(골든 재생, 직렬화 골든, 의존 방향·닫힌 집합·어댑터·쓰기 범위 규칙 7, 명령 표 교차, 가짜 포트)이 들어 있다(§4) |
| 화면·호스트 전체 | `bash test-native.sh` | 캐시된 상태에서 Node 시험까지 약 30초 | 화면 빌드, Node 시험 641건, 호스트 `cargo test`(아래 「기존 실패」). 호스트 시험이 실패하면 이 스크립트도 실패로 끝난다 |
| 받아들임 묶음 | 아래 | 수 초 | 설명서 원문 기대값 27건과 실행기 자체. 기준 jar가 필요하다 |
| 기준 맞대기 | §4 「기준과 맞대기」 | 엔진 12분(Java가 도는 만큼) | 기준 Java 엔진과 같은 입력의 응답·폴더 바이트·MCP 대화를 견준다. 어긋남이 0이어야 한다. 같은 하네스를 Java 없이 녹화본과 견주는 골든 재생은 `bash engine-rs/tools/golden/run.sh replay`(약 1분)다 |
| 실제 창 점검 | `pnpm test:*:acceptance` | 종류당 수 분 | 실제 WKWebView, 앱 안의 Rust 엔진, 격리 HOME. 먼저 `(cd engine-rs && cargo build -p ais-cli)`로 점검이 쓸 실행 파일을 지어 둔다 |

`test.sh`는 `dist/`를 건드리지 않는다. `build.sh`는 `dist/ais-cli`를 만들고, 실제 창 점검은 `dist/`가 아니라 `engine-rs/target/debug/ais-cli`(또는 `AISURADA_CLI_BIN`)를 쓴다.

**기존 실패.** 호스트 `cargo test`에서 `ask::tests` 4개(`test_gemini_missing_binary_message`, `test_gemini_success_with_fake_binary`, `test_resolve_claude_with_env_var`, `test_resolve_gemini_with_env_var`)는 Java를 지우기 전 `main`에서도 실패한다. 병렬로 돌리면 `runner::tests` 둘과 `terminal::` 셋이 더 실패하는 일이 있고, `-- --test-threads=1`로 돌리면 통과한다. 5d에서 직렬로 돌린 결과는 154건 통과, 4건 실패(위 `ask::tests`), 5건 무시다. 바꾼 쪽의 회귀와 구분하려면 변경 전 `main`과 같은 명령으로 비교한다.

### 엔진 시험 쓰기

외부 시험 프레임워크 없이 Rust 기본 시험 도구(`cargo test`)를 쓴다.

- **단위 시험**은 각 파일의 `#[cfg(test)]` 모듈에 둔다.
- **통합 시험**은 `engine-rs/crates/ais-core/tests/fixture_vault.rs`(합성 vault를 `tools/compat/make-fixture.sh`로 만들어 읽기 명령의 핵심 동작을 확인), `engine-rs/crates/ais-cli/tests/`의 `mcp_stdio.rs`(진짜 `mcp` 프로세스), `register.rs`(가짜 에이전트 CLI로 등록), `old_sidecar.rs`(임시 HOME과 가짜 `launchctl`로 옛 사이드카 정리)에 둔다. `lock_interop.rs`는 기준 jar(`AISURADA_JAR`)가 있을 때만 돈다.
- 시계와 id는 환경 변수로 고정한다: `AISURADA_TEST_NOW`(시각, 도장과 평가 시각도 따름), `AISURADA_TEST_RANDOM`(id의 무작위 조각). 안 주면 아무 일도 안 한다. 계약이 아니라 시험용이다.
- 임시 HOME·vault·잠금 폴더(`AISURADA_LOCK_DIR`)만 만들고 지운다. 사용자의 HOME이나 vault를 건드리는 시험을 쓰지 않는다.
- 동작을 바꿨으면 시험과 함께 기준 맞대기(§4)도 돌려 어긋남이 0인지 본다. 기준과 일부러 다르게 만든 동작은 `docs/rust-engine-step*.md`의 해당 기록에 이유를 적는다.

### 받아들임 묶음 (34장)

```bash
ORACLE="$(bash engine-rs/tools/compat/build-oracle.sh)"   # 기준 jar를 태그에서 지어 둔다(§4)
AISX_IMPL_JAVA="$(command -v java)" AISX_IMPL_JAR="$ORACLE/ais-engine.jar" \
  node fixtures/expect/run.mjs --engine fixtures/aisurada.adapter.json --out <빈 폴더>
node fixtures/expect/run.mjs --self-check --mutants --out <빈 폴더>
node fixtures/runner-test.mjs
```

이 묶음은 기준 Java 엔진을 부른다. Rust `ais-cli`는 JSON 키를 정렬해서 쓰므로 키 차례까지 글자로 견주는 이 묶음과는 기대값 27건 가운데 일부(5d에서 잰 값은 11건이 완전히 같고 16건이 키 차례만 다름)가 맞지 않는다. 그래서 묶음은 기준 jar로만 돌리고, Rust와의 견줌은 키 차례를 무시하는 맞대기(§4)가 맡는다.

`--out`은 결과를 쓸 폴더다(저는 매번 새 임시 폴더를 줬다). 케이스를 더하거나 `fixtures/expect/`의 파일을 고치면 `node fixtures/build-bundle.mjs`로 `bundle.json`의 sha256 목록을 다시 적는다. 진짜 묶음(읽기 레인 460건)은 이 저장소에 없다.

### 화면·호스트 시험

`package.json`의 `test:*`가 종류별 진입점이다.

| 종류 | 진입점 | 비고 |
|---|---|---|
| 순수 로직(Node) | `test:screen`, `test:workspace`, `test:navigation`, `test:interaction`, `test:data`, `test:editor`, `test:views`, `test:autosave`, `test:chapters`, `test:cli-script`, `test:connect` | 호스트 없이 돈다. `test:cli-script`는 앱에 든 `ais-cli` 스크립트를, `test:connect`는 설정의 Claude 연결 화면을 잰다 |
| 4부 | `scripts/chapter03-04.test.mjs`, `chapter06`, `chapter08`, `chapter09`, `chapter10`, `part4-ask-run.test.mjs` | `test-native.sh`가 부른다 |
| 글꼴과 크기 | `scripts/typography.test.mjs` | 설정의 「글꼴과 크기」(위 §7 「글꼴과 화면 크기」). `test-native.sh`가 부른다 |
| 계약 | `node --test scripts/contracts.test.mjs` | 번들 식별자, 권한, CSP |
| 호스트 | `cargo test --manifest-path native-host/src-tauri/Cargo.toml --locked` | |
| 실제 창 | `test:acceptance`, `test:workspace:acceptance`, `test:navigation:acceptance`, `test:interaction:acceptance`, `test:data:acceptance`, `test:editor:acceptance`, `test:autosave:acceptance`, `test:views:acceptance` | 종류별 한 번에 하나씩. 4부를 뺀 판을 잰다 |
| 실제 창(4부) | `test:part4:acceptance`(켠 판 9건), `test:ask-marks:acceptance`(끈 판 5건) | `test-native.sh`에 들어 있지 않으니 따로 돌린다 |

실제 창 점검은 실행마다 별도 앱 식별자와 임시 HOME·vault·잠금 폴더·빈 `ZDOTDIR`를 만들어서 쓴다. 성공하면 임시 자료를 지우고, 실패하면 `Diagnostics retained: <경로>`를 찍는다. `AISURADA_WORKSPACE_REPORT=<절대 경로>.json`이나 `AISURADA_CHECK_REPORT=<절대 경로>.json`으로 결과를 JSON으로 남길 수 있다. `--only=<사례 id>`로 일부만 돌릴 수 있다(편집기 왕복).

**호출 기록(`AISURADA_CALL_LOG`).** 점검은 앱이 엔진을 부른 호출을 세어 확인하는데(첫 새로고침 묶음, 본문 쓰기 횟수 따위), 호출 길이 둘(`sidecar_call` 문자열 목록, `engine_call` 호출 객체)이라 어느 길로 갔든 같은 문자열 목록(`toArgv`)으로 세야 한다. 호스트가 환경변수 `AISURADA_CALL_LOG`에 준 파일에 호출을 한 줄씩 JSON으로 적고(`call_log.rs`, 변수가 없으면 아무것도 안 한다), `workspace-acceptance.mjs`가 이것을 읽어 `/calls`로 내준다. 호출 객체의 인자는 이름 차례로 정렬돼 적히므로 점검은 인자 칸의 차례를 따지지 않는다. `data-checks.ts`와 `autosave-checks.ts`는 화면 안에서 두 길을 직접 감싸 센다.

**4부 점검.** 위 8종은 4부를 뺀 판(`build:noclaude`, `--no-default-features`)을 잰다. 4부를 켠 판은 `pnpm test:part4:acceptance`가 잰다(`pnpm build`로 4부 화면을 짓고 호스트를 기본 기능으로 띄운다). 가짜 `claude`(`AISURADA_CLAUDE_BIN`)와 격리 HOME의 가짜 전사(`~/.claude/projects/…/<세션 id>.jsonl`)를 쓰므로 토큰을 쓰지 않는다. 점검 코드는 `native-host/scripts/part4-checks.js`다.

| 재는 것 | 안 재는 것 |
|---|---|
| 노트 화면이 렌더 루프 없이 뜸, 표 줄 점, 하위 노트 | 노트 옆 세션 레일과 레일 대화 |
| **묻기 왕복**: 글 선택 → 「AI에게 묻기」 → 「묻기」 → 가짜 `claude`의 답이 「AI 문답」 칸에 붙음 → 엔진의 `ask.json`과 본문 표식까지 | 세션 입력창으로 보내기, 받아쓰기, 굳히기 |
| 세션 화면: 전사 없음 안내와 CLI 칸, **전사가 있는 세션**의 대화(`recent-sessions`와 화면) | 실제 `claude` |

`test:ask-marks:acceptance`는 4부를 끈 판에서 호스트에 `ask_runtime`이 없고 노트의 문답 표식이 그대로 남는지 잰다.

### 동시에 돌리면 안 되는 것

`build.sh`는 `dist/`를 지우지 않지만 `dist/ais-cli`를 덮어쓴다. `cargo build`·`cargo test`가 같은 `engine-rs/target/`을 쓰므로 같은 폴더에서 둘 이상을 동시에 돌리면 잠금을 기다리거나, 점검이 쓰는 `engine-rs/target/debug/ais-cli`가 도중에 바뀐다. 실제 창 점검이 도는 동안 같은 폴더에서 다른 터미널·세션이 빌드하지 않는다. 병렬로 확인할 일이 있으면 `git worktree add --detach <경로> <커밋>`으로 커밋만 떼어 놓고 거기서 돌린다. `cp -cR native-host/src-tauri/target <경로>/native-host/src-tauri/target`으로 호스트의 cargo 산출물(약 12GB)을 APFS 복제하면 빠르다. 엔진의 `engine-rs/target/`은 워크트리마다 새로 지어도 호스트보다 훨씬 작다.

## 9. 앱 손 확인

자동 시험이 못 재는 것(메뉴 클릭, 모니터 조합, 스킴·파일 연결, 한글 조합, 4부 전체 흐름)은 [`beta-hand-check.md`](beta-hand-check.md)에 점검표와 격리 실행법이 있다.

## 10. `docs/` 읽는 법

| 이름 꼴 | 내용 |
|---|---|
| `engine-manual/` | 1부 설명서 사본 일부 |
| `spec-chapter-NN-*.md` | 1부 장별 구현 명세 |
| `rebuild-p<부>-chapter<NN>-complete.md` | 부·장별 구현 기록 (무엇을 만들었고 무엇으로 검증했고 무엇을 못 했나) |
| `rebuild-p<부>-*-fixes-complete.md`, `-review-*` | 리뷰 반영 기록 |
| `rebuild-p4-plan.md`, `rebuild-p3-plan.md` | 부별 구현 순서 |
| `review-*`, `plan-*`, `prompt-*` | 초기 장 작업의 리뷰·계획·지시 기록 |
| `beta-hand-check.md`, `developer-guide.md` | 지금 쓰는 안내서. 가이드의 HTML 사본은 `pnpm --dir native-host docs:guide`로 다시 만든다 |

작업 규칙(커밋과 푸시, 작업 뒤 정리, 문서 갱신, 이름 규칙)은 저장소 루트의 [`CommonRule.md`](../CommonRule.md) 한 곳에서 관리한다. `CLAUDE.md`는 그 파일을 가져다 쓴다.

설명서 원문(PDF, 로컬 md)은 이 저장소 밖에 있다. `docs/`의 사본은 원문을 다시 쓴 판이라 문구·종료 코드·바이트 예시가 원문과 다른 곳이 있다. **다를 때는 원문과 현재 코드의 시험이 기준이다.** 완료 기록은 그날의 상태를 적은 것이라 이름 변경 이후 코드와 글자가 다를 수 있다.

### 옛 이름 대응표

`docs/` 아래 기록과 설명서 원문은 옛 이름을 쓴다. 읽을 때 아래처럼 바꿔 읽는다.

| 옛 이름 | 지금 이름 | 비고 |
|---|---|---|
| `.withlog/` (vault 폴더) | `.aisurada/` | 옛 vault는 폴더를 옮겨야 열린다 |
| `WITHLOG_*`, `--withlog.*`, `withlog.*` | `AISURADA_*`, `--aisurada.*`, `aisurada.*` | 환경 변수, 설정 키, 시험 속성 |
| `withlog://` | `aisurada://` | 딥링크 스킴 |
| `co.kcd.withlog` | `co.kcd.aisurada` | 번들 id, LaunchAgent 레이블 접두 |
| `wl-cli` | `ais-cli` | 클라이언트 jar와 명령(지금은 실행 파일 이름) |
| `wl-engine.jar`, `wl-sidecar.jar`, `wl-sidecar-noclaude.jar` | `ais-engine.jar`, `ais-sidecar.jar`, `ais-sidecar-noclaude.jar` | 지금 `dist/`에 생기는 것은 `ais-cli` 하나다. jar는 기준 jar(`build-oracle.sh`)에만 있다 |
| `WL-409`, `WL-412` … | `AIS-409`, `AIS-412` … | 오류 코드 |
| `WlException`, `WlCliMain` | `AisException`, `AisCliMain` | 자바 클래스(지금은 태그 `java-engine-final`에만 있다) |
| `WLQ1`, `WLG1` | `AIQ1`, `AIG1` | 소켓 프레임 머리. 4바이트 폭을 지키려고 `AI`로 바꿨다 |
| `wlframe://`, `x-wl-*`, `wl://menu` 따위 | `aisframe://`, `x-ais-*`, `ais://menu` | 호스트 프로토콜, 헤더, 이벤트 |
| `wl_close_tab` 따위 | `ais_close_tab` | 메뉴 id |
| `<!--wlc:`, `<!--wla:` | `<!--aisc:`, `<!--aisa:` | 노트 본문의 코멘트·문답 표식. 옛 노트의 표식은 새 이름으로 옮겨야 인식된다 |
| `WLX_*` | `AISX_*` | 받아들임 실행기 환경 변수 |

`docs/` 아래 파일과 외부 설명서 파일 이름(`withlog 1부 vault 엔진 설명서.md` 따위)은 바꾸지 않았다.

## 11. 커밋과 브랜치 관례

- 제목은 `type(scope): subject [Jira-ID]`. `type`은 `feat`/`fix`/`docs`/`style`/`refactor`/`test`/`chore`, `scope`는 바뀐 모듈 이름(`aisurada`, `native-host`), 제목은 영어 소문자 동사원형 50자 이내, Jira 번호가 없으면 `[Empty]`.
- 본문은 한글로 쓰고, 무엇을 바꿨는지보다 왜 바꿨는지를 먼저 적는다. 깨지는 변경은 `BREAKING CHANGE:` 푸터에 적는다.
- 장 작업은 끝나면 완료 기록(`docs/rebuild-p<부>-chapter<NN>-complete.md`)까지 같은 작업에 담는다.
- 한 작업 트리를 여러 도구가 같이 쓰는 일이 있다. 시작 전에 `git status`로 남의 수정이 있는지 보고, 있으면 그 파일은 건드리지도 커밋하지도 않는다. 브랜치를 바꾸면 같이 쓰는 쪽의 브랜치도 바뀐다.

## 12. 자주 막히는 곳

| 증상 | 원인과 처방 |
|---|---|
| 화면에 `모르는 명령이다: …`(예: `list-resources`)가 뜬다 | 명령이 `engine-rs/crates/ais-cli/src/registry.rs`의 목록에 없거나(§5 「명령을 새로 넣는 길」), 터미널과 에이전트에 등록된 `ais-cli`가 앱보다 낡았다. `dist/ais-cli doctor`가 보여 주는 실행 파일 자리와 MCP 등록을 확인하고 낡았으면 `doctor --fix`로 다시 등록한다 |
| `ais-cli 실행 파일을 못 찾았다 …`(종료 코드 127) | 개발용 번들의 `bin/ais-cli` 스크립트가 빌드 폴더에서 실행 파일을 못 찾았다. `(cd engine-rs && cargo build -p ais-cli)`로 짓거나 `AISURADA_CLI_BIN`에 자리를 준다. 실제 창 점검 중이면 같은 폴더에서 다른 빌드가 도는 중일 수도 있다(§8) |
| 앱 안에서 묻는데 Claude가 `ais-cli`를 못 찾는다 | 개발 실행에는 앱에 든 `bin/ais-cli`가 없거나 빌드 폴더의 실행 파일을 가리키는 스크립트다. PATH에 `ais-cli`가 있거나 `engine-rs/target/debug/ais-cli`를 지어 둔다. 정식 번들(`pnpm release:build`)에는 실행 파일이 든다(§7) |
| MCP 클라이언트가 `aisurada` 서버에 못 붙거나 곧 끊는다 | 서버의 표준 출력에 JSON-RPC가 아닌 글이 섞이면 클라이언트가 끊는다. mcp 모드에서 엔진 길의 출력은 호출마다 모아서 돌려주게 되어 있다(§5). 새로 출력을 더하는 코드는 `eprintln!`(표준 오류)로 쓴다. 직접 보려면 `AISURADA_VAULT`를 주고 `dist/ais-cli mcp`를 띄운 뒤 표준 입력에 `initialize` 요청 한 줄을 넣어 본다 |
| 문서를 `open`했더니 앱이 떴다 | 번들이 `.md` 기본 편집기로 등록돼 있다. 에디터나 브라우저로 연다 |
| 앱을 켰더니 진짜 기기의 launchd나 에이전트 등록이 바뀔 것 같다 | 정식 앱은 첫 실행에 한 번 옛 사이드카를 내리고 낡은 MCP 등록을 다시 등록한다(§4 「앱의 호출 길」). 개발 실행과 빌드 폴더 번들은 하지 않는다. 격리 HOME으로 띄우면 진짜 기기의 것을 건드리지 않는다([`beta-hand-check.md`](beta-hand-check.md)) |
| `cargo test --locked`가 락 파일 때문에 선다 | 패키지 이름이나 의존성을 바꿨다. 한 번 `cargo build`(락 없이)로 `Cargo.lock`을 갱신해 커밋한다 |
| acceptance 뒤 `$TMPDIR/ais-p3-*`가 쌓인다 | 실패한 점검의 진단 보존이다. 원인을 본 뒤 지운다 |
| 이름이나 문구를 바꿨더니 HTML 노트를 못 읽는다 | 글자 수를 숫자로 박아 둔 코드다(예: 마커 `<!--aisurada`의 길이). 숫자 대신 문자열의 `len()`(바이트)이나 글자 수를 세는 함수를 쓴다 |
| 이 셸의 `rm`이 확인을 묻는다 | `rm`이 `-i` 별칭이다. `/bin/rm -rf`를 쓴다 |
| zsh에서 `echo ====`가 `not found` | `=`로 시작하는 낱말을 zsh가 명령으로 푼다. 따옴표로 감싼다 |
| HTML 첨부 상자 안에서 외부 스크립트나 지도 타일이 안 뜬다(Leaflet 지도가 「지도를 불러오지 못했습니다」로 나온다) | 상자(`aisframe://`) 응답에 붙는 고정 CSP(`frame.rs`의 `CSP`)가 허용하지 않은 곳이다. 지도(Leaflet)용으로 `cdnjs.cloudflare.com`(스크립트·스타일·그림)과 타일 서버 `xdworld.vworld.kr`·`server.arcgisonline.com`(그림)만 열어 두었다. 다른 호스트가 필요하면 `CSP`에 더하고 다시 짓는다. 와일드카드, `https:` 같은 스킴만 쓴 출처, `connect-src`는 열지 않으며 `frame.rs`의 `csp_tests`가 이를 지킨다 |
| HTML 첨부 상자 아래에 빈 띠가 생기거나 상자가 늘 1600px로 나온다 | 상자 높이는 주입 스크립트 `frame_height.js`가 `<body>` 자손의 아래쪽 끝 최댓값으로 잰다. `overflow`가 `hidden`·`clip`·`scroll`·`auto`(또는 `contain: paint`)인 조상 안에서 잘려 안 보이는 부분은 빼고, 절대 위치 자손은 위치 기준 조상이 그 잘림 요소 안에 있을 때만 뺀다. 이를 빼지 않던 빌드에서는 지도 타일이 상자 밖으로 삐져나온 만큼(최대 타일 1장, 256px) 빈 띠가 생겼다. 화면에서는 측정값을 160~1600px로 자르므로 내용이 더 길면 상자 안에서 스크롤한다 |

## 13. 지금 상태와 한계

- 엔진 시험(`bash test.sh` 178건), 화면 Node 시험 641건, 기준 맞대기(어긋남 0), 34장 받아들임(기준 jar로 27건 모두 맞음), 실제 창 점검 10종 248건은 통과한다(2026-11-15, 5d). 호스트 `cargo test`는 기존 실패 4건(`ask::tests`)을 뺀 나머지가 직렬로 통과한다(§8). 수치와 근거는 [5d 기록](rust-engine-step5d-java-removal.md)에 있다.
- 사람이 앱을 직접 조작해 본 확인은 아직 없다. 점검표는 §9의 문서에 있다.
- MCP 서버(`ais-cli mcp`)는 진짜 엔진과 진짜 프로세스로 표준 입출력 왕복을 잰다. 진짜 Claude Code 2.1.318로는 격리 HOME에서 등록, 쓰기 모드로 바꿔 끼우기, 연결 확인(`✔ Connected`), 끊기까지 확인했다. 못 한 것: 모델이 실제로 도구를 부르는 흐름, Claude Desktop, MCP 규격 공식 문서와의 대조(받는 개정은 2024-11-05, 2025-03-26, 2025-06-18이고 규격은 다른 자료로 짰다).
- 설정 화면의 「Claude 연결」 단추는 4부 실제 창 점검에서 가짜 claude로 눌러 보았다. 진짜 claude로 화면의 단추를 눌러 본 적은 없다(손 확인 4-4).
- 설정의 「글꼴과 크기」는 브라우저(개발 서버, 샘플 vault)에서 다섯 모양 모두 눈으로 보았다. 실제 WKWebView 점검(`test:views:acceptance` 28건, `test:navigation:acceptance` 13건, 둘 다 4부를 뺀 판)에서 웹뷰 확대가 되는 것(`innerWidth`가 배율만큼 변함), ⌘ 단축키, 앱 글꼴 적용·오류·기본값, 열려 있는 터미널의 글꼴 변경, 150% 확대 중 터미널 화면이 칸 안에 드는 것까지 확인했다. **못 본 것:** 터미널과 편집기가 확대된 채로 눈에 어떻게 보이는지(글자가 또렷한지, 편집기 팝오버 위치)는 화면을 직접 보지 못했다(점검은 DOM 값과 크기만 잰다). 노트 창(`note-*`)에서 같은 권한으로 확대되는지는 점검이 없고(권한 목록이 메인과 같다는 것만 계약 시험이 잰다), 4부를 켠 판은 안 돌렸다.
- 앱에 든 Rust `ais-cli`는 릴리스 설정의 디버그 번들에서 `verify-release.mjs`로 확인했다(8건: 번들의 실행 파일이 노트를 만들고 읽는지, 표준 입출력 MCP가 도는지, Java가 안 들었는지). Java를 번들하지 않으므로 「Java가 없는 Mac」 확인은 필요 없어졌다. 서명·공증, 공개 배포용 최적화 빌드로 다시 지은 확인은 없다.
- 4부를 켠 판의 실제 창 점검은 있지만 얕다(묻기 왕복, 세션 화면, 설정의 Claude 연결 8건). 레일 대화, 세션 입력창, 굳히기는 안 잰다. 진짜 launchd에서 `ais-cli sweep`을 돌려 본 확인(이슈 #54, #61의 손 확인), 서명·공증·업데이트, 원본 아이콘은 없다.
- 물성 모양은 캘린더(일·주·월), 노트 읽기·편집, 설정, 빠른 검색, 시작 화면, 연결, 템플릿을 브라우저에서 눈으로 확인했다. 격자는 캘린더(일), 노트 편집, 설정을 밝게·어둡게로, 유리와 모던은 캘린더(일)를 밝게·어둡게로, 설정과 노트 편집을 밝게로 확인했다. 4부 화면(묻기 카드, 세션 화면, 레일 대화)과 이력 비교, 리포트, 휴지통은 물성 말고는 눈으로 확인하지 못했다.
- 외부 설명서와 달라진 곳: Codex·Gemini 세션을 노트에 적고 읽는 확장이 있다(§5 「세션 에이전트 확장」). `mins` 필드를 지웠다(위 「vault의 모양」). 데일리 리포트 명령(`list-reports`, `get-report`, `save-report`)도 엔진에서 뺐다. 이 저장소의 받아들임 묶음은 `mins`를 뺀 기대값으로 고쳐 두었고, 리포트 사례 2건(`list_reports/all`, `get_report/missing`)을 덜어 27건이다. 진짜 묶음(읽기 레인 460건)을 구해 돌리면 응답에 `mins`가 없는 만큼, 그리고 리포트 사례만큼 어긋난다.
- 설명서 원문이 제공되지 않은 부분(3부 18장, 4부 `ais-agent` 데몬, 진짜 받아들임 묶음)은 추측해서 채우지 않았다. 각 완료 기록의 「못 한 것」을 먼저 본다.
