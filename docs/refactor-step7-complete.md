# 리팩토링 7단계 완료 기록 — 입구 정리 전체

## 요약

**결론:** 7단계의 여섯 조각 가운데 네 조각(명령 표, 호출 맥락, 공개 면, 오류 번호)을 끝내 `main`에 올렸다. 나머지 둘(7-3 CLI 글 길 정리, 7-5 사용법 글 옮기기)은 **하지 않기로 했다.** 이유는 아래에 있다. 모든 조각에서 동작은 그대로이고, 골든 재생 어긋남은 0이다. 엔진 시험은 302건(다른 세션의 시험 포함), 호스트 시험은 158건이 통과한다.

계획: [`refactor-ddd-hexagonal-plan.md`](refactor-ddd-hexagonal-plan.md) §4 7단계. 추적: [이슈 #77](https://github.com/bnjjong/aisurada/issues/77).

```mermaid
flowchart LR
  subgraph 바깥["바깥 입구"]
    UI["화면 → 호스트 engine_call"]
    CLI["ais-cli 두 표면"]
    MCP["MCP 서버"]
  end
  subgraph 입구["엔진 입구 (이번 단계의 결과)"]
    CTX["CallContext<br/>누가·어느 세션 (7-2)"]
    ERR["오류: 종류와 문구만<br/>번호는 어댑터 표 (7-4)"]
    T["명령 표 한 곳<br/>engine/table.rs (7-1)"]
    API["pub 면 122항목<br/>나머지는 test-support (7-6)"]
  end
  UI -->|CallContext 앱| API
  CLI -->|CallContext 앱·AI| API
  MCP -.->|글 길 그대로 (7-3 안 함)| CLI
  API --> T
  T --> ERR
```

## 조각별 결과

| 조각 | 결과 | 기록 |
| --- | --- | --- |
| 7-1 명령 표 하나 | 완료. 이름 82개·쓰기 50개를 표 한 곳에서 낸다 | [기록](refactor-step7a-complete.md) |
| 7-2 호출 맥락 분리 | 완료(입구까지). `CallContext`, `Engine::call_as`. 처리 함수가 맥락을 직접 받는 일은 미룸 | [기록](refactor-step7b-complete.md) |
| 7-6 공개 면 줄이기 | 완료. 공개 항목 1,040개에서 122개로 | [기록](refactor-step7c-complete.md) |
| 7-4 오류 번호를 어댑터로 | 완료(번호 분리까지). 두 타입으로 가르기는 미룸 | [기록](refactor-step7d-complete.md) |
| 7-3 MCP·호스트의 CLI 글 길, `legacy` 재훑기 | **하지 않음** | 아래 |
| 7-5 `*_USAGE`를 CLI로 | **하지 않음** | 아래 |

## 확인한 것

| 확인 | 결과 |
| --- | --- |
| 엔진 `bash test.sh`(골든 재생 포함) | 302건 통과, 어긋남 0 |
| 호스트 `cargo test` | 158건 통과, 5건은 원래 무시 |
| 화면·호스트 `bash test-native.sh` | 통과(Node 시험 643건 실패 0, 호스트 158건) |
| 실제 창 점검 `test:data:acceptance` | 12/12 통과(쓰기 확인 값을 이어 쓰기, 창 사이 쓰기 반영) |
| 실제 창 점검 `test:editor:acceptance` | 116/116 통과(후속 노트 쓰기 포함) |
| 돌리지 않은 것 | 나머지 실제 창 점검 일곱 종류(`workspace`, `navigation`, `interaction`, `views`, `autosave`, `part4`, `ask-marks`)와 `tauri build --debug` 번들 검증. 화면 코드는 건드리지 않았고 호스트의 엔진 호출·오류 경로만 바뀌어서, 그 경로를 지나는 두 종류만 골랐다 |

## 끝 기준과 실제

| 계획의 끝 기준 | 실제 |
| --- | --- |
| 명령 하나를 더할 때 고칠 곳이 8~12곳에서 2곳(표 한 줄 + 처리 함수)으로 | 엔진 안은 2곳이다. 명령줄·MCP에 내놓는 명령은 그쪽 표(`registry.rs`, `spec_table.rs`, `mcp_tools.rs`)에 한 줄씩이 더 든다. 이 표들은 명령줄 사용법 글과 MCP 입력 스키마라서 엔진 표에 합치지 않았다 |
| `pub` 항목이 약 795개에서 약 30개로 | rustdoc JSON으로 센 값이 1,040개에서 122개다. `json`·`text` 모듈과 `vault`의 읽기 다섯 모듈이 남아 목표 30개에는 못 미친다 |
| CLI·MCP 골든 차이 0 | 0 |
| 호스트 계약(IPC) 골든 차이 0 | IPC 응답을 녹화한 테이프는 따로 없다. 호스트의 시험(`engine_call` 경로, 오류 `{code, kind, message}`의 글자와 칸 차례)이 같은 모양을 본다. 호스트 `cargo test` 158건이 통과한다 |

## 하지 않은 조각과 이유

- **7-3.** 호스트는 이미 엔진을 프로세스 안에서 직접 부른다(`engine_call`, 7-2 이후로는 `call_as`). MCP 서버도 같은 프로세스 안에서 `crate::run`을 부르므로 프로세스나 입출력 경계는 없다. 남은 것은 MCP 인자를 JSON 글로 눌렀다 `parse_tool`이 다시 읽는 안쪽 한 번의 왕복이다. 이 길은 **관찰되는 한도**(인자 칸 64개, 칸마다 1 MiB, 그 오류 글과 종료 코드)를 함께 지키고 있어서, 왕복을 걷어 내려면 한도 검사를 다시 구현하고 MCP 클라이언트가 보는 오류 글이 안 달라지는지 시험해야 한다. 이득이 작고 위험은 오류 글에 있어서 하지 않았다. `legacy.rs`의 vault 재훑기 5곳은 옛 응답 모양을 만드는 CLI 쪽 책임이고(앱은 Rust 응답을 그대로 받는다), 코어의 공개 읽기 모듈 다섯이 이 때문에 열려 있다. 이것을 코어의 질의 함수로 바꾸는 일은 응답 모양을 건드리지 않지만 규모에 비해 이득이 적어 남긴다.
- **7-5.** `*_USAGE` 글은 코어의 **오류 문구 안에** 들어 있다(「agenda 이 필요하다: create-doc agenda=<…> …」). 화면이 받는 문구와 같은 글이다. CLI로 옮기면 화면 쪽 오류 글이 바뀐다. 이 리팩토링의 결정 5(오류 문구는 바이트까지 그대로)와 부딪혀서 하지 않았다. 옮기려면 화면의 오류 문구 정규식([#76](https://github.com/bnjjong/aisurada/issues/76))을 먼저 `kind` 기준으로 바꾸고 실제 창 점검이 필요하다.

## 남은 일(모두 [이슈 #77](https://github.com/bnjjong/aisurada/issues/77)에 있다)

- ~~처리 함수가 `&CallContext`를 직접 받고 요청 타입의 `caller`·`context` 칸을 지우기~~ 끝냈다([후속 기록](refactor-step7e-complete.md))
- 오류를 `DomainError`·`InfraError` 두 타입으로 가르기(처리 함수 반환 타입 수백 곳)
- `json`·`text` 모듈의 공개 항목을 필요한 것만 남기기
- 사용법 글을 CLI로 옮기기(결정 5와 화면 정규식 교체가 먼저)
- 8단계 크레이트 분리(선택)
