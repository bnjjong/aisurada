# Rust 엔진 이전 5단계 5b — MCP 서버, 에이전트 등록, 옛 사이드카 정리

> 이 문서는 2026-11-15 시점의 기록이다. 고치지 않는다.

## 결론

Java `ais-cli`의 직접 하는 명령 7개를 모두 Rust `ais-cli`로 옮겼다: MCP 서버(`mcp`), 에이전트 등록(`claude-mcp`, `codex-mcp`, `gemini-mcp`), 진단(`doctor`), 옛 사이드카 정리(`sweep`), 안내(`install`).
MCP 서버는 Java 서버와 같은 JSON-RPC 대화를 한 줄씩 견주어 **174건, 어긋남 0건**이다. 실제 Claude Code CLI(2.1.325)가 임시 HOME에서 우리 Rust 서버에 `✔ Connected`로 붙었고, Codex·Antigravity·Gemini CLI의 등록·상태·제거도 실제 CLI로 확인했다.
`doctor`·`sweep`·`install`은 사이드카가 없어져서 옮기지 않고 **옛 사이드카를 걷어 내는 쪽으로 다시 만들었다**(계획 문서의 방향 그대로다).
앱과 에이전트에 등록된 MCP는 아직 Java를 쓴다. 이 단계는 앱 동작을 바꾸지 않았다.

```
에이전트(Claude·Codex·Gemini) ──표준 입출력 JSON-RPC──► ais-cli mcp ──► 명령줄과 같은 길 ──► Rust 엔진
                                                         (사이드카 없음, 호출마다 vault를 다시 정함)
```

## 계획과 달라진 것 (확인이 필요한 것)

1. **이미 등록된 MCP는 5c에서 깨진다.** 5단계 계획은 「`Resources/bin/ais-cli` 경로가 그대로라서 등록된 MCP가 안 깨진다」고 적었지만, Java 시절의 등록은 `Resources/bin/ais-cli`가 아니라 **번들 안의 `java`와 `ais-cli.jar`**(`<앱>/Contents/Resources/runtime/…/bin/java -jar <앱>/Contents/Resources/ais-cli.jar mcp`)를 가리킨다. 5c에서 번들에서 이 둘이 빠지면 Claude·Codex·Gemini에 등록된 서버가 뜨지 못한다.
   - 5b의 대응: 등록 명령의 `status`에 `stale`(낡은 등록: 명령에 `.jar`가 있다)을 더했다. `doctor`가 같은 것을 「★낡은 등록」으로 보여 준다. 다시 `add`하면 새 등록으로 바꿔 끼운다(읽기·쓰기 모드는 `status`의 `write`로 알 수 있다).
   - **5c에서 할 일:** 새 앱의 첫 실행에서 `stale`인 등록을 같은 모드로 다시 `add`한다(결정 4의 사이드카 정리와 같은 자리).
2. **`doctor`는 vault가 없어도 종료 코드 2로 멈추지 않는다.** 진단 도구가 진단할 것을 못 보여 주고 죽는 것이 더 나빠서, vault 줄에 「못 정했다」고 적고 나머지를 보여 준다.

## 만든 것

| 자리 | 하는 일 |
|---|---|
| `crates/ais-cli/src/mcp.rs` | MCP 서버. 한 줄에 JSON-RPC 하나. 초기화(요청한 개정이 3개 안에 있으면 그것, 아니면 최신), `ping`, `tools/list`, `tools/call`, 오류 코드(-32700·-32600·-32601·-32602·-32603), 알림은 답하지 않음. 도구 호출은 `crate::run`(명령줄과 같은 길)으로 엔진에 보낸다. Codex는 호출마다 `_meta.sessionId`(없으면 `threadId`)를 싣고, 클라이언트 이름에 `codex`가 있을 때만 그 호출의 세션 출처로 쓴다 |
| `crates/ais-cli/src/mcp_tools.rs` | 도구 38개의 이름·설명·입력 스키마·쓰기·삭제성 깃발. **생성 파일**이다. `engine-rs/tools/compat/gen-mcp-tools.sh`가 Java `McpTools`에서 뽑는다. 기본은 읽기 도구만, `--write`면 영구 삭제(`purge_doc`)만 뺀 전부 |
| `crates/ais-cli/src/register.rs` | `claude-mcp`(`status`·`add`·`remove`·`config`), `codex-mcp`·`gemini-mcp`(`status`·`add`·`remove`). 에이전트 CLI의 `mcp` 하위 명령만 부르고 설정 파일은 직접 고치지 않는다. 등록하는 명령은 **이 명령을 돌린 `ais-cli` 실행 파일**에 `mcp`(또는 `mcp --write`)를 준 것이고 vault는 박지 않는다 |
| `crates/ais-cli/src/agent_cli.rs` | 에이전트 CLI 찾기(환경변수, 홈의 `.local/bin`, Homebrew, `/usr/local/bin`, `PATH` 차례)와 실행(홈 폴더에서, 40초 제한, 출력 모으기) |
| `crates/ais-cli/src/old_sidecar.rs` | 옛 사이드카 흔적 찾기(우리 레이블 `co.kcd.aisurada.sidecar.*`의 plist, `run/*.json` 상태 파일, `bin/ais-*.jar`, `logs/sidecar-*.log`, `last-sweep`)와 걷어 내기(`launchctl bootout`) |
| `crates/ais-cli/src/{doctor,install}.rs` | `doctor [--prune [--dry-run]]`, `sweep [--dry-run]`, `install` |
| `examples/mcp_compat.rs`, `tools/compat/DriveMcp.java` | Java MCP 서버와 대화를 견주는 도구 |

## 확인한 것

```bash
./build.sh                                                                   # dist/*.jar
(cd engine-rs && cargo run -q -p ais-cli --example mcp_compat -- ../dist)    # Java와 MCP 대화 맞대기
(cd engine-rs && cargo test -p ais-cli)                                      # 단위 29 + MCP 프로세스 3 + 등록 8 + 정리 6
```

| 확인 | 결과 |
|---|---|
| **MCP 대화 맞대기** | 87가지 메시지를 읽기 모드와 쓰기 모드로 돌려 174건, 어긋남 0. 두 서버가 도구 호출을 같은 가짜 호출기(`DriveMcp.java`)로 받아서, 초기화·도구 목록 38개·호출 결과 모양·오류 코드·Codex 세션 출처·깨진 JSON·여러 모양의 id를 본다. 도구가 엔진에서 하는 일은 5a의 명령줄 맞대기(2,064건)가 이미 견줬다 |
| MCP 프로세스 | 실제 `ais-cli mcp`를 표준 입출력으로 부려 합성 vault에서 초기화·목록·`list_docs`(옛 모양 배열)·엔진 오류(종료 코드 줄)·읽기 모드의 쓰기 거절·`--write`로 `create_doc` 뒤 `get_doc`(사실층 포함) |
| 등록 명령 | 가짜 에이전트 CLI로 `add`가 먼저 지우고 넣는 차례, 등록 JSON, 모드(`--write`), 낡은 등록 표시, 사용법 오류(종료 코드 2), 실패 글, 안 도는 CLI |
| **실제 CLI**(임시 HOME) | Claude Code 2.1.325: `add`·`status`·`remove`, `status`의 상세에 `✔ Connected`. Codex: `add --write`·`status`(`args: mcp --write`)·`remove`. Antigravity(`agy`)와 Gemini CLI: `add`·`status`·`remove`(목록 줄 모양이 둘 다 맞게 읽힌다) |
| 정리 | 가짜 HOME과 가짜 `launchctl`로: 우리 레이블만 bootout하고 plist·상태 파일·jar·로그를 지움, 잠금 파일과 남의 항목은 남김, 아직 도는 프로세스는 지우지 않고 알림(종료 코드 1) |

실제 CLI 확인은 진짜 설정을 건드리지 않도록 **임시 HOME**에서만 했다(이 세션에 이미 `aisurada` MCP가 등록돼 있어서 같은 이름으로 덮어쓰면 안 된다). 인증이 필요한 실제 대화(`claude -p`로 도구를 부르는 것)는 하지 않았다.

## 일부러 다르게 둔 것

1. **등록의 모양.** 등록하는 값이 `java -jar ais-cli.jar mcp`에서 `<ais-cli 실행 파일> mcp`로 바뀐다. 그래서 `add` 응답에서 `jar` 칸이 빠지고, 서버 환경변수 `AISURADA_SIDECAR_JAR`가 없어진다(`config`의 `env`는 `{}`). `status`에는 `stale` 칸이 늘었다.
2. **`doctor`.** 소켓·상태 파일·plist·사이드카 jar 진단이 사라지고, vault와 출처, vault의 git, 이 실행 파일, 에이전트별 MCP 등록(낡은 등록 표시), 남은 옛 사이드카를 보여 준다. `--plist`는 아무 일도 안 한다(알림만). `--prune`은 `sweep`까지 한다.
3. **`sweep`.** 이전에는 오래 조용한 사이드카만 내렸다. 지금은 옛 사이드카를 **모두** 내리고 지운다(더 이상 쓸 사이드카가 없으므로). 하루에 한 번 자동 청소는 없다.
4. **`install`.** jar를 복사하지 않고 안내만 한다. 남은 옛 jar가 있으면 알린다.
5. **잠금 파일은 안 지운다.** 쓰기 잠금(`~/Library/Application Support/aisurada/locks/`)은 Rust 엔진이 쓰는 것이라 `sweep`이 건드리지 않는다.
6. **서버 시작 안내.** 표준 오류에 한 줄 적는 글은 같고, 프로토콜 출력은 표준 출력으로만 낸다.

## 남은 일

- 5c: 호스트가 `engine_call`만 쓰게 하고, 새 앱의 첫 실행에서 `ais-cli sweep`과 낡은 등록의 재등록을 부른다. 번들에서 Java를 빼고 `Resources/bin/ais-cli`를 바이너리로 바꾼다. 러너의 `claude-mcp config` 후처리(`runner/protocol.rs`)와 `AISURADA_SIDECAR_KEEP_REGISTRATION` 따위 사이드카 전제를 정리한다.
- 손 확인(이슈 #54, #61)은 5c 전에 결과가 필요하다.
