# Rust 엔진 이전 3단계 W1 — 설정 파일 쓰기(`init-workspace` 빼고)와 쓰기 맞대기 도구

## 요약

**결론:** 쓰기 명령 10개(아젠다·프로젝트의 만들기·고치기·지우기, 템플릿 저장·삭제, 사실층)를 Rust 엔진으로 옮겼고, 쓰기를 Java 엔진과 맞대는 도구(같은 명령열을 두 사본에 돌려 **응답과 폴더 전체 바이트**를 비교)와 쓰기 스위치를 만들었다. Java와 208건에서 어긋남이 없다. 쓰기 스위치(`AISURADA_ENGINE_WRITE=rust`)는 **기본 꺼짐**이라 앱의 동작은 달라지지 않는다. W1의 나머지인 `init-workspace`와 실제 창 확인은 아직 못 했다.

이 기록은 [W0 기록](rust-engine-step3-w0.md)에 이어지고 정리안 A~H(전부 추천대로)를 따른다.

## 1. 만든 것

| 부품 | 자리 | 하는 일 |
| --- | --- | --- |
| 쓰기 한 번의 길 | `write/pipeline.rs`의 `WriteScope` | 잠금 → 표식 검사 → 쓰기용 색인 맞추기 → 쓰기 앞 커밋 판단. 범위를 벗어나면(성공이든 실패든) 쓴 쪽을 자동 커밋 대기 기록에 남기고 잠금을 푼다 |
| 템플릿 쓰기 | `commands/template_write.rs`, `write/template_file.rs` | `save_template`(새로 만들기, 고치기, html·json 템플릿을 고치면 md로 바꾸고 옛 파일을 지움), `save_ritual_template`(회고 템플릿 고치기, 같은 rev가 둘이면 거절), `delete_doc_template`(빌트인과 회고 템플릿은 거절, 없는 id는 빈 목록으로 답함). 본문에 들여쓴 블록이 있으면 응답에 경고가 붙는다 |
| 사실층 쓰기 | `commands/facts_write.rs` | `update_agenda_facts`: 앞뒤 공백을 떼어 쓰고(2,000자 상한) 빈 값이면 파일을 지운다 |
| 경고 | `write/warnings.rs` | 들여쓴 제목·인용·콜아웃·하위 페이지 카드·표를 찾아 알린다(노트 쓰기에서도 쓴다) |
| 프로젝트 쓰기 | `commands/project_write.rs`, `write/project_file.rs` | 만들기·고치기·지우기. 저장소 목록(`repos`, JSON 글)은 모양, 이름, 경로, GitHub 주소, `githubId`를 엄격히 검사하고 한 줄 JSON으로 머리에 쓴다. 아젠다가 쓰는 프로젝트는 지우지 않는다 |
| 아젠다 쓰기 | `commands/agenda_write.rs` | 만들기(이름에서 slug, 같은 이름은 `-2`부터 `-100`, 가장 적게 쓰인 색), 고치기(준 칸만, 빈 값은 지움), 지우기(노트가 있으면 거절, 비면 휴지통 폴더로 이동) |
| slug, 파일 내용 | `write/slug.rs`, `write/agenda_file.rs` | 제목에서 폴더·파일 이름을 만들고 아젠다 파일(`id`, `slug`, `title`, `hue`, 있는 것만 `repo`, `project`, `projectRepo`, 요약)을 쓴다 |
| 유휴 타이머 | `write/autocommit.rs`의 `ensure_ticker` | 이 프로세스가 vault에 처음 쓸 때 5초마다 `tick()`을 부르는 스레드를 하나 띄운다 |
| 쓰기 스위치 | 호스트 `engine.rs`, 엔진의 `WRITE_COMMANDS` | 호스트는 쓰기 명령을 `AISURADA_ENGINE_WRITE=rust`일 때만 화면에 알리고, 꺼져 있으면 `engine_call`도 거절한다. 읽기 스위치(`AISURADA_ENGINE=java`)가 우선이고, 스냅숏 쓰기 설정(`AISURADA_VAULT_SNAPSHOT_ENABLED=true`)이 켜져 있으면 쓰기를 Java에 맡긴다 |
| 화면 연결 | `data/commands.ts`, `workspace/engine.ts`, `data/vault.ts` | `write.createAgenda/updateAgenda/deleteAgenda`, `write.createProject/updateProject/deleteProject`, `write.saveTemplate/saveRitual/deleteTemplate`, `write.facts`와 `read.commit`이 호출 객체가 됐고, `perform`과 `engineRefusal`이 호출 객체도 받는다. 스위치가 꺼져 있으면 `toArgv`가 예전과 같은 인자 목록으로 풀어 Java로 보낸다 |
| 두 사본 맞대기 | `examples/compat_check.rs --write-fixture=<합성 vault>` | 합성 vault를 사본 둘로 복사하고 쓰기 표식을 만든 뒤, 같은 명령열을 Java(단발)와 Rust로 돌리면서 명령마다 응답과 폴더 전체를 견준다. 시계와 id 난수는 두 엔진에 같은 값으로 고정한다 |

`commit_vault`(앱을 열고 닫을 때 커밋을 시도)도 쓰기 스위치를 따른다. Rust 엔진이 쓴 것만 Rust의 대기 기록에 남기 때문에, 쓰기를 Java에 두고 커밋만 Rust로 부르면 커밋 메시지의 「쓴 쪽」 표시가 사라진다.

## 2. 결과

| 확인 | 내용 | 결과 |
| --- | --- | --- |
| Java와 쓰기 맞대기 | 아젠다 27단계(성공 15, 오류 12), 프로젝트 36단계(저장소 JSON의 잘못된 입력 14가지, 아젠다가 쓰는 프로젝트의 삭제 거절과 이동 뒤 삭제 포함), 템플릿 31단계(새로 만들기, html 템플릿 고치기, 빌트인·회고 거절, 들여쓴 블록 경고 포함), 사실층 10단계(상한 경계, 지움). 단계마다 응답과 폴더 전체의 바이트를 견줌 | 208건, 어긋남 0 |
| Rust 시험 | `cargo test -p ais-core` | 단위 68, 통합 12, 상호 운용 1 통과 |
| 정적 검사 | `cargo clippy --all-targets -D warnings`, `cargo fmt --check`(엔진) | 통과 |
| 호스트 시험 | `cargo test engine::` | 4개 통과(쓰기 명령이 스위치 뒤에 숨는 것 포함) |
| 화면 시험 | `engine-route` 18개(4개 새로), `data` 51, `workspace` 34, `navigation` 30, `interaction` 42, `autosave` 45, `views` 27, `editor` 123, `screen` 6 | 모두 통과. 타입 검사 통과 |

맞대기가 처음에 잡은 어긋남 13건은 모두 두 사본의 폴더 경로가 달라서 생긴 응답의 딥링크(`ws=`) 차이였고, 도구가 경로를 맞춰 비교하게 고쳤다. 폴더 비교는 처음부터 어긋남이 없었다.

## 3. Java와 다르게 한 것과 남긴 것

- **slug 만들기**는 Java가 유니코드 일반 범주(글자 `L*`, 숫자 `N*`)로 글자를 가르는데 Rust 표준 라이브러리에는 범주 조회가 없어서 `is_alphabetic`/`is_numeric`을 쓴다. 한글·영문·숫자·이모지는 같고, 다른 문자 체계의 결합 부호(`Mn`, `Mc`)는 Java가 구분자로 보고 Rust는 글자로 본다. 그런 이름에서 slug가 달라질 수 있다.
- `update_agenda`와 `create_agenda`의 인자가 JSON `null`이면 **없는 것**으로 본다(`containsKey`와 `null`을 가르지 못한다). 화면은 빈 글(`""`)로 지움을 뜻하므로 영향이 없다.
- 응답 필드(`precondition`이 없는 아젠다 쓰기 응답은 `id`, `slug`, `title`, `hue`, 있는 것만 `repo`/`project`/`projectRepo`, `summary`, `deepLink`)는 Java와 같게 유지했다(정리안 D-가).
- 자동 커밋 대기 기록은 아직 프로세스 메모리다(이슈 #58).

- **저장소 목록의 JSON 문법 오류**(괄호가 안 맞는 글 따위)의 문구는 Java의 JSON 읽개와 다르다(앞부분 `repos 는 JSON 배열이어야 한다: `은 같다). 모르는 키가 여러 개면 Java는 적힌 차례의 첫 키를, Rust는 이름 차례의 첫 키를 알린다. 그 밖의 검사 문구 14가지는 맞대기에서 같았다.

## 4. 확인하지 못한 것과 남은 일

- **`WRITE_COMMANDS` 누락 결함을 고쳤다**(`cf4be1c`). 프로젝트 쓰기 3개가 목록에 빠져 쓰기 스위치 없이도 Rust로 갈 수 있었다. COMMANDS의 모든 명령이 알려진 읽기 16개와 `WRITE_COMMANDS` 가운데 정확히 한 곳에 있어야 한다는 시험을 더해 같은 실수를 막았다.

- **실제 앱 창에서 확인하지 못했다.** 정리안 H(제가 격리 앱을 조작해 확인)는 W1이 끝난 직후에 한다. 이슈 #54, #61.
- **W1의 나머지는 `init_workspace`다.** 빌트인 템플릿 시드(`templates` JSON 배열)의 검사 문구가 Java의 JSON 읽개와 재직렬화(입력 순서 보존)에 기대서 아직 못 옮겼다. 쓰기 파이프라인은 이 명령만 표식 검사를 건너뛴다(구현은 이미 `WriteScope::begin`에 있다). 
- 쓰기 스위치를 켠 앱의 자동 커밋(타이머가 실제로 한 커밋을 남기는 것)은 호스트에서 확인하지 못했다. 엔진 단위 시험으로만 확인했다.
- W0에서 못 맞댄 원자적 쓰기, 경로 가두기, 시험용 시계·id는 이번 맞대기에서 아젠다 쓰기를 통해 함께 검증됐다(이슈 #63의 첫 항목).
