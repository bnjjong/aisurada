# 지운 Java 시험 13개를 Rust에 그대로 돌려 본 결과 (이슈 #81)

기록일 2026-11-17. 이 문서는 그날의 상태를 적은 기록이라 고치지 않는다. 새 대조를 하면 새 기록을 쓴다.

## 요약

**결론:** 점검 줄 수가 큰 Java 시험 13개 가운데 11개를 **시험 원문 그대로** 진짜 Rust `ais-cli`에 돌렸다(나머지 둘, `VaultGitTest`와 `Chapter31IntegrationTest`는 Java 내부 클래스를 직접 부르는 시험이라 5절의 방법으로 갈음했다). 시험을 옮겨 쓰지 않고 엔진 호출만 Rust 실행 파일로 바꿔 끼워서, Java가 하던 점검을 줄 단위로 Rust에 적용했다. 6개는 어긋남이 0이었고 나머지는 어긋남이 전부 받아들인 갈래뿐이었다. **새로 찾은 실제 차이는 둘이어서 고쳤다.** 나머지 어긋남은 아래 갈래 아홉으로 모두 설명된다.

이 방법은 [#57 대조](rust-engine-java-test-map.md)의 한계를 메운다. 그때는 시험 이름과 녹화본(골든 테이프) 항목 수로 덮인다고 판단했을 뿐 점검 줄을 대조하지 않았다. 골든 테이프는 「테이프에 있는 입력에서 Java와 같다」만 보장하기 때문이다.

```
   지운 Java 시험 원문 ──(점검 그대로)──▶ EngineExecutor.execute(vault, 인자)
                                                   │
                      원래: Java 엔진 안에서 실행     │  바꿔치기(shim): 진짜 Rust ais-cli 프로세스를 띄워
                                                   ▼  같은 인자·같은 vault·같은 시계와 난수로 실행
                                          응답, 종료 코드, 오류 글을 시험이 그대로 점검
```

바꿔치기는 Java 시험이 엔진을 부르는 단 하나의 길(`EngineExecutor.execute`)만 갈아 끼운다. 시험이 vault 파일을 직접 만들고 고치는 부분은 그대로 돌고, Rust가 같은 폴더에서 일한다. 세션 표식(`\0ais-session-id`)은 환경 변수로, 시험 시계·난수·시간대·월별 폴더 설정은 환경 변수로 넘긴다.

## 1. 결과

| Java 시험 | 점검 | Rust에서 | 어긋남 |
|---|--:|---|---|
| `ApprovalsTest` | 435 | **전부 통과** | 0 |
| `ResourcesTest` | 405 | **전부 통과** | 0 |
| `TasksTest` | 567 | **전부 통과** | 0 |
| `ProjectReposTest` | 191 | **전부 통과** | 0 |
| `AgentSessionTest` | 129 | **전부 통과**(Codex·Gemini 기록 읽기 포함) | 0 |
| `SessionCommandsTest` | 66 | **전부 통과** | 0 |
| `McpServerTest` 3부(진짜 서버 프로세스 왕복) | 252 | **전부 통과**(승인, 읽기 전용, vault 없음, 세션 도장) | 0 |
| `McpServerTest` 2부(규격) | 147 | 통과(가짜 호출기에 기대는 4건만 제외) | 4(시험 장치) |
| `AgendaFactsTest` | 520 | 통과 | 15(갈래 A·B·C·E·F) |
| `CommentCommandsTest` | 359 | 통과 | 5(갈래 A·B·C) |
| `Chapter24Test`(템플릿) | 449 | 통과 | 35(갈래 A·B·C·I) |
| `Chapter31Test`(이력) | 332 | 통과 | 19(갈래 A·G) |
| `VaultGitTest`, `Chapter31IntegrationTest` | | Java 내부 클래스를 직접 부르는 시험이라 그대로는 못 돌렸다. 명령줄에서 보이는 일곱 가지를 따로 견줬다(5절) | 0 |

Java 엔진에 먼저 돌려 시험 자체가 건강한지 확인했다(`ApprovalsTest` 435, `ResourcesTest` 405, `TasksTest` 567, `AgentSessionTest` 129, `ProjectReposTest` 191, `Chapter31Test` 347이 같은 점검 수로 통과). 위 Rust 숫자는 같은 시험의 점검 수다. 어긋남이 나와도 시험이 멈추지 않게 `throw new AssertionError`를 「기록하고 계속」으로 바꾼 사본을 썼다.

## 2. 찾아서 고친 차이 둘

골든 테이프에 없던 입력이다. 둘 다 시험이 고치기 전에는 실패하고 고친 뒤에 통과하는 것을 확인했고, Rust 시험으로 고정했다(`ais-cli/tests/agenda_facts_edges.rs`).

| 명령 | Java | Rust(고치기 전) | 처리 |
|---|---|---|---|
| `update-agenda-facts facts=`(비우기) | 사실층 파일 자리에 **빈 폴더**가 있으면 그 폴더를 지운다(`Files.deleteIfExists`). 안이 있는 폴더는 지우지 못하고 AIS-500 | 빈 폴더에서도 「파일을 지우지 못했다」(AIS-500) | **고침** (`commands/facts_write.rs`) |
| `facts-brief`가 계산한 날짜의 연도 | 연도 −1을 `-0001-12-28`로 적는다(`LocalDate.toString`) | `-001-12-28`(세 자리). 입력 `to=0000-01-03`이면 닿는다 | **고침** (`kernel::time::iso_year`를 재사용) |

## 3. 받아들인 갈래

한 갈래에 시험 점검이 여럿 걸려 있다. 아래는 모두 **그대로 둔다**. 다음 대조에서 같은 모양이 나오면 이 표로 가른다.

| 갈래 | Java | Rust | 근거와 판정 |
|---|---|---|---|
| **A. 응답 JSON의 키 순서** | 명령마다 정한 순서(보통 `id`가 앞) | 이름순(알파벳) | 맞대기 하네스와 골든 재생이 응답을 JSON 값으로 읽어 견줘서 처음부터 순서를 재지 않던 곳이다. 앱·MCP·시험은 모두 JSON 파서라 영향이 없고 눈으로 읽는 순서만 다르다. 맞추려면 명령마다 순서표가 필요하고 얻는 것이 없다. 시험의 「key order」, 「exact bytes」, 「warning last」 점검이 이 갈래다 |
| **B. 짝 없는 서로게이트** | JVM 글은 짝 없는 반쪽 글자를 담을 수 있어 `\uD83D`로 적고 출력에서 `?`로 바꾼다 | Rust 글(UTF-8)에는 그런 것이 있을 수 없다 | 입력이 UTF-8이라 닿지 않는다. 앞서 3단계에서 받아들인 차이다 |
| **C. 처리되지 않은 JVM 예외** | 명시 태그 `!!int abc`나 만 겹 깊이의 대괄호 같은 입력에서 처리되지 않은 예외로 명령 전체가 종료 코드 1로 죽는다(`Application run failed`) | 그 노트·파일만 건너뛰거나 `unreadable`로 알리고 일을 이어간다 | 죽는 쪽이 사고다. Rust가 더 낫다. `reindex-paths`의 같은 경우를 #57에서 받아들였다 |
| **D. 유니코드 숫자와 확장 연도 입력** | `Character.digit`이 아랍 인도 숫자(`۲`)·전각 숫자(`２`)를 받고, 연도 `-0001`·`+10000`을 받는다 | ASCII 숫자와 네 자리 양수 연도만 받는다 | 판 번호·`min-new-file`·날짜 자리에 이런 입력을 줄 사용자가 없다. `plan-day`의 확장 연도와 같은 판단이다. 이 입력들은 시험 사본에서 뺐다(`prepare.py`의 SKIPS). 보충 평면 글자(`𐐨`)의 대소문자 접기도 같은 갈래다 |
| **E. 깨진 UTF-8 사실층 파일** | 없는 파일로 본다 | 대체 글자(`�`)로 읽는다 | 이슈 #56의 4e 댓글에서 이미 받아들였다. 글을 버리지 않는 쪽이다 |
| **F. 「모르는 명령」 문구의 층** | 엔진 층은 `모르는 명령이다: 이름`, 사용자가 쓰는 `ais-cli` 층은 `알 수 없는 이름: 이름` | 층이 하나(`알 수 없는 이름`) | Java 시험이 엔진 층을 직접 불러서 생긴 어긋남이다. Java `ais-cli.jar`에 같은 입력을 돌려 `알 수 없는 이름: …`을 내는 것을 확인했다. Rust와 같다 |
| **G. 지운 기능과 Java 프로세스 안의 상태** | 옛 스냅숏 쓰기(`refs/aisurada/snapshots`), serve 모드의 메모리 대기 기록 | 스냅숏 쓰기는 5단계에서 지웠고(#56), 대기 기록은 `.git/aisurada-pending.json` 파일로 프로세스가 나눠 쓴다 | 같은 일을 하는 Rust 단위 시험이 있다(`write/autocommit.rs`의 `the_pending_record_is_shared_through_a_file_in_git_dir` 따위). 시험 사본에서 Java 내부 호출(`VaultGit.serving(v)`)은 Java 자신을 시험하는 것이라 못 견준다 |
| **H. 이슈 #80의 일부러 한 변경** | `list_comments`가 기록 층 노트에도 열린다 | 도구 표면(AI)이 기록 층 노트의 코멘트를 읽으면 AIS-409 | 점검 하나를 시험 사본에서 뺐다 |
| **I. `list-templates`의 검사 순서** | 갈래(`category`)를 먼저 검사하고, 못 읽은 템플릿 진단 줄을 오류보다 앞에 낸다 | `bodies`를 먼저 검사하고 진단 줄이 없다 | 둘 다 틀리게 준 호출에서 어느 AIS-400이 먼저 나오느냐와 표준 오류의 덧줄뿐이다. 종료 코드와 표준 출력은 같다 |

## 4. 하지 못한 것

- **Java 내부 클래스를 직접 부르는 점검**은 Rust에 겨눌 수 없다: `ArgsParser`로 MCP 스키마 키를 엔진 허용 키와 견주는 부분(1부), 가짜 호출기가 받은 인자(2부 일부), `SessionAgents.fromEnvironment` 같은 환경 변수 읽기 규칙. 대응하는 Rust 쪽 시험이 맡는다: `ais-cli/tests/table_contracts.rs`(스키마 키와 필수 키), `ais-cli/src/env.rs`의 단위 시험(세션 환경 변수), `mcp_stdio.rs`(`_meta` 세션).
- **`Chapter31IntegrationTest`**는 소켓 serve와 `ais-cli` 프레임 왕복이라 지운 기능이다.
- 시험 사본에서 일부 입력을 뺐다(갈래 D·H). 뺀 목록은 `prepare.py`의 SKIPS에 있다.

## 5. `VaultGitTest`: 명령줄에서 보이는 일곱 가지를 직접 견줌

`VaultGitTest`는 Java `VaultGit` 클래스에 가짜 시계와 가짜 git 실행기를 끼우는 단위 시험이라 그대로 못 돌린다. 같은 동작 가운데 명령줄에서 보이는 것을 같은 git 저장소에 기준 Java 엔진과 Rust `commit-vault`로 돌려 응답, 표준 오류, 커밋 메시지 바이트, 작성자, 파일 상태를 견줬다(`commit_vault_twin.py`). **일곱 시나리오가 모두 같다.**

1. 외부 편집 커밋: 훅 끔, 서명 설정을 덮어씀, 작성자 `aisurada-auto`, 커밋 메시지 바이트, `.tmp` 파일은 제외
2. `index.lock` 나이: 새것과 599초는 건너뜀, 601초는 지우고 커밋(표준 오류 경고 글 포함)
3. 진행 중 표지 다섯(`rebase-merge`, `rebase-apply`, `MERGE_HEAD`, `CHERRY_PICK_HEAD`, `REVERT_HEAD`)
4. 다른 브랜치와 detached HEAD
5. 끔(`off`)과 저장소 아님(`.git`이 파일일 때 포함)
6. 노트 id 중복 제거(옮긴 노트), BOM·JSON 노트 제외, 201개일 때 꼬리 200개 상한
7. 머리 id 모양 열두 가지(따옴표, BOM, 들여쓰기, 공백, 닫는 줄 없음, 중복, CRLF, 200자, `_` 시작, `.`)

가짜 실행기로 재던 재시도 간격(30초에서 480초), 대기 값 합치기, 큰 출력 비우기는 Rust 단위 시험이 맡는다(`write/autocommit.rs`, `ais-core/tests/git_cli.rs`).

## 6. 다시 돌리는 법

```bash
# 기준 엔진과 Rust 실행 파일이 있어야 한다(JDK 21, git, python3)
(cd engine-rs && cargo build -p ais-cli)
AISURADA_ORACLE_DIR=<임시 폴더> bash engine-rs/tools/compat/java-on-rust/run.sh              # 열두 묶음 전부
AISURADA_ORACLE_DIR=<임시 폴더> bash engine-rs/tools/compat/java-on-rust/run.sh AgendaFactsTest   # 하나만
AISURADA_ORACLE_DIR=<임시 폴더> AIS_CLI=$PWD/engine-rs/target/debug/ais-cli python3 engine-rs/tools/compat/java-on-rust/commit_vault_twin.py
```

`run.sh`는 시험 원문을 태그에서 꺼내(`prepare.py`) 바꿔치기 클래스(`shim/`)와 함께 컴파일해서 돌리고, 시험마다 어긋남 개수와 이름을 요약한다. 위 3절의 갈래에 없는 어긋남이 나오면 새 차이다. 이 도구는 기준 jar가 있는 동안만 쓸모가 있어서, 맞대기 도구를 지우는 때(`docs/developer-guide.md` §4)에 함께 지운다.
