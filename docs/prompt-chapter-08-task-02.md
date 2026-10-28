/Users/df/github/withlog 프로젝트에서 **8-2 — vault별 재조정 코디네이터**를 구현해 줘. 계획 제안에서 멈추지 말고 실제 코드 작성·경합 테스트·타입 검사·빌드와 현황 문서 업데이트까지 완료해 줘.

## 먼저 읽을 자료

1. 적용되는 AGENTS.md, package.json, tsconfig.json
2. docs/plan-chapter-08-task-02.md 전체 — 이번 개발의 상세 동작·검증 계약
3. docs/spec-chapter-08-index-and-freshness.md 전체
4. docs/plan-chapter-08-index-and-freshness.md와 docs/review-chapter-08-task-01.md
5. src/engine 전체, 특히 commands/reindex.ts, indexing/types.ts·validators.ts, 공통 결과 타입과 기존 테스트

최근 확인 기준은 8-1 구현 완료, 테스트 12개 파일·175개 통과와 빌드 성공이야. 현재 상태를 다시 확인하고 사용자의 기존 변경은 보존해 줘. 명세와 계획이 충돌하면 명세를 우선하고 판단을 보고해 줘.

## 구현 범위

React/Zustand·파일 감시와 독립적인 코디네이터를 만들어 줘. 권장 파일은 src/engine/indexing/coordinator.ts, refreshPlan.ts, coordinatorTypes.ts 및 관련 테스트야. 기존 ReindexClient·DecodedResponse·validator·실패 분류를 재사용하고 같은 계층을 중복 구현하지 마.

### 1. vault별 실행과 세대

- vaultId에 연결된 ReindexClient 팩토리, 시계, 예약 스케줄러를 주입받아 줘. vaultId를 명령 argv로 보내거나 경로 해시를 새로 계산하지 마.
- requestRefresh/getSnapshot/subscribe/flush/dispose 또는 동등한 공개 API를 제공해 줘. 세부 계약은 상세 계획서 2절을 따라 줘.
- vault별 requestedGeneration·runningGeneration·lastSuccessfulGeneration과 pending, 마지막 성공 보고서/시각, 마지막 실패를 분리해 줘.
- 같은 vault의 실제 실행은 한 번에 하나야. 같은 예약 구간의 중복 요청을 합치고 서로 다른 vault는 독립적으로 처리해 줘.
- 시작할 때 포착한 요청 묶음과 실행 중 새 pending을 분리해 줘. 성공은 포착한 세대까지만 처리하고 새 변경이 있으면 후속 실행을 예약해 줘.
- full 응답이 와도 실행 시작 뒤에 들어온 이벤트를 지우지 마.
- 실패하면 마지막 성공 보고서와 성공 시각을 유지하고, 실패 묶음과 실행 중 새 이벤트를 미처리로 남겨 줘. 캐시를 빈 것으로 교체하거나 최신이라고 표시하지 마.
- 실패 직후 자동 재시도 루프를 돌리지 마. 다음 명시적 요청이 복구를 재개하고 우선 full reindex를 사용하게 해 줘.
- 늦은 응답은 vaultId·실행 id·수명과 대조해 반영해 줘. 오래된 예약 callback, dispose 이후 응답, 다른 vault 응답이 현재 상태를 덮지 않게 해 줘.
- flush는 실행 가능한 대기가 소진되거나 실패로 정지하면 완료해야 해. 실패 pending 때문에 무한히 기다리거나 재시도하지 마.
- getClient throw/Promise reject도 로컬 실행 오류로 보존하고 실행 플래그를 정리해 줘. 엔진 WL/종료 코드를 임의로 만들지 마.
- dispose는 새/예약/후속 실행과 통지를 막되 이미 시작한 실제 엔진 작업을 취소했다고 주장하지 마. 상태 초기화로 진행 중 vault 잠금을 조기에 풀지 마.
- 외부에서 snapshot을 수정해 내부 큐/배열/맵을 바꿀 수 없게 해 줘. 엔진 원본 응답은 변경하지 마.

### 2. 실행 계획과 선택적 paths

- 기본 정책은 full reindex야. paths는 명시적인 opt-in에서만 켜고, 순수 실행 계획 함수로도 검증 가능하게 해 줘.
- vault_open/manual/recovery, 이전 실패, full 요구가 섞인 묶음은 full이야.
- opt-in의 external_change이며 경로가 모두 안전한 notes 하위이고 전송 예산 이내일 때만 reindexPaths를 선택해 줘.
- 빈 경로/전체 notes/노트 밖 이벤트/모호한 경로, '--' 시작, 첫 글자 아닌 곳의 '=', NUL, 빈 조각 및 .·..가 있으면 보수적으로 full을 선택해 줘. 경로를 고쳐서 보내거나 이벤트를 버리지 마.
- 경로 원문·순서와 입력 배열을 유지해 줘. 예산 초과를 감추기 위한 중복 제거는 하지 마.
- 기본 경로 인자 예산은 61개고 명시적 전송 능력이 있으면 최대 63개야. 설정은 1~63 정수 범위를 검사하고 잘못된 설정은 로컬 오류로 처리해 줘.
- UTF-8 1MiB를 넘는 경로 인자는 full로 전환해 줘. 실제 CLI 전체 argv·환경 크기는 네이티브 전송부의 후속 책임이야.
- 201개를 소켓으로 보내 엔진의 200개 초과 fallback을 기대하지 마. 예산 초과는 full로 전환하고 chunk 분할은 추가하지 마.
- 저수준 createReindexClient의 경로 원문 전달 계약은 변경하지 마. 이 선택은 코디네이터의 앱 정책이야.

### 3. 갱신 범위와 건강 검사 범위

- 성공 보고서에는 요청 명령과 응답 mode를 함께 보존해 줘.
- reindex 성공의 전체 건강 검사와 reindex-paths fallback의 노트 전체 갱신은 달라. 후자에는 템플릿·회고·휴지통 검사가 없어.
- paths의 빈 skipped/unsharded를 vault 전체 정상 판정으로 사용하지 마.
- 이전 full 보고서의 안내를 최근 부분 보고서로 지워 버리지 마. 적어도 해당 영역의 건강 상태가 아직 재확인되지 않았다는 정보가 남아야 해.
- skipped/conflicts/unsharded는 성공 응답이어도 원문과 차례를 보존해 줘. 완료 여부와 문제 안내를 분리해 줘.
- 이벤트 세대 처리 완료가 실제 디스크와 색인의 완전 일치를 증명한다고 설명하지 마. 엔진 관문의 낡음 한계는 남아 있어.
- 코디네이터는 노트 목록·본문·초안을 소유하지 않아. removed나 skipped로 노트 데이터를 삭제하거나 초안을 덮지 마.

## 테스트와 완료 기준

상세 계획서 6절 행렬을 모두 검증해 줘. 실제 sleep 대신 deferred Promise, 수동 스케줄러, 가짜 시계를 사용해 줘.

- 실행 전 중복 병합, 실행 중 변경 후 후속 실행, full 응답 중 새 이벤트 유지.
- vault A/B 순서 역전, 이전 성공 뒤 실패, 실패 중 새 이벤트 보존, 명시적 복구.
- 팩토리 throw·Promise reject의 플래그 정리, flush 종료, dispose와 늦은 응답.
- 기본 full 및 paths opt-in, 61/62·63/64 경계, 201개·잘못된 예산.
- 경로 문법과 Unicode UTF-8 상한, 노트 밖 이벤트, 입력 배열·원문 보존.
- paths/full fallback의 건강 범위 부족, 기존 안내 유지, skipped/conflicts 포함 성공.
- 공개 API 예시의 컴파일과 실행, 기존 175개 동작 유지.

index.ts와 README를 갱신하고 npm test, npm run build를 실행해 줘. 이번 변경으로 발생한 실패는 해결해 줘. 테스트 개수를 목표로 불필요한 구현 복제 테스트를 늘리지 마.

## 범위 제한

이번에는 8-2만 구현해 줘. 실제 파일 감시, 타이머 debounce/TTL, UI·스토어·라우터, 네이티브 연결, DB/SQLite, VaultKey, 파일 쓰기·git, 7-1 보완/7-2 초안 상태 머신, 전체 노트 캐시는 하지 마. 원래 명세에 없는 엔진 동작을 추가하지 마.

기존 7-1 미분류 실패·미정의 코드 조합 보완은 별도로 남아 있어. 이번에 조용히 섞어 고치거나 완료로 기록하지 마. 실제 엔진·파일 감시·공식 수용 묶음 검증을 수행하지 않았으면 통과를 주장하지 마.

완료 뒤 /Users/df/Downloads/aisurada 엔진 해부도.html의 app-progress와 app-index-plan도 백업 후 업데이트해 줘. 실제 파일의 Unicode 정규화 형태를 확인하고 Java 엔진 기존 본문·도표·앵커·스크립트는 유지해 줘. 8-2의 실제 결과만 기록하고 8-3은 계획 단계로 남겨 줘. 원격 Claude 문서는 이번 대상이 아니야.

최종 보고에는 구현 결과, 공개 API·사용 예시, 변경 파일, 경합·정책 테스트와 빌드 결과, HTML 반영 여부, 선택한 정책과 남은 제한을 적어 줘. 합리적인 구현 선택은 자율적으로 진행하고 끝까지 완료해 줘.
