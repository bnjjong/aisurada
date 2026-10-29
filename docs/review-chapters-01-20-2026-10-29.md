# aisurada 1~20장 구현 누락 점검

검토일: 2026-10-29. 대상: `/Users/df/github/aisurada`의 Java 엔진.

**최초 점검 결과:** 1~20장 전체 구현 완료로 보기는 어렵다. `reindex-paths` 1개 명령과 쓰기 공통 단계의 vault git 자동 커밋·쓴 쪽 기록이 미구현이다.** 9~20장의 나머지 명령은 실행 경로가 있고, 이번 정상 경로 호출도 성공했다. 영속 색인을 쓰지 않는 것은 설명서가 허용한 선택이다.

**후속 개발 완료:** 아래 1·2절의 누락은 같은 날 구현하고 검증했다. `reindex-paths`와 자동 커밋·작성자 기록을 사용할 수 있다. 이 문서의 점검 수치와 표는 최초 스냅숏의 기록이며, 현재 구현과 시험은 [후속 완료 노트](./rebuild-p1-chapters01-20-gaps-complete.md)를 기준으로 본다.

## 기준과 검토 범위

- 원문: `/Users/df/Documents/withlog 1부 vault 엔진 설명서.md`의 1~20장. 저장소의 HTML 해부도는 장 구성이 다르므로 명세 장 번호의 기준으로 삼지 않았다.
- 기준 HEAD: `1be3f1f0d0d11779fb0aa379e125ecd4fa643a4b`. 검토 시작 당시 미커밋 상태였던 20장 Java 구현도 포함한 **작업 폴더 사본**을 별도 임시 디렉터리에 고정해 빌드했다.
- Java 소스 104개, 내용과 경로를 합친 SHA-256: `d00298767909414e8a8c28996d2c3bf913f52a6143394a52a386e8708a48424f`. 파일별 값은 [스냅숏 기록](./review-repros/chapter01-20-snapshot-2026-10-29.json)에 있다.
- 명령 이름 등록만으로 구현됐다고 세지 않았다. 인자 읽기, 실행기 분기, 실제 파일 처리와 정상 호출을 확인하고, 공통 쓰기의 빠진 걸음은 실제 serve에서 재현했다.
- 이번 작업은 점검이다. 엔진 기능이나 진행 중인 20장 코드는 수정하지 않았다. 이 문서와 재현 스크립트·검사 근거를 추가했다.

## 1. 미구현 명령: reindex-paths

**관련 절: 8.5, 8.6의 필수 조건 1번.**

`CommandRegistry.COMMAND_NAMES`, 사용법, `wl-cli`의 전달 이름 목록에는 있지만 `CommandRegistry.isSupported()`에 없고, 인자 읽기와 실행기에도 구현이 없다. 그래서 유효한 요청도 명령을 실행하기 전에 다음과 같이 끝난다.

```text
reindex-paths notes/demo/root
종료 코드 3 / stdout 0바이트
WL-409 이 판에는 reindex-paths이 없다
```

설명서 8.6은 영속 색인을 생략하는 구현도 이 명령의 문법·실패·응답을 구현해야 한다고 명시한다. 따라서 ‘색인이 없는 구현’으로 정당화되는 차이가 아니다. 경로별 갱신을 사용하는 호출자는 현재 이 명령을 사용할 수 없다.

필요한 구현은 옵션/무제한 자리 인자 문법, 경로 정리·범위 검사, 지정 자리의 진단 수집, `mode: paths`, 200개 초과 시 `mode: full` 분기다. 템플릿·회고·휴지통을 검사하는 full reindex를 그대로 호출하는 것만으로는 8.5와 같아지지 않는다.

근거: [CommandRegistry.java](../src/withlog/cmd/CommandRegistry.java), [ArgsParser.java](../src/withlog/args/ArgsParser.java), [EngineExecutor.java](../src/withlog/engine/EngineExecutor.java), [Reindex.java](../src/withlog/cmd/Reindex.java), [실행 기록](./review-repros/chapter01-20-audit-2026-10-29.json).

## 2. 공통 미구현: vault git 자동 커밋·쓴 쪽 기록

**관련 절: 3.2의 시작/정비/종료, 7.1의 4번과 6번. 상세 기능은 31장과 연결된다.**

워크스페이스 확인, 디스크 훑기, 명령별 처리, f1 검사, 도장, 원자적 쓰기와 파생 쓰기는 구현되어 있다. 그러나 다음 공통 동작은 없다.

- 쓰기 직전 다른 작성자/세션의 변경을 커밋하는 걸음.
- 일에 닿은 쓰기가 성공하거나 실패한 뒤 작성자·세션·첫/마지막 쓰기 시각을 메모리에 남기는 걸음.
- serve의 startup, idle/overdue, shutdown 자동 커밋과 그 잠금·건너뜀·재시도 처리.

`EngineConfig`는 자동 커밋 설정을 읽지만, `autocommitEnabled()`와 `autocommitIdleSeconds()`를 실제 동작에서 사용하지 않는다. serve의 정비는 상태 파일 갱신 뒤 유휴 종료로 넘어간다. 종료 코드에는 `shutdown git 커밋: 결정 2에 따라 생략`이라는 주석도 있다. README와 기존 완료 노트가 이미 밝힌 **의도적으로 남겨 둔 구현 범위**이며, 이번에 새로 생긴 회귀는 아니다. 다만 1~20장 명세와의 기능 차이로는 남아 있다.

실제 재현은 main 브랜치의 임시 git vault에 기본 커밋 1개를 만들고 노트에 외부 편집을 한 뒤, 자동 커밋을 켜고 idle 시간을 0초로 두어 같은 serve에서 진행했다.

| 시점 | 커밋 수 |
|---|---:|
| serve 시작 전 | 1 |
| startup 뒤 | 1 |
| 도구 set_favorite 쓰기 뒤 | 1 |
| 명령 set-favorite로 작성자 전환 뒤 | 1 |
| 정비 5초가 지난 뒤 | 1 |
| 종료 뒤 | 1 |

쓰기 2개는 정상 성공했지만 커밋되지 않은 변경이 남았다. 따라서 앱과 도구가 번갈아 쓰기 전에 이전 글을 이력으로 보호하는 안전망은 현재 없다. 단발 모드에서 자동 커밋하지 않는 것은 명세대로이며, 이 재현은 단발 호출만 보고 내린 결론이 아니다.

근거: [EngineExecutor.java](../src/withlog/engine/EngineExecutor.java), [EngineConfig.java](../src/withlog/config/EngineConfig.java), [ServeMode.java](../src/withlog/serve/ServeMode.java), [실행 기록의 automaticCommit](./review-repros/chapter01-20-audit-2026-10-29.json).

## 3. 검증 정비: 외부 시험의 오래된 코멘트 오류 문구

기존 외부 자가 시험을 새 빌드에 연결해 **922개 중 921개 통과, 1개 실패**를 확인했다. 실패는 `edge_cases.mjs`의 `19.5 BOM → unreadable`이다.

```text
기대: 내용이 JSON 이 아니다: notes/serving/cm1/comments.json
실제: 내용이 JSON이 아니다: notes/serving/cm1/comments.json
```

최초 점검에서는 외부 fixture 갱신 누락으로 판단했지만, 로컬 전체 설명서와 저장소의 19장 원문은 띄어 쓴 `내용이 JSON 이 아니다`다. 처음 사용자 메시지와 TS 요약에 있던 붙여 쓴 문구를 원문으로 잘못 구분했다. 후속 커밋 `50b450d`에서 엔진·19장 시험·요약을 로컬 전체 설명서에 맞춰 고쳤다. 따라서 이 실패는 외부 fixture의 오류로 확정할 수 없으며, 그 판단을 정정한다. 외부 시험 원본을 바꾸지 않은 후속 빌드에서 922/922가 통과했다.

전체 시험의 기반은 `/Users/df/github/withlog-p1-rebuild/selftest/`다. 사본을 임시 빌드 폴더에 두고 adapter를 이번 JAR로 바꿔 실행했다. 공식 수용 묶음의 실행을 뜻하지 않는다. 19장 Java 회귀는 저장소의 `bash test.sh`로 독립 실행되어 **308개 단언이 통과**했다.

## 4. 장별 확인표

아래 ‘명령 구현’은 실행 경로와 이번 정상 호출을 확인했다는 뜻이다. 모든 비정상 입력·파일 시스템 경계가 완전히 검증됐다는 뜻으로 쓰지 않는다. 13~20장 쓰기는 모두 위 7.1의 git/쓴 쪽 누락을 함께 가진다.

| 장 | 확인한 구현 | 남은 점 |
|---|---|---|
| 1 파일 형식 | NoteFile/NoteMeta/Frontmatter, UTF-8·UTF-16, 도장·토큰, 폴더/옛 파일 처리 | 초기화 명령 init-workspace의 상세 구현은 23장 범위 |
| 2 노트 밖 파일 | 아젠다·사실층·템플릿·회고·휴지통 읽기와 파일 규칙 소비 | 프로젝트 CRUD·템플릿/회고 쓰기·내보내기는 21장 이후 범위 |
| 3 실행/수명 | 단발, serve, 잠금·소켓·상태 파일, wl-cli·plist·청소·설치·doctor | startup/정비/shutdown git 미구현 |
| 4 프레임 | 요청/응답 프레임, 길이·버전·EOF, 상태/WL 코드, 세션 표식 | 이번 검사에서 추가 미구현을 찾지 못함 |
| 5 인자 | 도구 JSON·명령 key=value, 표면별 스키마·차례 | reindex-paths 문법은 8장 누락에 포함 |
| 6 출력/실패 | stdout 바이트·키 순서·봉투, 종료/WL 분류, 예상 밖 실패 | 외부 오류 문구 fixture 1개 갱신 필요 |
| 7 쓰기 파이프라인 | 표식·훑기, f1, 도장, 원자적 쓰기·이동, 카드/마커 파생 쓰기 | 쓴 쪽 검사·기록과 자동 커밋 미구현 |
| 8 색인/신선도 | 새로 훑는 NoteIndex와 full reindex, 진단 3갈래 | reindex-paths 미구현. 영속 DB 생략 자체는 허용됨 |
| 9 list_docs | 두 표면의 목록 명령·필터·응답 | 공통 색인 차이는 8.6 허용 범위로 구분 |
| 10 search_docs | 검색·평문 추출·대목·응답 | 이번 검사에서 추가 미구현을 찾지 못함 |
| 11 상세 읽기 | get_doc/get_sub_note, 부모 사슬, 본문·코멘트·토큰 | 이번 검사에서 추가 미구현을 찾지 못함 |
| 12 태그 | list_tags 집계·필터·제한·정렬 | 이번 검사에서 추가 미구현을 찾지 못함 |
| 13 노트 생성 | create_doc, 아젠다 연계 생성, 폴더·파일·경고 | 공통 git 단계 누락 |
| 14 노트 수정 | update_doc, f1, 제목/본문/메타, 이동·경고 | 공통 git 단계 누락 |
| 15 메타 쓰기 | set_favorite/evaluate_doc/link_docs | 공통 git 단계 누락 |
| 16 하위 목록 | list_sub_notes, 직계 목록·본문 옵션 | 이번 검사에서 추가 미구현을 찾지 못함 |
| 17 하위 쓰기 | 생성·수정·삭제, 카드 제거·컨테이너 토큰 | 공통 git 단계 누락 |
| 18 첨부 | 목록·글 첨부 쓰기, ref, 이름/확장자 검사 | 공통 git 단계 누락 |
| 19 코멘트 | 세 상태 봉투, 쓰기 3개, 마커 제거·본문 토큰 | 공통 git 단계 누락. 원문 예시 충돌과 19.9 호환 동작은 별도 기록대로 유지 |
| 20 아젠다 | 목록·생성·수정·삭제와 create_doc 공용 부품 | 이번 사본에 미커밋 구현 포함. 시험 97개 통과. 공통 git 단계 누락 |

9~20장 명령과 reindex/reindex-paths를 합쳐 **엔진 명령 이름 26개**를 호출했다. 25개가 종료 0, reindex-paths만 종료 3/WL-409였다. 도구 이름과 명령 이름의 별칭을 두 개 명령으로 중복 집계하지 않았다. 별도 wl-cli 로컬 명령은 기존 3/4장 시험에서 확인했다.

## 5. 미구현으로 오인하지 않은 항목

- **SQLite/meta.db/관문·낡은 행의 재현**: 8.6은 색인 없는 구현을 허용한다. 숫자 응답 일부와 손편집 반영 시점의 허용 차이를 새 결함으로 세지 않았다. 단 reindex-paths의 필수 문법·응답은 예외가 아니다.
- **doctor의 빌드 커밋 진단**: 현재 JAR에 두 빌드 도장이 없고 일부 커밋 줄은 고정된 ‘못 쟀다’다. 하지만 3.6은 재구현에서 이 저장소 전용 진단을 바꾸거나 생략해도 된다고 명시한다. 선택 개선 항목이며 필수 미구현으로 세지 않았다. builtCommit의 null도 허용된 값이다.
- **21장 이후 명령**: 프로젝트 CRUD, 사실층 쓰기, init-workspace, 템플릿/회고 쓰기, 휴지통 변형, 내보내기, 샤드 이관, 계획/검사, history/commit-vault를 이 보고서의 ‘1~20장 누락 명령’에 섞지 않았다. 1~2장의 파일 개요와 다른 장의 교차 참조만으로 해당 명령이 완성됐다고 보지도 않았다.
- **부록 D의 미결정 특이점**: 원문의 현재 동작을 호환하려고 남긴 불편함과 실제 미구현을 구분했다.

## 검사 근거와 다시 실행

- [명령/자동 커밋 재현 JSON](./review-repros/chapter01-20-audit-2026-10-29.json)
- [소스 스냅숏과 파일별 해시](./review-repros/chapter01-20-snapshot-2026-10-29.json)
- [전체 외부 자가 시험 원문](./review-repros/chapter01-20-selftest-2026-10-29.log): 921/922, 실패 1건의 기대/실제 포함
- [Java 빌드/19장 회귀 원문](./review-repros/chapter01-20-java-test-2026-10-29.log): 308개 통과
- [재현 스크립트](./review-repros/chapter01-20-audit.py): 매번 별도의 임시 vault·HOME·git 저장소·소켓을 만들고 지운다. launchd를 사용하지 않는다.

```sh
./build.sh
python3 docs/review-repros/chapter01-20-audit.py --root "$PWD" --output /tmp/aisurada-audit.json
bash test.sh
```

실사용 vault의 데이터를 변경하지 않았다. 당시의 구현 우선순위는 reindex-paths와 git/쓴 쪽 기능이었다. 두 기능의 후속 개발과 재검증은 [완료 노트](./rebuild-p1-chapters01-20-gaps-complete.md)에 있다.
