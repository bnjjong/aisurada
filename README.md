# aisurada

withlog vault를 읽어 JSON으로 답하고 새 노트를 쓰는 엔진과, 그 엔진을 띄워 두고 부르는 클라이언트 `wl-cli`입니다. Java 21 하나와 SnakeYAML만으로 짓습니다.

이름은 〈신세기 GPX 사이버 포뮬러〉의 아스라다에서 따온 말장난(AI + 아스라다)입니다. 사람 옆에서 달리는 AI라는 자리가 마음에 들었습니다.

## 하는 일

vault 폴더를 부를 때마다 훑어서 답합니다. 색인 파일을 만들지 않습니다. vault에 쓰는 명령은 새 노트를 만드는 `create_doc` 1개뿐입니다.

엔진은 두 가지 모양으로 돕니다.

- **단발 모드**: 명령 1개를 처리하고 끝납니다.
- **serve 모드**: vault 1개를 맡아 유닉스 도메인 소켓에서 요청을 받습니다. 요청마다 vault를 다시 훑어서, 떠 있는 동안 바뀐 파일도 다음 요청에 보입니다. 10분 동안 요청이 없으면 스스로 내려갑니다.

`wl-cli`는 serve 모드 엔진에 붙어 명령을 보내고 답을 그대로 흘립니다. 떠 있는 엔진이 없으면 macOS launchd에 LaunchAgent로 등록해 띄우고 30초까지 기다립니다.

| 명령 | 답하는 것 |
|---|---|
| `list_docs` | 노트 목록 (유형, 태그, 날짜, 즐겨찾기 필터) |
| `search_docs` | 제목, 메타, 본문, 첨부 글을 찾아 대목과 함께 |
| `list_tags` | 태그와 쓰인 횟수 |
| `get_doc` | 노트 1건 (본문, 도장, 코멘트) |
| `list_sub_notes`, `get_sub_note` | 하위 노트 목록과 1건 |
| `list_attachments` | 노트나 하위 노트의 첨부 |
| `list_templates`, `get_template` | 문서·리추얼 템플릿 |
| `list_reports`, `get_report` | 회고 |
| `list_trash` | 휴지통 |
| `reindex` | vault를 한 번 훑어 읽지 못한 파일이 있는지 알려 준다 |
| `create_doc` | 새 노트 1건을 쓴다. 없는 아젠다면 아젠다 파일도 만들고, 같은 제목이나 들여쓴 블록 같은 경고 5가지를 함께 답한다 |

도구 이름(`get_doc`)과 명령 이름(`get-doc`)을 모두 받습니다. `create_doc`은 vault에 워크스페이스 표식 `.withlog/workspace.json`이 있어야 쓰고, 파일은 같은 폴더의 임시 파일에 쓴 뒤 원자적 이름 바꾸기로 둡니다. `update_doc` 같은 다른 쓰기 명령처럼 이 판에 없는 명령은 `WL-409`로 답합니다. 4부(Claude 연동) 명령 7개(`get-ask`, `read-session` 등)는 이름 인자와 자리 인자 검사를 먼저 거친 뒤 `WL-409 이 판에는 4부(Claude 연동)가 없다.`로 답합니다. vault의 git 자동 커밋은 짓지 않았습니다. 시험용 JVM 속성 `withlog.test.now`(시계)와 `withlog.test.random`(id의 무작위 조각)은 계약이 아니고, 주지 않으면 아무 일도 안 합니다.

## 빌드

JDK 21 이상이 필요합니다. `JAVA_HOME`이 있으면 그것을, 없으면 `PATH`의 `javac`와 `jar`를 씁니다.

```bash
./build.sh
```

SnakeYAML 2.4는 처음 빌드할 때 Maven Central에서 내려받아 체크섬을 확인합니다. 산출물은 3개입니다.

| 파일 | 쓰임 |
|---|---|
| `dist/wl-engine.jar` | 엔진. 단발 모드와 serve 모드 |
| `dist/wl-sidecar.jar` | `wl-engine.jar`와 같은 파일. `wl-cli`가 설치하고 띄우는 이름 |
| `dist/wl-cli.jar` | 클라이언트 |

## 실행

### 단발 모드

vault 폴더의 절대 경로를 환경 변수나 인자로 줍니다.

```bash
export WITHLOG_VAULT=/path/to/vault

java -jar dist/wl-engine.jar list-docs
java -jar dist/wl-engine.jar search-docs 첫
java -jar dist/wl-engine.jar get_doc '{"id":"nddemo-00000001"}'
java -jar dist/wl-engine.jar list_tags '{}'

java -jar dist/wl-engine.jar list-docs --withlog.vault=/path/to/vault
```

가장 작은 vault는 아젠다 파일 1개와 노트 1개입니다.

```text
vault/
├── .withlog/agendas/demo.md
└── notes/demo/hello/note.md
```

```markdown
---
id: ags-00000001
slug: demo
title: 데모
---
```

```markdown
---
id: nddemo-00000001
agenda: demo
type: free
title: 첫 노트
created: '2026-06-16T10:00:00+09:00'
tags: [예시]
---
안녕하세요, 첫 노트입니다.
```

### serve 모드와 `wl-cli`

엔진을 직접 띄워 두면 `wl-cli`는 엔진을 띄우려고 launchd를 부르지 않고 그 소켓에 붙습니다.

```bash
export WITHLOG_VAULT=/path/to/vault

# 터미널 1: 엔진을 띄운다. Ctrl-C로 내린다
java -jar dist/wl-engine.jar serve

# 터미널 2: 떠 있는 엔진에 묻는다
java -jar dist/wl-cli.jar list_tags '{}'
java -jar dist/wl-cli.jar list-docs limit=5
java -jar dist/wl-cli.jar doctor
```

떠 있는 엔진이 없을 때 `wl-cli`는 `WITHLOG_SIDECAR_JAR`가 가리키는 jar나 `install`로 깐 jar를 launchd로 띄웁니다.

```bash
export WITHLOG_SIDECAR_JAR=$PWD/dist/wl-sidecar.jar
java -jar dist/wl-cli.jar list_tags '{}'
```

> **launchd를 건드립니다.**
> - `wl-cli`가 엔진을 띄울 때는 `~/Library/LaunchAgents/co.kcd.withlog.sidecar.<키>.plist`를 쓰고 `launchctl`을 부릅니다.
> - 엔진이 동시 상한(기본 3개)을 넘으면 가장 오래 조용한 엔진을 내립니다. 하루 한 번은 24시간 넘게 조용한 엔진도 내립니다.
> - 레이블과 실행 상태 파일이 withlog 앱과 같은 꼴입니다. 그래서 withlog 앱이 깔린 기기에서는 그 앱의 엔진도 내릴 대상이 될 수 있습니다.
> - 이 저장소의 시험은 모두 임시 홈 폴더와 가짜 `launchctl`로 했습니다. 진짜 `launchctl`로는 아직 돌려 보지 않았습니다.
>
> 등록한 엔진을 지우려면 `wl-cli doctor`로 레이블과 plist 자리를 보고, `launchctl bootout gui/$(id -u)/<레이블>` 뒤에 그 plist를 지웁니다.

`wl-cli`가 스스로 하는 명령도 있습니다.

| 명령 | 하는 일 |
|---|---|
| `doctor` | 키, 레이블, 소켓, plist, 실행 상태, 동시 상한, 하루 청소를 진단한다. `--plist`는 쓸 plist를 보여 주고, `--prune`은 vault 폴더가 사라진 LaunchAgent를 지운다 |
| `sweep` | 24시간 넘게 조용한 엔진을 내리고 죽은 흔적을 지운다. `--dry-run`은 보여 주기만 한다 |
| `install` | `wl-cli.jar`와 `wl-sidecar.jar`를 `~/Library/Application Support/withlog/bin/`에 깐다. `--cli`, `--sidecar`로 원본을 고른다 |
| `--help`, `--list` | 사용법과 이름 목록 |

### 환경 변수

| 이름 | 쓰임 | 기본값 |
|---|---|---|
| `WITHLOG_VAULT` | vault 폴더 | 없음 |
| `WITHLOG_SIDECAR_SOCKET` | 소켓 자리. 103바이트 이하 | `~/Library/Caches/withlog/wl-<해시>.sock` |
| `WITHLOG_SIDECAR_JAR` | plist에 실을 엔진 jar | `install`로 깐 jar |
| `WITHLOG_SIDECAR_JAVA` | plist에 실을 `java` | `wl-cli`를 돌리는 JVM의 `java` |
| `WITHLOG_CLI_JAR` | `install`이 깔 `wl-cli.jar` 원본 | 없음 |
| `WITHLOG_READY_TIMEOUT_SECONDS` | 띄운 뒤 붙기를 기다리는 초 | 30 |
| `WITHLOG_SIDECAR_LIMIT` | 동시에 떠 있을 엔진 수 | 3 |
| `WITHLOG_SWEEP_QUIET_HOURS` | 청소가 내릴 만큼 조용한 시간 | 24 |
| `WITHLOG_IDLE_TIMEOUT_MINUTES` | serve 모드가 스스로 내려가기까지 조용한 분 | 10 |

## 종료 코드

| 코드 | 뜻 |
|---|---|
| 0 | 성공. 답은 stdout에 JSON 하나와 줄바꿈 1개입니다 |
| 1 | 예상 밖 실패. stderr에 스택 트레이스가 나옵니다 |
| 2 | vault를 못 찾았거나 모르는 명령입니다 |
| 3 | `WL-…` 코드가 붙은 실패 (`WL-404` 노트 없음, `WL-400` 인자 오류 등). stderr에 한 줄이 나오고 stdout은 비어 있습니다 |
| 4 | `wl-cli`만 냅니다. 엔진에 못 닿았거나, 띄우지 못했거나, 응답 프레임이 어긋났습니다 |

serve 모드 엔진은 유휴로 내려가면 0, `SIGTERM`을 받으면 143으로 끝납니다.

## 소스 구성

`src/withlog/` 아래에 패키지 11개가 있습니다.

| 패키지 | 맡은 일 |
|---|---|
| `cli` | 엔진 진입점, `wl-cli`와 그 명령(`doctor`, `sweep`, `install`), launchctl 부르기 |
| `config` | 설정 이름과 값 읽기 |
| `engine` | 명령 1개를 돌려 상태, stdout, stderr로 돌려줌. 단발 모드와 serve 모드가 같이 씀 |
| `serve` | 소켓, 잠금, 실행 상태 파일, 유휴 종료 |
| `frame` | 소켓 위의 요청 프레임(`WLQ1`)과 응답 프레임(`WLG1`) |
| `args` | 도구 표면의 JSON 인자와 명령 표면의 `key=value` 인자 읽기 |
| `cmd` | 명령 14개와 이름 목록 |
| `write` | 쓰기 공통 부품. 표식 확인, 원자적 쓰기, id와 폴더 이름, 프론트매터 YAML, 경고 |
| `vault` | 아젠다, 노트, 템플릿, 회고, 휴지통을 읽는 부품 |
| `text` | 다듬기, UTF-16 자르기, URI 등 글 도구 |
| `out` | 정해진 바이트 모양의 JSON 쓰기 |

## 검증 요청

검증을 맡은 분은 [`docs/handoff-round2.md`](docs/handoff-round2.md)를 먼저 보세요. 단발 모드의 빌드부터 묶음 실행까지 순서가 있습니다.

## 라이선스

[MIT](LICENSE)
