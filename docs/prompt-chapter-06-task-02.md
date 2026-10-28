/Users/df/github/withlog 프로젝트에서 **6장 기반 작업 2 — 봉투 검사와 회고 읽기 클라이언트**를 구현해 줘. 계획 제안에서 멈추지 말고 코드 작성, 회귀 테스트, 빌드 검증까지 완료해 줘.

## 먼저 읽을 자료

1. 적용되는 AGENTS.md 및 현재 프로젝트 설정
2. docs/plan-chapter-06-task-02.md 전체 — 이번 개발의 상세 범위·API·검증 기준
3. docs/spec-chapter-06-output-and-errors.md 전체 — 응답 계약의 정본
4. docs/spec-chapter-05-arguments.md의 5.4 도구 표면
5. docs/plan-chapter-06-output-and-errors.md의 작업 2 및 후속 작업 경계
6. src/engine 전체, 기존 테스트, README, package.json, tsconfig.json

이전 검토 기준으로 작업 1과 보완 개발은 테스트 31개 및 빌드가 통과했어. 현재 코드 상태를 다시 확인하고 기존 변경을 보존해 줘. 계획서와 명세가 충돌하면 명세를 우선하고 선택 이유를 최종 보고에 남겨 줘.

## 이번 구현 범위

### 1. 불완전 목록 봉투

- reports/templates/trash 키를 받는 재사용 가능한 타입과 순수 타입 가드 factory를 구현해 줘.
- 정상 응답인 T[]와 불완전 응답인 { [목록 키]: T[], incomplete: ... }를 모두 검사해 줘.
- skipped는 1 이상의 정수, conflicts는 숫자 0, note는 6.3의 고정 문구와 정확히 같아야 해.
- 정상 빈 배열과 불완전 빈 목록을 구분하고, 모든 원소를 주입받은 validator로 검사해 줘.
- 템플릿·휴지통의 전체 행 DTO는 추측하지 말고 합성 원소 validator로 봉투 동작만 검증해 줘.

### 2. 검색 봉투

- 결과 행 validator를 주입받는 검색 봉투 validator를 구현해 줘.
- query는 문자열, total/returned는 0 이상의 정수, docs는 모든 원소 검증을 통과한 배열이어야 해.
- returned === docs.length, total >= returned를 검사해 줘.
- total === returned이면 truncated/hint 키가 모두 없어야 해.
- total > returned이면 truncated:true와 6.3의 정확한 hint 문자열이 모두 있어야 해.
- query를 trim하거나 요청 값으로 교체하지 마. 실제 검색 행 DTO와 검색 호출 클라이언트는 10장 없이 구현하지 마.

### 3. warning

- 객체 응답의 base validator와 조합 가능한 선택적 warning 검사를 구현해 줘.
- 키가 없으면 그대로 두고, 있으면 문자열인지 검사해 줘.
- LF, 공백, 경고 순서를 유지하고 stderr와 합치거나 재정렬하지 마.
- 빈 warning의 생성 정책이나 경고별 문구를 새로 정하지 마. 이번 범위는 문자열 타입 검사와 원문 보존이야.
- warning 계약이 없는 회고 읽기나 배열 목록에 임의로 붙이지 말고, 공통 조합은 합성 객체 응답으로 검증해 줘.

### 4. 회고 읽기 클라이언트

- 목록 원소의 id/kind/date 문자열과 hasBody/hasMemo boolean을 검사해 줘.
- 단건의 id/kind/date 문자열과 필수 body/memo의 string|null을 검사해 줘. 키 누락을 null로 채우지 마.
- kind 전체 열거값, id 정규식, 정렬, 추가 필터는 예시로 추측하지 마. 전체 명령 장을 구현했다고 보고하지 마.
- Transport를 주입받는 createReportsClient 등의 API로 listReports()와 getReport(id)를 제공해 줘.
- listReports는 { command:'list_reports', args:['{}'] }, getReport는 { command:'get_report', args:[JSON.stringify({ id })] } 형태로 호출해 줘.
- 기존 decodeResponse와 명령별 validator를 조합하고 성공·실패·계약 오류·전송 실패의 원본 정보를 유지해 줘.
- id를 trim하거나 클라이언트에서 WL 코드를 만들지 마. bodies=true, 쓰기·삭제·추가 옵션은 이번 API에 추가하지 마.

## 공통 원칙

- 기존 ResponseValidator<T> 순수 타입 가드 계약을 유지해 줘.
- 값 변환, 키 채우기/삭제, falsy 제거, 배열 정렬을 하지 마. 알려진 필드의 계약을 검증하고 미확보한 추가 키는 보존해 줘.
- null, false, 빈 배열, 빈 문자열, 키 누락을 서로 구분해 줘.
- 스키마 실패는 기존 schema_mismatch로 전달하고 WL-500으로 포장하지 마.
- 상세 파일 배치와 API는 계획서를 기준으로 하되 현재 코드에 맞는 합리적인 조정은 자율적으로 해 줘.

## 검증 및 완료 기준

계획서 8절의 테스트 행렬과 fixture 정책을 따라 줘. 특히 다음을 검증해 줘.

- 목록 세 키, 정상/불완전 빈 목록, 잘못된 incomplete 필드·고정 문구, 잘못된 원소가 뒤에 섞인 배열.
- 검색 0건/미잘림/잘림, 건수 불일치, truncated:false, hint 누락·단독·오문구.
- 회고 null/false/빈 문자열/누락 키/잘못된 타입/추가 키 보존.
- 여러 줄 warning과 stderr의 동시 보존.
- 회고 클라이언트의 명령 이름·JSON 인자 한 칸·id 원문 보존과 성공/부분 결과/404/전송 실패/스키마 실패 전달.
- 원본 stdout/stderr 바이트와 데이터가 변경되지 않음.

6.1 회고 단건, 6.2 회고 목록, 6.3 검색 0건은 명세 예시 바이트 fixture로 관리해 줘. incomplete 예시는 문서의 설명용 예시이며, 잘린 검색과 경계 사례는 합성 fixture임을 표시해 줘. 출처 절과 끝 LF 포함 여부를 기록하고, 키 순서·2칸 들여쓰기·고정 문구·끝 LF를 포함한 기대 바이트를 검증해 줘.

소비 validator와 바이트 fixture 검증의 역할을 구분해 줘. 새 JSON 파서, 출력 생성기, 실제 엔진 없이 전체 출력 바이트 계약을 검증했다고 주장하는 작업은 하지 마.

README와 index.ts의 공개 API를 갱신하고, npm test 및 npm run build를 실행해 줘. 기존 사례를 유지하고 이번 변경으로 발생한 실패를 해결해 줘. 공식 448건 묶음은 실행하지 않았으면 통과를 주장하지 마.

이번 범위는 작업 2뿐이야. 화면·라우터·스토어 변경, 실제 Tauri/CLI/소켓, 코멘트 상세 봉투, 쓰기·충돌 복구, 전체 엔진 재구현, 관련 없는 리팩터링은 진행하지 마.

최종 보고에는 구현 결과, 공개 API와 사용 예시, 변경 파일, 테스트/빌드 결과, 명세 근거와 남은 제한을 적어 줘.
