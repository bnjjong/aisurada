# Rust 엔진 이전 4단계 계획 — 남은 Java 명령 옮기기

## 요약

**결론:** 3단계(쓰기 경로)까지 옮기고 나서도 Java에 남은 vault 명령을 다섯 묶음(4a~4e)으로 나눠 옮긴다. 순서는 **앱이 직접 쓰는 것부터**(태스크 → 자원·승인 → 묻기·채팅), 그다음 세션 읽기, 마지막에 AI 전용 잔여 명령이다. 기준은 3단계와 같다: 두 사본에 같은 명령을 Java와 Rust로 돌려 응답과 **폴더 전체 바이트**가 같아야 한다. 쓰기 스위치(`AISURADA_ENGINE_WRITE=rust`)는 기본 꺼짐을 유지한다.

이 문서는 [3단계 W4b 기록](rust-engine-step3-w4b.md)에 이어진다. 묶음마다 기록(`rust-engine-step4<글자>-….md`)을 남긴다.

## 1. 남은 것

```
              Java에 남은 vault 명령(약 9,000줄)
   ┌───────────────┬──────────────────────┬────────────────────┬──────────────┬──────────────┐
   │ 4a 태스크      │ 4b 자원·승인           │ 4c 묻기·채팅         │ 4d 세션 읽기   │ 4e 노트 잔여   │
   │ 명령 9개       │ 명령 15개              │ 명령 4개            │ 명령 3개       │ 명령 8개       │
   │ 약 1.7천 줄    │ 약 2.8천 줄            │ 약 1천 줄           │ 약 2.6천 줄    │ 약 0.8천 줄    │
   └───────────────┴──────────────────────┴────────────────────┴──────────────┴──────────────┘
   그 뒤 5단계: ais-cli와 MCP 서버(cli/ 3.5천 줄), 사이드카(serve/), JVM 제거
```

| 묶음 | 명령 | 이유와 메모 |
| --- | --- | --- |
| 4a 태스크 | `create_task`, `get_task`, `list_tasks`, `update_task`, `transition_task`, `link_tasks`, `add_task_comment`, `delete_task`, `restore_task` | 앱의 태스크 화면과 러너가 쓴다. 파일을 **진짜 SnakeYAML**로 써서 방출기를 옮겨야 한다 |
| 4b 자원·승인 | `list_resources`, `get_resource`, `save_resource`, `delete_resource`, `restore_resource`, `set_resource_ref`, `usable_resources`, `list_resource_versions`, `list_resource_changes`, `get_resource_change`, `approve/reject_resource_change`, `resolve_resource_edit`, `get/set_resource_policy` | 스킬·에이전트 페이지가 쓴다. 파일은 SnakeYAML(4a의 방출기를 쓴다)이고, 승인 기록은 앱 폴더(`approvals/…`)에 있다 |
| 4c 묻기·채팅 | `get_ask`, `append_ask_turn`, `list_chats`, `append_chat_turn` | 호스트의 묻기(`ask.rs`)와 세션 화면이 쓴다 |
| 4d 세션 읽기 | `read_session`, `session_titles`, `recent_sessions` | Claude·Codex·Gemini·Antigravity 대화 기록을 읽는다. 어느 형식까지 옮길지는 그때 정한다 |
| 4e 노트 잔여 | `write_attachment`, `export_markdown`, `shard_notes`, `verify_shard`, `plan_day`, `check_notes`, `facts_brief`, `reindex_paths` | 주로 AI 표면이 쓴다 |

## 2. 이어받은 결정

- **파일은 Java와 바이트가 같다**(3단계 결정 A). 태스크·자원 파일은 노트 머리와 달리 SnakeYAML이 쓰므로 방출기를 옮긴다.
- **읽기 명령은 호스트가 알리면 바로 Rust로, 쓰기는 스위치가 켜졌을 때만**(결정 G). 새 읽기(`get_task`, `list_tasks`)는 맞대기를 통과한 뒤 기본으로 Rust가 맡는다.
- **맞대기는 합성 vault 사본 둘의 폴더 전체 바이트**(결정 E). 오류 문구도 코드까지 엄격하게 견준다.
- 응답 필드 이름은 그대로 두고 오류만 `{code, kind, message}`(결정 D).

## 3. 완료 기준(묶음마다)

1. 맞대기가 새 명령의 정상·오류 사례에서 어긋남 0이다(Java가 성공한 단계 수와 `HARNESS_TRACE`로 실제 경로 확인).
2. Java 없이도 도는 Rust 통합 시험이 있다(AI 호출처럼 Java 명령줄로 견줄 수 없는 규칙 포함).
3. 화면은 호출 객체로 부르고, 스위치가 꺼져 있으면 예전 인자 목록 그대로 Java로 간다. 화면 시험 전체와 `typecheck`가 통과한다.
4. 기록 문서와 손 확인 점검표를 갱신한다. `main`에 빨리 감기로 올린다.

## 4. 알려진 위험

- **SnakeYAML 방출기**: 따옴표 규칙과 블록 글(`|`, `|-`, `|+`)이 까다롭다. 진짜 SnakeYAML 2.4가 쓴 91만 건에 가까운 사례와 맞대어 어긋남 0을 확인했다([4a 기록](rust-engine-step4a-tasks.md) §2).
- **러너·승인 알림**은 호스트 안에서 `ais-cli`(Java)로 태스크·자원 명령을 부른다. 화면은 Rust로 가는데 러너는 Java로 가는 기간이 생긴다. 둘은 같은 파일 잠금을 쓰므로 안전하고, 쓰기 스위치 기본값을 켤 때 러너도 `engine_call` 쪽으로 옮긴다.
- **세션 읽기(4d)**: 형식이 네 가지이고 일부는 protobuf다. 범위를 사용자와 정하기 전에는 시작하지 않는다.
