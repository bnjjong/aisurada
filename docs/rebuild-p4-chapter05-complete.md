# 5장 claude -p 기본 길 재구현 및 검증 보고서

2026-11-04 2차 리뷰에 따라 동작 설명을 정정했다. 아래 과거 테스트 수와 출력은 당시 실행 기록이며, 최신 코드의 검증 결과는 [2차 리뷰 반영 기록](rebuild-p4-second-review-complete.md)을 따른다.


## 1. 개요 및 목적
- **대상**: withlog 4부 Claude 연동 설명서 5장 「claude -p 기본 길」
- **핵심 목표**:
  1. 호스트 커맨드 `ask_runtime` 및 `ask_claude` 구현.
  2. 비동기 I/O를 방해하지 않도록 async worker 밖의 전용 스레드 풀에서 동기 프로세스 실행.
  3. 프롬프트를 CLI 인자가 아닌 표준입력(stdin) 파이프로 전달.
  4. 세션 플래그 불일치(`is already in use` ↔ `No conversation found`) 시 1회 플래그 뒤집기 재시도.
  5. 타임아웃 시 SIGTERM 전송 후 3초 유예, 미종료 시 SIGKILL 강제 회수.
  6. 부록 C의 **D12 결함 고침** (에러 첫 줄 + 꼬리 400자 조합)을 통해 장문의 정책 경고 발생 시에도 원인 보존.

---

## 2. 주요 계약 및 구현 내용

### 2.1 호스트 커맨드 인터페이스
- `ask_runtime`: 현재 구현은 환경변수와 무관하게 항상 `"cli"`를 반환한다.
- `ask_claude`:
  - 파라미터: `vault`, `session_id`, `model`, `effort`, `prompt`, `resume`, `timeout_seconds`, `keep_alive`, `seed`, `system`, `route`, `cwd`, `resume_only`, `persona`
  - 반환값: `{ ok: bool, answer: string, error?: string, elapsed_ms: number, missing_cwd?: string }`
  - 작업 폴더 결정: `cwd`가 유효한 절대경로 폴더가 아니면 `vault`로 물러서며 `missing_cwd`를 결과에 기재.

### 2.2 CLI 인자 및 자식 환경 구성 (5.2, 5.3)
- **인자 목록**:
  `-p --session-id <대화id> --model <모델id> --effort <effort> --permission-mode auto --permission-prompts none [--append-system-prompt <페르소나>]`
  (이어 묻기 또는 뒤집힌 경우 `--resume <대화id>` 사용)
- **표준입력(stdin)**:
  - 프롬프트 내용은 명령행 인자에 포함되지 않고 stdin 파이프로 기록 후 즉시 flush/close.
  - 새 대화에 `seed` 메시지가 제공된 경우 `## 앞 대화` 마크다운 블록을 조립하여 주입.
- **자식 환경변수**:
  - `WITHLOG_VAULT`: 노트 보관소 절대경로.
  - `PATH`: `~/.local/bin`, `/opt/homebrew/bin`, `/usr/local/bin`이 앞단에 보강된 PATH.

### 2.3 플래그 뒤집기 (5.4)
- 첫 번째 실행 실패 시 표준오류(stderr) 판정:
  - `is already in use`가 포함되고 기존이 `--session-id`였던 경우: `--resume`으로 뒤집어 1회 재실행.
  - `No conversation found`가 포함되고 기존이 `--resume`이며 `resume_only`가 아닌 경우: `--session-id`로 뒤집어 1회 재실행.
- `resume_only: true`인 물음이 세션 미존재로 실패한 경우:
  `이 기계에서 그 세션의 대화 기록을 못 찾아서 안 보냈다. 새 대화로 바꿔 보내면 앞 맥락 없는 대화가 같은 id 로 생긴다.` 문구를 에러 앞에 전치.

### 2.4 타임아웃 및 프로세스 강제 종료 (5.6)
- 지정된 `timeout_seconds` 경과 시:
  1. 공통 `process::run_typed`가 `/bin/kill -TERM`을 보낸다.
  2. 3초간 정상 종료 대기 (`TERM_GRACE`).
  3. 여전히 미종료 시 `Child::kill`로 강제 종료하고 자식을 거둔다.
- 에러 메시지: `N초 안에 답이 안 왔다. 터미널에서 claude --resume <대화id> 로 이어 볼 수 있다.`

### 2.5 D12 결함 고침: 에러 문구 추출 (5.8)
- 표준오류/표준출력의 길이가 400자를 초과할 때 단순 꼬리(tail) 400자만 취하면, 첫 줄의 핵심 실패 원인이 뒤이은 수백 자의 회사 보안/MCP 정책 경고 문구에 잘려나가는 결함 해결.
- **새 규칙**: 첫 줄이 꼬리 400자 안에 이미 포함되어 있지 않은 경우, `첫줄(최대 400자)\n…꼬리 400자` 형태로 합성하여 반환.

---

## 3. 검증 결과

### 3.1 호스트 단위/통합 테스트 (`ask::tests`)
`cargo test --manifest-path native-host/src-tauri/Cargo.toml -- ask::tests`
- **11개 테스트 전수 통과**:
  1. `test_augmented_path_structure`: Homebrew 및 local bin PATH 확장 검증.
  2. `test_ask_route_default_is_cli`: 기본 런타임 `"cli"` 반환 검증.
  3. `test_determine_work_dir_fallbacks`: `cwd` 유효성 및 vault fallback 검증.
  4. `test_format_cause_under_400_chars`: 400자 이하 일반 에러 포맷 검증.
  5. `test_format_cause_first_line_and_tail_400`: 400자 초과 시 첫 줄 및 생략 부호 포맷 검증.
  6. `test_resolve_claude_with_env_var`: 환경변수 우선순위 탐색 검증.
  7. `test_fake_claude_success_stdin_and_args`: 가짜 스크립트로 stdin 전달, CLI 인자 100% 매칭, `WITHLOG_VAULT` 환경변수 전달 및 응답 수신 검증.
  8. `test_fake_claude_flag_flip_on_already_in_use`: 1회 시도 실패(`is already in use`) 시 2회차에 `--resume`으로 뒤집어 성공하는 플래그 전환 검증.
  9. `test_fake_claude_no_record_prefix_on_resume_only`: `resume_only` 시 대화 없음 실패 안내 문구 결합 검증.
  10. `test_fake_claude_timeout_reaps_child`: 1초 타임아웃 시 프로세스 회수 및 안내 문구 반환 검증.
  11. `test_fake_claude_d12_first_line_and_huge_policy_warning`: 450자 경고 문구가 뒤따를 때 첫 줄 원인 보존(D12) 검증.

### 3.2 2~3부 호스트 전체 테스트 (`bash test-native.sh`)
- Rust 테스트 76건, Node acceptance 테스트 20건, Vite noclaude 빌드 100% 통과 (종료 코드 0).

---

## 4. 커밋 및 형상 관리
- `native-host/src-tauri/src/ask.rs`: 5장 `ask_runtime`, `ask_claude`, 프로세스 제어 및 단위 테스트 구현.
- `native-host/src-tauri/src/main.rs`: Tauri command 등록 (`ask_runtime`, `ask_claude`).
- `native-host/src-tauri/Cargo.toml`: `libc` 의존성 및 `claude` feature 구성.
