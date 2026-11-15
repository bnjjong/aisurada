# Rust 엔진 이전 5단계 5d — Java 지우기

> 이 문서는 2026-11-15 시점의 기록이다. 고치지 않는다.

## 결론

**Java 엔진을 저장소에서 지웠다.** `src/`(169개 파일)와 `test/`(33개 파일), 옛 `build.sh`·`test.sh`, 사이드카, 점검 하네스의 Java 엔진을 모두 걷어 냈고, 이제 저장소의 엔진은 Rust(`engine-rs/`)뿐이다. `./build.sh`는 `dist/ais-cli` 실행 파일 하나(약 4.5 MiB)를 37초에 짓고, `bash test.sh`는 `cargo test --workspace` 178건을 약 14초에 돌린다.
옛 Java 엔진은 태그 [`java-engine-final`](https://github.com/bnjjong/aisurada/tree/java-engine-final)에 그대로 남아 있고, Rust 엔진과 답을 맞대 보는 기준(오라클)으로만 쓴다. 기준 jar는 `engine-rs/tools/compat/build-oracle.sh`가 태그에서 지어 `engine-rs/target/oracle/`에 둔다.
5a~5c에서 정한 대로(결정 3-가: 한 번에 지운다) 지웠고, 본문을 덮기 전에 스냅숏을 남기던 선택 기능은 **옮기지 않고 함께 지웠다**(아래 「알아 둘 것」 1).

```
5c까지                              5d 이후
src/ test/  (Java 엔진)   ──삭제──►  (없음. 태그 java-engine-final에만 있다)
engine-rs/  (Rust 엔진)             engine-rs/  (유일한 엔진)
dist/*.jar 4개                      dist/ais-cli 1개
기준 jar = dist/                    기준 jar = build-oracle.sh가 태그에서 지음
```

## 한 일

| 자리 | 내용 |
|---|---|
| 맞대기 기준 | `engine-rs/tools/compat/build-oracle.sh`: 태그에서 `build.sh`와 `src/`만 꺼내 임시 폴더에서 짓고 jar 4개를 `engine-rs/target/oracle/`에 둔다. 같은 태그 커밋이면 다시 짓지 않는다. `compat_check`, `cli_compat`, `mcp_compat`, `lock_interop`, 두 표 생성기, `yaml-spike`가 이것을 쓴다 |
| 호스트 | `sidecar.rs`는 Rust `ais-cli` 길만 남겼다(`execute`, `call_blocking`). `engine.rs`에서 되돌리는 스위치 3개(`AISURADA_ENGINE`, `AISURADA_ENGINE_WRITE`, 스냅숏 판정)를 없앴고 쓰기 명령도 늘 Rust 엔진이 받는다 |
| 점검 하네스 | `acceptance.mjs`, `workspace-acceptance.mjs`에서 격리 Java 사이드카와 호출 기록용 `java` 래퍼를 걷어 냈다. 준비 단계는 `engine-rs/target/debug/ais-cli`(또는 `AISURADA_CLI_BIN`)로 한다. `package.json`의 `backend:jars`를 없앴다 |
| 빌드와 시험 | `build.sh`는 `cargo build --release --locked`로 `dist/ais-cli`를 만든다. `test.sh`는 `cargo test --workspace`(잠금 폴더는 임시)만 돈다. `test-native.sh`는 `build.sh`를 부르지 않는다. 릴리스 워크플로의 Java 설치 단계를 지웠다 |
| 두 표의 위치 | `mcp_tools.rs`(MCP 도구 38개)와 `spec_table.rs`(명령 명세)는 Java에서 뽑아 만든 파일이었다. 이제 이 파일들이 원본이라 머리말을 「직접 고친다」로 바꿨다. 생성 스크립트는 `engine-rs/target/oracle-tables/`에 기준 표를 뽑아 견주는 용도로만 쓰고, 현재 파일을 덮어쓰지 않는다 |
| 이름 | 명령줄 길 호출 함수 `javaCall`을 `cliCall`로 바꿨다(동작 변화 없음) |
| 규칙·안내서 | `CLAUDE.md`, `CommonRule.md`(구성·worktree 준비·시험 표), `README.md`(빌드·실행·환경 변수·종료 코드·소스 구성), `docs/developer-guide.md`, `docs/mcp-guide.md`, `docs/beta-hand-check.md`, `docs/user-guide.html`, `native-host/README.md`를 Rust 기준으로 고쳤다 |

## 확인한 것

| 확인 | 결과 |
|---|---|
| 엔진 맞대기(`compat_check`, 합성 vault) | 읽기 527건, 쓰기 2,488건, `commit_vault` 11건, 합계 **3,026건 어긋남 0** (태그에서 지은 기준 jar로) |
| 쓰기 잠금 맞대기(`lock_interop`) | 통과(Java 쓰기가 Rust 잠금을 기다린다) |
| `ais-cli` 맞대기(`cli_compat`) | **2,064건 어긋남 0**, `history` 포함 vault는 2,194건 어긋남 0 |
| MCP 맞대기(`mcp_compat`) | **174건 어긋남 0** |
| 기준 표와 현재 표 | `gen-mcp-tools.sh`와 `gen-spec-table.sh`가 기준에서 뽑은 두 파일이 현재 `mcp_tools.rs`, `spec_table.rs`와 글자까지 같다(`diff` 출력 없음) |
| `bash test.sh` | **178건 통과**, 14초(컴파일이 캐시된 상태) |
| `bash test-native.sh` | Node 시험 **641건 통과**. 호스트 `cargo test`는 직렬(`--test-threads=1`)로 154건 통과, 4건 실패(`ask::tests`, Java를 지우기 전 `main`에서도 실패하던 것), 5건 무시. 병렬로 돌리면 `runner::tests` 둘과 `terminal::` 셋이 더 실패하고 직렬로는 통과한다 |
| `./build.sh`, 시작 절차 | 37초에 `dist/ais-cli` 4,710,880바이트. 개발자 가이드 §3의 절차(`init-workspace` → `create-agenda` → `create-doc` → `list-docs`)를 그대로 돌려 통과. 종료 코드는 모르는 명령 2, vault 없음 2, 노트 없음 3 |
| MCP 도구 수 | `tools/list`를 직접 불러 읽기 19, `--write`로 37을 세었다(README와 `mcp-guide.md`에 17/35, 13/28로 적혀 있던 낡은 값을 고쳤다) |
| 실제 창(WKWebView) 점검 | 10종 **248건 모두 통과**: `test:acceptance` 26, `workspace` 9, `navigation` 13, `interaction` 11, `data` 12, `editor` 116, `autosave` 17, `views` 28, `part4` 10, `ask-marks` 6(5c와 같은 건수). 점검은 `engine-rs/target/debug/ais-cli`와 앱 안의 Rust 엔진으로 돌고 Java 엔진을 띄우지 않는다. **`autosave`는 첫 실행에서 1건(`native-child-body-and-title-commit-use-separate-writes`)이 실패했다.** 마지막 본문 쓰기에 옛 제목이 없다는 단언이었고, 같은 폴더에서 두 번 다시 돌리니 모두 통과했다. 원인은 확인하지 못했고 타이밍에 기댄 점검의 간헐 실패로 본다 |
| 정식 번들(릴리스 설정의 디버그 번들) | `prepare-cli.mjs` + `tauri build --config tauri.release.conf.json --debug --bundles app` 뒤 `verify-release.mjs` **8건 통과**: 번들에 `ais-cli`와 라이선스·사용자 가이드가 든다, Java 실행 환경과 jar가 없다, 버전 0.0.6, ARM64와 서명, 한글·공백 경로에서 워크스페이스 만들기, PATH 없이 노트를 만들고 다른 프로세스로 읽기, MCP 등록이 번들의 `ais-cli`를 가리킨다, 표준 입출력 MCP(initialize, tools/list, tools/call). `ais-cli` 4,600 KiB. 디버그 번들이라 앱 크기(75,176 KiB)는 5c에서 잰 최적화 빌드(31 MB)와 다르다 |
| 34장 받아들임 묶음(기준 jar) | 27건 중 27건 맞음, 실행기 시험 `runner-test.mjs` 73건 통과(`fixtures/aisurada.adapter.json`이 기준 jar를 부른다) |

## 알아 둘 것

1. **스냅숏 쓰기가 사라졌다(깨지는 변경).** 환경 변수 `AISURADA_VAULT_SNAPSHOT_ENABLED=true`(본문을 덮기 직전에 노트별 git 참조에 옛 바이트를 남기는 선택 기능, 기본 꺼짐)는 Rust 엔진에 옮기지 않았다. 켜고 쓰던 사용자는 더 이상 이 안전망을 받지 못한다. 이미 남은 스냅숏은 `history`가 계속 읽는다. 일반 쓰기에는 git 자동 커밋이 이력을 남긴다.
2. **34장 받아들임 묶음은 기준 jar로만 돈다.** 묶음은 stdout을 키 차례까지 글자로 견주는데 Rust `ais-cli`는 JSON 키를 정렬해서 쓴다. 기대값 27건 가운데 11건은 글자까지 같고 16건은 키 차례만 다르다. 그래서 묶음은 오라클 기준을 유지하고(`AISX_IMPL_JAR`에 기준 jar를 준다), Rust와의 견줌은 키 차례를 무시하는 맞대기(`cli_compat`)가 맡는다.
3. **맞대기 도구를 돌리려면 JDK 21이 필요하다.** 평소 빌드와 시험(`./build.sh`, `bash test.sh`, `test-native.sh`)에는 필요 없다. 태그가 없는 복제본은 `git fetch origin tag java-engine-final`로 받는다.
4. **두 표를 고칠 때는 파일을 직접 고친다.** 생성 스크립트를 예전 방식으로 돌리면 Rust 쪽에서 더한 항목이 지워지기 때문에, 이제 스크립트가 파일을 덮어쓰지 않는다.
5. **예전 Java 판을 쓰던 기기의 옛 사이드카.** 정식 앱은 새 판을 처음 열 때 한 번 정리한다(5c). 손으로 하려면 `ais-cli sweep --dry-run`으로 먼저 본다. 개발 실행과 빌드 폴더의 번들은 하지 않는다.
6. **`docs/rebuild-*`, `docs/review-*`, `docs/rust-engine-step*.md`와 `docs/handoff-round2.md`, `docs/releases/`의 기록은 고치지 않았다.** 이 기록들에는 Java 시절의 명령(`java -jar dist/ais-engine.jar …`)이 그대로 적혀 있다.

## 남은 일

- 사용자의 실제 앱 손 확인(이슈 [#54](https://github.com/bnjjong/aisurada/issues/54), [#61](https://github.com/bnjjong/aisurada/issues/61)). 이 단계의 자동 점검은 이를 대신하지 못한다. 점검표는 [rust-engine-hand-check.md](rust-engine-hand-check.md)에 있다.
- 호스트 `ask::tests` 4건의 기존 실패와 병렬 때 불안정한 시험 5건.
- 앞으로 이 저장소의 `docs/`와 규칙 파일에서 Java를 다시 언급할 일이 생기면 맞대기 기준(오라클)에 한해서만 쓴다.
