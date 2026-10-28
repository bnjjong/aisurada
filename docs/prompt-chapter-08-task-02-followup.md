/Users/df/github/withlog 프로젝트에서 **8-2 재조정 코디네이터의 검토 결과 2건**을 수정해 줘. 실제 코드 수정과 회귀 테스트·타입 검사·빌드·현황 문서 갱신까지 완료해 줘. 8-3으로 범위를 확장하지 마.

## 먼저 읽을 자료

- 적용되는 AGENTS.md와 프로젝트 설정
- docs/review-chapter-08-task-02.md — 재현 결과
- docs/plan-chapter-08-task-02.md
- docs/spec-chapter-08-index-and-freshness.md의 8.3~8.5
- src/engine/indexing/coordinator.ts, coordinatorTypes.ts, refreshPlan.ts 및 관련 테스트
- 기존 공통 DecodedResponse·원본 응답 타입, README, usageExamples.test.ts

별도 검토에서 npm test 14개 파일·209개 통과, npm run build 성공을 확인했어. 하지만 아래 경계는 기존 테스트에서 잡히지 않았고 실제로 재현됐어. 현재 코드를 확인하고 사용자 변경은 보존해 줘.

## 1. 오래된 예약 콜백의 자동 재실행 방지

현재 schedule은 취소 함수를 반환하지 않아도 되는 공개 계약이야. 아래 순서에서 명시적 복구 없이 실패한 요청이 다시 실행돼.

1. schedule: job => { jobs.push(job) }처럼 취소 함수를 반환하지 않는 수동 스케줄러를 주입.
2. requestRefresh로 작업을 예약.
3. flush를 호출해 즉시 실행하고 그 요청은 실패시킴. 상태는 failed, pending은 보존, 실제 호출은 1회.
4. 스케줄러 큐에 남은 jobs[0]()을 나중에 호출.
5. 현재는 executeNextBatch가 실패 pending을 다시 소비해 두 번째 요청을 보냄. 두 번째를 성공시키면 명시적 복구 요청 없이 failed→idle로 바뀜.

수정 요구:

- 스케줄러의 물리적 cancel 지원 여부와 무관하게 예약 콜백을 논리적으로 무효화해 줘. 예약 id/세대 같은 식별자를 사용하고 flush·실행 시작·재예약·dispose에서 이전 callback의 유효성을 정리해 줘.
- 취소 함수를 반환하지 않는 공개 schedule 계약을 계속 지원해 줘. 해당 형태를 금지하거나 타입에서 삭제하는 것으로 회피하지 마.
- cancel이 호출되어도 이미 전달된 callback은 나중에 올 수 있어. 취소 함수를 반환하는 스케줄러에서도 오래된 callback이 실행·복구·pending 소비를 하지 않도록 해 줘.
- 오래된 callback이 새 명시적 요청의 pending까지 먼저 소비하지 않게 해 줘.
- 실패 뒤에는 자동 재시도 없이 정지하고, 다음 명시적 requestRefresh에서만 full 복구를 재개하는 계약을 유지해 줘.
- 실행 중 새 이벤트의 후속 실행, 동일 구간 병합, vault별 실행 1개, flush의 정상 종료를 유지해 줘.

회귀 테스트:

- cancel 없는 스케줄러 + flush + 실패 + 옛 callback: 호출 횟수 1회 유지, failed 및 pending 유지.
- cancel 없는 스케줄러 + flush + 성공 + 옛 callback: 중복 호출 없음.
- cancel 함수가 있어도 취소 후 옛 callback 도착: 무시.
- 실패 후 새 복구 요청이 예약된 상태에 옛 callback 도착: 새 예약을 침범하지 않음.
- 이후 새 유효 예약/flush로 정상 full 복구 가능.
- dispose 이후 옛 예약 callback: 새 실행과 구독 알림 없음.

## 2. 스냅숏 격리 및 원본 응답 소유권

현재 createSnapshot은 루트와 pendingRequests 배열만 동결하고, 요청 원소·lastSuccess·중첩 배열/객체는 내부 상태와 공유해. 아래 변경이 내부에 반영됨을 재현했어.

```typescript
snapshot.pendingRequests[0].trigger = 'CHANGED_EXTERNALLY';
snapshot.lastSuccess.completedAt = -1;
snapshot.lastSuccess.data.skipped.push({ kind: 'note', path: 'injected', reason: 'external mutation' });
```

또 finishWithSuccess의 Object.freeze(data)가 클라이언트가 돌려준 원본 응답을 직접 동결하고 있어. 처리 전 Object.isFrozen(originalData)는 false, 처리 후에는 true가 됐어. 이는 원본 응답을 변경하지 않는 계약과 어긋나.

수정 요구:

- 코디네이터 소유 상태와 외부 입력/엔진 원본 응답의 소유권을 분리해 줘.
- pending 요청 원소/paths, lastSuccess/healthScope/data/skipped/conflicts/unsharded, unverifiedPriorHealth 및 실패 정보가 스냅숏을 통해 내부에서 변경되지 않게 해 줘.
- 독립 복사·동결 등 적절한 방법을 사용하되 원본 응답·원본 배열·추가 키를 직접 동결하거나 수정하지 마.
- 원본을 보유한 호출자가 나중에 원본 응답의 중첩 배열을 바꿔도 보관된 보고서와 이후 스냅숏이 오염되지 않도록 해 줘.
- 새로 받은 스냅숏의 수정 시도뿐 아니라 이전 스냅숏 보존 및 구독자 간 격리도 검증해 줘.
- 실패에는 raw.stdout/stderr: Uint8Array, Error 및 cause:unknown이 올 수 있어. non-empty Uint8Array에 무조건 Object.freeze를 적용하거나 JSON 직렬화로 타입·바이트·원인 정보를 손실시키지 마. 외부가 바꿀 수 있는 바이트는 내부 소유 데이터와 공유하지 않는 등 구체적인 정책을 정해 줘.
- 특이한 추가 키나 원인 값의 복사가 지원되지 않으면 성공한 엔진 실행을 조용히 다른 결과로 바꾸지 말고, 지원 범위와 로컬 오류 정책을 명확히 해 줘. 불필요한 범용 직렬화 프레임워크는 만들지 마.

회귀 테스트:

- snapshot의 pending 원소/paths 변경이 이후 실행 계획과 내부 세대에 영향 없음.
- 성공 보고서의 시각·healthScope·skipped 원소/배열·conflicts·unsharded·이전 건강 경고 수정이 내부와 다른 구독자/스냅숏에 영향 없음.
- 원본 응답이 처리 전후 동결되지 않고 내용·추가 키가 유지됨.
- 호출자 보유 원본의 변경이 이미 저장한 보고서에 영향 없음.
- 실패 보고서의 원본 바이트와 원인 보존, 스냅숏으로부터 내부 바이트 오염 방지, TypedArray 동결 오류 없음.

## 범위 및 완료 기준

- 변경은 8-2 예약 수명·스냅숏 소유권과 관련 테스트·README로 제한해 줘. 기존 209개 동작을 유지하고 회귀 테스트를 추가해 줘.
- 세대 추적, 실행 중 새 pending, 실패 정지·명시적 복구, vault 격리, healthScope와 이전 건강 안내, paths 안전성·61/63 예산 계약을 유지해 줘.
- 8-3 파일 감시·UI·스토어, 실제 엔진/DB/소켓, 7-1 쓰기 정책 보완은 함께 구현하지 마.
- npm test, npm run build를 실행하고 이번 변경으로 발생한 실패를 해결해 줘. 실제 sleep 대신 deferred mock·수동 스케줄러를 사용해 줘.
- README와 공개 사용 예시가 실제 API·지원 범위와 맞게 갱신돼야 해. 필요한 예시는 타입 검사와 실행 테스트를 함께 해 줘.
- /Users/df/Downloads/aisurada 엔진 해부도.html을 실제 파일명 확인 후 백업하고 app-progress/app-review-8-2에 보완 결과·최신 검증 수치를 반영해 줘. 기존 Java 엔진 본문·도표·앵커·스크립트는 유지하고 8-3은 후속 계획으로 남겨 줘. 저장소 HTML 사본도 동기화해 줘.
- 원격 Claude 문서 갱신이나 공식 수용 묶음 통과를 주장하지 마.
- 최종 보고에는 재현별 수정 결과, 예약 무효화와 복사/바이트/원인 보존 정책, 변경 파일, 테스트·빌드 결과, HTML 반영 여부, 남은 제한을 적어 줘.

계획만 제안하고 멈추지 말고 실제 구현과 검증을 끝까지 완료해 줘.
