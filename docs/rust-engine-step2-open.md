# Rust 엔진 이전 2단계 둘째 묶음 — 노트 하나를 여는 길의 읽기 명령 6개

## 요약

**결론:** 노트 하나를 열 때 앱이 부르는 읽기 명령 6개(`get_doc`, `list_sub_notes`, `get_sub_note`, `list_comments`, `list_attachments`, `get_agenda_facts`)를 Rust 엔진으로 옮기고 앱이 이 6개를 Rust로 읽게 연결했다. 앞 묶음과 합쳐 읽기 명령 12개가 Rust다. Java 엔진과 응답을 맞대 어긋남이 없고(합성 vault와 진짜 vault 2개에서 모든 노트와 후속 노트로 내려가며 518건), 화면·호스트 시험도 통과한다. 앞 묶음과 마찬가지로 **실제 앱 창에서는 확인하지 못했다.** 되돌리는 스위치(`AISURADA_ENGINE=java`)는 그대로다.

이 기록은 [첫 묶음 기록](rust-engine-step2-read.md)에 이어지고, 정리안 A~F를 같은 방식으로 적용했다.

## 1. 새로 정한 것

| 항목 | 내용 |
| --- | --- |
| 후속 노트 줄 | `rootId`(속한 메인 노트)와 `parentId`(바로 위 노트)를 더하고, 부모 사슬을 담던 `ref`와 `parent`, 그리고 `date`·`time`을 뺐다. `folder`가 붙는다. |
| 후속 노트 찾기 | Rust 엔진은 부모 사슬(`루트/자식/…`)과 후속 노트 id 하나를 둘 다 받는다. 사슬은 Java와 같은 오류까지 같게 맞췄고, id 하나만 주는 방식이 새 호출용이다. |
| 목록 감싸기 | `list_sub_notes`와 `list_attachments`도 `{items, total, returned, truncated}`다. 코멘트와 사실층은 목록이 아니라서 그대로 객체다. |
| 오류 | AI 호출자가 기록 층 노트를 열려고 하면 `{code: "AIS-409", kind: "forbidden"}`이다(Java의 `AIS-409` 가운데 「허락되지 않음」). |
| 화면 | 후속 노트는 화면이 아직 부모 사슬로 부르고 응답의 `ref`와 `parent`를 쓴다. 어댑터가 부른 사슬로 이 값을 다시 채워서 화면 코드는 그대로다. |

## 2. 결과

| 확인 | 내용 | 결과 |
| --- | --- | --- |
| Java와 응답 맞대기 | 합성 vault(노트 21건), 진짜 vault, 손 확인용 vault에서 모든 노트의 `get_doc`·코멘트·첨부, 모든 후속 노트(사슬과 id 하나 두 방식)의 목록·단건·코멘트·첨부, 모든 아젠다의 사실층, 오류 16종 | 518건(앞 묶음 항목 포함), 어긋남 0 |
| Rust 단위·통합 시험 | `cargo test -p ais-core` | 35개 통과 |
| 호스트 시험 | `engine::`(실제 Tauri 호출 경로로 `get_doc`·`list_sub_notes`·오류 종류) | 3개 통과 |
| 화면 시험 | `engine-route` 12개(3개 새로) | 통과 |
| 기존 화면 시험 | `data` 51, `workspace` 34, `interaction` 42, `autosave` 45, `navigation` 30, `views` 27, `tasks-ui` 18, `resources-ui` 15, `runner-ui` 13, `contracts` 4 | 모두 통과 |
| 타입 검사 | `pnpm typecheck` | 통과 |

합성 vault에는 코멘트 파일의 경계(객체·배열 모양, 규칙에 안 맞는 항목, 같은 id, 대소문자, JSON이 아님, 200KB 초과), 첨부 이름 규칙의 경계(숨김 파일, `.tmp`, 확장자 없음, 폴더, 특수문자, 그림과 일반 파일 참조), 사실층(정상, 상한 초과, 빈 파일), 깊이 3 이상의 후속 노트와 바로 아래가 아닌 사슬을 더했다.

## 3. Java 엔진과 다르게 한 것과 안 옮긴 것

- **후속 노트 id만으로 찾기**는 새 방식이다. 같은 id의 후속 노트가 여러 메인 노트에 있으면 만든 시각이 빠른 메인 노트 쪽을 고른다(실제로는 id가 겹치지 않는다).
- **AI 표면용 덧붙임은 아직 안 옮겼다.** AI가 부르는 `get_doc`은 아직 Java 엔진(`ais-cli`, MCP)이 처리하므로 사실층(`agendaFacts`)과 `deepLink`는 Rust 응답에 없다. AI 표면을 옮길 때 호출자 `ai`에 붙인다.
- 코멘트 파일은 `serde_json`으로 읽는다. Java가 직접 만든 JSON 읽개와 극단적인 입력에서 다를 수 있지만 맞댄 사례에서는 어긋남이 없었다.

## 4. 확인하지 못한 것과 남은 일

- **실제 앱 창에서 확인하지 못했다.** 확인 방법은 첫 묶음 기록과 같다(`docs/beta-hand-check.md`의 격리 실행법으로 노트를 열어 후속 노트 탭, 코멘트, 사실층이 예전과 같은지 본다).
- **화면의 부모 사슬 계산(`chain()`)과 폴더 추측(`folder()`)은 아직 그대로다.** Java 경로가 사슬을 필요로 해서 Java 경로를 걷어낼 때 함께 지운다.
- **셋째 묶음:** `search_docs`, `list_tags`, `get_template`, `history`.
- 개발자 가이드 §8의 시험 수치는 전체 시험을 돌리지 않아 고치지 않았다.
