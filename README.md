# aisurada

withlog vault를 읽어 JSON으로 답하는 읽기 전용 엔진입니다. Java 21 하나와 SnakeYAML만으로 짓습니다.

이름은 〈신세기 GPX 사이버 포뮬러〉의 아스라다에서 따온 말장난(AI + 아스라다)입니다. 사람 옆에서 달리는 AI라는 자리가 마음에 들었습니다.

## 하는 일

vault 폴더를 부를 때마다 훑어서 답합니다. 색인 파일을 만들지 않습니다. vault와 홈 폴더는 읽기만 하고 고치지 않습니다.

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

도구 이름(`get_doc`)과 명령 이름(`get-doc`)을 모두 받습니다. 쓰기 명령처럼 이 판에 없는 명령과 `serve`는 `WL-409`로 답합니다.

## 빌드

JDK 21 이상이 필요합니다. `JAVA_HOME`이 있으면 그것을, 없으면 `PATH`의 `javac`와 `jar`를 씁니다.

```bash
./build.sh
```

SnakeYAML 2.4는 처음 빌드할 때 Maven Central에서 내려받아 체크섬을 확인합니다. 산출물은 `dist/wl-engine.jar` 하나입니다.

## 실행

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

## 종료 코드

| 코드 | 뜻 |
|---|---|
| 0 | 성공. 답은 stdout에 JSON 하나와 줄바꿈 1개입니다 |
| 1 | 예상 밖 실패. stderr에 스택 트레이스가 나옵니다 |
| 2 | vault를 못 찾았거나 모르는 명령입니다 |
| 3 | `WL-…` 코드가 붙은 실패 (`WL-404` 노트 없음, `WL-400` 인자 오류 등). stderr에 한 줄이 나오고 stdout은 비어 있습니다 |

## 소스 구성

`src/withlog/` 아래에 패키지 6개가 있습니다.

| 패키지 | 맡은 일 |
|---|---|
| `cli` | 진입점, 명령 이름과 도구 이름 풀이, 종료 코드 |
| `args` | 도구 표면의 JSON 인자와 명령 표면의 `key=value` 인자 읽기 |
| `cmd` | 명령 13개 |
| `vault` | 아젠다, 노트, 템플릿, 회고, 휴지통을 읽는 부품 |
| `text` | 다듬기, UTF-16 자르기, URI 등 글 도구 |
| `out` | 정해진 바이트 모양의 JSON 쓰기 |
