# 8-2 보완 재검토 — 중첩 격리 추가 보완 필요

검토일: 2026-10-28

## 확인된 해결과 검증

- npm test: 14개 파일·219개 테스트 통과.
- npm run build: TypeScript·Vite 빌드 성공.
- cancel 없는 스케줄러에서 requestRefresh → flush 실패 → 옛 callback을 실행하는 독립 재현을 다시 수행했다. 호출 1회와 failed 상태가 유지되어 예약 무효화 문제는 해결됐다.
- 필수 성공 필드와 pending 원소 동결, 원본 data 직접 동결 제거, getSnapshot 간 실패 바이트 복사도 반영됐다.

다만 스냅숏 격리를 모두 완료했다고 판단할 수는 없다. 아래는 기존 요구에 포함된 중첩 추가 키·cause·구독자 격리의 남은 경계다. 새로운 기능 요구가 아니다.

## 1. 성공 응답 추가 키의 중첩 객체가 공유됨

위치: src/engine/indexing/coordinator.ts의 cloneReindexResponse(89행).

`...source` 및 `{ ...item }`은 얕은 복사다. 추가 필드에 `{ extra: { nested: { value: 'original' } } }`가 포함된 정상 성공 응답을 보관한 뒤 `snapshot.lastSuccess.data.extra.nested.value = 'MUTATED'`를 실행하면 이후 스냅숏과 클라이언트 원본 모두 MUTATED로 바뀌었다.

추가 키는 순수 JSON 중첩 객체만으로도 이 문제가 재현된다. 응답 루트와 항목의 추가 키를 보존하면서 내부 소유 사본으로 분리해야 한다. 알려진 필드만 재구성해 추가 키를 버리거나 원본 중첩 객체를 동결해서는 안 된다.

## 2. 실패 cause/detail 등 중첩 값이 공유됨

위치: 같은 파일의 cloneRefreshFailure, 특히 coordinator_error 가지(157행).

getClient가 `{ context: { value: 'original' } }`를 던지는 경우, 실패 스냅숏의 cause.context.value를 바꾸면 내부 실패 상태와 원래 던진 객체도 바뀌었다. cause를 그대로 복사하고, contract violation의 detail 등도 얕게 복사하기 때문이다.

원인 보존과 참조 공유는 다른 요구다. 지원할 값과 정책을 명시하고, Error의 name/message/stack/cause 및 사용자 속성을 잃지 않으면서 원본과 스냅숏의 변경이 내부로 전파되지 않게 분리해야 한다. JSON 직렬화로 Error/Uint8Array를 손실시키는 방식은 피한다.

## 3. 한 통지에서 구독자들이 mutable 바이트를 공유함

위치: 같은 파일의 notifySubscribers(278행 부근).

createSnapshot을 한 번 호출한 뒤 모든 리스너에 같은 객체를 전달한다. Uint8Array는 의도적으로 동결하지 않아 앞 구독자가 실패 raw.stderr.fill(65)를 실행하면 다음 구독자도 원문 대신 A로 채워진 바이트를 받았다.

내부 상태의 바이트는 이후 getSnapshot 복사로 보호되지만 구독자 간 격리는 충족하지 못한다. 변경 가능한 바이트·원인 값은 수신자별 독립 사본으로 전달하거나 동등한 격리 계약으로 구현해야 한다.

## 후속 범위

예약 id 수정은 유지한다. 남은 작업은 응답 추가 키·실패 중첩 정보·수신자별 바이트 격리와 회귀 테스트다. 8-3 UI·실제 감시·DB와 7장 보완을 섞지 않는다. 사용자가 지정한 HTML에는 219개 테스트·빌드 통과와 예약 결함 해결을 기록하되 중첩 격리는 추가 보완 필요로 구분한다.

이번 재검토에서는 애플리케이션 소스를 수정하지 않았다.
