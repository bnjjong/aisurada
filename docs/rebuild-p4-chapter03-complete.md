# 3장 묻기 한 번 재구현 및 검증 보고서

## 1. 개요 및 목적
- **대상**: withlog 4부 Claude 연동 설명서 3장 「묻기 한 번」
- **핵심 목표**:
  1. 편집기 단추 줄, 표 줄 단추, HTML 상자 머리에서 「AI에게 묻기」 진입 구현.
  2. 묻는 칸 팝오버(`AskComposePopover`), 모델 고르개, effort 고르개, 그림 첨부(최대 8장, 5MB 제한).
  3. 물음 글 조립: 시스템 프롬프트, 대상 노트 ID 줄, 아젠다/저장소, 작성 세션 대화 묶음, 물은 대목, 질문.
  4. 부록 C의 **D4 결함 고침** (실패 회차 발생 시에도 작성 세션 대화 누락 방지: 성공한 회차가 0개일 때 세션 대화 묶음 포함).
  5. 노트별 독립 FIFO 큐 (`noteQueues`)를 통한 동일 노트 대화 갈라짐 방지.
  6. 브라우저 저장소(`withlog.ask.v1`)를 통한 묻는 중(pending), 미확인(unseen), 버려진 물음(abandoned) 상태 관리.

---

## 2. 주요 계약 및 구현 내용

### 2.1 묻는 칸 및 옵션 선택 (`askPopover.tsx`)
- **모델 목록 (3.3)**:
  - `claude-sonnet-5-5` ("Sonnet 3.5 / 5.5") [기본값]
  - `claude-haiku-5-5` ("Haiku 3.5 / 5.5")
  - `claude-opus-5-5` ("Opus 3.5 / 5.5")
- **Effort 목록 (3.3)**:
  - `low` ("낮음") [기본값]
  - `medium` ("보통")
  - `high` ("높음")
- **그림 첨부 (2.8, 3.2)**:
  - 최대 8장, 5,000,000바이트 이하 파일만 허용.
  - 지원 확장자: png, jpg, gif, webp, avif, bmp, svg.
  - 첨부 시 `write_attachment_bytes`로 `attach/ask-<time36>-<uuid8>.<ext>` 저장 후 상대경로 전달.

### 2.2 물음 글(프롬프트) 조립 및 D4 결함 고침 (3.4, 3.6, 3.7)
- **페르소나 / 시스템 프롬프트**:
  `너는 사용자의 withlog 노트 안에서 질문을 받는다. 노트는 「<제목>」(<자리>)이다.`
- **프롬프트 본문 구조**:
  ```markdown
  노트: <노트id>
  아젠다: <slug>

  ## 작성 세션 대화
  ### 사용자
  ...
  ### Claude
  ...

  ## 물은 대목
  > <고른 대목>

  ## 질문
  <질문 내용>
  ```
- **D4 결함 고침 (c5b929ea)**:
  - 기존에는 단순히 회차 개수(0개)로만 작성 세션 묶음 여부를 결정하여 첫 물음이 실패(그림 저장 실패, 프로세스 종료 등)하면 둘째 물음에서 세션 대화가 영구 누락되던 결함 수정.
  - **새 규칙**: 성공한 회차(`!turn.error && turn.answer`)의 수가 0개일 때만 작성 세션 대화를 싣도록 개선.

### 2.3 줄 서기 (3.5) 및 상태 관리 (`state.ts`)
- **노트별 FIFO 큐 (`enqueueNoteTask`)**:
  - 동일한 노트에 들어온 질문은 앞선 질문의 응답 및 파일 저장이 완료될 때까지 비동기 큐에서 대기 (`queued: true`).
  - 다른 노트의 질문은 병렬로 독립 처리.
- **`withlog.ask.v1` 로컬 스토리지 관리 (3.11)**:
  - `pending`: 질문 시작 시 즉시 등록, 완료/실패 시 해제.
  - `unseen`: 응답 도착 시 미확인 점(dot) 표시용 플래그.
  - `abandoned`: 앱 재시작 시 미완료된 `pending` 항목을 버려진 물음으로 자동 전환.

---

## 3. 검증 결과
- **테스트 파일**: `native-host/scripts/chapter03-04.test.mjs`
  - `Chapter 3: State module reflects Claude enabled and maintains pending counters`: 통과
  - `Chapter 3: Underline ID conforms to ak<time36>-<uuid8> specification`: 통과
  - `Chapter 3: Model and effort options conform to 3.3 specification`: 통과
  - `Chapter 3 & 4: Unsaved answers counter tracking`: 통과
- **호스트 회귀 테스트 (`bash test-native.sh`)**:
  - Vite `noclaude` 빌드에서 계약(4부 명령 누출 0건) 유지 확인.
  - Vite 일반 빌드에서 4부 플러그(`src/plugs/claude`) 컴파일 및 번들링 성공.
  - 전체 Rust 76건, TypeScript 100% 통과.
