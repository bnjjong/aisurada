# 2장 문답 표식과 문답 파일 재구현 및 검증 보고서

## 1. 개요 및 목적
- **대상**: withlog 4부 Claude 연동 설명서 2장 「문답 표식과 문답 파일」
- **핵심 목표**:
  1. 본문 문답 표식(`<!--wla:<id>-->...<!--/wla:<id>-->`)과 밑줄 id 규칙 정의.
  2. 노트 폴더의 문답 파일(`ask.json`) 포맷 및 1MB 상한 계약 준수.
  3. 엔진 명령 `get-ask` 및 `append-ask-turn`의 원자적 쓰기 구현.
  4. 부록 C의 **D1 결함 고침** (1MB 상한 초과 시 `WL-500` 사전 거절 및 기존 파일 보존) 반영.
  5. 기존 1부(`test.sh`), 2~3부(`test-native.sh`) 회귀 없이 100% 테스트 통과.

---

## 2. 주요 계약 및 구현 내용

### 2.1 밑줄 id와 문답 표식 (2.1)
- **밑줄 id 규칙**: `ak<시각36진수>-<무작위 UUID 앞 8자>` (예: `akmhj0q2rs-9c1e5a7b`)
- **본문 표식**: `<!--wla:<id>-->고른 본문 글<!--/wla:<id>-->`
- **이어 묻기**: 새 `ak...` id를 부여받으며 본문 표식은 생성하지 않고 `markId`로 첫 밑줄을 가리킴.
- **HTML 상자**: 첫 물음은 `markId`와 회차 `id`가 `html:<첨부경로>`이고 `quote`는 `HTML문서:<라벨>`.

### 2.2 문답 파일 자리 (2.2)
- 뿌리 노트: `<노트폴더>/ask.json`
- 하위 노트: `<그하위노트폴더>/ask.json`
- 단일 파일 하위 노트는 거절 (`WL-500` - "폴더로 안 올라간 단일 파일 하위 노트에는 문답을 두지 않는다").

### 2.3 ask.json 포맷 (2.3) 및 D1 크기 상한 계약 (2.5)
```json
{
  "v": 1,
  "noteId": "ndm1abcd-00000001",
  "sessionId": "3f2a9c1e-5b7d-4e8f-9a0b-1c2d3e4f5a6b",
  "turns": [
    {
      "id": "akmhj0q2rs-9c1e5a7b",
      "markId": "akmhj0q2rs-9c1e5a7b",
      "askedAt": "2026-11-03T02:15:40.512Z",
      "quote": "고른 글",
      "question": "질문",
      "answer": "답변",
      "model": "claude-sonnet-5-5",
      "effort": "low",
      "elapsedMs": 7600,
      "sessionLines": 21,
      "images": ["attach/ask-01.png"]
    }
  ]
}
```
- **너그러운 파싱 규칙**:
  - `v`: 읽을 때는 버리고, 쓸 때 항상 1로 저장.
  - `noteId`, `sessionId`: 없으면 기본값 보충.
  - `turns`: 배열이 아니면 빈 배열로 처리, 객체가 아닌 원소는 무시.
  - `quote`, `error`: 빈 문자열이면 없는 키로 간주하여 필터링.
  - `images`: `attach/<이름>` 형식만 허용 (최대 8장).
- **D1 고침 (cc12691b)**:
  - 파일 크기 상한은 1,000,000 바이트 (1MB).
  - 회차 추가 후 전체 파일 직렬화 바이트가 1,000,000 바이트를 초과하면 `WL-500`("문답 파일이 상한을 넘어 붙일 수 없다: 붙이면 N바이트다. 상한은 1000000바이트다.")으로 사전 거절.
  - 임시 파일(`ask.json.<uuid>.tmp`)에 먼저 쓰고 원자적 교체(`ATOMIC_MOVE`)를 수행하여 기존 `ask.json` 파일의 무결성 보존.

### 2.4 엔진 명령 구현

#### `get-ask <노트 id> [하위 노트 id]`
- 파일이 없으면 `null` 반환 (종료 코드 0).
- 파일이 있으면 `v`와 `path`를 제외한 `{noteId, sessionId, turns}` 반환.

#### `append-ask-turn <노트 id> [하위 노트 id] session-id=... turn-id=... mark-id=... asked-at=... [quote=...] question=... answer=... [error=...] model=... effort=... elapsed-ms=... [session-lines=...] [images=...]`
- 파일이 없으면 새 실타래를 생성하고 전달받은 `session-id` 사용.
- 파일이 이미 존재하면 파일 내부의 `sessionId`가 우선(이김).
- 반환값: 갱신된 실타래 전체 + `"path": "<vault 상대 ask.json 경로>"`.

---

## 3. 검증 결과

### 3.1 2장 단위 및 계약 검증 (`withlog.AskCommandsTest`)
- 실행: `java -cp dist/wl-engine.jar:build/test-classes withlog.AskCommandsTest`
- 검증 항목 (총 46 checks passed):
  1. 인자 누락 검증: `get-ask`, `append-ask-turn` 필수 인자 및 포맷 검사 (`WL-400`).
  2. 존재하지 않는 노트 조회 검증 (`WL-404`).
  3. 빈 실타래 조회 시 `null` 반환 검증.
  4. 첫 턴 추가 및 `path` 필드 포함 확인.
  5. `get-ask`로 읽은 데이터에서 `v`와 `path` 제거 확인.
  6. 둘째 턴 추가 시 기존 `sessionId` 유지 확인 (기존 세션 우선).
  7. 20,000자 상한 초과 인자 검사 (`WL-400`).
  8. **D1 고침 검증**: 990,000바이트 이상의 `ask.json`에 턴 추가 시 `WL-500` 발생 및 기존 파일 100% 보존 확인.

### 3.2 1부 전체 회귀 테스트 (`./test.sh`)
- 모든 기존 1부 챕터(19, 22, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 소켓 서버) 및 `AskCommandsTest` 통과 (종료 코드 0).

### 3.3 2~3부 호스트 회귀 테스트 (`bash test-native.sh`)
- Rust 테스트 71건, Node acceptance 테스트 20건, Vite noclaude 빌드 모두 통과 (종료 코드 0).

---

## 4. 커밋 및 형상 관리
- `src/withlog/ask/AskCommands.java`: 2장 명령 및 파서 구현.
- `src/withlog/engine/EngineExecutor.java`: 4부 스위치 및 라우팅 추가.
- `test/withlog/AskCommandsTest.java`: 2장 통합 검증 테스트.
- `test.sh`: 4부 문답 검증 단계 추가.
