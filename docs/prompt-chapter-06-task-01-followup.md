/Users/df/github/withlog 프로젝트에서 6장 기반 작업 1의 보완 개발을 완료해 줘. 아래 검토 결과를 실제 코드와 대조한 다음 수정과 회귀 테스트까지 진행해 줘. 작업 2로 범위를 넓히지 마.

## 먼저 확인할 자료

- 적용되는 AGENTS.md
- docs/spec-chapter-06-output-and-errors.md
- docs/spec-chapter-04-socket-protocol.md의 응답 프레임과 오류 처리
- docs/spec-chapter-05-arguments.md의 명령 표면과 도구 표면
- docs/plan-chapter-06-output-and-errors.md
- src/engine 전체, 관련 테스트, package.json, tsconfig.json

기존 검토에서는 npm test의 27개 테스트와 npm run build가 통과했지만, 아래 동작이 누락되거나 모호했어. 현재 상태가 달라졌으면 이미 해결된 부분은 보존하고 남은 문제를 해결해 줘.

## 1. 전송 실패와 신호 종료에도 복구 지침 전달

현재 decodeResponse는 local_failure와 signal을 transport_failure로 반환하지만 복구 지침을 제공하지 않아. timeout이나 응답 수신 중단은 쓰기가 이미 반영된 뒤에 발생할 수 있으므로, 상태 재조회가 필요하다는 정보를 호출자가 받을 수 있어야 해.

- DecodedTransportFailure에도 일관된 복구 지침을 제공해 줘.
- 요청이 전달된 뒤 timeout·연결 끊김·프레임 손상이 발생했거나, 전달/완료 여부를 모르는 경우에는 쓰기가 반영되었을 수 있음을 표현해 줘.
- 신호 종료에도 같은 불확실성을 고려해 줘.
- 실행/전송 전 실패가 명확한 경우와 실행 여부가 불명확한 경우를 구분할 최소 정보가 필요하면 타입에 추가해 줘. 불필요하게 복잡한 상태 모델은 만들지 마.
- 로컬 실패에 엔진 종료 코드나 WL 코드를 임의 부여하지 마.
- 실제 재조회·재시도·자동 생성·쓰기 재전송은 구현하지 말고 행동 지침만 데이터로 제공해 줘.
- 원본 실패 정보, 신호명, 제공된 stdout/stderr는 보존해 줘.
- API 변경에 맞춰 MockTransport와 README도 갱신해 줘.

회귀 테스트: timeout, 응답 수신 중 프레임 손상/연결 중단, 신호 종료에서 필요한 재조회 지침이 전달되는지 검증해 줘. 실행 전 실패가 명확한 시나리오도 함께 검증해 줘.

## 2. ResponseValidator 반환 계약 통일

현재 ResponseValidator는 타입 가드, { valid, data } 결과 객체, 검증 후 데이터 반환 함수를 동시에 허용하고, decodeResponse가 반환값의 모양으로 구분해. 이 때문에 정상 데이터에 valid 필드가 있으면 결과 객체로 오인할 수 있어.

재현 예:

```typescript
// stdout: {"valid":false,"id":"report-1"}\n
decodeResponse(raw, data => data)
// 현재는 정상 데이터의 valid를 검증 결과로 해석해 schema_mismatch가 됨.
```

- 반환 계약을 하나로 통일해 줘. 이번 계층의 목적에 맞춰 타입 가드 `(data: unknown) => data is T`를 우선 고려해 줘.
- 검증 결과의 데이터 모양으로 validator 종류를 추측하지 마.
- 공통 디코더에서 응답 데이터 변환·키 채우기·다듬기를 하지 마. UI용 매핑은 별도 계층의 역할이야.
- validator가 false를 반환하거나 예외를 던지면 계약 오류로 처리하고 원본 응답을 보존해 줘.
- 기존 사용처·테스트·README를 새 계약에 맞게 수정해 줘. API 변경 이유도 기록해 줘.

회귀 테스트: valid 필드가 true/false인 정상 객체를 타입 가드로 검사하면 원문 그대로 통과해야 해. nullable 필드, false, 빈 배열, 누락 키도 유지되어야 해. 검증 실패와 예외는 schema_mismatch여야 해.

## 3. 소켓 응답의 프레임 정보 필수화

현재 RawCommandResponse는 mode와 frameMeta가 독립적으로 선택 가능해서 mode:'socket'인데 frameMeta가 없는 값을 허용해. classifyFailure는 이 경우 stderr로 WL 번호를 추출하고 대상 없음 등으로 분류해 버려.

재현 예:

```typescript
classifyFailure({
  mode: 'socket',
  exitCode: 3,
  stdout: new Uint8Array(),
  stderr: new TextEncoder().encode('WL-404 Note not found\n'),
})
// 현재 not_found, wlCodeSource:'stderr'가 됨.
```

- RawCommandResponse를 mode에 따른 판별 유니온 등으로 바꿔 소켓 응답에 frameMeta를 필수로 만들어 줘.
- CLI와 소켓의 메타데이터 혼용도 타입 수준에서 막아 줘.
- 외부 입력이 타입을 우회할 수 있으므로, 디코더 경계에서도 소켓 frameMeta 누락을 검사해 계약 오류로 처리해 줘. JSON 해석과 실패 분류 전에 확인해 줘.
- 소켓 실패 분류에서 stderr 추출로 대체하는 경로를 제거해 줘. classifyFailure를 직접 사용하는 경우에도 잘못된 소켓 응답이 도메인 실패로 오분류되지 않게 일관된 입력 계약을 정해 줘.
- 프레임 errorCode:0은 명시적인 코드 없음이야. stderr의 WL 코드로 덮어쓰지 마.
- 프레임 errorCode:400과 stderr 앞부분 WL-999가 충돌하면 400을 유지해 줘.
- 알려지지 않은 숫자 코드도 원본을 보존해 줘.
- 실제 소켓 바이트 파서나 연결 구현은 이번 범위에 추가하지 마.

회귀 테스트: frameMeta가 없는 소켓 입력은 성공/실패 모두 계약 오류여야 하고, stderr에 WL-404가 있어도 not_found로 분류되면 안 돼. 프레임 0·400 우선순위, CLI의 첫 WL 일치 추출, 미지 코드 보존도 유지해 줘. 타입 검사에서도 잘못된 소켓/CLI 조합을 막는지 확인해 줘.

## 4. 도구 호출 예시의 인자 문법 수정

get_doc·get_report 같은 밑줄 이름은 도구 표면이므로 JSON 객체 문자열 한 칸을 인자로 받아. 현재 README와 테스트의 일부 예시는 명령 표면인 id=값을 사용해 mock에서는 성공해도 실제 엔진에서는 실패할 수 있어.

```typescript
// 도구 표면
{ command: 'get_doc', args: [JSON.stringify({ id: 'ndfixture-00000001' })] }

// 명령 표면을 예시로 쓸 때는 하이픈 이름과 해당 명령 문법을 사용
```

- README와 관련 테스트·사용 예시를 찾아 올바른 도구 인자로 수정해 줘.
- 문서의 validator 예시도 DTO의 모든 필수 필드와 배열 원소 타입을 검사하게 해 줘. 일부 필드만 검사하고 DTO 전체의 타입 가드라고 선언하지 마.
- MockTransport가 호출 문법 전체를 검증하는 엔진 흉내를 내게 만들 필요는 없어. 올바른 요청이 기록되는지는 검증해 줘.
- 과거 요청용 프롬프트 파일은 기록으로 유지하고 수정하지 마. 현재 라이브러리 사용 문서와 실행 코드/테스트를 수정해 줘.

## 완료 기준과 범위

- 위 네 가지 문제를 회귀 테스트로 검증해 줘.
- npm test와 npm run build를 실행하고 이번 변경으로 생긴 실패를 해결해 줘.
- 기존 핵심 동작인 종료/WL 조합 분류, 성공 stderr 전달, 실패 stdout 계약 위반, 원본 바이트 보존, 요청별 mock 격리를 유지해 줘.
- 테스트 숫자를 늘리는 것보다 실제 누락 동작과 경계를 검증해 줘.
- 이번 작업은 src/engine 및 관련 테스트·문서의 보완이야. 봉투별 검사, 명령별 전체 DTO, 화면 변경, Tauri/CLI/실제 소켓 연결, 엔진 재구현은 진행하지 마.
- 사용자의 기존 변경은 보존하고 관련 없는 리팩터링은 하지 마. 합리적인 설계 선택은 자율적으로 진행해 줘.
- 계획 제안만 하고 멈추지 말고 구현과 검증까지 완료해 줘.
- 최종 보고에는 문제별 수정 결과, API 변경과 사용 예시, 변경 파일, 테스트/빌드 결과, 남은 제한을 적어 줘. 공식 448건 검증 묶음 통과는 주장하지 마.
