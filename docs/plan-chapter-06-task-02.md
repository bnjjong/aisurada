# 작업 2 개발 계획 — 봉투 검사와 회고 읽기 클라이언트

작성일: 2026-10-26

목표는 작업 1의 `Transport → decodeResponse → ResponseValidator<T>` 흐름에 명령별 응답 검사를 연결하는 것이다. 정상 빈 목록과 불완전 목록을 구분하고, 검색의 잘림 정보를 검증하며, 회고 목록·단건 읽기를 mock으로 호출할 수 있는 상태까지 완성한다.

## 1. 기준과 범위

- [전체 개발 계획](./plan-chapter-06-output-and-errors.md)의 작업 2를 구체화한다.
- [6장](./spec-chapter-06-output-and-errors.md)의 6.1~6.4를 응답 계약의 근거로 삼는다.
- 도구 호출 인자는 [5장 5.4](./spec-chapter-05-arguments.md)에 따라 JSON 객체 문자열 한 칸으로 전달한다.
- 선행 작업은 공통 타입, 실패 분류, 순수 타입 가드 기반 디코더, MockTransport다. 최근 확인한 기준은 테스트 31개와 빌드 통과다.

이번에 구현할 것:

1. `reports`·`templates`·`trash` 불완전 목록 봉투의 재사용 가능한 타입과 validator.
2. 결과 행 validator를 주입받는 검색 봉투 validator.
3. 객체 응답의 선택적 `warning` 검사 및 원문 보존.
4. 6장에 명시된 회고 목록·단건 필드의 validator와 읽기 클라이언트.
5. 고정 바이트 fixture, 경계 사례, mock을 통한 호출·디코딩 통합 테스트.

화면, 상태 스토어, 실제 Tauri/CLI/소켓, 쓰기 명령은 후속 작업이다. 검색·템플릿·휴지통의 전체 행 DTO나 호출 클라이언트도 해당 명령 장 없이 만들지 않는다. 코멘트 봉투는 11·19장 확보 후 다룬다.

## 2. API 구성

아래 이름은 권장안이다. 현재 코드에 맞게 조정하되 순수 validator와 실제 호출 함수를 구분한다.

| 파일 | 역할 |
|---|---|
| `src/engine/envelopes.ts` | 불완전 목록·검색 봉투 타입, validator factory, 고정 문구 |
| `src/engine/warnings.ts` | 객체 응답의 선택적 warning 타입과 validator 조합 |
| `src/engine/commands/reports.ts` | 회고 응답 타입·validator, 주입받은 Transport를 사용하는 읽기 함수 |
| `src/engine/__fixtures__/chapter06/` | 명세 예시 및 합성 경계 fixture와 출처 기록 |
| `src/engine/__tests__/envelopes.test.ts` | 목록·검색 봉투 검사 |
| `src/engine/__tests__/warnings.test.ts` | warning 타입·줄 순서·원문 보존 |
| `src/engine/__tests__/reports.test.ts` | 회고 필드, 호출 인자, 공통 디코더 통합 |
| `src/engine/index.ts`, `README.md` | 공개 API와 실행 가능한 사용 예시 |

공통 규칙:

- 반환 계약은 기존 `ResponseValidator<T> = (data: unknown) => data is T`를 유지한다.
- 봉투 validator는 내부 원소 validator를 실제로 호출한다. 배열 여부만 보고 `T[]`라고 선언하지 않는다.
- 타입 검사는 값을 고치지 않는다. 배열/객체를 다시 만들거나 키를 넣고 빼거나 정렬하지 않는다.
- 객체 모양 검사는 null과 배열을 구분하고, 필수·조건부 키의 존재 여부를 확인한다.
- 명세에 없는 추가 키는 보존하고 기본적으로 허용한다. 알려진 키의 잘못된 타입·조건은 거절한다. 추가 키를 자동 삭제하거나 임의의 전체 키 허용 목록을 만들지 않는다.
- 스키마 검사 실패는 기존 디코더의 `schema_mismatch`로 전달한다. 새로운 WL 코드를 만들지 않는다.

## 3. 불완전 목록 봉투

권장 API:

```typescript
type IncompleteListKey = 'reports' | 'templates' | 'trash';

interface IncompleteInfo {
  skipped: number;
  conflicts: 0;
  note: string;
}

type ListResponse<K extends IncompleteListKey, T> =
  | T[]
  | ({ [P in K]: T[] } & { incomplete: IncompleteInfo });

// 실제 타입 서명은 위 ListResponse와 기존 ResponseValidator를 사용해 구현한다.
// createListResponseValidator(key, itemValidator)
```

검사 규칙:

- 정상 결과는 배열 자체다. 빈 배열도 정상이다.
- 봉투이면 명령에 맞는 목록 키와 `incomplete`가 모두 있어야 한다.
- `skipped`는 1 이상의 정수다. 0이면 정상 배열로 나가야 한다는 6.3의 조건과 어긋난다.
- `conflicts`는 숫자 0이다. 문자열 `"0"`이나 양수는 거절한다.
- `note`는 `일부 파일을 읽지 못했어요(형식 오류·크기 초과) — 목록이 불완전할 수 있어요.`와 정확히 같아야 한다.
- 배열과 봉투 모두 원소 validator로 각 행을 검사한다.
- 다른 명령의 목록 키가 대신 들어온 봉투, 목록 키만 있고 incomplete가 없는 객체는 거절한다.
- 파일별 실패 정보는 stderr로 유지한다. stdout 봉투에서 파일명·이유를 추출하거나 만들어 넣지 않는다.

템플릿·휴지통은 합성 원소 validator로 봉투 동작만 시험한다. 이 결과를 실제 템플릿·휴지통 DTO 검증 완료로 보고하지 않는다.

## 4. 검색 봉투

`createSearchEnvelopeValidator(itemValidator)` 형태로 구성하고 전체 검색 호출 API는 만들지 않는다.

필수 필드는 `query: string`, `total: number`, `returned: number`, `docs: T[]`다. 숫자는 0 이상의 정수이며 다음 관계를 함께 검사한다.

| 조건 | 허용되는 모양 |
|---|---|
| 항상 | `returned === docs.length`, `total >= returned` |
| `total === returned` | `truncated`, `hint` 키가 모두 없음 |
| `total > returned` | `truncated === true`, 정해진 `hint`가 모두 있음 |

`hint`는 `agenda·type·tag·dateFrom으로 좁히거나 limit을 올리세요`와 정확히 같아야 한다. `truncated: false`, null, hint 단독, 필요한 hint 누락은 거절한다.

query는 실제 응답 문자열을 그대로 유지한다. 요청 query와 맞추려고 trim하거나 교체하지 않는다. 결과 행은 주입된 validator로 검사한다. 10장이 없으므로 실제 검색 행의 필드를 예제에서 추측하지 않는다.

## 5. warning

`withOptionalWarning(baseValidator)` 같은 조합 함수를 두어 응답 객체의 선택적 warning을 검사한다.

- 키가 없으면 그대로 통과한다. `warning: undefined`를 추가하지 않는다.
- 키가 있으면 문자열이어야 한다. null·배열·숫자는 거절한다.
- 빈 문자열의 생성 정책이나 경고별 정확한 문구는 명령 장·부록 C 없이 새로 정하지 않는다. 이번 소비 계층은 문자열 타입과 보존만 검사한다.
- 여러 줄 문자열의 LF, 줄 순서, 공백을 보존한다. split·trim·중복 제거·재결합하지 않는다.
- warning과 stderr 진단은 각각 유지한다. 서로 합치거나 한쪽을 숨기지 않는다.
- 원래 배열인 목록이나 warning 계약이 없는 회고 읽기 응답에 warning을 임의로 붙이지 않는다. 합성 객체 응답으로 공통 조합 동작을 시험한다.

## 6. 회고 읽기

6장에 나온 필수 필드만 검증하는 타입을 정의한다. 이는 전체 회고 명령 장을 구현했다는 뜻이 아니다.

| 응답 | 필수 필드 |
|---|---|
| 목록 원소 | `id`, `kind`, `date`: 문자열 / `hasBody`, `hasMemo`: boolean |
| 단건 읽기 | `id`, `kind`, `date`: 문자열 / `body`, `memo`: string 또는 null |

- 단건의 body·memo는 키가 필수다. 키 누락을 null로 보충하지 않는다.
- 목록의 `hasMemo: false`를 누락시키거나 null로 바꾸지 않는다.
- kind의 전체 열거값, id 정규식, 목록 정렬, 조회 필터는 예시 하나로 정하지 않는다. 날짜도 실제 시각 객체로 변환하지 않는다. 이번 검사는 표에 적힌 필수 필드와 타입을 대상으로 한다.
- 더 많은 필드가 오면 보존한다. 알려진 필드만 뽑아 새 객체를 만들지 않는다.
- `list-reports bodies=true`는 명령 표면에 대한 별도 계약이다. 이번 도구 목록 API에 bodies 옵션을 추측해 추가하지 않는다. 본문 포함 원소의 구조는 단건 validator로 독립 검증할 수 있다.

권장 호출 API:

```typescript
createReportsClient(transport).listReports()
// transport.execute({ command: 'list_reports', args: ['{}'] })
// Promise<DecodedResponse<ListResponse<'reports', ReportSummary>>>

createReportsClient(transport).getReport(id)
// transport.execute({ command: 'get_report', args: [JSON.stringify({ id })] })
// Promise<DecodedResponse<ReportDetail>>
```

명령 클라이언트는 Transport를 주입받고 기존 decodeResponse와 해당 validator를 조합한다. 실패·전송 실패·계약 오류를 새 오류로 감싸서 원본을 잃지 않는다. 입력 id를 trim하거나 클라이언트에서 WL 오류를 만들어 반환하지 않는다. 저장·삭제·추가 필터·자동 재시도는 구현하지 않는다.

## 7. 구현 순서

1. **봉투 타입·목록 validator**: 세 목록 키, 양쪽 응답 모양, 정수·고정 문구 규칙을 구현하고 테스트한다.
2. **검색·warning validator**: 건수와 조건부 키의 관계, 원소 검사, 여러 줄 보존을 테스트한다.
3. **회고 validator·읽기 클라이언트**: JSON 인자, null 예외, 목록 봉투, 공통 실패 결과 전달을 mock으로 통합 검증한다.
4. **fixture·문서·export 정리**: 원본 바이트 근거를 기록하고 전체 테스트와 빌드를 실행한다.

단계마다 검증한 뒤 다음 단계로 진행하되, 이 네 단계를 한 개발 세션의 작업 2 범위로 완료한다.

## 8. 검증과 완료 기준

| 대상 | 필수 사례 |
|---|---|
| 목록 | 정상 [], 정상 여러 행, 불완전 빈 목록, 불완전 일부 목록, 세 키의 선택과 오용 |
| incomplete | skipped 0/음수/소수/문자열, conflicts 비0, note 변형, 필수 키 누락 |
| 검색 | 0건, 미잘림, 잘림, total/returned/docs 불일치, 음수/소수, truncated:false, hint 단독·누락·오문구 |
| 원소 | 목록·검색 각각 올바른 행 뒤에 잘못된 행이 섞이면 전체 validator 실패 |
| 회고 | memo/body null, 빈 문자열, 키 누락, 잘못된 타입, hasMemo:false, 추가 키 보존 |
| warning | 키 없음, 여러 줄, 공백 포함 원문, 잘못된 타입, stderr와 동시 전달 |
| 클라이언트 | 명령 이름·JSON 인자 한 칸, getReport id 원문, 성공·불완전 성공·진단·404·전송 실패·스키마 실패 |
| 원본 보존 | raw.stdout/stderr 동일, 누락 키가 여전히 없음, false/null/[] 및 추가 키 유지 |

fixture 정책:

- 6.1 회고 단건, 6.2 회고 목록, 6.3 검색 0건 예시는 명세 예시 바이트로 보관한다.
- 6.3 incomplete 예시는 문서 자체의 설명용 예시이며, 잘린 검색·오류 사례는 합성 자료라고 표시한다.
- fixture에 출처 절, 명세 예시인지 합성인지, 끝 LF 포함 여부를 기록한다.
- 2칸 들여쓰기·키 순서·고정 문구·끝 LF는 고정 기대 바이트와 비교한다. 객체 동등성 검사만으로 바이트 계약 검증을 대신하지 않는다.
- 소비 validator는 스키마와 관계를 검사한다. 키 순서·공백·중복 키·모든 JSON escape를 감시하는 새 JSON 파서나 출력 생성기는 이번에 만들지 않는다. fixture 검증의 범위를 실제 엔진 출력 검증으로 확대해서 보고하지 않는다.

완료 조건:

- 위 동작을 테스트하며 기존 31개 사례도 유지한다. 테스트 수 자체를 목표로 삼지 않는다.
- `npm test`, `npm run build`가 통과한다.
- 회고 클라이언트의 README 예시가 실제 공개 API와 일치한다.
- validator가 원본 응답을 수정하지 않고 모든 목록·검색 원소를 검사한다.
- 정상 빈 결과·불완전 결과·잘린 검색을 구분할 정보가 호출자에게 남는다.
- 실제 엔진이나 공식 448건 묶음을 실행하지 않았으면 명확히 기록한다.
- UI·Tauri·쓰기 코드 변경 없이 작업 2가 독립적으로 완성된다.

완료 후 작업 3에서 이 회고 클라이언트를 `/review` 화면에 주입해 로딩·빈 목록·부분 결과·실패를 표시한다.
