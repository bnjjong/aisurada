# 오늘 작업 전반 재검토 — 2026-10-28

**후속 보완 완료:** 아래 네 건을 수정했으며 24개 파일·587개 테스트와 빌드가 통과했다. [수정 결과와 검증 범위](./review-workspace-fixes-2026-10-28.md)를 따른다. 아래 본문은 수정 전 재현 이력이다.

검토 기준: `8a19947fb15f065de13976d78146852c94adce78`의 현재 소스와 오늘 작성한 계획·검토 기록. 이 커밋은 워크스페이스 전체를 처음 등록한 커밋이므로 각 결함이 오늘 새로 생겼다고 단정하지 않는다. 6·7장 응답/토큰·실패 정책, 8장 재조정/복제, 12·14·16장 클라이언트, 로컬 호스트와 16장 화면의 연결을 확인했다.

**결론: 기존 테스트·빌드는 통과하지만 아래 P2 문제 네 건은 보완이 필요하다.** 애플리케이션 소스를 수정하지 않고 재현과 검토 결과를 기록했다.

## 1. [P2] 화면 모드 변경으로 같은 vault의 reindex를 중복 실행할 수 있다

위치: [`SubNotesPanel.tsx:48`](../src/components/SubNotesPanel.tsx#L48), [`SubNotesPanel.tsx:84`](../src/components/SubNotesPanel.tsx#L84), [`engineHost.ts:114`](../host/engineHost.ts#L114).

`refreshIndex`는 새 클라이언트로 reindex를 직접 호출한다. 갱신 중에도 본문 옵션을 바꿀 수 있고, 옵션 변경 effect가 `indexEpoch`를 증가시키면서 `indexing=false`로 되돌린다. 실행 중인 프로세스는 계속 살아 있는데 버튼이 다시 활성화된다. 호스트에도 vault별 직렬화가 없다.

8초 지연하는 격리 시험용 실행 파일과 실제 브라우저·HTTP 호스트로 다음을 재현했다.

1. 색인 갱신 클릭 직후 버튼 disabled=true.
2. 본문 미리보기 전환 뒤 첫 프로세스가 끝나기 전에 disabled=false.
3. 갱신을 다시 클릭하면 같은 vault의 두 reindex 프로세스가 동시에 실행된다. 첫 요청의 완료 처리는 epoch가 달라 폐기된다.

관찰된 동시 실행 수는 2다. 프로세스 시작 시각은 1,981ms 차이이며 각각 약 8초간 실행됐다. [브라우저 재현 근거](./review-repros/2026-10-28-browser-evidence.json)에 기록했다. 실제 DB 손상은 재현하지 않았으며, 중복 작업과 색인 갱신 완료/신선도 판단 경합이 확인된 문제다.

수정 방향: 화면 모드와 별도로 vault별 실행 수명을 관리하고 기존 ReindexCoordinator를 연결한다. 여러 화면/탭의 호출도 허용할 경우 호스트 경계에서도 직렬화 또는 중복 병합을 보장한다. 진행 중 모드·부모·vault 전환 및 두 화면의 갱신 경합을 검증한다.

## 2. [P2] 토큰에 바인딩한 대상 문맥이 외부 객체 변경으로 바뀐다

위치: [`preconditions.ts:100`](../src/engine/write/preconditions.ts#L100).

`createScopedToken`은 전달받은 target을 복사하지 않고 그대로 저장한다. 호출자가 보관한 변경 가능한 객체를 다음 노트/vault로 재사용하면 기존 토큰의 소유 대상까지 바뀐다. 원래 다른 대상에서 읽은 토큰인데도 `combinePrecondition`의 문맥 대조를 통과한다. 입력 객체는 TypeScript의 readonly 매개변수에 정상 전달 가능한 변경 가능한 객체이므로 타입 우회가 없어도 발생한다.

```typescript
const target = { kind: 'root_note' as const, vaultId: 'vault-a', docId: 'note-a' };
const token = createScopedToken(target, 'token-for-note-a');
target.vaultId = 'vault-b';
target.docId = 'note-b';
combinePrecondition({ id: 'note-b' }, token,
  { kind: 'root_note', vaultId: 'vault-b', docId: 'note-b' });
// 검토 당시: 오류 없이 { id: 'note-b', f1: 'token-for-note-a' }
```

엔진의 실제 토큰 비교까지 우회했다고 주장하는 결과는 아니다. 앱에서 제공하는 vault/노트 대상 격리가 깨져 잘못된 요청을 조기에 막지 못한다. 수정 시 문맥을 독립 사본으로 만들고 반환 객체와 문맥을 동결한다. 원본 target은 동결하지 않는다. 루트·하위 노트·vault 변경 및 원본/반환값의 변경 경계를 검증한다.

## 3. [P2] running 통지 중 dispose하면 폐기 뒤에도 엔진 호출이 시작된다

위치: [`coordinator.ts:161`](../src/engine/indexing/coordinator.ts#L161), [`coordinator.ts:344`](../src/engine/indexing/coordinator.ts#L344).

실행 직전 running 상태를 통지한 뒤 `isDisposed`를 다시 검사하지 않고 getClient/reindex를 호출한다. 구독자가 running 통지에서 dispose하는 경우, 아직 전달하지 않은 엔진 요청이 폐기 뒤 시작된다. 같은 통지의 구독자 배열 사본도 계속 순회해 다른 구독자에게 폐기 후 알림이 한 번 전달된다.

재현 결과는 `dispatched=1`, `notifiedAfterDispose=1`이다. 이미 실행 중인 네이티브 프로세스를 취소하지 않는 기존 정책과는 다르다. 이 경우 dispose 당시 엔진 호출은 아직 시작하지 않았다.

수정 방향: 통지에서 돌아온 뒤, 주입된 콜백을 거친 뒤 실제 호출 직전에 폐기/실행 식별자를 확인한다. 각 구독자 호출 사이에도 폐기를 확인한다. running 구독자의 dispose, 여러 구독자, getClient 콜백의 재진입을 회귀 검증한다.

## 4. [P2] 잘못된 UTF-8 성공 응답을 대체 문자로 바꿔 정상 처리한다

위치: [`decodeResponse.ts:155`](../src/engine/decodeResponse.ts#L155).

stdout을 `TextDecoder(..., { fatal: false })`로 해석한다. JSON 문자열 내부의 잘못된 UTF-8 바이트가 U+FFFD로 치환되면 JSON 파싱과 문자열 validator를 모두 통과한다. `{"body":"<0xFF>","precondition":"token"}` 바이트를 주입했을 때 `kind='success'`, `body='�'`를 반환했다.

원본 바이트를 보관하더라도 소비자가 사용하는 data.body는 이미 바뀌었다. 손상된 본문과 정상처럼 보이는 토큰을 같이 소비할 수 있다. 실제 엔진에서 이런 응답을 관찰하거나 파일이 손상된 것은 아니며, 전송/출력 손상을 성공으로 취급하는 공통 디코더의 경계 문제다.

수정 방향: 성공 stdout의 UTF-8 디코딩을 엄격하게 수행하고 실패를 계약 오류로 분리한다. 원본 바이트는 보존한다. 유효한 U+FFFD 본문은 허용하면서 잘못된 바이트, 다중 바이트 중간 절단, 정상 한글/보충 문자와 구별하는 테스트가 필요하다. stderr의 진단 표시 정책은 성공 stdout과 별도로 정한다.

## 검증 및 범위

- 기본 `npm test`: 21개 파일·552개 통과, 실제 JAR 통합 6개 skip.
- JAR/Java 실행 경로를 명시한 전체 `npm test`: **22개 파일·558개 모두 통과**.
- `npm run build`: 프런트·호스트 TypeScript와 Vite 빌드 통과.
- 위 네 문제의 별도 재현은 기존 테스트의 통과와 별개다. 2~4번은 [독립 재현 스크립트](./review-repros/2026-10-28.mjs)로 확인했다. `node docs/review-repros/2026-10-28.mjs`로 실행할 수 있으며 실제 엔진/파일 쓰기는 호출하지 않는다.
- 1번은 지연하는 자체 시험 프로세스와 브라우저로 재현했다. 실사용 vault는 사용하지 않았다.
- 7-1의 미분류 쓰기/종료-WL 조합 분류, 8-2의 중첩 JSON·Error·바이트 격리, 14-1의 미지원 키 차단 보완은 현재 코드와 회귀 테스트에서 유지됨을 확인했다. 전체 기능이나 모든 가능한 입력의 무결성을 증명하는 검토는 아니다.
- 원본 SQLite 엔진의 공식 수용 묶음, 실제 노트 저장·이동/복구는 이번에도 실행하지 않았다. Git 상태와 HTML 사본 동기화, 기존 앵커/스크립트 보존도 확인했다.

우선순위는 실제 화면의 중복 색인 실행을 먼저 막고, 토큰 문맥 격리와 폐기 경계를 보완한 뒤 공통 stdout 디코딩을 강화하는 순서다. 수정 후에는 이번 재현을 회귀 테스트로 추가한다.
