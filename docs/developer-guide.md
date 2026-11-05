# aisurada 개발자 사용 가이드

이 문서는 저장소를 처음 받은 개발자가 빌드하고, 돌려 보고, 시험하고, 고치는 데 필요한 것을 모았다. 2026-11-04 판(`main` 브랜치)을 기준으로 쓰고, 명령과 수치는 이 판에서 직접 확인한 것만 적었다. 읽어서만 확인한 것은 「확인 방법」을 함께 적었다.

> `docs/` 아래 기록은 이름을 바꾸기 전에 쓴 것이라 옛 이름이 그대로 적혀 있다. 읽다가 낯선 이름이 나오면 [§10의 대응표](#10-docs-읽는-법)로 바꿔 읽는다.

## 1. 구성 한눈에 보기

```
   화면(React, WKWebView)  ── Tauri IPC ──  호스트(Rust)  ── 자식 프로세스 ──  ais-cli.jar
        3부 · 4부 화면                       2부 · 4부 모듈                         │ 유닉스 소켓
                                                                                 ▼
                                                              엔진(Java, ais-sidecar.jar) ── vault 폴더
                                                              1부 · 4부 명령             (마크다운 + .aisurada/)
```

| 부 | 무엇 | 어디 | 언어 |
|---|---|---|---|
| 1부 | vault를 읽고 쓰는 엔진과 클라이언트 `ais-cli` | `src/aisurada/` | Java 21 + SnakeYAML |
| 2부 | Tauri 2 호스트(창, PTY, 파일, 프로세스 실행) | `native-host/src-tauri/` | Rust |
| 3부 | 화면(편집기, 목록, 검색, 이력, 설정, 터미널) | `native-host/src/` | React 19 + TypeScript |
| 4부 | Claude 연동(묻기, 세션 화면, 레일 대화) | 엔진 `ask`·`chat`·`session`, 호스트 `ask.rs` 따위, 화면 `plugs/claude/` | 세 언어 모두 |

엔진은 색인 파일이 없다. 명령을 받을 때마다 vault 폴더를 훑는다. 화면은 엔진을 직접 부르지 않고 호스트의 `sidecar_call`로 `ais-cli.jar`를 실행하며, `ais-cli`가 소켓으로 엔진에 붙는다.

## 2. 준비물

| 도구 | 버전 | 쓰임 |
|---|---|---|
| JDK | 21 이상 | 엔진, `ais-cli`, 엔진 시험 (확인한 판: OpenJDK 21.0.11) |
| Node.js | 22.12 이상 | 화면 빌드와 시험 (확인한 판: 26.9.0) |
| pnpm | 10.33.4 | `package.json`의 `packageManager`로 고정 |
| Rust / cargo | 안정판 | 호스트 (확인한 판: 1.99.1) |
| Xcode 명령줄 도구 | | Tauri 번들, WKWebView |
| git | | vault 자동 커밋과 이력이 실제 git을 부른다 |
| python3 | | acceptance 하네스의 java 호출 기록기가 쓴다 (확인한 판: 3.14.2) |

macOS 전용이다. launchd와 WKWebView, AppKit(받아쓰기)을 쓰기 때문이다. SnakeYAML 2.4는 처음 `./build.sh`를 돌릴 때 Maven Central에서 받아 SHA-1을 확인한다(`lib/`는 git이 무시한다).

## 3. 처음 시작하기

```bash
# 1. 엔진과 클라이언트를 짓는다 (dist/에 jar 4개가 생긴다)
./build.sh

# 2. 가장 작은 vault를 만든다 (임시 폴더에서 해 보자)
export VAULT_DIR=$(mktemp -d)/vault && mkdir -p "$VAULT_DIR"
java -jar dist/ais-engine.jar init-workspace name=시험 'templates=[]' --aisurada.vault="$VAULT_DIR"

# 3. 읽기와 쓰기 (아젠다 이름이 slug가 된다. 한글 이름이면 slug도 한글이다)
java -jar dist/ais-engine.jar create-agenda 'title=demo' --aisurada.vault="$VAULT_DIR"
java -jar dist/ais-engine.jar create-doc agenda=demo type=free title=제목 body=본문 --aisurada.vault="$VAULT_DIR"
java -jar dist/ais-engine.jar list-docs --aisurada.vault="$VAULT_DIR"

# 4. 엔진을 띄워 두고 클라이언트로 부른다 (터미널 둘). 변수는 export하지 말고 명령 앞에 붙인다
AISURADA_VAULT="$VAULT_DIR" AISURADA_SIDECAR_SOCKET="$HOME/aisurada-dev.sock" java -jar dist/ais-engine.jar serve            # 터미널 1
AISURADA_VAULT="$VAULT_DIR" AISURADA_SIDECAR_SOCKET="$HOME/aisurada-dev.sock" java -jar dist/ais-cli.jar list-docs limit=5  # 터미널 2 (VAULT_DIR을 같은 값으로 다시 준다)
```

4번의 `AISURADA_*` 값을 `export`하면 같은 터미널에서 띄우는 앱도 그 값을 물려받는다. 소켓 경로가 vault 하나에 고정돼서 앱이 여는 다른 vault와 어긋나고, 그 소켓에 엔진이 없으면 §7의 `AIS-400` 오류가 난다.

`create-doc`은 `agenda=`, `type=`, `title=`, `body=`가 필요하다(`type=`을 빼면 `AIS-400`). 응답에는 `id`, `path`, 쓰기 전 토큰 `precondition`(`f1:…`)이 들어 있다.

`ais-cli`는 떠 있는 엔진이 없으면 launchd로 엔진을 띄우려고 한다. 이걸 피하려면 4번처럼 엔진을 먼저 직접 띄운다(§6). 끝나면 터미널 1에서 `Ctrl-C`로 엔진을 내리고 `rm "$HOME/aisurada-dev.sock"`로 소켓을 지운다.

산출물 4개:

| 파일 | 쓰임 |
|---|---|
| `dist/ais-engine.jar` | 엔진. 단발 모드와 serve 모드 (4부 켠 판) |
| `dist/ais-sidecar.jar` | `ais-engine.jar`와 같은 파일. `ais-cli`가 설치하고 띄우는 이름 |
| `dist/ais-sidecar-noclaude.jar` | 4부 클래스를 안 실은 판. 4부 명령 7개는 `AIS-409`로 답한다 |
| `dist/ais-cli.jar` | 클라이언트 |

## 4. 저장소 구조

```
src/aisurada/          엔진과 클라이언트 (Java)
test/aisurada/         엔진 시험 (JDK만, 외부 프레임워크 없음)
fixtures/              34장 받아들임 묶음과 실행기 (node)
native-host/
  src/                 화면 (React)
  src-tauri/           호스트 (Rust)
  scripts/             화면·호스트 시험, acceptance 하네스
docs/                  설명서 사본, 장별 완료 기록, 리뷰 기록 (§10)
build.sh               jar 4개를 짓는다
test.sh                엔진 시험 전체
test-native.sh         화면·호스트 시험 전체
```

### 엔진 패키지 (`src/aisurada/`)

| 패키지 | 맡은 일 |
|---|---|
| `cli` | 진입점(`Main`, `AisCliMain`), `doctor`·`sweep`·`install`·`mcp`·`claude-mcp`, launchctl 부르기 |
| `config` | 설정 이름과 값 읽기 (`EngineConfig`) |
| `engine` | 명령 1개를 돌려 상태·stdout·stderr로 돌려줌 (`EngineExecutor`). 단발과 serve가 같이 씀 |
| `serve` | 소켓, 잠금, 실행 상태 파일, 유휴 종료 |
| `frame` | 소켓 위의 요청 프레임(`AIQ1`)과 응답 프레임(`AIG1`) |
| `args` | 도구 표면의 JSON 인자와 명령 표면의 `key=value` 인자 읽기 |
| `cmd` | 명령 실행기(파일 51개)와 이름 목록(`CommandRegistry`) |
| `write` | 쓰기 공통 부품: 표식 확인, 원자적 쓰기, 폴더 옮기기, id, 프론트매터, 경고, 선결 조건 토큰, 편집 도장 |
| `vault` | 아젠다, 프로젝트, 노트, 템플릿, 회고, 휴지통 읽기 |
| `git` | vault 자동 커밋, 이력, 스냅숏 |
| `text` | 다듬기, UTF-16 자르기, URI 따위 글 도구 |
| `out` | 정해진 바이트 모양의 JSON 쓰기 |
| `ask`, `chat`, `session` | 4부 명령 7개 (§7의 4부 스위치 참고) |

### 호스트 모듈 (`native-host/src-tauri/src/`)

`workspace.rs`(vault 허용·만들기), `sidecar.rs`(`sidecar_call`), `files.rs`(그림 첨부 쓰기, 기본 앱 열기), `markdown.rs`(외부 마크다운), `notes.rs`(노트 창), `terminal.rs`(PTY), `secrets.rs`(비밀 가림), `menu.rs`, `layout.rs`(창 자리), `frame.rs`(`aisframe://`), `self_write.rs`, `process.rs`(자식 실행과 시간 상한), `ipc.rs`(이벤트 이름). 4부는 `ask.rs`, `agents.rs`, `session_image.rs`, `dictation.rs`, `session_shell.rs`이고 cargo 기능 `claude` 아래에 있다.

### 화면 모듈 (`native-host/src/`)

`workspace/`(controller, 부팅, 저장 키, 엔진 다리), `data/`(vault 스토어, 쓰기 큐, 자기 쓰기), `editor/`(Lexical 편집기와 마크다운 왕복), `navigation/`(주소, 탭, 스크롤), `interaction/`(단축키, 링크), `ui/`(화면들, 터미널), `plugs/claude/`(4부 화면)와 `plugs/empty/`(4부 없는 판의 빈 구현). 화면 모양 CSS는 `src/styles.css`(기본 모양)와 `src/skin-*.css`(물성·격자·유리·모던)다(§7 「화면 모양」).

## 5. 엔진 쓰는 법

### 두 가지 모양

- **단발 모드**: `java -jar dist/ais-engine.jar <명령> [인자...]` 명령 1개를 처리하고 끝난다.
- **serve 모드**: `java -jar dist/ais-engine.jar serve` vault 1개를 맡아 유닉스 소켓에서 요청을 받는다. 요청마다 vault를 다시 훑는다. 요청이 없으면 10분 뒤 스스로 내려간다.

### 두 가지 표면

같은 일을 이름만 달리 부른다. 도구 표면은 밑줄 이름에 JSON 한 덩이, 명령 표면은 하이픈 이름에 `key=value`다.

```bash
java -jar dist/ais-engine.jar get_doc '{"id":"nddemo-00000001"}'   # 도구 표면
java -jar dist/ais-engine.jar get-doc nddemo-00000001              # 명령 표면
```

도구 29개와 명령 61개는 `src/aisurada/cmd/CommandRegistry.java`에 있다. 도구는 모두 명령에 짝이 있지만, 아젠다·프로젝트·사실층·4부 따위는 명령 표면에만 있다. 이름 목록은 `java -jar dist/ais-cli.jar --list`로 본다.

### 설정

우선순위는 **명령줄 `--키=값` > JVM `-D키=값` > 환경 변수 > 기본값**이다. 키는 7개이고 모르는 `--키`는 조용히 버린다.

| 키 | 환경 변수 | 기본값 |
|---|---|---|
| `aisurada.vault` | `AISURADA_VAULT` | 없음(필수) |
| `aisurada.db.home` | `AISURADA_DB_HOME` | `~/Library/Application Support/aisurada/db` |
| `aisurada.notes.shard.by.month` | `AISURADA_NOTES_SHARD_BY_MONTH` | 정하지 않음 |
| `aisurada.idle.timeout.minutes` | `AISURADA_IDLE_TIMEOUT_MINUTES` | 10 (`dev` 프로필이면 0) |
| `aisurada.vault.autocommit.enabled` | `AISURADA_VAULT_AUTOCOMMIT_ENABLED` | true |
| `aisurada.vault.autocommit.idle.seconds` | `AISURADA_VAULT_AUTOCOMMIT_IDLE_SECONDS` | 300 |
| `aisurada.vault.snapshot.enabled` | `AISURADA_VAULT_SNAPSHOT_ENABLED` | false |

같은 키를 명령줄에 여러 번 주면 `=`가 있는 칸의 값만 쉼표로 잇는다. 불리언은 `true/on/yes/1`과 `false/off/no/0`만 받는다.

`ais-cli`와 호스트가 읽는 환경 변수:

| 이름 | 쓰임 |
|---|---|
| `AISURADA_SIDECAR_SOCKET` | 소켓 자리. **102바이트 이하**여야 한다. 기본은 `~/Library/Caches/aisurada/ais-<해시>.sock` |
| `AISURADA_SIDECAR_JAR` | launchd plist에 실을 엔진 jar |
| `AISURADA_SIDECAR_JAVA` | plist에 실을 `java` (호스트는 이걸로 `ais-cli`를 도는 `java`도 고른다) |
| `AISURADA_CLI_JAR` | 호스트가 부를 `ais-cli.jar`, `install`이 깔 원본 |
| `AISURADA_READY_TIMEOUT_SECONDS` | 띄운 뒤 붙기를 기다리는 초 (30) |
| `AISURADA_SIDECAR_LIMIT` | 동시에 떠 있을 엔진 수 (3) |
| `AISURADA_SWEEP_QUIET_HOURS` | 청소가 내릴 만큼 조용한 시간 (24) |
| `AISURADA_CLAUDE_BIN` | 호스트가 쓸 `claude` 실행 파일 (4부) |

### 종료 코드

| 코드 | 뜻 |
|---|---|
| 0 | 성공. stdout에 JSON 하나와 줄바꿈 1개 |
| 1 | 예상 밖 실패. stderr에 스택 트레이스. 쓰기 명령이 vault 사전 훑기에서 서면 이것도 1 |
| 2 | vault를 못 찾았거나 모르는 명령 |
| 3 | `AIS-…` 코드가 붙은 실패. stderr 한 줄, stdout 비어 있음 |
| 4 | `ais-cli`만 낸다. 엔진에 못 닿음 |
| 5 | 선결 조건 토큰(`f1`)이 어긋나 아무것도 안 씀(`AIS-412`) |

### vault의 모양

```
vault/
├── .aisurada/
│   ├── workspace.json          이게 있어야 쓰기 명령이 돈다 (init-workspace가 만든다)
│   ├── agendas/<slug>.md       아젠다 파일 (프론트매터 id, slug, title)
│   ├── projects/<slug>.md
│   ├── facts/<slug>.md         아젠다 사실층
│   ├── templates/{doc,ritual}/
│   ├── reports/                회고
│   └── trash/                  휴지통
└── notes/<아젠다 slug>/<노트 폴더>/
    ├── note.md                 프론트매터(id, agenda, type, title, created, tags …) + 본문
    ├── attach/                 글·그림 첨부
    ├── sub/<하위 노트>/note.md  하위 노트(재귀)
    ├── ask.json, chats/        4부 문답과 레일 대화
    └── …
```

HTML 노트와 템플릿은 `---` 대신 `<!--aisurada` … `-->` 머리를 쓴다.

### 쓰기 규칙 (고칠 때 지킬 것)

- 쓰기는 같은 폴더의 임시 파일에 쓴 뒤 **원자적 이름 바꾸기**로 둔다. 실패하면 다른 길로 다시 하지 않고 `AIS-500`으로 선다.
- `update_doc`, 메타 쓰기 3개, `update_sub_note`는 직전에 읽은 토큰 `f1`이 지금 파일 바이트와 맞아야 쓴다. 안 왔거나 어긋나면 `AIS-412`, 종료 코드 5.
- 편집 도장(`editedBy`, `editedAt`)은 파일 글이 실제로 달라질 때만 새로 찍는다.
- 쓰기 명령은 일하기 전에 vault를 훑고, 노트로 못 읽는 파일이 있으면 아무것도 안 쓰고 종료 코드 1로 선다.
- serve는 시작·종료·유휴·15분 경과·작성자 전환 때 vault를 자동 커밋한다. `.git`이 있는 `main` 브랜치에서만 돈다. 단발 모드는 자동 커밋하지 않는다.

### 명령을 새로 넣는 길

1. `src/aisurada/cmd/`에 실행기를 만든다. 쓰기 공통 부품은 `write/`에 있다.
2. `CommandRegistry`의 `COMMAND_NAMES`(도구 표면도 있으면 `TOOL_NAMES`), 쓰기면 `WRITE_COMMANDS`, 그리고 `USAGE_LINES`에 이름과 사용법을 넣는다.
3. 시험을 `test/aisurada/`에 두고 `test.sh`에 한 줄 넣는다(§8).
4. 설명서 장이 있으면 그 장의 바이트 예시와 맞는지 대조하고, 받아들임 묶음에 케이스를 더할 수 있으면 더한다(§8).

### MCP 서버 (`ais-cli mcp`)

Claude 같은 MCP 클라이언트가 노트를 도구로 쓰게 하는 모드다. 표준 입출력으로 한 줄에 JSON-RPC 메시지 하나씩 주고받고(`initialize`, `ping`, `tools/list`, `tools/call`), 입력이 닫히면 끝난다. 프로토콜은 `src/aisurada/cli/McpServer.java`, 도구 정의는 `McpTools.java`에 있다. 이것은 설명서 밖의 확장이다. 외부 설명서에는 MCP가 없고, 예전 Node MCP 서버는 C119에서 지웠다.

- **도구.** 이름은 엔진 도구 29개와 같다. 기본은 읽기 13개만 내놓고 `--write`를 주면 28개를 내놓는다. `purge_doc`(영구 삭제)는 어느 쪽에도 없다. 모델이 되돌릴 수 없는 삭제를 하지 못하게 하려는 것이다. 읽기와 쓰기의 구분은 `CommandRegistry.WRITE_COMMANDS`에서 나오므로 두 곳이 어긋나지 않는다.
- **호출 길.** 도구 호출은 단발 `ais-cli`와 같은 길(`runEnginePipeline`: 소켓 찾기, 엔진 띄우기, 요청 왕복)을 JVM 하나에서 되풀이한다. 이 길은 곳곳에서 `System.exit`을 부르는데, `AisCliMain`의 종료 호출은 모두 `exit()` 하나를 거친다. mcp 모드에서만 `exit()`이 프로세스를 끝내는 대신 `CliExit`을 던지고, 그 호출만 `isError`로 답한다. 이 길이 표준 출력·오류에 쓰는 글은 호출마다 모아서 돌려주므로 프로토콜 출력에 섞이지 않는다.
- **vault.** `AISURADA_VAULT` 하나다. 서버 하나가 vault 하나를 맡는다.
- **스키마를 고칠 때.** `McpTools`의 입력 스키마 키는 `ArgsParser`가 받는 키와 같아야 한다. `McpServerTest`가 엔진의 「모르는 키다 … 쓸 수 있는 것은 […] 뿐이다」 문구와 도구 29개를 모두 대조하고, 필수 키도 하나씩 빼 보며 잰다. 도구의 인자를 바꾸면 `McpTools`도 고친다. 안 고치면 이 시험이 선다.
- **Claude에 연결.** `ais-cli claude-mcp status|add [--write]|remove|config`(`ClaudeMcpCommand`)가 Claude Code에 서버를 등록한다. Claude의 설정 파일을 직접 고치지 않고 `claude mcp add-json --scope user`만 부른다. 등록하는 Java와 jar는 이 명령을 돌린 것이라, 앱에서 부르면 앱에 든 것이 등록된다. `config`는 등록하지 않고 Claude Desktop 설정에 붙여 넣을 `mcpServers` 조각만 출력한다. 설정 화면의 「Claude 연결」(`src/plugs/claude/connect.tsx`)은 `sidecar_call`로 이 명령을 부르기만 하므로 호스트 커맨드를 더하지 않았다(4부 9개 그대로). 4부를 끈 판에는 이 화면이 없다(`plugs/empty/connect.tsx`). 연결은 지금 연 워크스페이스 하나에 묶이고, 다른 워크스페이스에서 다시 연결하면 바뀐다.

시험은 `bash test.sh`에 들어 있다. `McpServerTest`는 스키마 대조, 노출 규칙, 가짜 호출기로 프로토콜, 진짜 엔진과 진짜 `mcp` 프로세스의 왕복(읽기·쓰기·낡은 `f1` 거절·휴지통·영구 삭제 거절·vault 없는 환경에서도 서버가 사는지)을 잰다. `ClaudeMcpCommandTest`는 등록 상태를 파일에 두는 가짜 `claude`로 `status`·`add`·`remove`·`config`를 잰다.

## 6. `ais-cli`와 launchd — 건드리기 전에

`ais-cli`는 떠 있는 엔진이 없으면 **`~/Library/LaunchAgents/co.kcd.aisurada.sidecar.<키>.plist`를 쓰고 `launchctl`을 부른다.** 엔진이 동시 상한(3개)을 넘으면 가장 오래 조용한 엔진을 내리고, 하루 한 번 24시간 넘게 조용한 엔진도 내린다. 이 저장소의 시험은 모두 임시 HOME과 가짜 `launchctl`로 돌아서 진짜 launchd는 쓴 적이 없다.

안전하게 돌리는 법:

- 엔진을 먼저 직접 띄운다(`AISURADA_SIDECAR_SOCKET`을 짧은 경로로 주고 `serve`). 그러면 `ais-cli`가 launchd를 부르지 않고 그 소켓에 붙는다.
- `java -Duser.home=<격리 HOME>`으로 띄운다. 안 그러면 4부 명령이 진짜 `~/.claude/projects`를 읽는다. (`$HOME` 환경 변수는 JVM의 `user.home`을 바꾸지 못한다.)
- `ais-cli doctor`로 키, 레이블, 소켓, plist를 진단한다. 등록한 엔진을 지우려면 `launchctl bootout gui/$(id -u)/<레이블>` 뒤에 그 plist를 지운다.

앱을 격리해서 띄우는 전체 순서는 [`beta-hand-check.md`](beta-hand-check.md)의 §1에 있다.

## 7. 네이티브 호스트와 화면

### 개발 실행

```bash
bash build.sh                       # 번들이 jar를 자원으로 싣는다
cd native-host
pnpm install --frozen-lockfile      # postinstall이 @claude-plug 꾸러미를 꽂는다
pnpm tauri dev                      # 실제 앱까지 (포트 1420, 충돌하면 실패)
pnpm dev                            # Vite 서버만
pnpm preview                        # 호스트 없이 번들 CSP로 화면만 본다
```

`pnpm tauri dev`는 README에 적힌 대로 쓰는 길이고 이 문서를 쓰면서 새로 돌려 보지는 않았다. 개발 서버에 페이지가 붙어 있으면 `curl -X POST localhost:1420/__deeplink --data-raw 'aisurada://doc/<id>'`로 열린 main 창을 이동할 수 있다(개발 판 전용, 번들에서는 빠진다).

**개발 실행은 엔진 jar를 자동으로 넘기지 않는다.** 번들 판은 자원 폴더의 `ais-sidecar.jar`를 `ais-cli`에 넘기지만, `tauri dev`와 `target/…/bundle` 아래 번들은 낡은 사본을 쓰지 않으려고 넘기지 않는다(`sidecar.rs`의 `execute`). 앱이 vault를 열 때 엔진이 없으면 `ais-cli`가 launchd로 엔진을 띄우려 하고, 이때 jar를 못 찾으면 `AIS-400 AISURADA_SIDECAR_JAR 가 없고 …`로 선다. 앱으로 워크스페이스를 만들거나 열어 보려면 앱을 띄우는 터미널에서 먼저 이렇게 한다.

```bash
unset AISURADA_VAULT AISURADA_SIDECAR_SOCKET              # §3 예시의 값이 남아 있으면 지운다
export AISURADA_SIDECAR_JAR="$PWD/dist/ais-sidecar.jar"   # 반드시 절대 경로다(plist에 그대로 실린다)
```

대신 한 번 `java -jar dist/ais-cli.jar install --cli dist/ais-cli.jar --sidecar dist/ais-sidecar.jar`로 `~/Library/Application Support/aisurada/bin/`에 깔아도 된다. 깐 사본은 빌드할 때마다 다시 깔아야 한다.

**이 길은 진짜 launchd를 쓴다.** vault마다 `~/Library/LaunchAgents/co.kcd.aisurada.sidecar.<키>.plist`가 생기고 `launchctl`에 올라간다. 이 저장소의 시험은 모두 가짜 `launchctl`이라 이 길을 자동으로 잰 적이 없다. 끝나면 `java -jar dist/ais-cli.jar doctor`로 레이블과 plist를 보고, `launchctl bootout gui/$(id -u)/<레이블>` 뒤에 그 plist를 지운다. launchd를 건드리지 않고 앱을 보려면 [`beta-hand-check.md`](beta-hand-check.md)의 격리 실행법을 쓴다. 이때는 엔진 하나가 vault 하나만 맡으므로 그 vault만 열 수 있다.

### 번들 만들기

```bash
cd native-host
pnpm tauri build --debug --bundles app   # 산출: src-tauri/target/debug/bundle/macos/aisurada.app
```

`--debug`를 빼면 최적화 빌드다. `beforeBuildCommand`가 화면과 `build.sh`를 새로 돌려서 `dist/`를 지웠다 다시 만든다. **설치나 launchd 등록은 하지 않는다.** 단 번들은 `.md`/`.markdown`의 기본 편집기로 등록되므로(`fileAssociations`) `open 문서.md`가 이 앱을 진짜 HOME으로 띄운다.

### 앱에 든 `ais-cli`

`native-host/src-tauri/resources/ais-cli`(셸 스크립트)가 번들의 `Contents/Resources/bin/ais-cli`로 들어간다(`tauri.conf.json`의 `resources`).

- 같은 앱의 Java(`Resources/runtime/bin/java`)로 같은 앱의 `ais-cli.jar`를 돈다. 번들 Java는 릴리스 설정(`tauri.release.conf.json`)으로 지은 번들에만 있고, 없으면 PATH의 `java`를 쓴다. 심볼릭 링크로 불러도 앱 안의 자리를 찾는다. 엔진 jar는 `AISURADA_SIDECAR_JAR`로 앱의 `ais-sidecar.jar`를 넘긴다. 단 `target/…/bundle` 아래의 개발용 빌드는 낡은 사본을 쓰지 않으려고 넘기지 않는다(호스트 `sidecar.rs`의 규칙과 같다).
- **셸 내장 기능만 쓴다.** Finder나 launchd가 띄운 환경은 PATH가 비어 있다. 실제 번들 검증에서 `dirname` 때문에 서던 것을 고쳤다. 외부 명령은 절대 경로로 부르는 `readlink` 하나뿐이다.
- 호스트는 앱이 뜰 때 이 폴더를 `ask.rs`의 `set_tools_dir`로 알려 두고, `claude -p`를 띄울 때 PATH 맨 앞에 얹는다. 그래서 앱 안에서 묻는 Claude는 사용자가 래퍼를 만들거나 Java를 깔지 않아도 `ais-cli`를 부를 수 있다. 앱 안 터미널(`pty_start_session`)의 PATH는 바꾸지 않았다. 사용자의 로그인 셸 환경을 따른다.
- 터미널에서 쓰려면 연동 가이드 2단계의 링크 한 줄이면 된다. 예전 가이드로 만든 `~/.local/bin/ais-cli`(시스템 `java`)가 있어도 앱이 띄우는 Claude에게는 번들 것이 먼저 잡힌다.
- 시험은 스크립트 `test:cli-script`(가짜 앱 폴더에서 진짜 셸 스크립트를 돌린다. PATH가 비었을 때, 링크로 불렀을 때, 개발용 빌드일 때 포함), PATH 얹기 `ask.rs`의 `bundled_tools_dir_comes_first_only_when_it_exists`, 실제 번들 `scripts/verify-release.mjs`(PATH 없이 스크립트로 노트를 읽고, MCP 등록 명령이 번들 Java를 가리키는지)다.
- **못 한 것.** Java가 없는 Mac에서는 돌려 보지 않았다. 이 기계에는 JDK가 있다. 스크립트가 번들 Java를 절대 경로로 고르는 것(`sh -x`)과 PATH를 비운 채 도는 것까지만 확인했다.

### 4부 스위치 3개

| 층 | 켠 판(기본) | 끈 판 |
|---|---|---|
| 엔진 | `ais-sidecar.jar` (`engine-build.properties`의 `part4=true`) | `ais-sidecar-noclaude.jar` (`part4=false`, 4부 클래스를 안 실음) |
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

### 계약을 건드릴 때

호스트 커맨드(2부 29개, 4부 9개)와 이벤트 4개(`ais://menu`, `ais://navigate`, `ais://note-windows`, `ais://self-write`), 번들 식별자, 권한 파일(`src-tauri/capabilities/main.json`·`note.json`)은 시험이 글자까지 잰다(`scripts/contracts.test.mjs`). 커맨드를 더하거나 권한을 바꾸면 이 시험과 `capabilities/`를 함께 고친다. `main`과 `note-*` 창의 권한은 일부러 다르다.

**설명서와 일부러 다르게 둔 것.** `create_workspace_dir({parent, name})`는 설명서대로라면 저장 위치(`parent`)가 없을 때 실패해야 하지만, 지금은 이름 검사를 모두 통과한 뒤 저장 위치를 만든다(`mkdir -p`). 새 워크스페이스 화면에서 없는 폴더를 적으면 「저장 위치를 찾을 수 없어요」로 막히던 것을 2026-11-04에 바꿨다. 저장 위치가 파일이면 여전히 실패하고, 잘못된 이름은 저장 위치를 만들기 전에 거절한다(`workspace.rs`의 `create_dir_makes_a_missing_parent_after_validating_the_name`).

## 8. 시험

### 한눈에

| 하는 일 | 명령 | 걸리는 시간 | 뭘 재나 |
|---|---|---|---|
| 엔진 전체 | `bash test.sh` | 수 분 | 실제 임시 vault, 실제 serve와 `ais-cli` 왕복 |
| 화면·호스트 전체 | `bash test-native.sh` | 수 분 | 타입 검사, 화면 빌드 2종, Node 시험 449건, `cargo test` 90건(무시 1건) |
| 받아들임 묶음 | 아래 | 수 초 | 설명서 원문 기대값 29건과 실행기 자체 |
| 실제 창 점검 | `pnpm test:*:acceptance` | 종류당 수 분 | 실제 WKWebView, 실제 Java 엔진, 격리 HOME |

두 전체 시험 모두 맨 앞에서 `build.sh`를 돌려 `build/`와 `dist/`를 **지우고 다시 만든다.**

### 엔진 시험 쓰기

외부 시험 프레임워크는 없다. 시험 클래스 하나가 `main`을 가지고 `check(...)`와 `eq(...)`로 센다. 장별 시험은 `test/aisurada/ChapterTestSupport.java`를 쓴다.

```java
try (ChapterTestSupport t = new ChapterTestSupport("chapter99")) {
    Path v = t.vault();                       // workspace.json과 아젠다가 있는 임시 vault
    var r = ok(v, "create-doc", "agenda=demo", "type=free", "title=제목", "body=본문");
    eq(r.get("agenda"), "demo", "아젠다가 그대로");
    check(r.get("precondition").toString().startsWith("f1:"), "쓰기 전 토큰이 따라온다");
}
```

- 시계와 id는 JVM 속성으로 고정한다: `aisurada.test.now`(시각, 도장과 평가 시각도 따름), `aisurada.test.random`(id의 무작위 조각). 속성을 안 주면 아무 일도 안 한다. 계약이 아니라 시험용이다.
- 새 시험 클래스는 `test.sh`에 `java -cp dist/ais-engine.jar:build/test-classes aisurada.<클래스>` 한 줄을 더한다. 더하지 않으면 안 돈다.
- `test/aisurada/cli/`의 시험(`McpServerTest`, `ClaudeMcpCommandTest`)은 `aisurada.cli` 패키지에 있어서 `AisCliMain`의 패키지 비공개 부분(`CliExit`, `Call`)을 직접 본다. `ChapterTestSupport`는 쓰지 않고 자체 `check`/`eq`를 가진다. 클래스 이름은 `aisurada.cli.<클래스>`다.
- 임시 HOME·vault·소켓만 만들고 지운다. 사용자의 HOME이나 vault를 건드리는 시험을 쓰지 않는다.

### 받아들임 묶음 (34장)

```bash
./build.sh
AISX_IMPL_JAVA="$(command -v java)" AISX_IMPL_JAR="$PWD/dist/ais-engine.jar" \
  node fixtures/expect/run.mjs --engine fixtures/aisurada.adapter.json --out <빈 폴더>
node fixtures/expect/run.mjs --self-check --mutants --out <빈 폴더>
node fixtures/runner-test.mjs
```

`--out`은 결과를 쓸 폴더다(저는 매번 새 임시 폴더를 줬다). 케이스를 더하거나 `fixtures/expect/`의 파일을 고치면 `node fixtures/build-bundle.mjs`로 `bundle.json`의 sha256 목록을 다시 적는다. 진짜 묶음(읽기 레인 460건)은 이 저장소에 없다.

### 화면·호스트 시험

`package.json`의 `test:*`가 종류별 진입점이다.

| 종류 | 진입점 | 비고 |
|---|---|---|
| 순수 로직(Node) | `test:screen`, `test:workspace`, `test:navigation`, `test:interaction`, `test:data`, `test:editor`, `test:views`, `test:autosave`, `test:chapters`, `test:cli-script`, `test:connect` | 호스트 없이 돈다. `test:cli-script`는 앱에 든 `ais-cli` 스크립트를, `test:connect`는 설정의 Claude 연결 화면을 잰다 |
| 4부 | `scripts/chapter03-04.test.mjs`, `chapter06`, `chapter08`, `chapter09`, `chapter10`, `part4-ask-run.test.mjs` | `test-native.sh`가 부른다 |
| 계약 | `node --test scripts/contracts.test.mjs` | 번들 식별자, 권한, CSP |
| 호스트 | `cargo test --manifest-path native-host/src-tauri/Cargo.toml --locked` | |
| 실제 창 | `test:acceptance`, `test:workspace:acceptance`, `test:navigation:acceptance`, `test:interaction:acceptance`, `test:data:acceptance`, `test:editor:acceptance`, `test:autosave:acceptance`, `test:views:acceptance` | 종류별 한 번에 하나씩. 4부를 뺀 판을 잰다 |
| 실제 창(4부) | `test:part4:acceptance`(켠 판 7건), `test:ask-marks:acceptance`(끈 판 5건) | `test-native.sh`에 들어 있지 않으니 따로 돌린다 |

실제 창 점검은 실행마다 별도 앱 식별자와 임시 HOME·vault·소켓·빈 `ZDOTDIR`를 만들어서 쓴다. 성공하면 임시 자료를 지우고, 실패하면 `Diagnostics retained: <경로>`를 찍는다. `AISURADA_WORKSPACE_REPORT=<절대 경로>.json`이나 `AISURADA_CHECK_REPORT=<절대 경로>.json`으로 결과를 JSON으로 남길 수 있다. `--only=<사례 id>`로 일부만 돌릴 수 있다(편집기 왕복).

**4부 점검.** 위 8종은 4부를 뺀 판(`build:noclaude`, `--no-default-features`)을 잰다. 4부를 켠 판은 `pnpm test:part4:acceptance`가 잰다(`pnpm build`로 4부 화면을 짓고 호스트를 기본 기능으로 띄운다). 가짜 `claude`(`AISURADA_CLAUDE_BIN`)와 격리 HOME의 가짜 전사(`~/.claude/projects/…/<세션 id>.jsonl`)를 쓰므로 토큰을 쓰지 않는다. 점검 코드는 `native-host/scripts/part4-checks.js`다.

| 재는 것 | 안 재는 것 |
|---|---|
| 노트 화면이 렌더 루프 없이 뜸, 표 줄 점, 하위 노트 | 노트 옆 세션 레일과 레일 대화 |
| **묻기 왕복**: 글 선택 → 「AI에게 묻기」 → 「묻기」 → 가짜 `claude`의 답이 「AI 문답」 칸에 붙음 → 엔진의 `ask.json`과 본문 표식까지 | 세션 입력창으로 보내기, 받아쓰기, 굳히기 |
| 세션 화면: 전사 없음 안내와 CLI 칸, **전사가 있는 세션**의 대화(`recent-sessions`와 화면) | PROJECTS 레일, 실제 `claude` |

`test:ask-marks:acceptance`는 4부를 끈 판에서 호스트에 `ask_runtime`이 없고 노트의 문답 표식이 그대로 남는지 잰다.

### 동시에 돌리면 안 되는 것

`build.sh`, `test.sh`, `test-native.sh`, `tauri build`(내부에서 `build.sh`)는 `build/`와 `dist/`를 지운다. 이 중 둘을 동시에 돌리거나, 점검이 도는 동안 다른 터미널·세션에서 하나라도 돌리면 acceptance가 `ais-cli.jar를 못 찾았어요`로 깨진다. 실제로 겪었다. 병렬로 확인할 일이 있으면 `git worktree add --detach <경로> <커밋>`으로 커밋만 떼어 놓고 거기서 돌린다. `cp -cR native-host/src-tauri/target <경로>/native-host/src-tauri/target`으로 cargo 산출물(약 12GB)을 APFS 복제하면 빠르다.

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
| `wl-cli` | `ais-cli` | 클라이언트 jar와 명령 |
| `wl-engine.jar`, `wl-sidecar.jar`, `wl-sidecar-noclaude.jar` | `ais-engine.jar`, `ais-sidecar.jar`, `ais-sidecar-noclaude.jar` | |
| `WL-409`, `WL-412` … | `AIS-409`, `AIS-412` … | 오류 코드 |
| `WlException`, `WlCliMain` | `AisException`, `AisCliMain` | 자바 클래스 |
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
| `소켓 경로가 102 바이트를 넘는다` | `AISURADA_SIDECAR_SOCKET`을 `$HOME` 바로 아래 같은 짧은 경로로 준다 |
| 앱에서 `워크스페이스 생성 실패 … AIS-400 AISURADA_SIDECAR_JAR 가 없고 설치된 ais-sidecar.jar 도 없다` | 개발 실행의 앱이 엔진을 띄우려는데 jar를 못 찾는다(§7). 앱을 띄우는 터미널에서 `AISURADA_SIDECAR_JAR`를 절대 경로로 주고, §3 예시의 `AISURADA_VAULT`와 `AISURADA_SIDECAR_SOCKET`은 `unset`한 뒤 앱을 다시 띄운다 |
| `ais-cli.jar를 못 찾았어요 … 찾아본 자리: …/dist/ais-cli.jar` | `./build.sh`를 안 돌렸거나, 다른 곳에서 `build.sh`가 `dist/`를 지우는 중이다(§8) |
| 앱 안에서 묻는데 Claude가 `ais-cli`를 못 찾거나 `java`가 없다고 한다 | 개발 실행에는 번들 스크립트도 번들 Java도 없다. 릴리스 설정으로 지은 번들(`tauri.release.conf.json`)만 둘 다 싣는다. 개발 중에는 PATH에 `java`와 `ais-cli`가 있어야 한다(§7) |
| MCP 클라이언트가 `aisurada` 서버에 못 붙거나 곧 끊는다 | 서버의 표준 출력에 JSON-RPC가 아닌 글이 섞이면 클라이언트가 끊는다. mcp 모드에서 엔진 길의 출력은 호출마다 모아서 돌려주게 되어 있다(§5). 새로 출력을 더하는 코드는 `System.err`로 쓴다. 직접 보려면 `AISURADA_VAULT`를 주고 `java -jar dist/ais-cli.jar mcp`를 띄운 뒤 표준 입력에 `initialize` 요청 한 줄을 넣어 본다 |
| 문서를 `open`했더니 앱이 떴다 | 번들이 `.md` 기본 편집기로 등록돼 있다. 에디터나 브라우저로 연다 |
| 앱을 켰더니 진짜 launchd에 plist가 생길 것 같다 | §6처럼 엔진을 먼저 띄우고 격리 HOME으로 앱을 띄운다 |
| 4부 명령이 `AIS-409`로 선다 | 4부를 뺀 jar(`ais-sidecar-noclaude.jar`)를 쓰고 있다 |
| `cargo test --locked`가 락 파일 때문에 선다 | 패키지 이름이나 의존성을 바꿨다. 한 번 `cargo build`(락 없이)로 `Cargo.lock`을 갱신해 커밋한다 |
| acceptance 뒤 `$TMPDIR/ais-p3-*`가 쌓인다 | 실패한 점검의 진단 보존이다. 원인을 본 뒤 지운다 |
| 이름이나 문구를 바꿨더니 HTML 노트를 못 읽는다 | 글자 수를 숫자로 박아 둔 코드다(예: 마커 `<!--aisurada`의 길이). 숫자 대신 문자열의 `length()`를 쓴다 |
| 이 셸의 `rm`이 확인을 묻는다 | `rm`이 `-i` 별칭이다. `/bin/rm -rf`를 쓴다 |
| zsh에서 `echo ====`가 `not found` | `=`로 시작하는 낱말을 zsh가 명령으로 푼다. 따옴표로 감싼다 |

## 13. 지금 상태와 한계

- 엔진 시험, 화면·호스트 시험, 34장 받아들임, 4부를 뺀 실제 창 점검 8종은 통과한다. 수치는 `README.md`와 각 완료 기록에 있다.
- 사람이 앱을 직접 조작해 본 확인은 아직 없다. 점검표는 §9의 문서에 있다.
- MCP 서버(`ais-cli mcp`)는 진짜 엔진과 진짜 프로세스로 표준 입출력 왕복을 잰다. 진짜 Claude Code 2.1.318로는 격리 HOME에서 등록, 쓰기 모드로 바꿔 끼우기, 연결 확인(`✔ Connected`), 끊기까지 확인했다. 못 한 것: 모델이 실제로 도구를 부르는 흐름, Claude Desktop, MCP 규격 공식 문서와의 대조(받는 개정은 2024-11-05, 2025-03-26, 2025-06-18이고 규격은 다른 자료로 짰다).
- 설정 화면의 「Claude 연결」 단추는 4부 실제 창 점검에서 가짜 claude로 눌러 보았다. 진짜 claude로 화면의 단추를 눌러 본 적은 없다(손 확인 4-4).
- 앱에 든 Java로 도는 `ais-cli`는 릴리스 설정의 디버그 번들에서 실제로 확인했다(`verify-release.mjs` 10건). Java가 없는 Mac에서 설치해 본 확인, 서명·공증, 공개 배포용 최적화 빌드로 다시 지은 확인은 없다.
- 4부를 켠 판의 실제 창 점검은 있지만 얕다(묻기 왕복, 세션 화면, 설정의 Claude 연결 8건). 레일 대화, 세션 입력창, PROJECTS 레일, 굳히기는 안 잰다. 진짜 `launchctl`로 돌려 본 확인, 서명·공증·업데이트, 원본 아이콘은 없다.
- 물성 모양은 캘린더(일·주·월), 노트 읽기·편집, 설정, 빠른 검색, 시작 화면, 연결, 템플릿을 브라우저에서 눈으로 확인했다. 격자는 캘린더(일), 노트 편집, 설정을 밝게·어둡게로, 유리와 모던은 캘린더(일)를 밝게·어둡게로, 설정과 노트 편집을 밝게로 확인했다. 4부 화면(묻기 카드, 세션 화면, 레일 대화)과 이력 비교, 리포트, 휴지통은 물성 말고는 눈으로 확인하지 못했다.
- 설명서 원문이 제공되지 않은 부분(3부 18장, 4부 `ais-agent` 데몬, 진짜 받아들임 묶음)은 추측해서 채우지 않았다. 각 완료 기록의 「못 한 것」을 먼저 본다.
