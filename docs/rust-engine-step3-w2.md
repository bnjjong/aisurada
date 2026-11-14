# Rust 엔진 이전 3단계 W2 — 노트 머리만 고치는 쓰기

## 요약

**결론:** 노트의 머리(YAML)만 고치는 쓰기 3개(`set_favorite`, `evaluate_doc`, `link_docs`)를 Rust 엔진으로 옮겼다. 노트 파일을 처음 고치는 묶음이라 고치기 전 확인 값(`f1`) 검사, 머리 다시 짓기, 편집 도장 판정이 함께 들어왔다. Java 엔진과 합성 vault에서 **응답과 폴더 전체 바이트가 384건 모두 같다**(앞 묶음 포함). 쓰기 스위치(`AISURADA_ENGINE_WRITE=rust`)는 기본 꺼짐이라 앱의 동작은 그대로다. 실제 창 확인은 아직 못 했다([손 확인 안내](rust-engine-hand-check.md)).

이 기록은 [W1 기록](rust-engine-step3-w1a.md)에 이어진다. 다음은 W3(`create_doc`, `update_doc`, 후속 노트)다.

## 1. 만든 것

| 부품 | 자리 | 하는 일 |
| --- | --- | --- |
| 노트 머리 만들기 | `write/note_front.rs`의 `NoteFront` | 노트 머리 줄을 Java와 같은 차례와 따옴표 규칙으로 짓는다(`id`, `agenda`, `type`, `title`, `created`, 세션, `tier`, 편집 도장, 태그, 즐겨찾기, 템플릿, 평가, 연결) |
| 머리만 고쳐 쓰기 | `write/meta_writer.rs`의 `write_meta` | `f1` 검사 → 머리 읽기 → 머리를 새로 지음 → 편집 도장 판정 → 본문은 그대로 두고 원자적으로 쓰기 → 새 `f1` 계산 |
| 즐겨찾기 | `commands/note_meta_write.rs`의 `set_favorite` | 켜기·끄기·뒤집기, 라벨(빈 값이면 지움). 바뀐 것이 없으면 확인 값 없이도 `changed: false`로 답한다 |
| 평가 | `evaluate_doc` | 0~100 점수(0.5는 올림), 메모(다듬고 1,000자로 자름), `clear=true`로 지움 |
| 연결 | `link_docs` | `direct`·`indirect`·`attach` 연결을 더하고 뺀다. 이미 있거나 없으면 `changed: false`와 안내 글. 대상이 vault에 없으면 경고 |
| 화면 | `data/commands.ts`, `data/vault.ts` | `write.favorite`, `write.evaluate`, `write.link`가 호출 객체가 됐다. 확인 값(`f1`)은 인자의 맨 끝이라 Java 길로 풀면 예전 인자 목록과 같다 |

**편집 도장 판정.** 머리를 새로 지은 글이 파일의 옛 머리를 그대로 옮긴 글과 같으면 도장을 건드리지 않는다. 다르면 쓴 쪽(`app` 또는 `tool`)과 한국 시각(분까지, `+09:00`)으로 `editedBy`·`editedAt`을 새로 찍고, AI 도구가 세션과 함께 고치면 `editedSessionId`와 에이전트 이름(소문자, claude는 적지 않음)도 적는다. 고치면서 노트 머리의 줄 차례와 따옴표가 Java의 짓는 규칙으로 바로잡히는 것도 같다.

## 2. 결과

| 확인 | 내용 | 결과 |
| --- | --- | --- |
| Java와 쓰기 맞대기 | W2 57단계: 즐겨찾기(뒤집기, 라벨, 바뀐 것 없음, 확인 값 없음·남의 값·모양 틀림, html·기록 층·`tier` 값이 이상한·`created` 오프셋·제목이 공백인·JSON 노트), 평가(반올림, 0과 100, 메모 자르기, 지움, 범위 밖·글·NaN 아닌 오류), 연결(더함, 이미 있음, 종류, 없는 대상 경고, 뺌, 오류 9가지) | 모든 단계 어긋남 0. 앞 묶음까지 합쳐 **384건**(192단계, Java가 성공한 것 80단계, 나머지는 일부러 넣은 오류) |
| Rust 시험 | `cargo test -p ais-core` | 단위 73, 통합 14, 상호 운용 1 통과 |
| 정적 검사 | `cargo clippy --all-targets -D warnings`, `cargo fmt --check` | 통과 |
| 호스트 시험 | `cargo test engine::` | 통과 |
| 화면 시험 | `engine-route` 20개(1개 새로), `data` 51, `workspace` 34, `navigation` 30, `interaction` 42, `autosave` 45, `views` 27, `editor` 123, `screen` 6 | 모두 통과. 타입 검사 통과 |

## 3. 맞대기 도구에서 고친 것

- **오류 비교가 느슨했다.** 이전에 Java가 예상 밖 예외로 실패했을 때(스택 추적이 붙는 글)를 위해 「Java 글 안에 Rust 메시지가 들었으면 같다」는 규칙을 넣었는데, 이것이 오류 **코드**가 달라도(Java `AIS-400`, Rust `AIS-500`) 통과시키고 있었다. 「Java 글이 `Application run failed`로 시작할 때만」으로 좁혔다. 좁힌 뒤 앞 묶음의 모든 사례를 다시 돌려 어긋남 0을 확인했다.
- 그 과정에서 Rust의 머리 쓰기 오류(`frontmatter 에 쓸 수 없는 값이다`)가 `AIS-500`이던 것을 Java와 같은 `AIS-400`으로 고쳤다.
- 「Java가 성공한 단계 수」를 요약에 찍는다. 양쪽이 똑같이 실패해서 맞아 보이는 일을 가려내려는 것이다(처음에는 확인 값을 못 구한 단계가 숨어 있었다). `HARNESS_TRACE=1`로 단계마다 결과를 볼 수 있다.

## 4. Java와 같게 둔 한계와 차이

- **머리에 소수 점수(`score: 99.6`)가 든 노트는 Java도 못 고친다**(`frontmatter 에 쓸 수 없는 값이다: Double (99.6)`). 원문 머리를 다시 쓰는 판정에서 막히는 Java의 기존 동작이고, Rust도 같은 오류로 거절한다.
- **`score` 글 읽기:** Java의 `Double.valueOf`는 `5d`, 16진 부동소수 따위도 받는데 Rust는 `str::parse::<f64>`라 받지 않는다. 오류 글의 점수는 Java `Double.toString`처럼 `101.0`으로 찍는다.
- **확장자 폴더 노트**(`notes/…/<이름>.md/note.md`)는 `get_doc`이 확인 값을 주지 않아서 화면과 도구가 고칠 수 없다(Java도 같다). 맞대기에서는 같은 412 오류로 확인했다.
- 옛 스냅숏 쓰기는 안 옮겼다(스냅숏 설정이 켜지면 호스트가 쓰기를 Java로 보낸다).

## 5. 확인하지 못한 것과 남은 일

- **실제 앱 창에서 확인하지 못했다.** [손 확인 안내](rust-engine-hand-check.md)의 점검표에 즐겨찾기·평가·연결을 더해야 한다(이슈 #54, #61).
- 두 프로세스가 같은 노트를 번갈아 고치는 경합 시험은 하지 않았다(이슈 #63).
- 다음은 W3(`create_doc`, `update_doc`, 후속 노트 3개)이고 가장 크다: 본문 줄 규칙, 폴더 배치와 제목이 바뀔 때 폴더 이름 바꿈, 경고(`WarningsDetector`)의 나머지, 자동 저장 경로.
