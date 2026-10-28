# 8-2 상세 계획 — vault별 재조정 코디네이터

작성일: 2026-10-27

목표는 **같은 vault의 중복 재조정을 합치고, 실행 중 발생한 변경을 다음 실행까지 보존하며, 실패·늦은 응답을 최신 상태로 오인하지 않는 공통 계층**을 만드는 것이다.

근거: [8장 명세](./spec-chapter-08-index-and-freshness.md), [8장 전체 계획](./plan-chapter-08-index-and-freshness.md), [8-1 검토](./review-chapter-08-task-01.md). 8-1은 12개 파일·175개 테스트와 빌드 통과를 확인했다.

## 1. 이번 범위

- React/Zustand와 독립적인 vault별 코디네이터.
- 전체 갱신 기본 정책과 선택적으로 켤 수 있는 paths 실행 계획.
- 세대·실행 id·성공 보고서·실패·대기 변경의 분리.
- 완료 범위와 건강 검사 범위의 구분.
- deferred mock, 주입한 스케줄러/시계, 공개 API 예시 테스트.

실제 파일 감시·타이머 debounce·UI·DB·VaultKey·네이티브 연결·노트 캐시는 구현하지 않는다. 7-1 실패 정책과 7-2 초안 상태 머신도 별도 작업이다.

권장 파일은 `src/engine/indexing/coordinator.ts`, `refreshPlan.ts`, `coordinatorTypes.ts` 및 테스트다. 8-1 타입·클라이언트·공통 오류 분류를 재사용한다.

## 2. 주입할 경계와 공개 API

코디네이터는 vaultId를 명령 argv에 추가하지 않는다. vaultId에 연결된 ReindexClient를 팩토리로 주입받는다. 같은 이름의 id를 가진 다른 vault의 상태가 섞이지 않아야 한다.

API 권장안:

```typescript
createReindexCoordinator({
  getClient: (vaultId) => clients.get(vaultId),
  now: () => clock.now(),
  schedule: (job) => scheduler.enqueue(job),
  pathsEnabled: false,
  getPathBudget: (vaultId) => 61,
})

coordinator.requestRefresh({ vaultId, reason, paths? })
coordinator.getSnapshot(vaultId)
coordinator.subscribe(vaultId, listener)
coordinator.flush(vaultId)
coordinator.dispose()
```

API 이름은 현재 코드에 맞춰 조정할 수 있다. 동작 계약은 아래를 따른다.

- requestRefresh는 대기 변경을 먼저 기록하고 실행을 예약한다. 요청마다 새 세대를 부여하며, 같은 예약 구간의 요청은 한 실행으로 합친다.
- flush는 예약된 작업을 시작/소진하고 해당 vault가 실행 중도 실행 가능한 대기도 아닌 상태가 될 때 완료한다. 실패로 정지한 경우도 실패 상태와 미처리 변경을 남긴 채 완료하며 무한 대기하지 않는다.
- getSnapshot은 외부가 내부 큐·배열·맵을 바꿀 수 없는 스냅숏을 제공한다. 기존 엔진 응답 원본은 변경하지 않는다.
- subscribe 해제와 dispose 동작을 문서화한다. dispose 후 새 실행·자동 후속 실행·통지를 막고, 이미 시작한 네이티브 작업이 취소됐다고 주장하지 않는다.
- getClient나 실행 Promise가 throw/reject해도 실행 중 플래그가 영구히 남지 않는다. 예외는 코디네이터의 로컬 실행 오류로 보존하고 엔진 WL/종료 코드를 만들지 않는다.
- 벽시계는 표시용 완료 시각이다. 응답의 선후 판단은 시각 대신 단조 증가하는 세대·실행 id를 사용한다.

## 3. 상태와 실행 규칙

| 상태 정보 | 의미 |
|---|---|
| phase | idle / scheduled / running / failed 등 실행 상태 |
| requestedGeneration | 지금까지 받은 변경·요청 세대 |
| runningGeneration | 이번 실행 시작 때 포착한 세대 |
| lastSuccessfulGeneration | 성공 응답으로 처리 범위를 확인한 세대 |
| pending | 아직 실행하지 않았거나 실패 후 남은 변경·사유 |
| lastSuccess | 마지막 성공 보고서·요청 명령·갱신/검사 범위·시각 |
| lastFailure | 원본 실패 결과 또는 코디네이터 로컬 실행 오류 |
| needsRefresh | 미처리 변경, 실패, 초기 미확인 상태가 남았는지 |

실행 규칙:

1. vault별 실제 실행은 한 번에 하나다. 다른 vault는 병렬 실행할 수 있다. 이 보장은 이 코디네이터 인스턴스 내부의 앱 정책이며 엔진 전역 잠금이 아니다.
2. 시작 시 현재 pending을 별도 실행 묶음으로 떼어 놓는다. 그 뒤 들어온 이벤트는 새 pending에 쌓는다.
3. 성공은 시작 시 포착한 세대/묶음까지만 처리한다. 실행 중 새 변경이 있으면 다음 실행을 예약한다. full 응답도 새 pending 전체를 지우면 안 된다.
4. 실패하면 마지막 성공 보고서와 성공 시각을 유지한다. 실패한 실행 묶음과 실행 중 새 변경을 합쳐 미처리로 남기고 needsRefresh=true로 둔다.
5. 실패 직후 자동 재시도 루프를 돌리지 않는다. 수동/복구 요청 같은 다음 명시적 requestRefresh가 재개하며, 우선 full reindex로 확인한다.
6. 실행 결과는 vaultId·실행 id·유효한 수명과 대조해 반영한다. A의 늦은 응답이 B의 상태를 덮지 않는다. 오래된 예약 callback이나 dispose 후 응답도 새 상태를 만들지 않는다.
7. 상태 초기화/인스턴스 수명 변경을 지원할 경우 진행 중 Promise가 끝나기 전에 같은 vault의 잠금을 풀어 두 번째 실행을 보내지 않는다. 그런 기능이 필요 없으면 이번 API에 추가하지 않는다.

## 4. 실행 계획과 paths 정책

사유는 vault_open, external_change, manual, recovery 등으로 구분한다. focus/탭 전환/개별 읽기를 자동으로 갱신 이벤트로 연결하지 않는다.

기본은 모든 사유에 `reindex()`다. paths 최적화는 명시적 opt-in에서만 허용하며 순수 계획 함수도 단독 검증한다.

| 조건 | 계획 |
|---|---|
| vault 열기·수동·복구, 이전 실패 | full reindex |
| paths 비활성 | full reindex |
| 사유 묶음에 full 요구가 하나라도 있음 | full reindex |
| paths 활성 + 외부 변경 + 모든 경로가 안전한 notes 하위 + 예산 이내 | reindexPaths 원문 목록 |
| 경로 없음/예산 초과/안전하게 표현 못하는 경로/노트 밖 변경 | full reindex |

“안전한 경로”는 앱이 paths 최적화를 선택할 조건이다. 클라이언트의 입력 계약이나 엔진의 보안 검사를 대체하지 않는다.

- raw 경로에 `--` 시작, 첫 글자가 아닌 위치의 `=`, NUL, 모호한 빈 경로·빈 조각·.·.., notes/ 전체나 그 밖의 영역이 있으면 보수적으로 full을 고른다. 경로를 고쳐서 보내지 않는다.
- 유효 경로는 순서와 원문을 유지한다. 중복을 유지해도 되며 입력 배열을 변경하지 않는다. 예산 판단에서 중복 제거로 초과를 숨기지 않는다.
- 경로 인자 예산 기본은 61개다. 세션 표식 없이 argv 64칸을 쓸 수 있다는 전송부 정보가 있으면 63개까지다. 설정은 1~63 정수 범위에서 검사하고 잘못된 값은 로컬 설정 오류로 처리한다.
- 인자 1개의 UTF-8 길이가 1MiB를 넘으면 full로 전환한다. 실제 CLI의 전체 argv·환경 크기 제한과 인코딩의 최종 검사는 네이티브 전송부 책임이다.
- 200개 초과 fallback을 기대해 201개를 보내지 않는다. 예산을 넘으면 명시적 full로 전환하고, 임의 chunk 분할은 이번에 만들지 않는다.
- .withlog/reports 등 노트 밖 이벤트는 paths로 버리지 말고 full 건강 검사가 필요한 요청으로 보존한다.

## 5. 범위와 건강 상태

lastSuccess에는 요청 명령과 응답 mode를 함께 기록한다.

- reindex 성공: 갱신 범위는 full, 건강 검사 범위는 아젠다·노트·템플릿·회고·휴지통이다.
- reindex-paths의 paths 성공: 경로 범위 갱신과 아젠다 검사다. 검사를 하지 않은 영역을 정상이라고 표시하지 않는다.
- reindex-paths의 full 성공: 노트 갱신 범위는 full이지만 템플릿·회고·휴지통 건강 검사는 수행하지 않았다.
- skipped/conflicts/unsharded는 성공 보고서에 원문·차례를 보존한다. 종료 0과 문제 없음은 다른 판단이다.
- 최근 paths 보고서가 이전 full 건강 보고서를 “문제 없음”으로 덮어쓰지 않게 한다. 최소한 건강 범위 부족/기존 안내가 아직 확인되지 않았음을 표시할 수 있어야 한다.
- lastSuccessfulGeneration은 이벤트 처리 범위를 나타내며 디스크와 메타가 완전히 일치한다는 증명이 아니다. 엔진 관문의 한계와 옛 행 유지 규칙이 남는다.
- 코디네이터는 노트 목록·본문을 소유하지 않는다. removed 숫자나 skipped 경로로 노트 데이터를 삭제하거나 초안을 바꾸지 않는다.

## 6. 검증 행렬

실제 sleep 대신 deferred Promise와 수동 스케줄러·가짜 시계를 사용한다.

| 사례 | 확인할 것 |
|---|---|
| 같은 예약 구간 여러 요청 | 같은 vault의 실행 1번, 사유와 경로 병합 |
| 실행 중 추가 변경 | 성공 후 후속 실행 1번, 새 세대 유실 없음 |
| full 실행 중 새 이벤트 | full 응답도 실행 시작 이후 pending을 지우지 않음 |
| vault A/B 응답 순서 역전 | 각 상태·보고서·실패가 독립적으로 반영됨 |
| 성공→실패 | 이전 성공 보고서/시각 유지, needsRefresh=true |
| 실패 중 새 이벤트 | 실패 묶음과 새 pending 유지, 자동 무한 재시도 없음 |
| 명시적 복구 | full로 재실행하고 성공 후 해당 pending까지만 처리 |
| getClient throw·Promise reject | 로컬 오류 보존, 실행 플래그 정리, flush 종료 |
| dispose | 새/예약/후속 실행 차단, 늦은 통지 차단, 취소 주장 없음 |
| 기본 계획 | paths 입력이 있어도 opt-in 전에는 full |
| paths 예산 | 61/62 기본 경계, 명시적 63/64, 잘못된 예산, 201개 full 전환 |
| 경로 문법 | =·--·NUL·빈 값·.·..·노트 밖 경로, Unicode 바이트 상한, 원문/순서 유지 |
| 검사 범위 | paths/full fallback이 전체 건강 검사 완료로 오인되지 않음 |
| 성공 안내 | skipped/conflicts가 있어도 실행 성공, 문제 안내·기존 미확인 영역 유지 |
| API 예시 | 기존 client 주입과 타입 검사·실행 테스트, 원본 응답/입력 무변경 |

완료 조건은 기존 175개 동작 유지, 신규 경합·정책 테스트와 `npm test`, `npm run build` 통과다. 실제 엔진·파일 감시·DB·공식 수용 묶음을 검증한 것으로 보고하지 않는다.

## 7. 완료 시 문서 반영

README와 공개 export를 갱신한다. 사용자가 지정한 `/Users/df/Downloads/aisurada 엔진 해부도.html`의 app-progress와 app-index-plan을 백업 후 수정하고 실제 검증 결과를 반영한다. Java 엔진 1~20장·앵커·도표·스크립트는 유지하고 8-3은 후속 계획으로 둔다.

이번 문서는 계획이다. 8-2 코드 구현 완료를 뜻하지 않는다.
