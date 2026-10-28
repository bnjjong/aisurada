/Users/df/github/withlog 프로젝트에서 **8-1 — 재조정 응답과 명령 클라이언트**를 구현해 줘. 계획 제안에 멈추지 말고 코드 작성·회귀 테스트·타입 검사·빌드까지 완료해 줘.

## 먼저 읽을 자료

1. 적용되는 AGENTS.md 및 현재 package.json, tsconfig.json
2. docs/spec-chapter-08-index-and-freshness.md 전체 — 계약의 정본
3. docs/plan-chapter-08-index-and-freshness.md 전체 — 특히 3절의 8-1 범위와 완료 기준
4. docs/spec-chapter-05-arguments.md의 명령 표면, docs/spec-chapter-06-output-and-errors.md
5. src/engine 전체, 기존 테스트·fixture·README

최근 확인 기준은 테스트 10개 파일·146개 통과와 npm run build 성공이야. 현재 상태를 다시 확인하고 기존 변경을 보존해 줘. 명세와 계획이 충돌하면 명세를 우선하고 판단 근거를 기록해 줘.

## 1. 응답 타입과 순수 validator

권장 파일은 src/engine/indexing/types.ts와 validators.ts야. 기존 ResponseValidator<T> 타입 가드 계약을 유지하고, decodeResponse와 조합해 줘.

필수 응답 필드와 순서는 다음과 같아.

mode → elapsedMs → agendas → scanned → indexed → unchanged → removed → skipped → conflicts → unsharded

- mode: 'full' 또는 'paths'. 전체 응답 타입과 full만 허용하는 타입/validator를 구분해 줘.
- elapsedMs, agendas, scanned, indexed, unchanged, removed: 모두 0 이상의 정수.
- scanned === indexed + unchanged 관계를 검사해 줘.
- skipped: 모든 항목의 kind/path/reason을 검사해 줘. kind는 agenda, note, docTemplate, ritualTemplate, review, trash의 6가지이고 path/reason은 문자열이야.
- conflicts: 모든 항목의 kind/path를 검사해 줘. kind는 note 또는 review이고 reason 키는 없어야 해.
- unsharded: 모든 항목이 문자열인 배열.
- 필수 키 누락, 잘못된 타입, 배열 뒤쪽에 섞인 잘못된 항목도 거절해 줘. null과 배열을 객체 응답으로 허용하지 마.
- 빈 배열은 정상이고, 추가로 알려지지 않은 키는 원본에 보존해 줘.
- path/reason의 공백, Unicode, 줄바꿈, 배열 순서를 변경하지 마. reason 문구를 열거해 제한하거나 unsharded를 다시 정렬하지 마.
- docTemplate/ritualTemplate의 대소문자를 그대로 사용해 줘.
- removed는 삭제된 노트 수가 아니라 색인 행 수야. 다른 수치와 임의의 관계를 만들거나 그 숫자만큼 노트를 지우는 처리를 하지 마.
- 특정 elapsedMs나 110 같은 예시 건수를 실제 응답에 강제하지 마. 숫자 필드의 구조·관계와 구현별 실제 값을 구분해 줘.
- 잘못된 응답은 기존 schema_mismatch로 전달하고 WL-500이나 다른 엔진 오류로 변환하지 마.

## 2. Transport 주입 명령 클라이언트

권장 파일은 src/engine/commands/reindex.ts야. 아래 형태의 공개 API를 제공해 줘. 합리적인 이름 조정은 가능해.

```typescript
createReindexClient(transport).reindex()
// execute({ command: 'reindex', args: [] })
// Promise<DecodedResponse<FullReindexResponse>>

createReindexClient(transport).reindexPaths(paths)
// execute({ command: 'reindex-paths', args: [...paths] })
// Promise<DecodedResponse<ReindexResponse>>
```

- 두 명령은 명령 표면이야. reindex에 '{}'를 붙이거나 paths 배열을 JSON 문자열 한 칸으로 보내지 마.
- reindex()의 성공 응답은 mode:'full'만 허용하고, reindexPaths()는 paths/full을 모두 허용해 줘.
- reindexPaths([])는 그대로 인자 없는 reindex-paths 호출이야. 몰래 full reindex로 바꾸지 마.
- 입력 배열을 변경하지 말고 경로 순서·공백·중복·빈 문자열·특수문자를 그대로 전달해 줘. trim, .. 정리, 필터링, 노트 경로 접기, 중복 제거는 엔진의 역할이야.
- '='가 든 경로나 '--'로 시작하는 경로의 명령 문법 동작도 저수준 클라이언트에서 몰래 보정하지 마. 이를 감시 이벤트에서 안전하게 처리하는 정책은 8-2 범위야.
- 기존 decodeResponse와 validator를 조합해 성공·도메인 실패·계약 오류·전송 실패를 반환하고 원본 응답·진단·복구 정보를 보존해 줘.
- 재조정 전에 workspace 표식을 직접 확인하거나 색인/쓴 쪽 기록을 별도로 실행하지 마. 두 명령에는 쓰기 깃발이 없어.
- mode:'full'인 paths 응답을 허용하되, 그것이 템플릿·회고·휴지통 건강 검사까지 끝났다는 의미라고 설명하지 마. paths의 full fallback과 명시적 reindex는 검사 범위가 달라.

## 3. fixture·회귀 테스트·공개 예시

- src/engine/__fixtures__/chapter08/에 8.4의 full 응답 예시를 관리해 줘.
- 명세 예시는 elapsedMs가 예시 값이며, 나머지도 코드 읽기로 구성한 값이고 공식 묶음이 이 응답 바이트를 검증한 것은 아님을 출처에 명시해 줘.
- paths, full fallback, 오류 사례는 합성 fixture로 구분해 줘.
- 키 순서·2칸 들여쓰기·마지막 LF를 포함한 고정 기대 바이트로 검증해 줘. 객체 동등성 검사만으로 대신하지 마.
- 새 JSON 파서나 출력 생성기를 만들지 마. 소비 validator와 fixture 검증의 역할을 구분해 줘.

다음을 자동 검증해 줘.

1. reindex의 명령 이름·인자 없음, paths의 0/1/여러 경로와 인자 원문·순서 보존.
2. full·paths 정상 응답, paths 요청의 full 응답 허용, reindex 요청의 paths 응답 거절.
3. skipped 6종, conflicts 2종, 빈 배열, unsharded 문자열, 추가 키 보존.
4. 음수·소수·문자열 숫자, scanned 관계 불일치, 필수 키 누락, 잘못된 kind와 배열 뒤쪽의 잘못된 항목.
5. conflicts의 reason 키 거절, skipped의 reason 문자열 원문·줄바꿈 보존.
6. skipped/conflicts가 있어도 성공 결과와 stderr 진단이 함께 전달됨.
7. WL-400/500, 미분류 엔진 실패, 로컬 전송 실패, 스키마 오류의 원본 정보 보존.
8. fixture 바이트, 입력 객체/배열/원본 stdout·stderr 무변경.
9. 공개 API의 정상 응답과 손상 안내 예시가 실제 타입 검사 및 mock 실행을 통과함.

200개 초과 fallback을 합성 응답으로 검증하는 것과 실제 소켓 전송을 구분해 줘. 소켓 argv 상한은 64칸이므로 경로 201개를 실제로 보낼 수 있다는 사용 예시를 만들지 마. argv 예산·이벤트 묶음의 full 전환은 8-2 및 네이티브 전송부에서 구현할 일이야.

## 4. 범위 제한

이번에는 8-1만 구현해 줘. 다음은 하지 마.

- 8-2 코디네이터, 파일 감시, debounce/TTL 정책, 자동 재조회
- UI·라우터·Zustand 스토어 변경
- SQLite·meta.db 직접 접근, 색인 엔진·VaultKey 공식 구현
- 실제 Tauri/CLI/소켓 연결, 파일 쓰기·색인/DB·git
- 7-1 쓰기 정책 보완이나 7-2 상태 머신을 같은 변경에 섞기
- 관련 없는 리팩터링이나 명세에 없는 제약 추가

기존 7-1의 미분류 실패/미정의 종료·WL 조합 문제는 별도 보완 대상으로 남아 있어. 8-1 개발을 위해 기존 오류를 조용히 수정하거나 완료됐다고 기록하지 마.

## 5. 완료 및 문서 반영

- index.ts의 공개 export와 README를 갱신해 줘. README 예시는 MockTransport에 응답을 등록해 실행 가능하게 하고 타입 검사 대상 테스트로 검증해 줘.
- npm test, npm run build를 실행하고 이번 변경으로 생긴 실패를 해결해 줘. 기존 146개 동작을 유지하되 테스트 개수 자체를 목표로 삼지 마.
- 실제 엔진이나 공식 수용 묶음을 실행하지 않았다면 통과했다고 주장하지 마.
- 완료 후 사용자가 개발 현황 문서로 지정한 /Users/df/Downloads/aisurada 엔진 해부도.html의 app-progress와 app-index-plan 절도 업데이트해 줘. 파일명이 Unicode 정규화 형태에 따라 다를 수 있으므로 실제 파일을 확인해 줘.
- HTML 원본을 백업하고 기존 Java 엔진 1~20장·도표·앵커·스크립트는 유지해 줘. 앱의 8-1 구현과 자체 검증 결과만 갱신하고 8-2·8-3은 계획 단계로 남겨 줘. 원격 Claude 문서 갱신은 이번 범위가 아니야.
- 최종 보고에는 구현 결과, 공개 API와 사용 예시, 변경 파일, 검증 결과, HTML 반영 여부, 명세와의 차이 및 남은 제한을 적어 줘.

합리적인 구현 선택은 자율적으로 진행하고, 사용자의 기존 변경을 보존하면서 구현과 검증을 끝까지 완료해 줘.
