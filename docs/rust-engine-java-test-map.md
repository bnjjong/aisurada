# 지운 Java 시험 32개와 Rust 시험의 대조 (이슈 #57)

기록일 2026-11-16. 이 문서는 그날의 상태를 적은 기록이라 고치지 않는다. 새 대조를 하면 아래에 이어 적지 말고 새 기록을 쓴다.

## 결론

Java 엔진을 지운 5d(`b3493a3`)에서 Java 시험 32개(태그의 `test/aisurada/*Test.java`, 점검 줄 약 1,900개)도 같이 지웠다. 시험을 지키는 쪽이 어디인지 가려 보니 다음과 같다.

- **일곱 개는 Rust 시험으로 옮겼다**(여섯은 시나리오 전체나 핵심, 하나는 git 실행기와 git 없음). 그 가운데 필요한 곳은 같은 입력을 Java 기준 엔진에도 돌려 대조했다. 이 과정에서 Rust가 Java와 다른 곳 넷이 나왔다. **둘은 고쳤고**(`verify-shard`, `plan-day`), **둘은 의도한 차이로 받아들였다**(`reindex-paths`의 깨진 머리, `plan-day`의 확장 연도).
- **네 개는 대조할 것이 없다.** 지운 기능(옛 사이드카의 plist, 소켓 serve, 데일리 리포트)이 셋, 설정 출처를 환경 변수만으로 정한 결정이 하나다.
- **나머지 21개는 골든 테이프(Java 응답을 녹화한 것)와 Rust 통합 시험이 같은 명령을 덮는다고 판단했다.** 이 판단은 시험 이름과 테이프 항목 수로 한 것이지 점검 줄 하나하나를 대조한 것이 아니다. 줄 수가 큰 열세 개는 아래 「못 한 대조」에 따로 적었고 새 이슈로 올렸다(#57 댓글).

가운데 열의 「이식」은 Java 시험의 시나리오를 Rust 시험으로 옮겨 기준 엔진과 견주었다는 뜻이고, 「골든」은 옮기지 않고 녹화본·기존 시험으로 덮인다고 판단했다는 뜻이다.

## 대조표

점검 수는 `check(`·`eq(` 줄을 센 근사치다.

| Java 시험 | 점검 | 재던 것 | 지금 Rust에서 | 방식 |
|---|--:|---|---|---|
| `ReindexPathsTest` | 39 | `reindex-paths`의 경로 고르기, 진단, 명령줄 문법 | `ais-core/tests/reindex_paths.rs`, `ais-cli/tests/reindex_paths_cli.rs` | 이식, 기준 엔진 대조 |
| `Chapter28Test` | 39 | `shard-notes`, `verify-shard` | `ais-core/tests/shard_notes.rs`(핵심 흐름. 표의 유니코드 숫자·부호 같은 극단 입력은 안 옮김) | 이식, 기준 엔진 대조 |
| `Chapter30Test` | 24 | `check-notes` | `ais-core/tests/check_notes.rs` | 이식 |
| `Chapter29Test` | 17 | `plan-day` | `ais-core/tests/plan_day_and_export.rs` | 이식, 기준 엔진 대조 |
| `Chapter27Test` | 13 | `export-markdown` | 같은 파일 | 이식 |
| `VaultPointerTest` | 18 | 앱이 연 vault 쪽지의 읽기 규칙 | `ais-cli/tests/vault_pointer.rs` 4개 | 이식 |
| `VaultHistoryTest` | 39 | git 실패, 판 목록, 가짜 git 실행 파일 | `vault/history.rs` 6개, `ais-core/tests/git_cli.rs` 6개, `git_missing.rs`, `lists_note_versions_from_git`, 골든 `history` 215건 | 이식(git 실행기·git 없음) + 골든 |
| `PlistGeneratorTest` | 11 | 옛 사이드카의 launchd plist | 해당 없음. 옛 사이드카 정리는 `old_sidecar.rs` 10개 | 지운 기능 |
| `Chapter25Test` | 0 | (데일리 리포트를 지울 때 비워 둔 시험) | 해당 없음 | 지운 기능 |
| `Chapter25To30IntegrationTest` | 12 | 소켓 serve와 `ais-cli` 프레임 왕복 | serve가 없다. 명령줄은 골든 `cli` | 지운 기능 + 골든 |
| `YamlRoundTripTest` | 30 | YAML 왕복 | `frontmatter/tests.rs` 18개, 골든 `yaml-ground-truth.jsonl` 96건, `snake-cases.tsv` 2,232건 | 골든 |
| `McpServerTest` | 137 | MCP 스키마, 규격, 진짜 서버 왕복 | `mcp_stdio.rs` 7개, `table_contracts.rs` 5개, 골든 `mcp` 174건 | 골든 |
| `ClaudeMcpCommandTest`, `AgentMcpCommandTest` | 53, 41 | `claude-mcp`, `codex-mcp`, `gemini-mcp` 등록 | `register.rs` 8개 | 골든(가짜 CLI) |
| `Chapter26Test` | 51 | 휴지통(`delete-doc`, `restore-doc`, `purge-doc`, `empty-trash`) | `moves_notes_through_the_trash`, 골든 compat `delete-doc` 13·`restore-doc` 10·`purge-doc` 5 | 골든 |
| `CommentCommandsTest` | 91 | 코멘트 쓰기와 앵커 걷기(19장) | `writes_comments_and_strips_their_anchors`, 골든 compat 51건 | 골든 |
| `Chapter24Test` | 97 | 템플릿 쓰기와 부분 실패(24장) | `writes_templates_and_facts_through_the_write_foundation`, 골든 compat 31건 | 골든 |
| `AgendaFactsTest` | 116 | 사실층 읽기·쓰기(22장) | `opens_notes_sub_notes_comments_attachments_and_facts`, 위 시험, 골든 `update-agenda-facts` 12건·`get-agenda-facts` 6건 | 골든 |
| `ProjectReposTest` | 62 | 프로젝트 `repos`와 아젠다 `projectRepo` | `writes_projects_through_the_write_foundation`, `vault/project.rs` 2개 | 골든 |
| `AskCommandsTest`, `ChatCommandsTest` | 25, 38 | 문답과 레일 대화 파일 | `appends_ask_and_chat_turns_next_to_the_note`, `ask_chat_commands.rs` 3개, 골든 compat 83건 | 골든 |
| `Chapter32Test`, `Part4ReviewTest` | 43, 46 | 4부 없는 명령의 검사 순서, 1부가 모르는 파일의 수명, 4부 리뷰 23줄 | 위 시험들과 골든 `cli` | 골든 |
| `SessionCommandsTest`, `AgentSessionTest` | 67, 92 | 세션 전사 읽기, 세션을 연 에이전트를 노트에 적기 | `reads_session_transcripts_of_every_agent`, `env.rs` 4개, `mcp_stdio.rs`의 세션 시험 2개, 골든 `read-session` 39건 | 골든 |
| `ApprovalsTest`, `ResourcesTest` | 188, 128 | 스킬·에이전트 자원과 승인 워크플로 | `queues_ai_resource_changes_until_a_human_approves`, `resource/approvals.rs` 4개, `refs.rs`, `an_ai_change_waits_until_a_human_runs_the_approval`, 골든 compat 자원 줄 | 골든 |
| `TasksTest` | 177 | 태스크 규칙, 전이, 연결, 활동 | `runs_the_task_lifecycle`, `task/aggregate.rs` 9개, 골든 compat `create-task` 44·`transition-task` 20 | 골든 |
| `Chapter31Test`, `Chapter31IntegrationTest`, `VaultGitTest` | 78, 15, 73 | 자동 커밋, 작성자 전환, 이력과 스냅숏 | `write/autocommit.rs` 11개, `commit_vault_*`, 골든 `commit-vault` 11건 | 골든 |
| `Chapter33Test` | 37 | 설정 출처(명령줄, JSON, 시스템 속성)와 값 | 환경 변수만 읽기로 확정([#56](https://github.com/bnjjong/aisurada/issues/56) 댓글). 환경 쪽지는 `env.rs` | 결정(대조 안 함) |

## 대조해서 나온 차이 넷

골든 테이프에 없던 경우를 Rust로 옮겨 기준 엔진(`build-oracle.sh`로 지은 Java 엔진)에 같은 입력을 돌려 확인했다.

| 명령 | Java | Rust(고치기 전) | 처리 |
|---|---|---|---|
| `verify-shard` | 워크스페이스 표식(`workspace.json`)이 없어도 읽기 검증을 돈다(`ok:false`, 종료 코드 0). Java 시험이 이를 단언한다 | 첫 줄에서 표식을 요구해 「워크스페이스가 아니다」(AIS-400)로 거절 | **고침.** 읽기 명령이라 표식을 요구하지 않는다 |
| `plan-day`(AI 도구 호출) | 계획 노트에 세션 id를 안 적는다(29장 완료 기록의 「세션 id 없음이 고정값」) | 호출의 세션을 계획 노트에 `sessionId:`로 찍음 | **고침.** 호출자는 넘기되 세션만 뺀다 |
| `reindex-paths`(`mins: !!int abc` 같은 깨진 머리) | 처리되지 않은 `NumberFormatException`으로 명령 전체가 죽는다(`Application run failed`, 종료 코드 1) | 그 노트만 「프론트매터가 없다」로 건너뛰고 나머지를 훑는다 | **의도한 차이.** JVM의 우연한 예외라 옮기지 않았다 |
| `plan-day`의 날짜 | `-0001-12-31`, `+10000-01-01` 같은 `java.time` 확장 연도도 받는다 | 네 자리 연도만 받는다 | **의도한 차이.** 하루 계획에 쓸 곳이 없다 |

시험 파일에 각 차이의 이유를 주석으로 적었다. `plan-day`의 세션 고침과 `verify-shard`의 표식 고침은 시험이 고치기 전에는 실패하고 고친 뒤에 통과하는 것을 확인했다.

### 덧붙임: 보인 것, 안 고친 것

git 실행 파일이 없는 기계에서 `commit-vault`는 `skipped`에 이유 `not-main`을 답한다(git을 못 돌려 브랜치를 알 수 없어서 생긴 값이다). 죽거나 노트 쓰기를 막지는 않지만 이유 문구가 실제와 다르다. `git_missing.rs`가 현재 동작(건너뜀)을 고정하고 이유는 단언하지 않았다.

## 못 한 대조

옮기지 않은 21개는 모두 시험 이름과 테이프 항목 수로 덮인다고 본 것이지, 점검 줄 하나하나를 Rust 쪽과 대조하지는 않았다. 그 가운데 줄 수가 큰 열세 시험(아홉 묶음)을 큰 순서로 적는다. 나머지 여덟(YAML 왕복, MCP 등록 둘, 휴지통, 문답·대화 둘, 4부 검사 순서·리뷰 둘)은 줄 수가 작거나 기준 엔진과 직접 견준 입력이 많다(YAML은 SnakeYAML 기준값 96건과 방출기 2,232건). 골든 테이프는 어긋남 0으로 녹화된 뒤 이 저장소에서 계속 돌고 있어서 **같은 입력에서 Java와 같다**는 것은 보장하지만, 입력이 테이프에 없는 경우는 보장하지 않는다. 위에서 `verify-shard`가 그 틈에서 나왔다.

1. `ApprovalsTest`(188), `ResourcesTest`(128): 자원과 승인
2. `TasksTest`(177): 태스크
3. `McpServerTest`(137): MCP 서버
4. `AgendaFactsTest`(116): 사실층
5. `Chapter24Test`(97): 템플릿 쓰기
6. `AgentSessionTest`(92), `SessionCommandsTest`(67): 세션
7. `CommentCommandsTest`(91): 코멘트
8. `Chapter31Test`·`VaultGitTest`(151): 자동 커밋과 이력
9. `ProjectReposTest`(62)

## 다시 대조하는 법

```bash
# 기준 엔진을 짓는다(JDK 21, 처음에는 snakeyaml을 받는다). 꺼내 쓰는 Java 시험 원문은 태그에서 본다
AISURADA_ORACLE_DIR=<임시 폴더> bash engine-rs/tools/compat/build-oracle.sh
git show java-engine-final:test/aisurada/Chapter29Test.java      # 지운 시험의 원문
# 같은 vault에 같은 명령을 Java와 Rust로 돌려 견준다
java -Duser.home=<빈 폴더> -jar <임시 폴더>/ais-engine.jar <명령> [인자] --aisurada.vault=<vault>
engine-rs/target/debug/ais-cli <명령> [인자]          # AISURADA_VAULT와 AISURADA_LOCK_DIR을 임시 폴더로
```

기준 엔진의 `ais-cli.jar`는 소켓 경로가 102바이트를 넘으면 실패하므로(긴 임시 폴더 경로) 직접 견줄 때는 `ais-engine.jar`를 쓴다. 호출의 세션 표식(MCP 요청의 `_meta`)은 명령줄로 줄 수 없어서 Java와는 못 견주고 Rust 통합 시험으로만 확인한다.
