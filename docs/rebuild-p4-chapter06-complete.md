# 4부 6장(PROJECTS 레일) 구현 및 검증 완료 보고서

2026-11-04 2차 리뷰에 따라 동작 설명을 정정했다. 아래 과거 테스트 수와 출력은 당시 실행 기록이며, 최신 코드의 검증 결과는 [2차 리뷰 반영 기록](rebuild-p4-second-review-complete.md)을 따른다.


## 1. 개요 및 구현 범위
- **설명서**: withlog 4부 Claude 연동 설명서 6장 (PROJECTS 레일) 및 부록 C 결함 고침 (D14, D15).
- **목표**:
  - 네이티브 호스트 커맨드 4개 구현:
    - `list_claude_agents`: `claude agents --json` 실행 (10초 상한, SIGTERM/SIGKILL, 호스트 환경 그대로).
    - `alive_pids`: `ps -p <pids> -o pid=` 실행 및 D14 결함 고침 (5초 상한 및 타임아웃 종료).
    - `held_agent_sessions`: 데몬 세션 조회 (기본 길 `[]` 반환).
    - `release_agent_session`: 데몬 세션 해제 (기본 길 `"not_live"` 반환).
  - 프론트엔드 React 컴포넌트 및 스토어 구현:
    - `native-host/src/plugs/claude/projects.tsx`:
      - `RailProjects` 컴포넌트: `[data-rail-section="projects"]` 영역.
      - 6.1 한 번 읽는 차례 파이프라인 (목록 파싱, 4가지 병렬 호출, 끝난 행 결합).
      - 6.8 아젠다 매칭 (repo와 cwd의 접두사 및 최장 일치, 안 붙은 세션 수 카운트).
      - 6.9 이레 접기 (놓는 시각 7일 기준 분리, `"오래된 세션 N개 보기"` 토글) 및 프로젝트 접기 (`sess:<slug>`, `sess:(미분류)`).
      - 6.10 세션 줄 렌더링: 제목, 실행 상태(running 시 `.sess-run` 점, blocked 시 `멈춤`), 메타(지난 시간 글, 종류: 초록 `끝남` `.sess-ended`, `백그라운드`, `터미널`, `알 수 없음`, 브랜치), 복사 단추(`claude --resume <sessionId>`), 클릭 시 `/session/<sessionId>` 이동.
      - D15 결함 고침: `recent-sessions`의 `dropped > 0`일 때 `.sess-note[data-note="ended-dropped"]`로 `"이레 안에 끝난 세션 중 오래된 N개는 안 보여요. 한 번에 읽는 수를 넘었어요."` 표시.
      - 6.3 건너뛴 줄 수: `"모양이 아니라 건너뛴 줄 N개"`.

---

## 2. 세부 구현 내용

### 1) Rust 호스트 (`native-host/src-tauri/src/agents.rs` & `main.rs`)
- `list_claude_agents`:
  - `resolve_claude()`로 claude 실행 바이너리 탐색.
  - `cmd.arg("agents").arg("--json")` (`--all` 제외, 호스트 PATH 그대로 사용).
  - 10초 상한 초과 시 `process::run_typed`가 SIGTERM, 3초 유예, SIGKILL과 자식 회수를 수행 및 `"10초 안에 세션 목록이 안 왔다."` 반환.
  - 오류 발생 시 표준오류 마지막 400자 꼬리 반영.
- `alive_pids`:
  - **D14 결함 고침**: 5초 상한 초과 시 `process::run_typed`가 같은 3초 유예 규칙으로 ps를 거둠 및 `Err("ps가 5초 안에 안 끝났다.")` 반환.
  - 표준출력 줄 파싱으로 유효 pid 목록 반환.
- `held_agent_sessions` & `release_agent_session`:
  - 계약에 따라 각각 `Ok(vec![])`와 `Ok("not_live")` 반환.
- `main.rs`: `invoke_handler`에 4개 커맨드 등록.

### 2) React / TypeScript (`native-host/src/plugs/claude/projects.tsx`)
- `refreshSessions`:
  - 브라우저 환경(미리보기) 안전 격리 (`!isTauri()` 시 `no-bridge` 메시지).
  - 필수 필드(`sessionId`, `cwd`, `startedAt`) 검증 및 비정상 행 건너뜀 카운트.
  - `alive_pids`, `session-titles`(200개 청크), `held_agent_sessions`, `recent-sessions` 동시 병렬 질의.
  - `recent-sessions` 응답에서 `dropped` 카운트 추출 및 중복 없는 끝난 대화형 세션(`isEndedRow: true`) 덧붙이기.
- `RailProjects`:
  - `[data-rail-section="projects"]` 헤더(새로고침 버튼) 및 에러 줄 렌더링.
  - 아젠다 매칭: 최장 접두사 매칭, 미매칭 세션은 하단에 `"아젠다에 안 붙은 세션 N개 — 그 저장소 아젠다에 경로를 적으면 여기 붙어요."`로 집계.
  - 7일 기준 나이 가르기 및 `"오래된 세션 N개 보기"` 인라인 토글.
  - 클립보드 복사(1.6초 `"복사됨"` 피드백) 및 `/session/<sessionId>` 네비게이션.

### 3) 스타일 (`native-host/src/styles.css`)
- `.rail-projects-section`, `.sess-rail-head`, `.sess-note`, `.sess-project`, `.sess-toggle`, `.sess-agenda-head`, `.sess-row`, `.sess-item`, `.sess-title`, `.sess-run`, `.sess-meta`, `.sess-ended`, `.sess-branch`, `.sess-copy`, `.sess-more` 등 6장 CSS 규격 일체 추가.

---

## 3. 검증 결과

1. **Rust 네이티브 호스트 테스트**:
   - `cargo test --features claude`: 78개 테스트 통과 (agents 빈 pid, 자기 자신 pid 검사 등 포함).
2. **TypeScript / 화면 테스트**:
   - `pnpm -C native-host typecheck`: 컴파일 성공 (0 errors).
   - `pnpm -C native-host exec node --import tsx --test scripts/chapter06.test.mjs`: 5개 테스트 통과(1차 반영본 기준). 실제 재실행 결과는 2차 리뷰 반영 기록을 따른다.
   - `bash test-native.sh`: screen-contracts, workspace, navigation, data, editor, views, autosave, chapters 전체 회귀 없이 통과.
3. **Java 백엔드 통합 테스트**:
   - `./test.sh`: 전체 1~3부 테스트 및 4부 `AskCommandsTest`, `SessionCommandsTest` 전체 통과.
