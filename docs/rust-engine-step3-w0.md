# Rust 엔진 이전 3단계 W0 — 쓰기 공통 기반

## 요약

**결론:** 쓰기 명령을 옮기기 전에 깔아야 하는 공통 기반을 Rust 엔진(`ais-core`의 `write` 모듈)에 만들었다. 프로세스 간 쓰기 잠금, 원자적 쓰기, 경로 가두기, 고치기 전 확인 값(`f1`) 검사, 워크스페이스 표식 검사, 쓰기용 색인 맞추기, 시험용 시계·id, git 자동 커밋이다. 쓰기 명령은 아직 하나도 옮기지 않았고 앱의 동작은 달라지지 않는다(Java 엔진이 쓰던 그대로다). 단 Java 쪽도 **한 곳**을 고쳤다. 쓰기 파이프라인이 같은 파일 잠금을 잡는다. 앱을 열어 직접 확인하지는 못했다.

이 기록은 [읽기 이전 기록](rust-engine-step2-rest.md)에 이어지고, 쓰기 이전 정리안 A~H(전부 추천대로)를 따른다. 이번 묶음은 W0이고, 다음은 W1(설정 파일 쓰기)이다.

## 1. 무엇을 만들었나

| 부품 | 자리 | 하는 일 |
| --- | --- | --- |
| 쓰기 잠금 | `write/lock.rs` | vault별로 쓰려는 스레드를 도착 차례로 줄 세우고, vault 밖 잠금 파일에 운영체제 파일 잠금(`flock`)을 건다. 기다리는 `acquire`와 기다리지 않는 `try_acquire` |
| 원자적 쓰기 | `write/atomic.rs` | 임시 파일에 쓰고 디스크에 내린 뒤 이름을 바꿔 한 번에 교체. 대상 자리의 빈 폴더를 지우지 않고 실패한다 |
| 경로 가두기 | `write/confine.rs` | 빈 조각, NUL 글자, 기준 폴더 밖으로 나가는 경로를 거절 |
| 확인 값 검사 | `write/precondition.rs` | 본문 파일을 읽을 수 있는가(500) → 확인 값이 왔는가·모양·지금 파일과 같은가(412) |
| 표식 검사 | `write/marker.rs` | `.aisurada/workspace.json`이 없으면 쓰기가 일을 하지 않는다(400) |
| 시험 장치 | `write/apparatus.rs` | `AISURADA_TEST_NOW`, `AISURADA_TEST_RANDOM`으로 시각과 id 꼬리를 고정. Java의 JVM 속성과 같은 뜻 |
| 자동 커밋 | `write/autocommit.rs` | 쓴 쪽을 기록해 두었다가 유휴(300초)·쓴 쪽 교체·15분 경과 때 한 커밋으로 남긴다 |
| 쓰기용 색인 맞추기 | `vault/index.rs`의 `NoteIndex::reconciled`, `vault/comment_copy.rs` | 맞추기를 세우는 문제를 만나면 오류로 돌려준다 |
| `commit_vault` 명령 | `commands/commit_vault.rs` | 대기 중인 변경을 지금 한 번 커밋(앱이 열고 닫을 때 부른다) |

**프로세스 간 잠금.** 앱(Rust)과 Java 사이드카는 따로 도는 프로세스라서 JVM 안 잠금만으로는 같은 vault에 동시에 쓰는 것을 못 막는다. 잠금 파일은 vault 내용이 아니므로 vault 밖(`~/Library/Application Support/aisurada/locks/`, 시험은 `AISURADA_LOCK_DIR`)에 두고, 이름은 vault 실제 경로(심볼릭 링크를 푼 것)의 SHA-256이다. Java의 `WritePipeline.begin`이 같은 파일을 잡도록 고쳤다(`VaultFileLock`). macOS에서 Java `FileChannel.lock`과 Rust `flock`이 **서로를 막는 것을 양방향으로 확인했다**(실험과 실제 `ais-engine.jar`를 쓴 시험). 이 상호 작용은 macOS의 성질이고 Linux에서는 다를 수 있다. 앱이 macOS 전용이라 이 가정을 그대로 쓴다.

```
앱 쓰기(Rust) ─┐                         ┌─ 같은 vault 파일 + git
               ├─ vault 밖 파일 잠금 ────┤
AI 도구 쓰기(Java 사이드카) ─┘
```

**색인 맞추기가 세우는 문제.** 쓰기는 읽기와 달리 아래를 만나면 일을 하지 않고 실패한다: 같은 slug에 다른 id인 아젠다, 제목이 빈 아젠다, 코멘트 id가 파일 사이에서 겹치는 노트(노트 폴더를 통째로 복사해 둔 경우). `reindex`도 같은 규칙이라 Java처럼 실패한다.

## 2. 읽기에서 찾은 빈틈

코멘트 id가 파일 사이에서 겹치면 Java는 겹친 노트부터 뒤의 노트를 읽기 목록에서도 뺀다. Rust 읽기에는 이 검사가 없어서 그런 vault에서 노트 수가 달랐다. 쓰기용 맞추기를 만들면서 같은 검사를 읽기에도 넣었고(`NoteIndex::fresh`가 뒤 노트를 뺀다), 합성 vault의 맞대기로 Java와 같아진 것을 확인했다.

## 3. 결과

| 확인 | 내용 | 결과 |
| --- | --- | --- |
| Java와 자동 커밋 맞대기 | 합성 vault(git 이력 포함)의 사본 둘에 같은 변경을 만들어 Java(`commit-vault`)와 Rust가 각각 커밋: 변경 없음, 노트 고침·더함·지움과 노트 아닌 파일·`.tmp`, `main`이 아닌 가지, 새 `index.lock`, 병합 중, 저장소 아님 | 11건, 응답·커밋 메시지·작성자·담긴 파일·트리까지 어긋남 0 |
| Java와 읽기 맞대기(코멘트 id 겹침 vault) | 합성 vault에 겹침을 더하고 모든 읽기 사례 | 262건(`reindex`가 Java처럼 실패하는 것 포함), 어긋남 0 |
| `f1` 오류 문구 | Java 실제 실행의 세 가지 412 문구(없음, 모양, 낡음)와 Rust 소스의 글 | 글자까지 같음, 종료 코드 5 |
| 잠금 상호 운용 | Rust가 2.5초 쥔 동안 실제 `ais-engine.jar`의 `create-agenda`가 기다리는가(`tests/lock_interop.rs`) | 기다렸다 진행(약 2.7초), 잠금이 비었을 때는 바로 끝남 |
| Rust 시험 | `cargo test -p ais-core` | 단위 61, 통합 7, 상호 운용 1 통과 |
| 정적 검사 | `cargo clippy --all-targets -D warnings`, `cargo fmt --check` | 통과 |
| Java 시험 | `VaultGitTest` 133, `CommentCommandsTest` 362, `Chapter24Test` 449, `Chapter33Test` 168(`Chapter25Test`는 원래 0건) | 모두 통과. 시험 동안 잠금 파일은 `AISURADA_LOCK_DIR`로 격리한 폴더에만 생김 |
| 호스트 시험 | `cargo test engine::` | 3개 통과(변경된 `ais-core`로 빌드) |

자동 커밋의 단위 시험은 실제 git 저장소로 돌린다: 꼬리 줄과 노트 id 뽑기(지워진 노트 포함), 건너뛰는 이유 5가지, 10분 넘은 `index.lock` 지우기, 쓴 쪽이 바뀔 때 먼저 커밋, 유휴 300초와 15분 경과 정비, 쓰기 중이면 정비를 5초 뒤로 미루기.

## 4. Java와 다르게 한 것과 남긴 것

- **대기 기록은 프로세스 메모리에 둔다**(정리안 C-가). 앱(Rust)과 Java 사이드카가 따로 쓰는 동안 커밋 메시지의 「쓴 쪽」 표시가 어긋날 수 있다. 커밋은 `git add -A`라 변경이 빠지지 않고 `Aisurada-Notes:` 꼬리 줄은 변경에서 직접 뽑으므로 판 목록은 깨지지 않는다. JVM을 걷어낼 때 파일로 올린다(이슈 #58).
- **자동 커밋 설정**은 환경 변수(`AISURADA_VAULT_AUTOCOMMIT_ENABLED`, `AISURADA_VAULT_AUTOCOMMIT_IDLE_SECONDS`)에서만 읽는다. 값이 틀리면 Java처럼 오류를 내지 않고 기본값을 쓴다.
- **유휴 타이머는 아직 호스트에 달지 않았다.** `AutoCommit::tick()`을 주기적으로 부르는 일은 쓰기 명령이 켜지는 묶음(W1)에서 호스트에 단다.
- 경로 조각의 짝 없는 서로게이트 검사는 Rust 글에 그런 것이 있을 수 없어서 NUL 검사만 남았다.
- 잠금 파일을 지우지 않는다(지우면 기다리던 쪽과 새로 온 쪽이 서로 다른 파일을 잡을 수 있다). vault마다 작은 빈 파일이 하나 남는다.

## 5. 확인하지 못한 것과 남은 일

- 실제 앱 창에서 확인하지 못했다(이슈 #54).
- 두 프로세스가 실제로 같은 노트를 번갈아 쓰는 경합 시험은 하지 않았다. 잠금이 서로를 막는 것만 확인했다.
- 쓰기 명령이 없어서 원자적 쓰기, 경로 가두기, 시험용 시계·id는 Java와 응답으로 맞대지 못했고, 단위 시험과 Java 소스 대조로만 확인했다. W1부터 폴더 전체 바이트 맞대기(정리안 E)가 이것들을 함께 검증한다.
- 자동 커밋이 `commit_vault`로 앱 쓰기와 연결되는 것은 쓰기 이전 이후다. 지금 앱은 `commit-vault`를 Java 엔진으로 부른다.
- 호스트 안 유휴 타이머, 앱을 닫은 동안의 AI 쓰기 설계는 이슈 #58에 있다.
- **다음은 W1(설정 파일 쓰기):** 아젠다, 프로젝트, 템플릿, 사실층, `init-workspace`. 이 묶음에서 쓰기 스위치(`AISURADA_ENGINE_WRITE=rust`, 기본 꺼짐)와 두 사본 맞대기 도구를 만든다.
