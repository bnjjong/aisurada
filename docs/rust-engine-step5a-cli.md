# Rust 엔진 이전 5단계 5a — Rust `ais-cli`

> 이 문서는 2026-11-15 시점의 기록이다. 고치지 않는다.

## 결론

Java `ais-cli`가 하던 명령줄 일(명령 표면, 도구 표면, 옛 응답 모양, 종료 코드, `reindex-paths`)을 Rust 크레이트 `engine-rs/crates/ais-cli`로 옮겼다.
같은 합성 vault에 같은 명령줄을 Java 엔진(단발 모드)과 Rust `ais-cli`로 돌려 견준 결과는 **2,064건(`history` 포함 vault는 2,194건), 어긋남 0건**이다.
견준 호출 가운데 Java가 성공한 것은 553건(578건)이라 둘이 똑같이 실패해서 맞아 보이는 일은 가려냈다.
아직 못 옮긴 것은 직접 하는 명령(`doctor`, `sweep`, `install`, `mcp`, `claude-mcp`, `codex-mcp`, `gemini-mcp`)이고, 이것이 5b다.
앱과 MCP 등록은 아직 Java `ais-cli`를 쓴다. 이 단계는 앱 동작을 바꾸지 않는다(손 확인과 스위치 기본값은 그대로다).

```
명령줄 ──► parse(명령 표면) / tool(도구 표면) ──► map ──► ais-core Engine::call
                                                              │
출력 ◄── output(Java와 같은 JSON 모양) ◄── legacy(옛 응답 모양) ◄┘
```

앞쪽은 Java가 하던 인자 해석이고, 가운데는 Rust 엔진이 앱을 위해 새 모양으로 바꾼 읽기 응답 16개를 에이전트가 기대하는 옛 모양으로 되돌리는 층이다(5단계 결정 1-가).

## 만든 것

| 자리 | 하는 일 |
|---|---|
| `crates/ais-cli/src/registry.rs` | 도구 38개·명령 82개의 이름 표, `--help`, `--list`, 「알 수 없는 이름」 글. Java `AisCliMain`의 목록과 순서·문구가 같다 |
| `crates/ais-cli/src/spec_table.rs` | 명령 명세 82개(이름, 도구 이름, 쓰기·4부 여부, 사용법, 쓸 수 있는 옵션, 자리 인자 상한). **생성 파일**이다. `engine-rs/tools/compat/gen-spec-table.sh`가 Java 엔진의 `CommandRegistry`와 `ArgsParser`에서 뽑는다 |
| `crates/ais-cli/src/parse.rs` | 명령 표면 해석: `--`로 시작하는 칸은 버리고, 처음 `=`가 둘째 글자 이후면 이름 인자, 아니면 자리 인자. 모르는 옵션·자리 인자 초과 오류 글이 Java와 같다 |
| `crates/ais-cli/src/tool.rs` | 도구 표면 해석: JSON 한 덩이, 모르는 키 거절, 스키마 차례의 필수·형·길이·개수 검사, `parent` 쪼개기, `limit` 규칙. 호출자는 AI(`caller: "ai"`) |
| `crates/ais-cli/src/map.rs` | 해석 결과를 엔진 호출(이름과 JSON 인자)로 바꿈. 자리 인자에 이름을 붙이고, 형이 정해진 읽기 6개(`list-docs`, `search-docs`, `list-tags`, `list-templates`, `list-sub-notes`, `history`)는 글을 수·불리언·목록으로 바꿈 |
| `crates/ais-cli/src/legacy.rs` | 새 응답을 옛 모양으로 되돌림: 목록 응답은 배열로, 검색은 `{query, total, returned, docs}`, 하위 노트·첨부는 `{parent, …}` 봉투로. 새 응답에서 빠진 `agendaTitle`·`date`·`time`·`deepLink`·`ref`·`parent`는 요청 인자와 vault를 다시 읽어 채움. 못 읽은 템플릿·휴지통 항목은 훑기를 다시 불러 `incomplete` 봉투와 표준 오류 진단을 되살림. `get_doc` 도구에는 아젠다 사실층(`agendaFacts`)을 실음 |
| `crates/ais-cli/src/env.rs` | `AISURADA_VAULT`와 앱이 적어 둔 `current-vault.json`으로 vault 정하기, 경로 정리, 에이전트 세션 표식(`AISURADA_SESSION_*`, `CLAUDE_CODE_SESSION_ID`, `CODEX_*`) 읽기 |
| `crates/ais-cli/src/output.rs` | Java `JsonWriter`와 같은 모양(2칸 들여쓰기, LF, 빈 묶음 한 줄, 끝 LF 하나) |
| `ais-core` `reindex_paths` | 받은 vault 상대 경로가 가리키는 노트만 훑는 명령(`scan_note_paths`, `NoteIndex::reconciled_paths`). 200칸을 넘으면 노트 전체를 훑고 `mode`를 `full`로 적되 템플릿·휴지통은 열지 않는다(Java와 같다). 앱은 이 명령을 부르지 않는다 |
| `crates/ais-cli/examples/cli_compat.rs` | Java 단발 엔진과 명령줄 단위로 맞대는 도구 |

종료 코드는 Java와 같다: 0 성공, 2 이름이나 vault를 못 정함, 3 AIS 오류, 5 확인 값(`f1`) 어긋남(AIS-412), 1 안쪽 실패.

## 확인한 것

```bash
./build.sh                                               # dist/ais-engine.jar
engine-rs/tools/compat/make-fixture.sh <빈 폴더>           # 합성 vault(진짜 vault 내용 없음)
engine-rs/tools/compat/add-search-notes.sh <그 폴더>
(cd engine-rs && cargo run -q -p ais-cli --example cli_compat -- ../dist/ais-engine.jar <합성 vault>)
# git 이력이 있는 합성 vault로 `history`와 `commit-vault`까지
engine-rs/tools/compat/add-git-history.sh <다른 합성 vault>
(cd engine-rs && cargo run -q -p ais-cli --example cli_compat -- ../dist/ais-engine.jar <다른 합성 vault> --history)
(cd engine-rs && cargo test -p ais-cli)                  # 단위 시험 29개
```

| 맞댄 것 | 사례 |
|---|---|
| 도움말, `--list`, 알 수 없는 이름 | 종료 코드·글 확인(Java 클라이언트만 내는 글이라 Rust 쪽 값만 본다) |
| 명령 표면 해석 오류 | 모르는 옵션, 자리 인자 초과, `limit`·`bodies`·`favorite`·`tag`·날짜·유형 오류, 검색어, 존재하지 않는 id |
| 읽기 | 목록·검색·태그·템플릿 변형, 노트마다 열기(`get-doc`, 코멘트, 첨부, 후속 노트, `get-sub-note`), 검색어(제목·태그에서 뽑음), 아젠다 사실, 템플릿 |
| 쓰기(응답과 폴더 전체 바이트) | 아젠다·프로젝트·템플릿·노트·후속 노트·코멘트·첨부·휴지통·`plan-day`·`check-notes`·`facts-brief`·`export-markdown`·`shard-notes`·`reindex` |
| 태스크·자원·승인·문답·대화·세션 | 만들기·고치기·전이·연결·댓글·삭제·복원, 자원 만들기·참조·버전·승인 정책, 문답·대화 턴 추가, 세션 읽기 |
| 도구 표면 | 읽기·검색·필터·`limit` 오류, 노트마다 열기(기록 층 노트는 막힘), 노트 만들기부터 휴지통까지 쓰기, 자원 변경의 승인 대기 |
| `reindex-paths` | 폴더·노트·후속 노트·없는 자리·바깥으로 나가는 경로·충돌 사본 등 15가지와 201칸(엔진끼리) |
| `history`, `commit-vault` | git 이력이 있는 합성 vault 130건 |
| 세션 표식 | Rust만 본다(Java 단발 모드는 표식을 받을 길이 없다). 명령 표면은 세션을 남기지 않고 도구 표면만 `sessionId`·`sessionAgent`를 남긴다 |

## 일부러 다르게 둔 것

1. **키 차례.** Java는 넣은 차례로, Rust는 이름 차례로 낸다. JSON을 읽는 쪽에는 차례가 뜻이 없다. 승인 대기 파일(`.aisurada/pending/*.json`)의 키 차례도 같은 이유로 다르다(값은 같다).
2. **휴지통 경고 로그.** Java가 휴지통 목록을 못 읽은 항목이 있을 때 쓰기 명령의 표준 오류에 시각이 붙은 로그 한 줄을 찍는다. Rust는 찍지 않는다. (목록을 읽는 명령의 진단은 되살렸다.)
3. **오류 우선순위.** 인자가 둘 이상 틀린 요청에서 어느 오류를 먼저 내는지가 다를 수 있다. `limit`·`bodies`·`favorite`는 Java가 필터 검사 뒤에 보지만 여기서는 엔진 호출 전에 본다. 하나만 틀린 요청은 같다.
4. **칸 수·크기 한도.** Java `ais-cli`는 요청이 64칸을 넘거나 한 칸이 1 MiB를 넘으면 예외로 죽는다(종료 코드 1). Rust는 글 한 줄과 종료 코드 1을 낸다. 그래서 `reindex-paths`의 200칸 초과(전체 훑기로 물러서기)는 명령줄로는 닿지 않고 엔진 호출로만 닿는다.
5. **vault 폴더 검사.** 정해진 vault가 폴더가 아니면 Rust는 `AIS-400 vault 뿌리가 폴더가 아니다`와 종료 코드 2를 낸다. Java는 사이드카를 못 띄운 실패(종료 코드 4)로 갈린다.
6. **직접 하는 명령.** `doctor`, `sweep`, `install`, `mcp`, `claude-mcp`, `codex-mcp`, `gemini-mcp`는 `AIS-409`와 5b에서 옮긴다는 글을 낸다. `--list`의 이 명령 설명은 Java 문구 그대로라서 `install`에 「jar」가 남아 있다. 5c에서 고친다.
7. **서로게이트.** Rust 글에는 짝 없는 서로게이트가 있을 수 없어서 경로 조각 검사의 그 부분은 `NUL`만 본다(`ais-core`의 `confine`이 이미 그렇다).

## 남은 일

- 5b: MCP 서버(`mcp`)와 `claude-mcp`·`codex-mcp`·`gemini-mcp`, `doctor`, 옛 launchd 사이드카 정리(결정 4).
- 사용자의 손 확인(이슈 #54, #61)은 5b와 겹쳐 진행하고 5c 전에 결과가 필요하다(결정 5).
- 이 단계에서 `ais-cli` 바이너리를 번들에 넣거나 앱이 쓰게 하지 않았다. 번들은 5c에서 바꾼다.
