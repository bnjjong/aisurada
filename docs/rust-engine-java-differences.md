# Rust 엔진이 옛 Java 엔진과 다르게 두기로 확정한 것

## 요약

**결론:** 이슈 [#56](https://github.com/bnjjong/aisurada/issues/56)이 모아 둔 「Java와 다른 점」 12개와 댓글로 더해진 것을 모두 코드와 실제 실행으로 다시 확인해서 정리했다. **고칠 것은 하나였다.** 자동 커밋 스위치가 `0`·`off`·`no`·`FALSE`를 끄기로 읽지 않던 것이다. 이제 월별 폴더 설정과 같은 철자 집합을 쓴다. 나머지는 이미 반영돼 있거나, Java가 저장소에서 사라져 견줄 대상이 없어졌거나, Rust 쪽이 더 안전해서 그대로 둔다. 데이터를 잃을 수 있는 한 곳(손으로 고친 코멘트 파일)은 시험으로 고정했다. 반대로 Java에서 물려받은 구멍을 Rust가 뒤늦게 막은 것(기록 층 쓰기 거절, `commit_vault` 잠금)은 [3절](#3-java-뒤에-rust가-더한-보호)에 따로 모았다.

기준은 처음부터 「기능이 기준이고 JVM의 특이점은 재현하지 않는다」였다([첫 묶음 기록](rust-engine-step2-read.md)). Java 엔진은 5단계에서 저장소에서 지웠고(태그 `java-engine-final`) 기준 jar는 `engine-rs/tools/compat/build-oracle.sh`로 다시 짓는다. 그래서 아래 판정의 「Java」는 그 기준 jar와 골든 테이프(`engine-rs/golden/`)를 말한다.

## 1. 이슈 본문 12개

| # | 항목 | 판정 | 근거 |
| --- | --- | --- | --- |
| 1 | AI 표면 덧붙임: `search_docs`의 `hint`, 노트 줄의 `deepLink`, `agendaFacts` | **이미 반영** | `ais-cli/src/legacy.rs`가 붙인다. `hint`는 결과가 잘렸을 때, `agendaFacts`는 호출자가 AI일 때만이다. Java가 기록한 골든 테이프(`golden/cli.ndjson`)에 `hint` 3건, `deepLink` 80건, `agendaFacts` 34건이 들어 있고 재생에서 어긋남이 0이다. 실제 실행 파일로도 확인했다 |
| 2 | 옛 스냅숏 쓰기(`VaultSnapshot`) | **지움** | 5단계 d에서 옮기지 않고 지웠다([기록](rust-engine-step5d-java-removal.md), 커밋 [`b3493a3`](https://github.com/bnjjong/aisurada/commit/b3493a3)). 이미 남은 스냅숏은 `history`가 계속 읽는다 |
| 3 | 명령줄 인자 오류 문구(`limit 은 숫자여야 한다`) | **이미 반영** | `ais-cli/src/map.rs`가 `AIS-400 limit 은 숫자여야 한다: abc`를 낸다. 단위 시험이 `abc`와 `99999999999`를 고정한다. 도구 표면(JSON)은 모르는 키를 먼저 거절한다 |
| 4 | 쓰기 응답의 `warning` 문자열과 `precondition` 이름 | **그대로 둔다** | 두 이름은 AI 사용 안내(`docs/mcp-guide.md`)와 MCP 서버의 안내 글(「`get_doc`으로 읽어 `precondition` 값을 `f1`으로 넘긴다」)에 박혀 있고, AI가 이미 이 이름으로 부른다. 이름을 바꾸면 얻는 것은 모양뿐이고 깨지는 것은 호출자 전부다. 구조화가 필요해지면 새 필드를 더하고 옛 필드는 남긴다 |
| 5 | 자동 커밋 설정의 출처와 틀린 값 | **환경 변수만, 철자는 Java와 같게 고침** | Java의 설정 파일·속성 출처는 Java와 함께 없어졌다. 철자 집합은 Java가 `true`·`on`·`yes`·`1` / `false`·`off`·`no`·`0`(대소문자 무시)이었는데 Rust는 정확히 `false`만 껐다. `AISURADA_VAULT_AUTOCOMMIT_ENABLED=0`이나 `off`를 적은 사용자가 자동 커밋이 켜진 채로 있었다. 고쳤다(`text::parse_flag`). 모르는 값은 Java와 다르게 처리한다: Java는 `IllegalArgumentException`(`Invalid boolean value for …`)을 던져 엔진 조립을 멈췄다(`EngineConfig.parseStrictBoolean`). Rust는 멈추지 않고 기본값(켜짐)을 쓴다. 자동 커밋을 못 켠 채로라도 앱이 뜨는 쪽을 택했다 |
| 6 | 월별 폴더 설정(`AISURADA_NOTES_SHARD_BY_MONTH`)의 출처와 틀린 값 | **환경 변수만, 틀린 값은 없는 것** | 5번과 같은 철자 집합이고 이제 같은 함수(`parse_flag`)를 쓴다. 모르는 값이면 디스크를 보고 판정한다(Java는 엔진을 못 조립하고 멈췄다) |
| 7 | 새 노트의 기본 날짜·시각이 쓰는 시간대 | **그대로 둔다** | Rust는 운영체제의 시간대(`TZ`, `localtime_r`)를 쓴다. 그것이 사용자가 기대하는 시계다. JVM의 자체 시간대 자료와 갈릴 일은 Java가 사라져 없어졌다. 한국 시간대에서는 처음부터 같았다 |
| 8 | AI 호출(`caller: ai`)의 쓰기 규칙: 세션·에이전트 도장, `create_doc`이 층을 안 받음, `update_doc`이 날짜·시각을 안 받음, 원본 요청 절 경고 | **시험으로 고정돼 있음** | 통합 시험 `fixture_vault.rs`(AI `update_doc`의 날짜·시각 무시, AI `create_doc`의 층 거절·세션 도장·원본 요청 경고)와 진짜 `ais-cli mcp` 프로세스 시험 `mcp_stdio.rs`(세션 도장, Codex 세션 신뢰 규칙). 실제 실행 파일에서도 `create_doc`의 `tier`와 `update_doc`의 `date`가 `AIS-400 … 모르는 키다`로 거절되는 것을 확인했다. Java 쪽은 사라져 맞댈 수 없다 |
| 9 | `tags`·`links` 배열에 문자열이 아닌 항목이 올 때 오류 글의 숫자 모양(`1.0e3` → `1000.0`) | **그대로 둔다** | 앱은 문자열 배열만 보낸다. 닿는 것은 오류 글의 숫자 표기뿐이다 |
| 10 | 코멘트 파일 읽기: Rust는 엄격한 JSON, Java는 느슨한 읽개(따옴표 없는 글, 주석) | **엄격하게 두고 안전하게 서는 것을 시험으로 고정** | 손으로 고쳐서 엄격한 JSON이 아니게 된 파일은 읽기가 `state: unreadable`과 이유를 알리고, 쓰기(`create-comment`, `update-comment`, `delete-comment`)는 파일을 건드리지 않고 `AIS-409`로 선다. 말없이 덮어써서 기존 코멘트를 잃는 길이 없다. 시험은 `ais-cli/tests/hand_edited_comments.rs` |
| 11 | 휴지통: 보내기 직전의 첨부 글 훑기(결과를 안 씀)와 진단 줄 | **그대로 둔다** | 결과를 쓰지 않는 훑기와 로그 줄이라 응답이 같다. 응답의 `autoPurged`가 알리는 것도 같다 |
| 12 | 사소한 차이: 대소문자 무시 비교의 그리스 문자 일부, 커밋 이름은 ASCII 16진수만, git 오류 출력은 버림 | **그대로 둔다** | 좁은 경계이고 이 차이로 올라온 문제가 이슈에 없다. 필요가 생기면 이 표에 한 줄을 더해 다시 판단한다 |

## 2. 댓글로 더해진 것

모두 **그대로 둔다.** Rust 쪽이 같거나 더 안전하다.

| 출처 | 차이 | 판정 이유 |
| --- | --- | --- |
| [4b 기록](rust-engine-step4b-resources.md) §3 | 대기 변경 파일의 `args` 칸이 Java는 명령줄 옵션 순서, Rust는 옵션 이름 순서 | 같은 요청의 판정은 순서와 무관하다. Rust 쪽이 항상 같은 바이트를 낸다 |
| 4b | AI가 바로 적용하는 길과 AI의 승인 거절(`AIS-403`)은 Java 단발 실행으로 견줄 수 없었다 | Rust 통합 시험(`queues_ai_resource_changes_until_a_human_approves`)과 `mcp_stdio.rs`가 고정한다 |
| 4b | 옵션 값: Rust는 참거짓·숫자·배열·객체도 명령줄 글로 바꿔 받고 `null`은 안 준 것으로 본다. 모르는 옵션은 무시한다 | 명령줄 층(`ais-cli`)이 모르는 옵션을 `AIS-400`으로 먼저 거절한다(`save-resource zzz=1`로 확인). 사용자 길에서는 같다 |
| 4b | 승인 기록 폴더 바탕: Java 시스템 속성, Rust 환경 변수 `AISURADA_APPROVALS_DIR` | Java 속성은 Java와 함께 없어졌다 |
| 4b | 극단 오류 문구(깨진 승인 기록장의 JSON 오류 설명 따위), `Double.valueOf`만 받는 숫자 글, `tools` 안 명시 태그 값 | 깨진 기록장이나 손으로 쓴 대기 파일에서만 닿는다 |
| [4c](rust-engine-step4c-ask-chat.md) | 레일 대화 목록의 정렬이 쓰는 만든 시각은 흔한 ISO 모양만 읽는다. `elapsedMs`·`sessionLines`는 ASCII 숫자만 정수로 읽는다 | 드문 모양의 시각은 정렬에서 뒤로 갈 뿐 대화가 사라지지는 않는다 |
| [4d](rust-engine-step4d-sessions.md) | 깨진 UTF-8을 대체 글자로 바꾸는 단위, 2,000자 경계의 이모지(Java는 반쪽을 남기고 Rust는 통째로 뺀다), 같은 세션 id의 Codex 기록이 여럿일 때 고르는 것(Java는 폴더 도는 순서, Rust는 이름순) | Rust가 더 낫다. 반쪽 글자가 없고 결과가 항상 같다 |
| [4e](rust-engine-step4e-note-tools.md) | 깨진 UTF-8 사실층 파일을 Java는 없는 것으로, Rust는 대체 글자로 읽는다. 같은 이름·시각·id 꼬리의 플랜을 두 번 저장하면 Java는 실패하고 Rust는 새 폴더 이름을 고른다 | Rust가 더 낫다. 글을 버리지 않고 실패하지 않는다 |

## 3. Java 뒤에 Rust가 더한 보호

위 표들은 Java와 **같게 옮긴 뒤 다르게 둔 것**이다. 아래는 반대로 **Java에서 물려받은 구멍을 Rust가 옮기기를 끝낸 뒤에 막은 것**이라 Java와 일부러 다르다. 구조 개편(동작 불변)과 섞지 않으려고 별도 묶음으로, 골든(녹화한 기준 출력)을 근거와 함께 갱신하며 고쳤다.

| 이슈 | 무엇이 다른가 | Java | Rust |
| --- | --- | --- | --- |
| [#74](https://github.com/bnjjong/aisurada/issues/74) | **기록 층 노트 쓰기 거절.** 기록 층은 기계가 남긴 노트로 AI에게 안 보이고 고칠 수도 없어야 한다 | AI(`caller: ai`)가 노트 id만 알면 `update_doc`, `delete_doc`, 후속 노트·코멘트·첨부 쓰기로 기록 층 노트를 바꿀 수 있었다 | AI의 쓰기가 `ErrorKind::RecordTier`로 선다. 명령줄에서는 `AIS-409 도구 표면이 고칠 수 없는 노트다(층=record): <id>…`다. 앱(`caller: app`)은 그대로 쓴다(`commands/subnote.rs`의 `check_writable`) |
| [#80](https://github.com/bnjjong/aisurada/issues/80), [#82](https://github.com/bnjjong/aisurada/issues/82) | **휴지통의 기록 층.** 휴지통 목록은 보관된 노트 머리의 `tier`를 읽어 판정한다 | AI도 휴지통에서 기록 층 노트를 보고 되살릴 수 있었다 | AI에게는 목록에 안 보이고(`list_trash`), `restore_doc`과 `purge_doc`은 `RecordTier`로 선다. AI의 `empty_trash`는 자기에게 보이는 항목만 비우고 기록 층 보관물은 목록에 남긴다(그 항목은 응답에도 안 나온다) |
| [#74](https://github.com/bnjjong/aisurada/issues/74) | **`commit_vault`가 쓰기 잠금을 잡는다** | 잠금 없이 `AutoCommit`을 불러, 다른 쪽이 여러 파일을 쓰는 중에 커밋하면 쓰다 만 상태가 커밋에 들어갈 수 있었다(유휴 타이머의 정비 커밋은 잠금을 잡았다) | 쓰기와 같은 vault 잠금(`flock`)을 잡고, 쓰는 중이면 그 쓰기가 끝나기를 기다린다(`commands/commit_vault.rs`) |

**잠금에 시간 제한이 없다는 점은 차이가 아니라 같은 성질이다.** Java의 `FileChannel.lock()`도 Rust의 `flock`도 잠금을 얻을 때까지 무한정 기다린다. 다른 프로세스가 쥔 채 멈추면(죽은 프로세스의 잠금은 운영체제가 풀어 주므로 살아서 멈춘 경우다) 쓰기가 끝없이 기다리고, 시간 초과 오류는 없다. 기다리지 않는 길은 자동 커밋의 정비 커밋(`try_acquire`)뿐이다. 시간 제한을 두려면 새 동작이므로 따로 정한다([개발자 가이드](developer-guide.md)의 「쓰기 잠금과 잠금 파일」).

## 4. 이번에 바꾼 것과 시험

- `engine-rs/crates/ais-core/src/text.rs`의 `parse_flag`: `true`·`on`·`yes`·`1`이면 참, `false`·`off`·`no`·`0`이면 거짓(영문 대소문자와 앞뒤 공백 무시), 그 밖은 `None`. 자동 커밋 스위치(`write/autocommit.rs`)와 월별 폴더 설정(`vault/scanner.rs`)이 함께 쓴다.
- 시험 `ais-core/tests/settings_flags.rs`. 옛 코드로 되돌려 돌리면 `"FALSE"는 꺼야 한다`로 실패하는 것을 확인했다.
- 시험 `ais-cli/tests/hand_edited_comments.rs`(위 10번).

## 5. 앞으로

Java와 다른 동작을 새로 찾으면 이 표에 한 줄을 더한다. 기준 jar로 다시 짜서 견줄 수 있다(`docs/developer-guide.md` §4). 판정은 「고친다 / 그대로 둔다(이유) / 시험으로 고정한다」 셋 가운데 하나로 쓴다.
