# Rust 엔진 이전 2단계 셋째 묶음 — 검색·태그·템플릿 하나·판 목록

## 요약

**결론:** 읽기 명령 4개(`search_docs`, `list_tags`, `get_template`, `history`)를 Rust 엔진으로 옮겼다. 앞 묶음과 합쳐 읽기 명령 16개가 Rust이고, vault의 노트와 설정을 읽는 길은 모두 Rust 엔진이 처리한다. 앱이 이 묶음에서 실제로 부르는 것은 `history`뿐이라(편집 이력 창) 화면은 그 한 곳만 새 호출 방식으로 바꿨다. Java 엔진과 응답을 맞대 어긋남이 없다(합성 vault 687건, 진짜 vault 2개 각 523건과 577건). 앞 묶음과 마찬가지로 **실제 앱 창에서는 확인하지 못했다.** 되돌리는 스위치(`AISURADA_ENGINE=java`)는 그대로다.

이 기록은 [첫 묶음](rust-engine-step2-read.md), [둘째 묶음](rust-engine-step2-open.md)에 이어지고, 정리안 A~F를 같은 방식으로 적용했다.

## 1. 명령별 응답 모양

| 명령 | 응답 | 정리한 것 |
| --- | --- | --- |
| `search_docs` | `{items, total, returned, truncated}` | 줄은 `list_docs`의 줄(본문 없이)에 `matchedIn`(`id`·`title`·`meta`·`body`)이 붙고, 본문에서 걸렸으면 `snippet`(맥락 글)도 붙는다. Java가 되풀이하던 `query`는 뺐다. AI 표면 전용 `hint`는 안 옮겼다. |
| `list_tags` | `{items: [{tag, count}], total, returned, truncated}` | `total`은 상한(`limit`)을 걸기 전의 태그 수다. Java는 상한을 건 배열만 줘서 몇 개가 잘렸는지 알 수 없었다. |
| `get_template` | 템플릿 줄 + `body` | Java와 같다. |
| `history` | `{id, count, versions[], body?, noteBody?, matches?}` | Java와 같다. 판 번호가 범위 밖이거나 판이 없을 때의 오류는 `kind: "conflict"`(`AIS-409`)다. |

**검색이 하는 일.** 검색어가 노트의 아이디, 제목, 메타(아젠다 이름과 태그), 본문 중 어디에 처음 걸리는지 가려 순위를 매긴다(아이디 → 제목 → 메타 → 본문). 순위가 같으면 만든 시각이 늦은 노트가, 그것도 같으면 만든 시각 표기가 늦은 노트가, 그것도 같으면 id가 큰 노트가 먼저다. 위치와 길이는 Java와 같게 UTF-16 코드 단위로 세서 맥락 글(찾은 곳 앞 24칸, 뒤 76칸)이 같은 자리에서 잘린다. 본문은 마크다운 서식(코드 울타리, 링크, 그림, 제목, 인용, 목록 표지, 표, 강조)을 걷어 낸 글에서 찾고, HTML 노트는 태그를 걷어 내며, JSON 노트는 본문을 찾지 않는다. 본문 뒤에 HTML 첨부 글이 이어 붙는다.

**판 목록이 하는 일.** 판은 vault의 git 저장소(`main` 브랜치)에서 세 갈래로 모은다: (1) 노트 파일(`*note.md`)의 경로 이력, (2) 파일 이력에 없지만 커밋 메시지 꼬리 줄(`Aisurada-Notes: id id …`)이 가리키는 커밋(노트가 지금은 없을 때), (3) 옛 스냅숏 참조(`refs/aisurada/snapshots/<id>`). 읽히는 것만 남겨 시각 내림차순으로 놓고, 바로 앞 판과 내용(git 객체)이 같으면 접는다. 저장소가 없는 vault는 판이 없다. 판의 `takenAt`은 +09:00 시각이다.

```
노트 id ─┬─ git grep → note.md 경로 ─ git log --follow ──┐
         ├─ 커밋 메시지 꼬리 줄 ─ git diff-tree ──────────┼─ 같은 커밋·경로 하나로 ─ 내용이 같으면 접기 ─ 판 목록
         └─ refs/aisurada/snapshots/<id> ─ git log ───────┘
```

## 2. 결과

| 확인 | 내용 | 결과 |
| --- | --- | --- |
| Java와 응답 맞대기 | 합성 vault(노트 39건, git 이력 포함)에서 `search_docs`(노트의 제목·아이디·태그·본문 낱말에서 뽑은 검색어와 모서리 사례, 상한, 조건, 오류), `list_tags`, `get_template`, `history`(모든 노트, 번호·커밋·글자 찾기, 오류) | 687건(앞 묶음 항목 포함), 어긋남 0 |
| 진짜 vault 맞대기 | 진짜 vault, 손 확인용 vault(git 저장소 있음) | 같은 항목에서 523건, 577건(손 확인용 vault는 git 저장소가 있다). 어긋남 0 |
| Rust 단위·통합 시험 | `cargo test -p ais-core` | 47개 통과(단위 42, 통합 5) |
| 정적 검사 | `cargo clippy -p ais-core --all-targets -- -D warnings`, `cargo fmt --check` | 통과 |
| 호스트 시험 | `cargo test`(직렬) | 158개 통과, 기존 실패 `ask::tests` 4개(변경 전 `main`에서도 실패) |
| 화면 시험 | `engine-route` 14개(2개 새로), `data` 51, `workspace` 34, `navigation` 30, `interaction` 42, `autosave` 45, `views` 27, `editor` 123, `screen` 6 | 모두 통과 |
| 타입 검사 | `pnpm typecheck` | 통과 |

합성 vault에 더한 것: 검색 순위(제목·메타·본문·기록 층), 대소문자, 서식 제거, 긴 문단의 맥락 글, 이모지 뒤의 위치, 같은 시각의 동점, BOM, HTML·JSON 노트([`add-search-notes.sh`](../engine-rs/tools/compat/add-search-notes.sh)). 판 이력은 수정과 되돌림, 사람과 앱이 남긴 커밋, 스냅숏 참조 둘(하나는 한 커밋과 같은 내용), 이름이 바뀌었다 돌아온 폴더, 지금은 없는 노트 둘(한 커밋에 함께), `main`이 아닌 가지의 커밋이다([`add-git-history.sh`](../engine-rs/tools/compat/add-git-history.sh)). 이력을 만든 뒤 작업 트리는 원래 합성 vault와 같게 되돌아온다.

## 3. Java 엔진과 다르게 한 것과 안 옮긴 것

- **영문 대소문자 비교**는 Java가 글자(UTF-16 코드 단위) 하나씩 보는 규칙을 따랐다. `İ`(점 있는 대문자 I)는 Java처럼 `i`와 같게 본다. 그리스 문자 일부(`ᾳ` 따위 한 글자 변환 규칙이 따로 있는 것)는 옮기지 않았다.
- **커밋 이름 모양**은 소문자 16진수 40자만 받는다(Java는 다른 문자 체계의 숫자도 통과시켰다).
- **git 오류 출력**은 버린다(Java는 표준 출력에 섞어 읽었다). 경고 글이 경로나 커밋으로 잘못 읽히는 일이 없어진다.
- **`history`는 고른 판을 한 번 덜 읽는다.** Java는 모든 판을 읽은 뒤 고른 판을 다시 읽었다. 결과는 같다.
- **시간 제한**은 60초(옛 스냅숏 읽기는 5초)이고 넘으면 곧바로 죽인다(Java는 종료 신호를 먼저 보냈다). **시간 초과 경로는 시험하지 못했다.**
- **`limit`이 숫자가 아닌 글**일 때의 오류 문구는 Java(`limit 은 숫자여야 한다`)와 다르다. Rust는 인자를 JSON으로 받아 타입 오류가 먼저 난다. `ais-cli`를 Rust로 옮길 때 명령줄 해석에서 맞춘다.
- **AI 표면용 덧붙임은 안 옮겼다:** `search_docs`의 `hint`, 노트 줄의 `deepLink`와 `agendaFacts`. AI 표면을 옮길 때 호출자 `ai`에 붙인다.

## 4. 화면과 호스트

- `search_docs`, `list_tags`, `get_template`은 지금 화면이 부르지 않는다(화면 검색은 읽어 둔 노트를 화면이 직접 뒤진다). 그래서 화면 코드는 바꾸지 않았다. 처음 쓰는 곳이 생기면 그때 `data/commands.ts`에 호출 빌더와 `engine.ts`의 어댑터를 더한다.
- `history`는 편집 이력 창(`ui/History.tsx`)이 부른다. 문자열 목록으로 부르던 세 곳(판 목록, 글자 찾기, 판 본문)을 `read.history(id, …)`로 바꿔 Rust 길로 보낸다. 응답 모양이 같아서 어댑터는 없다. Java 길로 풀 때는 번호를 자리 인자(`history <id> <번호>`)로 둔다.
- 호스트(`engine.rs`)는 `COMMANDS` 목록에서 알아서 새 명령을 알리므로 코드를 바꾸지 않았다. IPC 시험에 `history` 호출과 `conflict` 종류 확인을 더했다.

## 5. 확인하지 못한 것과 남은 일

- **실제 앱 창에서 확인하지 못했다.** 확인 방법은 첫 묶음 기록과 같다(`docs/beta-hand-check.md`의 격리 실행법으로 노트의 이력 창을 열어 판 목록·글자 찾기·판 보기가 예전과 같은지 본다).
- git 실행기의 시간 초과 경로와, git이 없는 환경(실행 실패는 빈 결과로 처리한다)은 시험하지 못했다.
- 화면의 부모 사슬 계산(`chain()`)과 폴더 추측(`folder()`)은 Java 경로를 걷어낼 때 함께 지운다.
- 속도와 메모리는 재지 않았다.
- 개발자 가이드 §8의 시험 수치는 전체 시험을 돌리지 않아 고치지 않았다.
- **다음은 3단계(쓰기 경로)다.** 쓰기는 고치기 전 확인 값(`f1`), 쓰기 잠금, 이름 규칙, 자동 커밋이 얽혀 있어 읽기보다 위험하다. 시작하기 전에 명령별 정리안을 만들어 정한다.
