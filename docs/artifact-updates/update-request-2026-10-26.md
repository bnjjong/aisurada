현재 열려 있는 기존 아티팩트 “aisurada 엔진 해부도”(https://claude.ai/artifact/VuTQR2e9ese7TEWwBD6o1i)를 직접 업데이트해 주세요. 새 아티팩트나 복제본을 만들지 말고 기존 문서에 적용해 주세요. 기존 Java 엔진의 1~20장 설명·도표·앵커와 디자인은 유지하고, 아래 withlog 앱 작업 내역을 별도 개발 현황으로 추가해 주세요.

중요한 구분: 원문은 bnjjong/aisurada Java 엔진의 설명이고, 추가할 작업은 /Users/df/github/withlog의 React/Vite/TypeScript 앱 공통 계층입니다. 앱의 mock·타입·계약 테스트를 Java 엔진의 실제 파일 쓰기/소켓 구현 완료로 바꾸어 설명하면 안 됩니다. 기존 “Java 21, 60파일, 13개 읽기 명령” 같은 엔진 지표는 이번 앱 지표로 교체하지 마세요.

편집 위치:
1. 기존 목차에 “개발 현황 · withlog 앱 연동” 링크를 추가하고 새 section id="app-progress"로 연결하세요. 기존 #c17 및 모든 기존 앵커는 유지하세요.
2. 문서의 기존 본문/요약을 보존한 채 문서 끝에 새 절 “withlog 앱 연동 계층 개발 현황”을 추가하세요. 확인일 2026-10-26과 아래 상태표·흐름·남은 작업을 기존 디자인에 맞춰 넣으세요.
3. 기존 13장 답 쓰기, 14장 실패의 지도, 17장 serve 모드, 18장 소켓 프레임에는 짧은 안내 링크를 추가하세요. 안내의 뜻은 “앱 소비 계층의 개발 현황은 별도 절에 있으며, Java 엔진의 구현 범위와 다르다”입니다. 특히 사용자가 지정한 #c17에서 신규 현황으로 이동할 수 있어야 합니다.
4. 문서 상단에는 엔진 본문과 앱 개발 현황이 함께 들어 있다는 짧은 안내 링크만 추가하세요. 기존 성능 측정이나 엔진 파일 수는 손대지 마세요.

추가할 확정 내용:

## 구현 현황

| 범위 | 상태 | 구현 내용 |
|---|---|---|
| 6장 작업 1: 공통 응답 계약·실패 분류 | 구현 및 보완 검토 완료 | Transport, CLI/소켓 판별 유니온, stdout/stderr 원본 Uint8Array, 종료 코드·프레임 코드·출처 보존, 순수 타입 가드 기반 decodeResponse, 계약 오류 분리, MockTransport |
| 작업 1 보완 | 반영 확인 | 전송 시점 pre_dispatch/in_flight/unknown와 재조회 지침, validator 반환 계약 단일화, 소켓 frameMeta 필수와 런타임 누락 검사, 도구 인자를 JSON 한 칸으로 전달 |
| 6장 작업 2: 봉투·회고 읽기 | 주요 기능 검토 완료 | reports/templates/trash 정상 배열 또는 incomplete 봉투, 고정 note/skipped/conflicts, 검색 total/returned/docs 관계와 truncated/hint, 선택적 warning·stderr 원문 보존, 회고 목록·단건 필수 필드와 null 예외, createReportsClient |
| 작업 2 공개 사용 예시 보완 | 반영 확인 | isReadOnlyArray/isIncompleteListEnvelope로 readonly 배열 타입 분기, MockTransport 응답 등록, usageExamples.test.ts에서 컴파일 및 실행 검증 |
| 7-1: 토큰 전달과 쓰기 결과 해석 | 코드 구현·자체 테스트 통과, 실패 정책 보완 필요 | 선택적 precondition validator, f1 결합·중복 방지, vault/루트/하위 대상 문맥, link_docs from 토큰, containerBefore/After 및 bodyBefore/After 쌍 검사·비교, 쓰기 결과 정책 |
| 7-2: 초안·저장 상태 전이 | 계획 | 저장 중 추가 입력 보존, 요청·대상·판 대조, 충돌 후 재읽기와 명시적 재적용, 같은 대상의 앱 저장 순서 관리 |
| 7-3: 명령별 쓰기·실제 연결 | 후속 계획 | 전체 명령 명세 확보, 실제 Tauri/CLI 또는 소켓, 격리 vault 검증, 에디터·안전 종료 연결 |

## 앱에서 현재 구현한 호출 흐름

회고 읽기: 호출자 → createReportsClient → Transport(MockTransport) → decodeResponse → 명령별 순수 validator → 성공/실패/계약 오류/전송 실패.

쓰기 준비 계층: 엔진에서 받은 precondition → 대상 문맥 확인 → 요청 f1 결합 → 검증된 응답/토큰 쌍 해석 → 토큰 연결·본문 갱신·재조회 필요성 판단 데이터.

실제 엔진 실행, 파일 저장, 자동 재조회·재시도는 아직 이 흐름에 연결하지 않았습니다.

## 유지하는 계약

- 소켓 프레임 errorCode가 stderr 추출값보다 우선이며 0도 명시적인 코드 없음입니다. CLI의 첫 WL 정규식 일치에는 로그/파일명 오인식 한계가 있습니다.
- 종료 0이어도 stderr·warning·incomplete가 있을 수 있습니다. 성공 결과와 주의 정보를 함께 보존합니다.
- 실패 stdout, 손상 JSON, 스키마 불일치는 앱 계약 오류이며 WL-500으로 변환하지 않습니다.
- 응답 키 precondition과 요청 키 f1은 다릅니다. 앱은 해시를 만들거나 토큰을 trim/정규화하지 않습니다.
- 파생 쓰기의 before가 보유 토큰과 일치할 때만 after를 이어받을 수 있습니다. 일치해도 본문 캐시는 따로 갱신해야 하므로 오래된 본문에 새 토큰만 붙여 재저장하지 않습니다.
- WL-412는 보호 대상 본문 쓰기의 거절입니다. 선행 색인·git·쓴 쪽 기록까지 모두 없었다는 뜻은 아닙니다. 초안을 유지하고 최신 본문 위에 재적용합니다.
- 명령 전체는 트랜잭션이 아닙니다. WL-500이나 응답 유실에서 부분 반영 가능성을 고려하고 상태를 확인해야 합니다.

## 검증 이력과 한계

- 작업 1 최초: 27개 테스트 통과. 보완 뒤: 31개 통과.
- 작업 2 검토 시: 6개 테스트 파일, 88개 통과, npm run build 성공.
- 최신 2026-10-26 실제 재실행: 10개 테스트 파일, 146개 테스트 통과, npm run build(tsc && vite build) 성공.
- 최신 결과는 src/engine 및 src/engine/write와 공개 API 예시의 자체 검증입니다. 모든 명세 경계를 검증한 것은 아니며 공식 448건 수용 묶음 통과를 뜻하지 않습니다.
- 실제 파일 시스템의 SHA-256/fsync/rename/잠금, 색인 DB, git, Tauri/CLI/소켓 통합, 실제 저장 동작은 이번 앱 작업에서 검증하지 않았습니다. 기존 Java 엔진 본문의 별도 검증 이력과 혼합하지 않습니다.

## 7-1 후속 보완 사항(현재 코드 읽기 및 재현으로 확인)

1. src/engine/write/outcomes.ts의 determineWriteOutcome는 unclassified_execution_failure(단발 종료 1, 미지의 코드 등)를 기타 실패로 묶어 protected_body_not_written 및 requiresFilesystemInspection:false로 반환할 수 있습니다. 실제로는 쓰기 완료 여부를 단정할 수 없으므로 uncertain_or_partial과 재조회 지침으로 보완해야 합니다.
2. 같은 함수는 failure.category 또는 wlCode 또는 exitCode 중 하나만 맞아도 충돌로 판정합니다. 따라서 공통 분류기가 미분류로 보존한 종료 5 + 미지 WL 조합도 WL-412 충돌로 바뀔 수 있습니다. 기존 공통 분류 결과와 정의된 종료/WL 조합을 존중하고 미정의 조합을 확정적인 본문 미변경으로 바꾸지 않도록 보완해야 합니다.

이 두 항목 때문에 7-1을 최종 완료 또는 전체 저장 안전성 확보로 표시하지 마세요. 이번 문서 업데이트에서 소스를 수정한 것은 아닙니다.

## 관련 저장소 문서와 주요 파일

아래는 로컬 저장소 상대 경로입니다. 외부에서 열 수 있는 URL인 것처럼 링크를 꾸미지 말고 코드 경로로 표시하세요.

- docs/spec-chapter-06-output-and-errors.md
- docs/plan-chapter-06-output-and-errors.md
- docs/plan-chapter-06-task-02.md
- docs/review-chapter-06-task-02.md (당시 예시 보완 사항은 최신 코드에서 해결 확인)
- docs/spec-chapter-07-write-pipeline.md
- docs/plan-chapter-07-write-pipeline.md
- docs/prompt-chapter-07-task-01.md
- src/engine/{types,transport,decodeResponse,classifyFailure,envelopes,warnings}.ts
- src/engine/commands/reports.ts
- src/engine/write/{types,preconditions,derivedTokens,outcomes}.ts
- src/engine/README.md 및 src/engine/__tests__/usageExamples.test.ts

목표는 위 내용을 기존 문서에 실제 저장하는 것입니다. 대화 답변에만 요약하지 말고 아티팩트 본문을 수정해 주세요. 기존 문서가 잠겨 있어 직접 수정이 불가능하면 새 문서로 몰래 대체하지 말고 제한을 알려 주세요.
