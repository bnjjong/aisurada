# Rust 엔진 이전 4단계 4b — 스킬·에이전트 자원 15개 명령과 승인 기록

## 요약

**결론:** 스킬·에이전트 정의(자원)와 그 승인(#53) 명령 15개(읽기 7개, 쓰기 8개)를 Rust 엔진으로 옮겼다. 자원 파일은 태스크처럼 **진짜 SnakeYAML이 쓴 바이트**와 같게 쓰고(4a의 방출기를 그대로 썼다), 승인 기록장(`ledger.json`)과 대기 변경 파일(`.aisurada/pending/<id>.json`)도 Java와 같은 바이트다. Java 엔진과 합성 vault에서 **응답, 폴더 전체 바이트, 승인 기록 폴더가 1,800건 모두 같다**(앞 묶음 포함, 816단계, 이 가운데 자원은 187단계). 쓰기 스위치는 기본 꺼짐이고, 읽기 7개는 앱이 바로 Rust로 읽는다. 실제 창 확인은 아직 못 했다([손 확인 안내](rust-engine-hand-check.md)).

전체 계획은 [4단계 계획](rust-engine-step4-plan.md)에 있고, 이 기록은 [4a 태스크 기록](rust-engine-step4a-tasks.md)에 이어진다.

## 1. 하는 일

자원은 **글로벌**과 **프로젝트** 두 범위다. 프로젝트는 글로벌 자원을 ID(`sk…`, `ap…`)로 **참조**해야 쓸 수 있다. 파일은 `.aisurada/resources/` 아래에 있고 노트 훑기가 보지 않는다.

```text
.aisurada/resources/
  global/{skills/<slug>/SKILL.md, agents/<slug>.md}
  projects/<프로젝트 slug>/{skills/…, agents/…, refs.md}
  .trash/<id>/   지운 자원      .history/<id>/   고치기 전의 판(30개까지)
.aisurada/pending/<변경 id>.json   승인을 기다리는 변경(vault 안)
~/Library/Application Support/aisurada/approvals/<vault 해시>/{ledger.json, copies/}   승인한 판의 지문과 사본(vault 밖)
```

| 명령 | 하는 일 |
| --- | --- |
| `list_resources`, `get_resource`, `list_resource_versions` | 읽기. 목록과 조회는 **승인한 판**을 보여 준다. 승인 뒤에 파일이 직접 고쳐졌으면 승인해 둔 사본을 보이고 `approval: dirty`를 붙인다. 승인 기록이 없는 새 파일은 보이지 않는다 |
| `usable_resources` | 프로젝트가 쓸 수 있는 자원(프로젝트 자원 + 참조한 글로벌 자원)과 깨진 참조, 못 쓰는 스킬. 실행에 내려 주는 입력은 이것만 쓴다 |
| `save_resource` | 만들기와 고치기(`id`가 있으면 고친다). 고칠 때는 `f1`이 맞아야 하고, 바뀐 것이 없으면 파일도 기록도 안 바꾼다. 이름을 바꾸면 파일(스킬은 폴더)을 옮긴다. 고치기 전의 판은 이력에 남는다 |
| `delete_resource`, `restore_resource` | 휴지통으로 보내고 되살린다(스킬은 폴더째) |
| `set_resource_ref` | 프로젝트의 글로벌 자원 참조를 켜고 끈다(`remove`, 에이전트를 켤 때 그 스킬도 함께는 `with-skills`) |
| `list_resource_changes`, `get_resource_change` | 승인 대기·처리된 변경과 승인 안 된 직접 수정(`new`, `dirty`)을 보여 준다 |
| `approve_resource_change`, `reject_resource_change` | 대기 변경을 승인해 적용하거나 거절한다. **사람만** 한다(AI가 보내면 `AIS-403`) |
| `resolve_resource_edit` | 승인 안 된 직접 수정을 받아들이거나(`approve`) 승인한 판으로 되돌린다(`revert`. 새 파일이면 휴지통으로). 사람만 한다 |
| `get_resource_policy`, `set_resource_policy` | 승인 정책 네 칸(`project`, `global`, `refs`, `permission`)을 읽고 바꾼다. 바꾸는 것은 사람만 한다 |

**승인 워크플로(#53).** AI가 보낸 변경(`caller: "ai"`이거나 AI 세션 표식이 달린 요청)은 정책에 따라 바로 적용하거나 대기 변경으로 올린다. 사람이 보낸 변경은 늘 바로 적용한다. 권한 올림, 참조 켜기, 도구 칸 변경은 정책과 상관없이 늘 승인을 거친다. 기본 정책은 프로젝트 자원의 내용만 바로 적용하고, 글로벌 자원·권한 내림·참조 끄기는 승인을 거친다. 같은 요청을 되풀이해도 대기 변경은 한 건만 쌓인다(`duplicate`). 승인하면 올린 사람의 기록(세션, 도구, 올린 시각)으로 같은 명령 경로를 한 번 더 지나 적용한다. 그 사이에 기준이 바뀌어 적용하지 못하면 변경은 `stale`로 남고 오류를 돌려준다.

| 부품 | 자리 | 하는 일 |
| --- | --- | --- |
| 모양 | `resource/format.rs` | 파일 읽기·쓰기(머리말은 4a의 방출기)와 인자 검사 |
| 저장소 | `resource/store.rs` | 훑기, 승인한 판으로 훑기(`scan_served`), 만들기·고치기·이름 바꾸기, 휴지통, 이력 |
| 참조 목록 | `resource/refs.rs` | `refs.md` 읽기·쓰기와 승인 상태 |
| 승인 기록장 | `resource/approvals.rs` | 정책, 갈래 분류, 승인 판단, `ledger.json`과 사본, 처음 읽을 때 기준선 |
| 대기 변경 | `resource/changes.rs` | `.aisurada/pending/` 읽기·쓰기·정리(처리된 것 200개까지) |
| 쓸 수 있는 자원 | `resource/resolver.rs` | 참조를 풀어 프로젝트가 쓸 자원을 계산하는 한 곳 |
| 명령 | `commands/resource_commands.rs` | 15개 명령과 응답 모양 |
| 화면 | `data/commands.ts`의 `fromArgv`, `ui/resourcesClient.ts`, `ui/approvalsClient.ts` | 화면이 만드는 옛 인자 목록을 호출 객체로 바꿔 보낸다. 스위치가 꺼져 있으면 예전과 같은 인자 목록으로 Java에 간다 |

## 2. 결과

| 확인 | 내용 | 결과 |
| --- | --- | --- |
| Java와 쓰기 맞대기 | 자원 187단계. 만들기(글로벌·프로젝트, 스킬·에이전트, YAML 특수 글자, 도구 칸의 수·실수·날짜 글·여러 줄 글), 만들기 오류 30여 가지, 읽기(목록 거르기, 지침, 휴지통 목록, 조회, 옛 판 조회, 판 목록), 고치기(칸별, 이름 바꾸기로 파일·폴더 이동, 바뀐 것 없음, 확인 값 오류, 종류·범위 바꿈 거절), 참조(켜기·끄기·`with-skills`, 오류), 쓸 수 있는 자원, 지우기·되살리기(같은 이름 충돌), 정책, 손으로 고친 파일(`dirty`, 승인 기록 없는 `new`, 깨진 파일, 참조 목록)과 그 처리 | 모든 단계 어긋남 0 |
| 대기 변경 맞대기 | Rust 사본에서 AI 요청이 대기에 오르는 것(권한 올림, 글로벌 만들기·지우기, 참조 켜기·끄기, 도구 칸 변경, 같은 요청 되풀이)을 만들고 그 파일을 Java 사본에 옮긴 뒤, 목록·조회와 승인·거절·낡은 변경(`stale`)·이미 처리됨을 **두 엔진이 같은 파일로** 실행해 견줬다 | 모든 단계 어긋남 0 |
| 앞 묶음 포함 | 쓰기 맞대기 전체 | **1,800건**(816단계, Java 성공 395단계) 어긋남 0, 합계 2,327건 |
| Rust 시험 | `cargo test -p ais-core` | 단위 93(승인 4, 참조 1 새로), 통합 22(새로 2: AI 대기·승인·거절·낡은 변경·정책·직접 고친 파일, 옵션 값 읽기) 통과 |
| 정적 검사 | `cargo clippy --all-targets -D warnings`, `cargo fmt --check` | 통과 |
| 화면 시험 | `engine-route` 27개(1개 새로), `resources-ui` 15, `approvals-ui` 15, `tasks-ui` 18. `pnpm typecheck` | 모두 통과 |
| 호스트 시험 | 호스트 Rust 코드는 바꾸지 않았다 | 돌리지 않음 |

## 3. Java와 다르게 둔 것

- **AI 통로는 Java 단발 실행으로 견주지 못했다.** Java 단발 실행은 AI 세션 표식이나 도구 이름을 보낼 길이 없다. 그래서 AI 요청이 대기에 오르는 것(Rust 사본에만 실행)까지만 하네스에서 하고, 그 대기 변경 파일을 Java 사본에 그대로 옮겨 **승인·거절·낡은 변경은 두 엔진이 같은 파일을 받아 실행해 견줬다.** AI가 바로 적용하는 길(프로젝트 자원 `auto`), AI가 승인·거절·정책 변경을 거절당하는 길은 Rust 통합 시험(`queues_ai_resource_changes_until_a_human_approves`)으로만 확인했다.
- **대기 변경의 `args` 칸 순서.** Java는 명령줄에 준 옵션 순서로 적고 Rust는 옵션 이름 순서로 적는다(Rust 호출 객체는 순서를 모른다). 같은 요청인지 보는 비교는 순서와 상관없다. 하네스는 두 엔진에 이름 순서로 준다.
- **옵션 값의 종류.** Rust는 참거짓과 숫자도 명령줄 글처럼 받고(`remove: true` = `remove=true`), 배열·객체(`skills`, `tools`)는 압축 JSON 글로 바꿔 읽는다. `null`은 안 준 것으로 본다. 모르는 옵션은 무시한다(Java 명령줄 해석기는 거절한다).
- **승인 기록 폴더 위치.** Java는 시스템 속성 `aisurada.approvals.dir`, Rust는 환경 변수 `AISURADA_APPROVALS_DIR`로 바탕 폴더를 바꾼다. 없으면 둘 다 홈 아래 `Library/Application Support/aisurada/approvals`다(Java는 `user.home`, Rust는 `HOME`).
- **오류 문구의 극단 사례.** 승인 기록장이 깨졌을 때 JSON 읽기 오류의 괄호 속 설명, 자원 파일을 읽다가 입출력 오류가 났을 때의 설명은 Java가 JVM이 만든 글을 그대로 쓰는 곳이라 같게 맞추지 못했다. 대기 변경 파일의 숫자 칸은 정수와 일반 실수만 읽는다(Java `Double.valueOf`는 `1d`, `NaN` 같은 글자도 받는다). `tools` 안의 명시 태그 값(`!!binary` 따위)은 응답에서 `null`이다.

## 4. 확인하지 못한 것과 남은 일

- **실제 앱 창에서 확인하지 못했다.** 손 확인 점검표에 자원·승인 항목을 더했다(이슈 #54, #61). 자원 읽기 7개는 스위치 없이 바로 Rust로 간다. 승인 화면의 대기 변경은 AI 세션이 있어야 생겨서 점검표에서는 통합 시험으로 대신했다.
- 같은 자원을 두 프로세스가 동시에 고치는 경합 시험은 하지 않았다(이슈 #63). 승인 기록장은 프로세스 안에서만 잠근다(Java와 같다). 쓰기 명령은 vault 쓰기 잠금 안에서 기록장을 고치지만, 읽기 명령이 처음 만드는 기준선은 그 잠금 밖이다.
- 러너와 승인 알림이 부르는 자원 명령은 아직 `ais-cli`(Java)로 간다([4단계 계획](rust-engine-step4-plan.md) §4).
- 다음은 4c(`get_ask`, `append_ask_turn`, `list_chats`, `append_chat_turn`)다.
