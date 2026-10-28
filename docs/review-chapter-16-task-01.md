# 16-1 구현 결과 — 직계 하위 노트 목록 계약과 클라이언트

검증일: 2026-10-28

[16장 계획](./plan-chapter-16-list-sub-notes.md)의 16-1을 구현했다. 실제 전송부·파일 읽기·탐색 상태·UI와 하위 노트 쓰기는 이번 범위에 포함하지 않는다.

## 구현

- [`listSubNotes.ts`](../src/engine/commands/listSubNotes.ts)에 `SubNoteListLink`, `SubNoteListItem`, `ListSubNotesResponse`, 표면별 옵션·클라이언트와 `isSubNoteListItem` / `isListSubNotesResponse`를 추가했다. 필수 문자열·정수 mins·tags/links의 모든 원소와 선택 문자열을 검사하고, 공통 `decodeResponse`를 통해 원본·추가 키·진단·실패 분류를 보존한다.
- 도구는 `list_sub_notes`와 JSON 객체 한 칸, 명령은 `list-sub-notes`와 부모 자리 인자·선택 bodies 옵션을 보낸다. 명령 bodies는 boolean을 소문자로 직렬화하고 문자열은 원문 전달한다. undefined 생략과 빈 문자열을 구분하며 참조를 다듬거나 재정렬·reindex·재시도·표면 전환을 하지 않는다.
- 런타임에서는 일반 객체/null prototype 객체를 받고, 필수 parent의 타입·허용 키·명령 bodies 타입을 검사한다. 미지원 키는 undefined여도 로컬 TypeError로 거절한다. 명령 부모의 `--` 시작과 첫 글자 뒤 첫 `=`는 설정/옵션 충돌로 전송 전에 거절한다. 첫 글자가 `=`인 값과 빈 부모 등 나머지 문자열의 WL 판단은 엔진에 맡긴다.
- [`chapter16/fixtures.ts`](../src/engine/__fixtures__/chapter16/fixtures.ts)에 명세 예시 세 개와 본문·도장·json·읽기 실패·옛 md 경로·형제 순서·추가 키 합성 fixture를 추가했다. `evidence`는 명세가 보고한 수용 측정, 코드 읽기로 계산한 예시, 합성을 구분한다. 이 값은 로컬에서 실제 수용 묶음을 실행했다는 뜻이 아니다.
- 공개 export와 [`README`](../src/engine/README.md)에 등록된 mock을 사용하는 메인→자식 목록 및 본문 조회 예시를 추가했다. [`사용 예시 테스트`](../src/engine/__tests__/usageExamples.test.ts)는 공개 배럴을 통해 같은 호출 흐름과 argv를 검증한다.

## 검증 결과

- `npm test`: **19개 파일·535개 테스트 통과**. 신규 [16장 테스트](../src/engine/commands/__tests__/listSubNotes.test.ts) 186건과 사용 예시 1건이 포함된다.
- `npm run build`: TypeScript와 Vite 빌드 통과.
- 16.6 명세 JSON 블록 세 개를 직접 추출해 fixture 문자열과 UTF-8 전체 바이트를 독립 비교했다. 세 개 모두 일치했다. 테스트에도 별도 기대 문자열을 두어 토큰·path·ref·키 순서·들여쓰기·끝 LF를 비교한다.
- 필수 필드 누락/null/타입 오류, 선택 필드의 타입, 배열 뒤쪽 원소, 정확한 두 표면 argv, 부모 원문과 정규화된 응답의 차이, 미지원 키·로컬 입력 오류, 원소 순서·중복 태그·NFC/NFD·중첩 추가 키를 검증했다.
- 두 표면을 통해 CLI/소켓 WL-400/404/409, 코드 없는 종료 2, 종료 1·미분류 실패, 소켓 코드 0, 계약 위반, 전송 단계별 실패와 신호 종료를 주입해 결과를 확인했다. 자동 후속 호출이 발생하지 않는 것도 검증했다.

## 보장 범위와 다음 단계

응답 validator는 구조만 검증한다. 도장 쌍·세션 생략·본문 생성 조건·직계 관계·실제 정렬이나 참조 검증 순서를 클라이언트에서 재현하지 않는다. 추가 키는 보존하며, 메타·본문·토큰이 같은 판이거나 본문이 완전히 읽혔다고 보장하지 않는다.

실제 엔진 실행·디스크 I/O·층 게이트·색인 신선도 및 공식 111건 수용 묶음은 실행하지 않았다. 해당 동작의 mock 주입과 실제 엔진 측정은 별개다.

다음 단위는 **16-2 부모별 읽기 상태와 중첩 탐색 모델**이다. 실제 UI는 16-3, 격리 엔진 검증은 16-4에서 진행한다.
