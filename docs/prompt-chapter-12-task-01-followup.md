# withlog 12-1 테스트 보완 개발 요청

`/Users/df/github/withlog`에서 12장 태그 목록 구현의 검증을 보완해 주세요.

먼저 `docs/spec-chapter-12-list-tags.md`, `docs/plan-chapter-12-list-tags.md`, `docs/review-chapter-12-task-01.md`, `src/engine/commands/listTags.ts`, 기존 테스트와 fixture를 확인해 주세요. 12-1의 기존 구현은 15개 파일·259개 테스트와 빌드 통과가 확인됐습니다.

## 작업 범위

1. **12.6 fixture 바이트 검증**: `FIXTURE_LIST_TAGS_SPEC.rawBytes`를 명세 12.6 원문에서 독립적으로 작성한 기대 UTF-8 바이트와 전체 비교하는 회귀 테스트를 추가해 주세요. 두 태그와 count, `tag` 다음 `count`인 키 순서, 2칸 들여쓰기와 마지막 LF 1개를 검증해야 합니다. fixture의 `rawText`나 `hasTrailingLf` 메타데이터를 기대값으로 재사용하지 마세요. 이는 fixture 검증이며 실제 엔진 출력 검증이라고 주장하지 마세요.
2. **로컬 전송 실패**: `MockTransport`로 로컬 실패를 주입하여 태그 목록 클라이언트가 `transport_failure`와 원인·복구 지침을 공통 디코더의 계약대로 보존하는지 확인해 주세요. 엔진 종료 코드나 WL 코드를 임의로 만들지 않아야 합니다.
3. **검색어 원문 보존**: 도구·명령 양쪽 표면에 NFC와 NFD 검색어 및 특수문자를 전달하고 요청 인자에서 그대로 유지되는지 회귀 테스트를 추가해 주세요. 두 표면의 JSON 한 칸과 `query=` 인자 형식을 각각 확인합니다. 엔진의 검색 결과를 mock으로 검증했다고 주장하지 마세요.

기존 동작에 결함이 발견되면 위 범위에서 필요한 최소한의 수정과 회귀 테스트를 추가해 주세요. 12-2 화면·실제 전송·파일 감시, 8-2 및 7-1 별도 보완 작업으로 범위를 넓히지 마세요.

## 완료 기준

`npm test`와 `npm run build`를 실행하고 변경 파일, 추가된 테스트, 결과 및 실제 엔진이나 공식 수용 묶음 실행 여부를 보고해 주세요. 실행하지 않은 검증을 통과했다고 주장하지 마세요.
