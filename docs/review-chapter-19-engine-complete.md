# aisurada 19장 완료 — 코멘트 파일 읽기·쓰기와 본문 마커 제거

작성일: 2026-10-29

반영 저장소는 `/Users/df/github/aisurada`다. 기존 withlog TypeScript 클라이언트 작업과 구별하여, Java 엔진의 실제 명령·파일 처리를 구현했다. 작업 당시 함께 진행 중이던 20장 변경은 이 작업의 커밋에 포함하지 않는다.

## 구현

- `list_comments` 도구와 `list-comments` 명령을 실행기와 인자 읽기에 연결했다. 루트 id와 선택 하위 id 두 칸으로 직계 컨테이너의 파일을 찾고, 손자도 루트/손자 두 조각으로 조회한다. 기록 층 게이트를 추가하지 않았다.
- 명령 전용 `create-comment`, `update-comment`, `delete-comment`를 구현했다. 워크스페이스 확인·기존 색인 맞추기 후 대상/옵션을 검사하고, 읽기에서 남긴 항목에 변화를 얹어 코멘트 파일 전체를 다시 쓴다. 세 명령 모두 f1을 받지 않는다.
- 기존 `CommentsReader`가 파일에 도구 인자의 trim/빈 객체 보충을 적용하던 오류를 고쳤다. 빈 파일·공백뿐인 파일을 unreadable로 구별하고 `내용이 JSON 이 아니다: <경로>`의 바이트 문구를 맞췄다(처음 커밋 `b22a443`은 `JSON이`로 붙여 써서 설명서 1·11·19장, 부록 C와 달랐고, 20장 뒤 후속 커밋에서 띄어 쓴 문구로 되돌렸다). 파일 읽기 I/O 실패도 unreadable이며 그 위의 쓰기는 WL-409다.
- 파일은 v/comments 및 항목 id/createdAt/editedAt/anchor/body 순서, 2칸 들여쓰기와 끝 LF 하나로 쓴다. 짝 없는 서로게이트는 파일에서 소문자 유니코드 탈출로 보존하고 stdout에서는 기존 `?` 규칙을 유지한다. 코멘트 파일 링크는 가리키는 파일에 쓰고 마지막 항목 삭제 시 파일을 지운다.
- 삭제는 항목의 존재와 markdown 포맷을 확인한 뒤, 본문에서 그 id의 마커만 먼저 걷는다. 키워드는 대소문자를 무시하되 잡힌 id는 글자 그대로 비교한다. 마커 사이의 글, 다른 id, 공백이 삽입된 잘못된 마커는 남긴다. 코드 펜스와 반쪽 마커도 검사한다.
- 기존 하위 노트 카드 제거의 본문 읽기/파생 쓰기를 `DerivedBodyWriter`로 추출해 코멘트 삭제와 공유했다. 옛 도장·sessionId 보존, 모르는 프론트매터 키 소실, 하위 노트 폴더 이동, 본문 상한과 실패 문구를 유지한다. 본문을 썼을 때만 markerStripped=true와 bodyBefore/bodyAfter를 반환한다.
- `wl-cli`에 이미 등록된 다섯 이름을 실제 serve 엔진으로 호출해 요청/응답 프레임과 한글·줄바꿈 보존까지 확인했다.

## 검증

모든 파일 쓰기는 임시 vault에서 실행했다. 실제 사용자 vault, launchd, 기존 LaunchAgent를 사용하지 않았다.

| 검사 | 결과 |
|---|---|
| `bash test.sh` | JAR 3개 빌드 및 19장 Java 엔진·파일·serve/wl-cli **308개 단언 통과** |
| withlog HTTP 통합 | `WITHLOG_TEST_JAR=/Users/df/github/aisurada/dist/wl-engine.jar npm test -- host/chapter19.integration.test.ts` **6개 통과** |
| 기존 17장 회귀 | `WLR17_ROOT=/Users/df/github/aisurada node /Users/df/Documents/withlog-p1-verify/round17.mjs` **190/190** |
| 기존 18장 회귀 | `WLR18_ROOT=/Users/df/github/aisurada node /Users/df/Documents/withlog-p1-verify/round18.mjs` **110/110** |

새 회귀는 세 상태 봉투, 파일 JSON과 인자 JSON의 차이, 잘못된 UTF-8, 항목 탈락/id 선점·대소문자·정렬·UTF-16 절단, 정확한 파일 바이트, 수정 시 앵커 보존과 시계 역행, 명령/옵션 거절 순서, 못 읽는 파일 보존, 파일 링크, 손자 경로, 마커 제거/토큰, JSON·HTML·단일 파일 거절, 옛 하위 파일의 실패 보존, 크기 상한 초과 후 쓰기 차단, 깊은 JSON의 예상 밖 실패를 포함한다. 기존 17/18장 검증 스크립트는 이 저장소 밖에 있으며 공식 수용 묶음과 별개다.

## 유지한 차이와 미결정 사항

- 19.9의 읽기 탈락/절단 항목 소실, 형식에 맞지 않는 create의 comment_id 허용, vault 전체 코멘트 id 충돌, 코멘트 쓰기 크기 상한 부재를 임의로 고치지 않았다. 프로세스 벽시계에 +09:00을 붙이는 현재 시각 특이점도 유지했다.
- 이 엔진은 영속 색인을 두지 않고 매번 훑는다(8.6). 따라서 이미 색인된 원본의 옛 행 유지, 코멘트 표 낡음, 본문 쓰기 뒤 DB 재삽입 실패는 재현하지 않는다. 코멘트 색인 사본 읽기의 실패는 기존 훑기에서 더 일찍 명령을 세울 수 있다. git 자동 커밋·쓴 쪽 기록도 기존 미구현 범위다.
- 원문의 마커 표에는 대문자 id를 소문자 인자로 제거하거나 없는 공백을 추가하는 예시가 있다. 19.5/19.8/19.9.13의 반복된 명시 규칙인 ‘잡힌 id를 글자 그대로 비교하고 마커 조각만 제거’를 적용했다.
- 원본 Kotlin 엔진이나 제공되지 않은 공식 수용 묶음/golden 파일과 비교 실행한 결과는 아니다. 검증 대상은 이번 aisurada Java 구현과 설명서의 규칙이다.

원문은 [engine-manual/19-comments.md](./engine-manual/19-comments.md), 재실행 가능한 회귀는 `test/withlog/CommentCommandsTest.java`다.

## 후속: 19장 오류 문구 보완 — 2026-10-29

- list_comments의 잘못된 parent 오류는 이미 홑따옴표를 쓰고 있었다. 19.4 문서 예시의 백틱을 실제 응답과 같은 '노트id' / '노트id/하위노트id'로 맞췄다.
- create-comment와 update-comment의 body가 비었거나 공백뿐이면 오류를 `WL-400 코멘트 본문이 비었다.`로 낸다. 누락된 끝마침표를 추가했다.
- 빈 문자열·스페이스·탭·줄바꿈·유니코드 공백의 정확한 stderr 바이트와 코멘트/본문 파일 보존을 검증했다. 빈 parent와 공백 조각뿐인 parent의 홑따옴표도 검사했다.
- `bash test.sh` 통과: 코멘트 362개 + 경로 재색인 133개 + git 88개 = 583개. 오류 문구와 검증을 바꿨으며 파일 저장 규칙은 그대로다.
