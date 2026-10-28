# 14-1 구현 검토 — 노트 수정 응답 계약과 저수준 클라이언트

검토일: 2026-10-28

## 보완 완료 — 2026-10-28

아래 독립 재검토의 네 항목을 보완했다. 두 표면 모두 허용하지 않은 키와 `precondition`을 전송 전에 로컬 `TypeError`로 거절하며, `f1`과의 중복도 차단한다. 빈 id는 원문을 엔진에 보내고 `--` 시작 및 `=` 포함 id의 명령 표면 차단은 유지한다. README 예시에 mock fixture 등록을 추가하고 성공한 요청 판과 저장 중 변경된 초안을 구분했다. in-flight, WL-400/404/409, 미분류 실패, 빈 stdout 및 잘못된 JSON을 클라이언트 경유 테스트로 추가했다.

보완 후 `npm test`는 18개 파일·348개 테스트, `npm run build`는 성공했다. 실제 엔진 쓰기 및 공식 수용 묶음은 실행하지 않았다. 옵션은 일반 객체 또는 null prototype 객체여야 하며, 미지원 키는 값이 undefined여도 거절한다.

## 독립 재검토 당시 발견 사항

당시 `npm test`는 18개 파일·326개 테스트, `npm run build`는 성공했다. 14.6 명세의 JSON 블록 두 개와 fixture의 UTF-8 바이트를 별도로 대조해 일치함을 확인했다. 당시 발견한 문제는 다음과 같다.

1. **허용하지 않은 요청 필드가 조용히 삭제된다.** `assertValidToolOptions` / `assertValidCommandOptions`는 알려진 필드의 타입만 검사하고, 직렬화는 허용 필드만 새 객체/argv에 복사한다. 도구 입력 `{ id, title, date, links, precondition }`에서 `date`, `links`, `precondition`이 오류 없이 사라지고, 명령 입력의 `precondition`이나 미지 키도 사라진다. 가짜 Transport 호출 이력으로 두 표면 모두 재현했다. TypeScript의 구조적 타입 때문에 별도 변수에 담긴 추가 키는 타입 우회 없이도 전달될 수 있다. 특히 `precondition`을 `f1`로 잘못 전달하면 토큰이 빠진 채 요청되며, json 노트의 무토큰 메타 쓰기는 성공할 수 있다. 로컬 오류로 전송 전에 거절하거나 명세에서 허용한 다른 명시적 정책을 적용하되, 알 수 없는 필드나 `precondition`을 조용히 버리지 않아야 한다. `f1`과 `precondition`의 중복도 테스트한다.
2. **README 사용 예시의 mock 및 초안 설명이 부정확하다.** README 10은 mock에 성공 응답을 등록하지 않은 채 호출해 실제로 성공 예시가 실행되지 않는다. usageExamples 테스트는 별도로 응답을 등록한 다른 코드다. 또한 성공을 `초안 보존 해제`나 `초안 확정`으로 설명하지만, 저장 중 추가 입력은 아직 7-2/14-3에서 관리해야 한다. 저장된 요청 판만 확정하고 현재 초안을 무조건 지우지 않는다고 고친다.
3. **검증 공백.** `pre_dispatch, in_flight`를 함께 검증한다고 적은 테스트는 실제로 pre_dispatch만 주입한다. 계획의 in_flight, 미분류 엔진 실패, WL-400/404/409 전달, 빈 stdout·잘못된 JSON을 14장 클라이언트 경유로 확인한다. 기존 공통 테스트 통과와 해당 경로의 직접 검증은 구분한다.
4. **빈 id를 엔진에 보내기 전에 거절한다.** 두 표면 모두 빈 문자열 id를 로컬 `TypeError`로 처리한다. 14-1 계획은 타입이 맞는 값의 유효성과 WL-400 판단을 엔진에 맡긴다. 빈 id를 엔진에 전달하는 정책으로 맞추고 오류 분류를 테스트하거나, 별도 로컬 정책으로 유지하려면 계획과 공개 계약을 명시적으로 조정한다. `--` 등 실제 설정으로 해석될 수 있는 인자의 로컬 차단은 유지한다.

14장 최종 기능의 실제 엔진·실제 파일 쓰기 검증은 이번에도 수행하지 않았다.

## 결과

아래는 개발 세션이 작성한 최초 구현 보고다. 이후 보완 결과는 이 문서 맨 위에 기록했다. 이 검증은 mock 기반이며 실제 파일 쓰기·원자적 이동·동시성 경합이나 공식 수용 묶음을 확인한 결과가 아니다.

## 검증 내역

1. **응답 계약 및 순수 validator ([`isUpdateDocResponse`](../src/engine/commands/updateDoc.ts))**
   - 필수 필드 4개(`id`, `path`, `precondition`, `renamed`)의 타입 무결성 검증.
   - 선택적 `warning` 필드가 존재할 때 문자열 여부 검증 (null, 배열 등 거절).
   - 추가 확장 필드(extra keys) 손실 없는 수용 확인.
   - 필수 필드 누락, 잘못된 타입, null 등에 대한 순수 타입 가드 거절 확인.

2. **14.6 명세 공식 예시 바이트 대조 및 fixture 무결성**
   - 14.6절 예시 1(0002 본문만 수정): 독립 기대 바이트와 `rawBytes` 전체 일치 대조 (키 순서: id -> path -> precondition -> renamed, 2칸 들여쓰기, 끝 LF 1개).
   - 14.6절 예시 2(0001 제목 수정 및 warning 포함): 독립 기대 바이트와 `rawBytes` 전체 일치 대조 (키 순서: id -> path -> precondition -> renamed -> warning, 따옴표 이스케이프, 끝 LF 1개).
   - 단일 HTML 파일 경로, 다중 경고(LF 4줄), 스키마 불일치(precondition 누락/null, warning null, renamed 문자열) 픽스처 검증.

3. **표면별 요청 인자 생성 및 안전한 격리**
   - 도구 표면(`updateDocTool`): 단일 JSON 객체 한 칸(`['{...}']`)으로 호출. `body: ''`(본문 비우기)와 `tags: []`(태그 전체 삭제)는 `undefined`(생략)와 엄격히 구분되어 페이로드에 포함됨. `date`, `time`, `mins`, `links`가 노출되지 않음을 보장.
   - 명령 표면(`updateDocCommand`): 첫 번째 자리 인자로 `id` 전달, 이후 옵션은 `key=value` 형태 전달. `body=` 및 `tags=[]`, `mins=...` 직렬화 검증.
   - 자리 인자 안전성: `id`가 `--`로 시작하거나 `=`를 포함할 경우 설정 플래그/옵션 오해석 방지를 위해 로컬 오류로 즉시 거절.
   - 원문 보존: 문자열(NFC/NFD, 특수문자, 따옴표, 공백)을 클라이언트 단에서 임의로 trim/정규화하지 않고 원문 그대로 전달.

4. **실패 분류 및 결과 정책([`determineWriteOutcome`](../src/engine/write/outcomes.ts)) 연동**
   - 성공 응답: 새 `precondition`을 `nextToken`으로 등록하고 초안 보존 해제(`preserveDraft: false`).
   - `WL-412`(선결 조건 충돌): `conflict` 결과로 분류되어 사용자 초안을 보존(`preserveDraft: true`)하고 최신 본문 재조회 후 재적용(`actionGuidance: 'reread_and_reapply'`)을 안내. 새 토큰만 교체하여 옛 본문을 재전송하는 행위 금지(`canResendWithReplacedToken: false`).
   - `WL-500`(폴더 이동 후 저장 상한 초과 등): 파일 상태 재확인 요구(`requiresFilesystemInspection: true`) 및 초안 보존.
   - 스키마 불일치 및 전송 실패 시 초안 보존 및 적절한 복구 지침 확인.

## 다음 단계 (14-2 및 후속 작업)

- 14-2: 11장 `get_doc` / `get-doc` 전체 읽기 계약 및 기준 판(메타 색인 vs 본문 파일) 관리.
- 14-3: 7-2 초안 및 저장 상태 관리 (deferred mock, 늦은 응답 역전 방지, WL-412 초안 보존).
- 14-4: 격리된 임시 vault 및 실제 전송부 통합.
