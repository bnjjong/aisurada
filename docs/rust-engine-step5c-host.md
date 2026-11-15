# Rust 엔진 이전 5단계 5c — 앱 호스트와 번들

> 이 문서는 2026-11-15 시점의 기록이다. 고치지 않는다.

## 결론

앱이 엔진을 부르는 길과 번들에서 **Java가 빠졌다.** 앱의 `sidecar_call`과 러너·승인 알림은 같은 프로세스 안에서 Rust `ais-cli`를 부르고(사이드카도 프로세스 띄우기도 없다), 번들은 Java 실행 환경과 jar 대신 Rust 실행 파일 `bin/ais-cli` 하나를 싣는다.
앱 번들은 **173 MB → 31 MB**(Java 포함 0.0.6 설치본 대비), `ais-cli` 호출은 **약 14 ms**(Java 단발 엔진은 약 180 ms, Java `ais-cli`는 거기에 JVM 하나와 사이드카 왕복이 더 붙는다)다.
되돌리는 길(`AISURADA_ENGINE=java`)은 5d까지 개발 실행에서만 살아 있다(번들에는 Java가 없다). 쓰기 명령도 기본으로 Rust 엔진이 받는다.

```
화면 ─ engine_call ──────────────────────────────┐
     └ sidecar_call(명령줄) ┐                      ▼
러너·승인 알림 ─ call_blocking ┴─► ais_cli::run ─► Rust 엔진(앱 프로세스 안)
에이전트 ─ MCP(별도 프로세스) ─► bin/ais-cli mcp ─► Rust 엔진(.git 대기 기록으로 앱과 이어짐)
```

## 한 일

| 자리 | 내용 |
|---|---|
| `ais-core` 자동 커밋 | 대기 기록(쓴 쪽, 세션, 처음·마지막 쓴 시각)을 프로세스 메모리에서 `vault/.git/aisurada-pending.json`으로 옮겼다(이슈 #58, 결정 2). 읽고 고치는 일은 vault 쓰기 잠금을 쥔 채 하고, 정비 커밋은 잠금을 얻은 뒤 다시 읽어 중복 커밋을 막는다. 유휴 타이머는 앱과 MCP 서버만 돌리고, vault를 열 때 `watch`가 앞서 단발 `ais-cli`나 닫힌 앱이 남긴 기록을 이어받아 커밋한다. git 저장소가 아니거나 자동 커밋이 꺼졌으면 파일을 만들지 않는다 |
| `ais-cli` | `migrate`: 옛 사이드카를 걷어 내고 낡은(Java jar를 가리키는) MCP 등록을 같은 모드로 다시 등록한다. `run_once`는 표시 파일(`~/Library/Application Support/aisurada/rust-engine-migrated`)로 한 번만 한다. `doctor --fix`로 손으로도 한다. MCP 서버는 도구를 부를 때마다 자동 커밋을 지켜본다 |
| 호스트 `sidecar.rs` | 기본이 `ais_cli::run` 같은 프로세스 호출이다. 앱이 AI 세션 터미널에서 떴어도 세션 표식은 엔진에 넘기기 전에 지운다(#53). 엔진 안의 패닉은 종료 코드 1의 답으로 바꿔 앱을 지킨다. 옛 Java 길(`execute_java`)은 `AISURADA_ENGINE=java`와 스냅숏 쓰기 설정에서만 남겼다 |
| 호스트 `engine.rs` | 쓰기 명령도 기본으로 `engine_call`이 받는다. `AISURADA_ENGINE_WRITE=java`(쓰기만), `AISURADA_ENGINE=java`(전부), 스냅숏 설정이 끄는 스위치다 |
| 호스트 `migration.rs` | 정식 앱(번들 `bin/ais-cli`가 있고 빌드 폴더 번들이 아님)의 첫 실행에서 `migrate::run_once`를 백그라운드로 부른다. 개발 실행은 기기 전체에 영향을 주는 이 일을 하지 않는다 |
| 호스트 `workspace.rs` | vault를 열 때 자동 커밋을 지켜본다 |
| 러너 | 등록 유지 스위치(`AISURADA_SIDECAR_KEEP_REGISTRATION`)와 `is_guest_build` 전제를 지웠다. 수동 실제 시험(`#[ignore]`)은 Rust `ais-cli`로 돈다 |
| 번들 | `tauri.conf.json`에서 jar 둘을, 릴리스 설정에서 Java 런타임을 뺐다. `prepare-runtime.mjs`·`runtime-lock.json`을 지우고 `prepare-cli.mjs`(cargo release 빌드, strip·lto)를 더했다. `release.yml`과 `release-artifacts.mjs`에서 OpenJDK 소스 배포도 뺐다. 개발용 번들의 `bin/ais-cli`는 저장소의 `engine-rs/target` 빌드를 찾아 넘기는 스크립트다 |
| 검증 | `verify-release.mjs`를 다시 썼다: 번들에 Java·jar가 없고, 번들 `ais-cli`가 PATH를 비운 채 노트를 만들고 별도 프로세스가 읽고, 등록 명령이 번들 실행 파일을 가리키고, MCP 서버가 표준 입출력으로 초기화·목록·호출에 답하는지 본다 |
| 화면 | 연결 화면이 낡은 등록(`stale`)을 「다시 연결해 주세요」로 안내한다. 낡은 Java·사이드카 문구를 정리했다 |

## 확인한 것

| 확인 | 결과 |
|---|---|
| 엔진 맞대기(`compat_check`, 합성 vault) | 읽기 527건, 쓰기 2,488건, `commit_vault` 11건, **어긋남 0**(자동 커밋을 파일 기록으로 바꾼 뒤에도) |
| `ais-core` | 자동 커밋 시험 11개(프로세스 간 공유, 다른 프로세스의 유휴 커밋, 쓴 쪽이 바뀌면 먼저 커밋, 저장소 아님·꺼짐·깨진 파일) |
| `ais-cli` | 시험 29 + MCP 프로세스 4(단발 호출이 남긴 기록을 MCP 서버가 커밋) + 등록 8 + 정리·이주 10 |
| 호스트 `cargo test` | 162개 통과. 실패 6개는 기준선과 같다: `ask::tests` 4개는 변경 전 main에서도 실패하고, `runner::tests` 2개는 병렬일 때만 불안정해 직렬(`--test-threads=1`)로 30개 모두 통과한다 |
| **번들**(`release` 설정, app) | `verify-release.mjs` 8개 통과: Java·jar 없음, arm64, 서명, 노트 만들기·읽기, 등록 명령, MCP 왕복 |
| 크기 | 앱 **31 MB**(호스트 26 MB + `ais-cli` 4.5 MB). 비교: Java 포함 0.0.6 설치본 173 MB |
| 호출 시간(합성 vault 39노트) | Rust `ais-cli` 14 ms. Java 단발 엔진 180 ms(Java `ais-cli`는 JVM과 사이드카 왕복이 더 붙는다) |
| **실제 창(WKWebView) 점검** | 10종 248건 모두 통과(아래 「실제 창 점검」) |

## 알아 둘 것

1. **스냅숏 쓰기는 5d에서 사라진다.** 환경변수 `AISURADA_VAULT_SNAPSHOT_ENABLED=true`(노트를 덮기 전 옛 바이트를 git 참조에 남기는 선택 기능, 기본 꺼짐)는 Rust 엔진에 옮기지 않았다. 5c에서는 켠 사용자의 쓰기만 Java로 보내므로 개발 실행에서만 동작하고, 정식 번들에서는 「Java 엔진이 없다」는 오류가 난다. 5d에서 이 기능을 지우거나(추천) 옮길지 정해야 한다.
2. **이미 등록된 MCP는 첫 실행에서 다시 등록된다.** Java 시절의 등록은 번들의 `java -jar ais-cli.jar mcp`를 가리켜서 새 번들에서는 뜨지 못한다. 첫 실행의 이주가 같은 모드(읽기 전용 또는 `--write`)로 다시 등록하고, 못 하면 연결 화면이 「다시 연결해 주세요」로 알린다. 에이전트 CLI가 없는 사람은 해당 없다.
3. **점검 하네스는 아직 쓰지 않는 Java 엔진을 띄운다.** `pnpm test:*:acceptance`와 `scripts/acceptance.mjs`는 격리 Java 사이드카와 호출 기록용 `java` 래퍼를 띄우지만, 호스트는 이제 그것을 쓰지 않는다(환경변수 `AISURADA_CLI_JAR` 등은 무시된다). 점검 결과에는 영향이 없고 5d에서 걷어 낸다. 호출 수는 위의 `AISURADA_CALL_LOG`로 센다.
4. **앱을 처음 쓰는 기기에서는 이주가 할 일이 없다.** 표시 파일만 남기고 끝난다.
5. 되돌리는 길로 돌릴 때(`AISURADA_ENGINE=java`)는 옛 Java 사이드카가 다시 필요하지만, 이주가 이미 그것을 지웠을 수 있다. 그때는 Java가 새로 띄운다(예전과 같다).

## 실제 창 점검

사용자의 손 확인(이슈 #54, #61)을 대신하는 것은 아니지만, 같은 길(실제 WKWebView 창 + 앱 안의 Rust 엔진 + 격리 HOME·vault)로 자동 점검 10종 **248건이 모두 통과**했다: `test:acceptance` 26, `workspace` 9, `data` 12, `interaction` 11, `navigation` 13, `editor` 116, `autosave` 17, `views` 28, `part4` 10, `ask-marks` 6. 쓰기도 기본으로 Rust 엔진이 받는 상태에서 돌았다(저장 토큰, 자기 쓰기 억제, 외부 편집 충돌, 하위 노트·코멘트·템플릿 쓰기, 자동 저장 한 번 쓰기, 실제 `AIS-412`).

처음 돌렸을 때 5종이 깨졌고 원인은 제품이 아니라 **점검의 가정**이었다. 점검은 「앱이 엔진을 부른 호출 수와 차례」를 세는데, 예전에는 모든 호출이 문자열 목록(Java 프로세스 호출)이라 Java 호출 기록(`java-calls.jsonl`)이나 `sidecar_call` 감싸기로 셌다. 읽기는 2단계부터, 쓰기는 이번 단계부터 호출 객체 길(`engine_call`)로 가서 기록에서 사라졌다. 그래서 점검을 두 길 모두 센다: 호스트가 `AISURADA_CALL_LOG` 파일에 호출을 적고(`call_log.rs`) 점검 서버가 `toArgv`로 같은 문자열 목록으로 바꿔 내준다(`/calls`). 화면 안의 감싸기(`data-checks.ts`, `autosave-checks.ts`)도 두 길을 센다. 인자 차례는 호스트가 이름 차례로 정렬해 적으므로 한 점검(`list-docs limit=0 bodies=true`)은 차례를 따지지 않게 고쳤다. 또 `engine-part4`는 「4부 없는 엔진은 `get-ask`에 AIS-409」를 기대했는데, Rust `ais-cli`는 4부 명령을 늘 갖는다(`null`을 돌려준다). 호스트가 4부 명령을 등록하지 않는지만 본다.

## 남은 일

- 5d: Java 지우기(`src/`, `test/`, `build.sh`, `test.sh`, 사이드카, 점검 하네스의 Java 엔진, `execute_java`, 스냅숏 쓰기 결정), `java-engine-final` 태그, 문서(`CommonRule.md`, `CLAUDE.md`, 개발자 가이드, `mcp-guide.md`).
- 사용자의 실제 앱 손 확인(이슈 #54, #61). 이 단계의 자동 점검은 이를 대신하지 못한다.
