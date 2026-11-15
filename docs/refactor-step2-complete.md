# 리팩토링 2단계 완료 기록 — 값 객체와 닫힌 집합

## 요약

**결론:** 영역마다 글자 목록과 비교식으로 따로 적혀 있던 **닫힌 집합**(문서 유형, 노트 층, 연결 종류, 아젠다 색)을 공용 커널의 값 객체로 모았다. 계획에 있던 일곱 가지 가운데 **다섯 가지를 했고 둘(id 새 타입, `Created`)은 뒤로 미뤘다.** 동작은 그대로다: 골든 재생이 어긋남 0이고 엔진 시험 198건이 통과한다.

계획: [`refactor-ddd-hexagonal-plan.md`](refactor-ddd-hexagonal-plan.md) §4 2단계. 추적: [이슈 #77](https://github.com/bnjjong/aisurada/issues/77). 앞 단계: [1단계 기록](refactor-step1-complete.md).

## 한 일

| 값 객체 | 지운 사본 | 비고 |
| --- | --- | --- |
| `DocType`(`kernel/doc_type.rs`) | 똑같은 유형 목록 **10벌**(보고서는 9벌), 오류 문구 속 글자 목록 6벌 | 글자가 정확히 같아야 하는 곳(`parse`)과 대소문자를 무시하는 곳(`parse_ignore_ascii_case`)을 갈라 두었다 |
| `Tier`와 가시성 정책(`kernel/tier.rs`) | 「기록 층」 판정 12곳의 세 표기: `== "record"`, `!= "reading"`, `eq_ignore_ascii_case("record")` | `NoteMeta.tier`가 `String`에서 `Tier`가 됐다. `Tier::visible_to(Caller)`가 「AI에게 기록 층은 안 보인다」의 정본이다 |
| `LinkKind`(`kernel/link_kind.rs`) | 노트 연결 종류 목록 3벌(`direct`, `indirect`, `attach`) | |
| `TaskLinkKind`(`kernel/task_link.rs`) | `"blocks"`·`"relates"`·`"duplicates"` 글자 비교 열댓 곳과 반대쪽 이름 매핑 | `Link.kind`가 `String`에서 `TaskLinkKind`가 됐다. `inverse_label()`이 `blocked-by`·`duplicated-by`·`relates`를 돌려준다 |
| `Hue`(`kernel/hue.rs`) | `vault/agenda.rs` 안의 `HUES`를 쓰기·명령 쪽이 가져다 쓰던 길 | 목록을 커널로 옮겼다(`HUES`, `DEFAULT_HUE`, `is_hue`, `position_or_default`) |
| 내용 확인 값(`kernel/token.rs`) | `compute_token`의 자리 | 태스크·자원·쓰기가 노트 읽기 모듈(`vault/note_file.rs`)을 거쳐 가져오던 계산을 커널로 옮겼다. 명령 인자 `f1`, 응답 `precondition`, 파일 `token`은 모두 같은 값이라는 설명을 모듈 머리에 적었다 |

### 「기록 층」 세 표기가 같은 뜻이었던 근거

`vault/note_meta.rs`는 파일 머리의 `tier` 칸을 읽어 **정확히 `reading` 또는 `record` 둘 중 하나로** 정규화한다(비어 있거나 `reading`이면 읽기 층, 그 밖의 값은 모두 기록 층). 그래서 `NoteMeta.tier`에 대해 `!= "reading"`, `== "record"`, `eq_ignore_ascii_case("record")`는 항상 같은 결과다. 이 정규화를 `Tier::from_front`로 옮기고 단위 시험으로 못박았다(`"Reading"`처럼 대소문자가 다른 값도 기록 층이 된다). 이 일을 하는 동안 새로 찾은 동작 차이는 없다.

쓰기에는 기록 층 검사가 없다(AI가 id만 알면 기록 층 노트를 `delete_doc`할 수 있다). 이 단계는 동작을 바꾸지 않으므로 고치지 않았고 [#74](https://github.com/bnjjong/aisurada/issues/74)에 있다.

### 시험

- 커널 단위 시험 9건(각 값 객체의 목록, 읽기 규칙, 반대쪽 이름, 토큰 모양)
- `architecture.rs`의 `closed_set_literals_live_only_in_the_kernel`: `"record"`, `"reading"`, 유형 목록, 연결 종류 글자가 `kernel` 밖에 0건이다. 예외는 태스크 응답의 `blocks` 칸 이름 하나뿐이다. 일부러 `"record"`를 넣어 시험이 잡는 것을 확인했다.

## 숫자

| 항목 | 값 |
| --- | --- |
| 골든 재생 | 어긋남 0 (compat 3,186건, cli 2,194건, mcp 174건) |
| 엔진 시험 | 198건 통과(1단계 188건 + 새 10건), `bash test.sh` 약 1분 40초 |

## 미룬 것과 까닭

- **id 새 타입**(`NoteId`, `SubNoteId`, `AgendaId`, `AgendaSlug`, `TrashId`, `TaskKey`, `ResourceId`): 이 id들은 `NoteMeta.id`, `NoteRecord`, 요청 구조체, 응답, 파일 쓰기 등 수백 곳에서 `String`으로 오간다. 새 타입으로 바꾸면 규칙을 한곳에 모으는 이득은 작은데 바꿀 곳만 많다. id 모양의 정규식은 1단계에서 `kernel::ids`로 모았다. 새 타입은 id를 받는 애그리거트를 만드는 5·6단계에서 같이 하는 편이 낫다고 판단했다.
- **`Created`**(시각과 오프셋을 함께 보존하는 타입): `vault/note_meta.rs`의 `parse_created`가 이미 읽은 값(`formatted`, `date`, `time`, `utc_instant`)을 구조체로 돌려준다. 같은 규칙의 사본이 따로 있다는 증거를 찾지 못해서 이 단계에서는 하지 않았다. 필요하면 6단계(노트 애그리거트)에서 다룬다.
- 둘 다 [#77](https://github.com/bnjjong/aisurada/issues/77)의 체크리스트에 남겼다.

## 한계

- 응답과 파일에 적는 글자는 바뀌지 않았지만 `task::Link.kind`처럼 **공개 구조체의 필드 타입은 바뀌었다.** 호스트와 `ais-cli`는 이 필드를 직접 쓰지 않는다(빌드로 확인했다). 호스트(`native-host/src-tauri`) 빌드는 돌리지 않았다.
- 시험 코드(`#[cfg(test)]`)의 글자도 `closed_set` 시험이 센다. 지금은 위반이 없다.
- 호스트·화면 시험은 돌리지 않았다(결정 4-가).

## 다음

3단계(시계·난수·잠금·버전 관리 포트): [계획](refactor-ddd-hexagonal-plan.md) §4. `Clock`·`IdSource`·`VaultLock`·`VersionControl` 등 `trait`을 만들고 기본 어댑터는 지금처럼 환경 변수와 `static` 큐를 읽게 한다. 난수를 뽑는 순서를 지키는 것이 핵심이고 골든이 잡아 준다.
