# 8-2 구현 검토 — 보완 필요

검토일: 2026-10-27

기존 테스트는 14개 파일·209개 통과했고 npm run build도 성공했다. 다만 별도 경계 재현에서 아래 두 문제가 확인돼 8-2의 최종 완료 판단은 보류한다. 이번 검토에서는 소스를 수정하지 않았다.

## 1. 취소 불가능한 예약 콜백이 실패 요청을 자동 재실행함

위치: src/engine/indexing/coordinator.ts의 scheduleExecution(348행), flushVault(368행), executeNextBatch(263행).

공개 schedule 타입은 취소 함수를 반환하지 않아도 된다. schedule: job => { jobs.push(job) }로 주입하고 requestRefresh 후 flush하면 flush가 즉시 실행한다. 이 실행을 실패시켜 failed로 둔 뒤, 큐에 남은 jobs[0]()을 호출하면 명시적 복구 요청 없이 다시 실행된다. 재현에서는 호출 횟수가 1→2, 상태가 failed→idle로 바뀌었다.

물리적인 cancel 지원과 별개로 예약마다 식별자를 부여하고 flush·실행 시작·재예약·dispose에서 이전 예약을 논리적으로 무효화해야 한다. 이미 취소됐거나 오래된 callback은 실행·복구·pending 소비를 해서는 안 된다.

## 2. 중첩 스냅숏이 내부 상태를 노출하고 원본 응답을 직접 동결함

위치: 같은 파일의 createSnapshot(53행)과 finishWithSuccess(171행).

- pendingRequests 배열만 새 배열로 동결하고 원소는 공유한다. snapshot.pendingRequests[0].trigger를 바꾸면 이후 내부 요청에도 반영됐다.
- lastSuccess 객체와 data의 중첩 배열을 공유한다. snapshot.lastSuccess.completedAt=-1 및 snapshot.lastSuccess.data.skipped.push(...)가 이후 상태에도 반영됐다.
- 성공 처리의 Object.freeze(data)는 클라이언트가 돌려준 원본 응답을 직접 변경한다. 반환 전에 비동결이었던 원본이 코디네이터 처리 후 동결됐다.

코디네이터 소유 상태와 외부 입력/원본 응답의 소유권을 분리해야 한다. 스냅숏을 통해 요청·보고서·건강 범위·이전 경고·실패 정보가 변경되지 않도록 독립 복사와 동결 또는 동등한 방식을 사용하고 원본은 직접 동결하지 않는다. Uint8Array를 가진 실패 원본에는 일괄 Object.freeze를 적용하지 않는다.

## 수정 및 검증 범위

8-2의 예약 수명과 스냅숏 소유권만 보완하고 회귀 테스트를 추가한다. 기존 세대·pending·실패 정지/명시적 복구·다중 vault·건강 범위·paths 예산 계약을 유지한다. 8-3 감시/UI와 7장 쓰기 정책을 섞지 않는다.

사용자 지정 Downloads HTML의 구현 현황과 app-review-8-2를 이 결과에 맞게 보완 필요로 정정한다. 테스트 209개 통과와 실제 완료 판단을 구분한다.
