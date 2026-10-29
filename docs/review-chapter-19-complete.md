# 19장 작업 완료 — 코멘트 클라이언트와 로컬 호스트 연결

> 이 문서는 withlog TypeScript 클라이언트의 완료 기록이다. `/Users/df/github/aisurada`의 실제 Java 엔진 반영 결과는 [별도 완료 노트](./review-chapter-19-engine-complete.md)에 기록한다.

작성일: 2026-10-29

19장 설명서에 따라 TypeScript 엔진 응답 소비 계층에 코멘트 목록 읽기와 명령 전용 쓰기 3개를 구현했다. 파일 파싱·전체 재작성·본문 마커 제거는 연결된 외부 Java 엔진이 수행한다. 이 저장소에는 Java 엔진 소스와 원본 golden 규칙 파일이 포함되어 있지 않다.

## 구현 내용

- [`comments.ts`](../src/engine/commands/comments.ts): `list_comments` 도구와 `list-comments` 명령, `create-comment`, `update-comment`, `delete-comment` 명령 클라이언트와 공개 타입·응답 검증자를 추가했다. 코멘트 쓰기 도구 메서드는 제공하지 않는다.
- 목록의 `absent`, `loaded`, `unreadable`을 구별하고 상태와 모순되는 comments/reason 필드를 거절한다. 빈 목록과 읽기 실패를 합치지 않는다. 응답 문자열·항목 순서·대소문자·추가 키와 원본 stdout/stderr를 보존한다.
- 명령 대상은 루트 noteId와 선택 subNoteId 두 자리로 전송한다. 인자 문자열을 다듬거나 경로 사슬로 변환하지 않는다. 미지원 옵션과 잘못된 타입, 설정/옵션으로 해석될 자리 인자를 전송 전에 거절한다. 쓰기 세 개 모두 f1을 받지 않는다.
- 삭제 응답은 markerStripped가 true일 때만 bodyBefore/bodyAfter 두 문자열을 요구한다. false일 때는 두 키를 모두 금지한다. 기존 파생 토큰 평가와 쓰기 결과 정책에 연결해, 본문 재조회 전 전체 본문 저장 보류 및 실패 시 초안 보존·부분 반영 확인 지침을 사용한다.
- [`engineHost.ts`](../host/engineHost.ts): 코멘트 명령 5개를 허용하고, 같은 실제 vault 경로의 코멘트 쓰기와 reindex를 직렬화했다. 경로 alias도 같은 큐를 사용한다. 동일한 코멘트 쓰기도 각각 실행하며, 중간에 쓰기가 있는 reindex 요청은 앞의 갱신과 합치지 않는다. 서로 다른 vault는 독립적으로 실행한다.
- [19장 fixture](../src/engine/__fixtures__/chapter19/fixtures.ts)에 목록 loaded/absent의 간접 수용 근거와 unreadable/쓰기의 코드 읽기 예시를 구분했다. [엔진 README](../src/engine/README.md)에 호출 예시를 추가했다.

## 검증 결과

- `npm test`: **24개 파일 통과, 2개 파일 건너뜀 / 681개 테스트 통과, 12개 건너뜀**.
- `npm run build`: 프런트와 호스트 TypeScript 검사 및 Vite 프로덕션 빌드 통과.
- `git diff --check`: 통과.
- 클라이언트 시험은 상태 봉투·항목 구조, 두 읽기 표면의 argv, 명령 전용 쓰기, 선택 인자 생략과 원문 보존, 미지원 옵션, 삭제 토큰 조건, WL 실패/예상 밖 실패·전송 실패 전달을 검증했다.
- 호스트 시험은 실제 시험용 자식 프로세스를 사용해 HTTP→프로세스→클라이언트 전송, 원문 argv와 토큰 보존, 같은 vault/alias의 쓰기 직렬화, 동일 쓰기 요청의 개별 실행, 실패 뒤 후속 요청 실행, 쓰기 전후 reindex의 병합 방지를 검증했다. 시험용 프로세스의 응답은 합성 값이며 실제 코멘트 파일 동작의 증거가 아니다.

## 실제 엔진 검증과 남은 경계

[`chapter19.integration.test.ts`](../host/chapter19.integration.test.ts)에 임시 vault·HOME·DB/state만 사용하는 실제 Java 엔진 통합 시험 6개를 추가했다. 루트/자식/손자의 코멘트 경로, 달기/고치기의 본문 보존, 깨진/초과 파일의 WL-409와 파일 보존, 삭제 마커 제거와 본문 토큰, 없는 코멘트/JSON 본문의 거절, 마커 키워드와 id의 대소문자 규칙을 검증하도록 작성했다.

이번 실행은 WITHLOG_TEST_JAR가 설정되지 않아 이 6개와 기존 16장 통합 시험 6개를 건너뛰었다. 따라서 Java 파일 재작성·마커 제거 및 원본 수용 묶음/golden 규칙의 실행을 검증했다고 주장하지 않는다. 아래처럼 JAR을 명시하면 격리 통합 시험을 실행할 수 있다.

```sh
WITHLOG_TEST_JAR=/absolute/path/to/wl-sidecar.jar WITHLOG_TEST_JAVA=/absolute/path/to/java npm test
```

19.9의 읽기 탈락 항목 소실, 잘못된 comment_id 허용, vault 전체 코멘트 id 충돌, 쓰기 크기 상한 부재는 부록 D 결정 없이 클라이언트에서 변경하지 않았다. 원문의 마커 예시 표와 글자 그대로 id를 비교한다는 규칙의 충돌도 [구현 계약](./spec-chapter-19-comments.md)에 기록했다. 통합 시험은 반복해서 명시된 글자 그대로 비교 규칙을 기준으로 한다.

이 작업은 클라이언트 API와 로컬 호스트 연결 범위이며 코멘트 편집 UI나 Java 엔진 재구현은 포함하지 않는다. Git 커밋·푸시의 최종 결과는 작업 종료 보고에서 별도로 확인한다.
