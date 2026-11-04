# withlog 4부 Claude 연동 구현 계획 (Master Plan)

기준 문서는 사용자가 제공한 **「withlog 4부 Claude 연동 설명서」 (2026-11-04, 226쪽 PDF)** 이다.  
1부(vault 엔진), 2부(네이티브 호스트), 3부(화면)에 이어 4부(Claude 연동)를 소스 코드 없이 설명서의 계약과 동작 명세를 바탕으로 단계별(장별)로 스텝 바이 스텝 구현하고 검증한다.

---

## 1. 공통 원칙 및 기준

1. **계약 준수 (0장 0.2)**
   - **엔진 명령 7개**: `get-ask`, `append-ask-turn`, `list-chats`, `append-chat-turn`, `read-session`, `session-titles`, `recent-sessions`
   - **호스트 커맨드 9개**: `ask_claude`, `ask_runtime`, `list_claude_agents`, `alive_pids`, `held_agent_sessions`, `release_agent_session`, `write_session_image`, `pty_start_session`, `start_dictation`
   - **바깥 실행 `claude` (기준판 2.1.317)**: 기본 길 `claude -p` 실행, 인자 구성, 표준입력 prompt 전달, 자식 프로세스 환경변수(`WITHLOG_VAULT`, `PATH` 등), 상한 타임아웃 및 프로세스 종료.
   - **파일 포맷 및 상한**: 노트 폴더 안 `ask.json`, `chats/<대화 id>.json` (상한 각 1,000,000 바이트), `attach/ask-...` 그림, 본문 문답 표식 `<!--wla:...-->`.
   - **브라우저 스토어 키**: `withlog.ask.v1`, `withlog.ask.note-<노트 id>.v1`, `withlog.ask.legacy.v1`.

2. **기본 길 계약과 깃발 길(부록 E) 분리**
   - 기본 경로 `claude -p`를 메인 계약으로 온전히 구현한다.
   - 깃발 길(`WITHLOG_ASK_VIA_AGENT_SDK`, `WITHLOG_ASK_VIA_HTTP`)과 `wl-agent` 데몬은 계약 밖(부록 E)이므로, `ask_runtime`은 늘 `cli`, `held_agent_sessions`는 빈 배열 `[]`, `release_agent_session`은 `not_live`로 안전하게 응답하여 호환성을 보장한다.

3. **부록 C 얼어 있는 특이점 결정(13개 고침, 94개 지킴) 반영**
   - **D1/D19 (C9, C74)**: `append-ask-turn`, `append-chat-turn` 쓰기 시 1,000,000 바이트 초과 여부 사전 검사 (초과 시 `WL-500` 종료코드 3, 파일 보존).
   - **D4 (C22)**: 첫 물음이 실패(그림 저장 실패 등)하더라도 다음 물음에 작성 세션 대화 묶음이 실리도록 「답을 받은 회차가 0개인가」로 판정.
   - **D12 (C41)**: 실패 글에 표준오류 첫 줄 + 꼬리 400자 표시 (회사 정책 경고 줄로 인한 실패 원인 잘림 방지).
   - **D14 (C49)**: `alive_pids` 실행 시 `ps` 명령에 5초 상한 설정.
   - **D15 (C50)**: `recent-sessions`에서 200개 초과된 `dropped` 세션 수를 PROJECTS 레일에 표시.
   - **D18 (C59)**: `read-session`이 전사를 대체 문자(U+FFFD)로 유연하게 읽어 불완전한 UTF-8 바이트 1개로 전체 실패하지 않도록 처리.
   - **D20 (C73)**: 레일 대화 페르소나에서 세션 대화가 실제로 실리지 않았을 때 「그 세션의 대화는 실리지 않았다.」로 정직하게 명시.
   - **D22/D24 (C75, C90)**: 레일 대화 및 세션 화면의 회차 시각을 기기 현지 시간대(`YYYY-MM-DD HH:mm`)로 표시.
   - **D25 (C96)**: 세션 변경 시 우측 레일 탭을 기본값인 「관련 노트」로 리셋(안 시킨 CLI 탭/셸이 자동 실행되는 현상 방지).
   - **D26 (C91)**: 서브에이전트 바에서 끝난 세션의 경우 결과 없는 호출을 「지금 도는지는 몰라요」로 표시.
   - **D29 (C98)**: 전사를 노트로 굳힐 때 도구 인자를 인라인 코드로 안전하게 감싸서 본문 마크다운/표식 오염 방지.

4. **검증 및 회귀 방지**
   - 각 스텝마다 엔진 단위 테스트, 호스트 검사, 화면 단위/통합 테스트를 추가하고 `./test.sh` 및 `bash test-native.sh`가 항상 100% 통과하도록 유지한다.

---

## 2. 단계별(장별) 실행 순서

| 단계 | 기준 장 | 핵심 주제 및 구현 내용 | 산출물 / 검증 |
|---|---|---|---|
| **Step 0** | **0장 & 1장** | **4부 경계, 빌드 스위치 및 `claude` 탐색 기본기**<br>- 4부 스위치 3개 연동 (엔진 jar, 호스트 feature `claude`, 화면 모드)<br>- `claude` 바이너리 탐색 순서 (WITHLOG_CLAUDE_BIN -> ~/.local/bin -> brew -> PATH)<br>- 호스트 자식 실행 환경 설정 (`WITHLOG_VAULT`, PATH 확장) | - 호스트 `ask.rs` 탐색/환경 로직<br>- 빌드 스위치 단위 테스트 |
| **Step 1** | **2장** | **문답 표식과 문답 파일 (엔진 저장소 및 파일 계약)**<br>- 밑줄 id 규칙 `ak<시각36진수>-<uuid8자>`<br>- 본문 문답 표식 `<!--wla:...-->`<br>- `ask.json` 포맷 및 1MB 상한(읽기/쓰기 체크, D1 고침)<br>- 엔진 명령 `get-ask`, `append-ask-turn` 원자적 쓰기<br>- 묻기 첨부 그림(`attach/ask-...`) 및 HTML 상자 문답 id(`html:...`) | - Java 엔진 `AskThreadCommands`, `AskThreadFile`<br>- `Chapter32Test` 및 문답 엔진 테스트 |
| **Step 2** | **5장** | **`claude -p` 기본 길 (네이티브 실행 엔진)**<br>- 호스트 커맨드 `ask_claude` 및 `ask_runtime`<br>- stdin을 통한 prompt 전달<br>- 플래그 뒤집기 1회 (`is already in use` <-> `No conversation found`)<br>- 응답 대기 상한(타임아웃) 및 SIGTERM/SIGKILL 종료<br>- 실패 문구 조합 (첫 줄 + 꼬리 400자, D12 고침) | - Rust `ask.rs` 호스트 구현<br>- 가짜 `claude` 프로세스 기반 테스트 |
| **Step 3** | **3장** | **화면 드래그 묻기 한 번 (프론트엔드 입력 및 파이프라인)**<br>- 편집기 단추 줄 / 표 줄 / HTML 상자에서 「AI에게 묻기」 진입<br>- 묻는 칸 Popover, 모델/effort 고르개<br>- prompt 조립: 작성 세션 대화 묶음, 아젠다 저장소, 노트 id, 대목<br>- 노트별 FIFO 큐(줄 서기) 및 `withlog.ask.v1` 브라우저 스토어(pending/abandoned)<br>- D4 고침: 실패 회차 발생 시에도 작성 세션 대화 누락 방지 | - React `useAskStore`, `AskComposePopover`<br>- 프론트엔드 ask 테스트 |
| **Step 4** | **4장** | **문답을 보이는 자리 (UI 렌더링)**<br>- 노트 아래 「AI 문답」 칸 (`.ask-panel`)<br>- 밑줄 호버 카드 (`.ask-hovercard`), 핀 고정, 닫기 지연(180ms)<br>- 문단 밑줄 고리 (`.lex-ask-busy`), 표 줄 점 (`.lex-rownote-dot.is-ask`), HTML 단추<br>- 「요청 중」 실시간 타이머 및 버려진 물음 줄<br>- 모델 마크다운 렌더링, 기호 칩(`code.is-symbol`), 긴 답 접기 (`.ask-fold`) | - React `AskPanel`, `AskHoverCard`, `ModelProse`<br>- 렌더링 및 인터랙션 테스트 |
| **Step 5** | **7장** | **전사 읽기 (엔진의 Claude Code 세션 전사 파서)**<br>- `~/.claude/projects/<folder>/<id>.jsonl` 탐색<br>- 전사 파싱: user/assistant 메시지, tool_use/tool_result 매칭<br>- UTF-8 유연 해독(D18 고침: 깨진 바이트 대체 문자로 처리)<br>- 엔진 명령: `read-session` (페이지네이션, author 옵션)<br>- 엔진 명령: `session-titles` (머리/꼬리 64KB 읽기, custom-title/ai-title)<br>- 엔진 명령: `recent-sessions` (7일 이내 cli 세션 200개, dropped 수집) | - Java 엔진 `SessionTranscript`, `SessionTitles`, `RecentSessions`<br>- 전사 jsonl 파싱 단위 테스트 |
| **Step 6** | **6장** | **PROJECTS 레일 (세션 목록 및 탐색)**<br>- 호스트 커맨드 `list_claude_agents`, `alive_pids` (5초 상한, D14 고침)<br>- 화면 세션 목록 스토어 및 아젠다 repo 매칭 묶기<br>- 7일 기준 접기 (`.sess-more`), 프로젝트 폴딩 (`sess:<slug>`)<br>- `dropped` 세션 안내 표시 (D15 고침)<br>- 클릭 시 `/session/:id` 라우팅 및 resume 명령어 복사 | - Rust `agents.rs`, React `RailProjects`<br>- 레일 렌더링 및 탐색 테스트 |
| **Step 7** | **8장** | **노트 옆 세션 레일과 레일 대화**<br>- 노트 메타 줄 세션 칩 (`sessionId`, `editedSessionId`) 및 「열기」 단추<br>- 노트 옆 세션 레일 (`.nsrail`, 폭 392~960px 조절)<br>- 레일 대화: `chats/<대화 id>.json`, 엔진 명령 `list-chats`, `append-chat-turn` (1MB 상한, D19 고침)<br>- 세션 대화 24,000자 스냅샷 묶음 및 페르소나 (D20 고침: 대화 없을 때 정직한 명시)<br>- 현지 시간대 표시 (D22 고침) | - Java 엔진 `RailChatStore`<br>- React `NoteSessionRail`, `RailChat`<br>- 레일 대화 저장 및 파이프라인 테스트 |
| **Step 8** | **9장** | **세션 화면 (`/session/:id`)**<br>- 세션 화면 라우트, 머리 메타 줄, 대화 뷰어 (사용자/어시스턴트/도구 줄)<br>- 하단 독: 서브에이전트 바(D26 고침: 끝난 세션은 도는지 모른다고 표시), 할 일 바 (`todos`), 마지막 실행 상태 (`runState`)<br>- 우측 레일: 관련 노트, 링크 목록, 탭 리셋 (D25 고침)<br>- 시각 현지화 (D24 고침)<br>- 전사를 노트로 굳히기 (`create-doc`, 도구 인자 인라인 코드화 D29 고침) | - React `SessionRoute`, `SessionTranscript`, `PinTranscriptModal`<br>- 세션 화면 통합 테스트 |
| **Step 9** | **10장** | **세션 입력창과 CLI 탭**<br>- 세션 화면 하단 입력창: 세션 실행 여부 판정 (`live-terminal`, `live-background`, `no-record`)<br>- 세션 입력창 이미지 붙이기 (`write_session_image`), 파일 드래그앤드롭, 받아쓰기(`start_dictation`)<br>- 우측 레일 CLI 탭: `pty_start_session` (resume / attach 지원)<br>- 3부 터미널 연동 및 프로세스 분기 | - Rust `session_shell.rs`, `session_image.rs`, `dictation.rs`<br>- React `SessionComposer`, CLI 탭 연동 |
| **Step 10** | **부록 A~E & 종합 검증** | **부록 C 결함 최종 점검 및 4부 종합 검증**<br>- 13가지 결함 고침 항목 전수 재확인<br>- 4부 커맨드 9개 / 엔진 명령 7개 골든 및 계약 검증<br>- 전체 통합 빌드 및 리그레션 테스트 통과 | - 종합 검증 리포트 작성 (`rebuild-p4-complete.md`) |

---

## 3. 진행 방식

- 사용자의 요청에 따라 **각 장별 / 스텝별로 순차적으로 진행**한다.
- 각 스텝 착수 전 구현 목표와 명세를 정리하고, 구현 및 검증을 완료한 후 결과를 보고하고 다음 스텝으로 넘어간다.
