# aisurada

**macOS Apple Silicon 배포:** [0.2.0 다운로드](https://github.com/bnjjong/aisurada/releases/tag/v0.2.0) · Java 없는 Rust 엔진 판 · [설치 안내와 제한 사항](docs/releases/0.2.0.md)

사용 방법은 [HTML 사용자 가이드](docs/user-guide.html)에 정리했습니다. 파일을 내려받아 브라우저에서 열면 검색·목차·코드 복사·인쇄를 사용할 수 있습니다.

aisurada vault를 읽어 JSON으로 답하고 새 노트를 쓰는 엔진(`ais-core`)과, 그 엔진을 부르는 명령줄 `ais-cli`입니다. Rust로 짓고, 실행 파일 `ais-cli` 하나가 전부입니다. JVM도 상주 프로세스도 없습니다. 예전 Java 엔진은 태그 `java-engine-final`에만 남겼고, Rust 엔진과 답을 맞대 보는 기준(오라클)으로만 씁니다.

데스크톱 앱은 [native-host](native-host/README.md)에 있습니다. 2부 Tauri 호스트 위에 3부 React 화면을 단계별로 연결하고 있습니다. 제공된 13장까지 workspace·리치 편집기·자동저장·충돌 처리와 노트 메타·목록·검색·이력 복원·외부 Markdown·설정 화면을 연결했습니다. [3부 구현 순서](docs/rebuild-p3-plan.md)와 [11~13장 기록](docs/rebuild-p3-chapters11-13-complete.md)에 구현·검증 및 미제공 자료 때문에 재구성한 범위를 구분했습니다. 14~18장과 부록의 본문은 제공되지 않았습니다.

이름은 〈신세기 GPX 사이버 포뮬러〉의 아스라다에서 따온 말장난(AI + 아스라다)입니다. 사람 옆에서 달리는 AI라는 자리가 마음에 들었습니다.

## 하는 일

vault 폴더를 부를 때마다 훑어서 답합니다. 색인 파일을 만들지 않습니다. vault에 쓰는 명령은 엔진 명령 표(`engine/table.rs`)에서 쓰기로 적힌 50개입니다. 그 가운데 설명서가 정한 31개는 19장의 코멘트 쓰기 3개, 20장의 아젠다 쓰기 3개, 21장의 프로젝트 쓰기 3개, 22장의 사실층 쓰기 1개, 23장의 `init-workspace`, 24장의 템플릿 쓰기 3개, 25~30장의 회고·휴지통·내보내기·샤드·플랜 쓰기 8개를 포함합니다. 기본 노트 쓰기는 새 노트를 만드는 `create_doc`, 노트의 제목과 본문을 고치고 정식 자리로 옮기는 `update_doc`, 노트 메타를 고치는 `set_favorite`, `evaluate_doc`, `link_docs`, 노트나 하위 노트에 글 첨부를 쓰는 `write_attachment`, 그리고 하위 노트를 만들고 고치고 지우는 `create_sub_note`, `update_sub_note`, `delete_sub_note`입니다. 나머지 19개는 확장과 4부 쪽입니다. 태스크 쓰기 7개, 자원 쓰기 4개와 승인 쓰기 4개, 문답·대화를 잇는 `append-ask-turn`·`append-chat-turn`, `facts-brief`, `commit-vault`입니다(아래 「프로젝트 자원」, 「태스크」).

`ais-cli`는 명령 1개를 처리하고 끝나는 단발 실행 파일입니다. 호출마다 vault를 새로 훑어서 답하므로 상주 프로세스가 없고, 떠 있는 동안 바뀐 파일도 다음 호출에 보입니다. 표준 입출력으로 오래 도는 것은 MCP 서버로 서는 `ais-cli mcp`뿐입니다. 데스크톱 앱은 같은 코드를 프로세스 안에서 직접 부릅니다.

아래 명령 예시의 `dist/ais-cli`는 `./build.sh`가 만드는 실행 파일입니다. vault는 `export AISURADA_VAULT=/path/to/vault`로 주는 것으로 봅니다([실행](#실행)).

| 명령 | 답하는 것 |
|---|---|
| `list_docs` | 노트 목록 (유형, 태그, 날짜, 즐겨찾기 필터) |
| `search_docs` | 제목, 메타, 본문, 첨부 글을 찾아 대목과 함께 |
| `list_tags` | 태그와 쓰인 횟수 |
| `get_doc` | 노트 1건 (본문, 도장, 코멘트) |
| `list_sub_notes`, `get_sub_note` | 하위 노트 목록과 1건 |
| `list_attachments` | 노트나 하위 노트의 첨부 |
| `list_templates`, `get_template` | 문서·리추얼 템플릿 |
| `list_trash` | 휴지통 |
| `reindex` | vault를 한 번 훑어 읽지 못한 파일이 있는지 알려 준다 |
| `reindex-paths` | 지정한 경로만 훑는다. 받은 자리 인자가 200개를 넘으면 노트 전체로 전환한다 |
| `create_doc` | 새 노트 1건을 쓴다. 없는 아젠다면 아젠다 파일도 만들고, 같은 제목이나 들여쓴 블록 같은 경고 5가지를 함께 답한다 |
| `update_doc` | 제목, 본문, 태그, 템플릿, 날짜, 시각, 아젠다를 고치고 그 값으로 다시 계산한 자리로 옮긴다. 폴더 노트는 폴더째 옮기고, 옛 단일 파일은 폴더로 올리거나 이름만 바꾼다. `agenda`는 이미 있는 아젠다(slug나 이름)만 받고 아젠다를 새로 만들지 않는다 |
| `set_favorite` | 즐겨찾기 별과 라벨을 고친다. 값이 그대로면 파일을 안 연다 |
| `evaluate_doc` | 평가(점수, 메모, 시각)를 새기거나 지운다 |
| `link_docs` | 다른 노트로의 연결 1건을 잇거나 끊는다. 없는 노트로 이으면 경고를 싣는다 |
| `write_attachment` | 노트나 하위 노트 폴더의 `attach/`에 글 첨부 1개(svg, md, txt, json 같은 글 확장자 12가지)를 쓴다. 같은 이름이면 덮고, 본문에 붙일 `ref`를 함께 답한다 |
| `create_sub_note` | 루트 노트나 하위 노트의 `sub/` 아래에 하위 노트 폴더와 `note.md`를 쓴다. 부모는 `노트id/하위id/…` 사슬이고 마디마다 직계 자식인지 본다 |
| `update_sub_note` | 하위 노트의 제목, 본문, 태그(명령 표면만)를 고치고 다시 계산한 폴더 이름으로 같은 `sub/` 안에서 폴더째 옮긴다. 본문만 주면 첫 헤딩이 제목이 된다 |
| `delete_sub_note` | 직계 컨테이너 본문에서 그 하위 노트의 카드 줄을 걷고 하위 노트 폴더를 휴지통 없이 통째로 지운다. 카드를 걷었으면 컨테이너의 앞뒤 토큰을 함께 답한다 |

도구 이름(`get_doc`)과 명령 이름(`get-doc`)을 모두 받습니다. `create_doc`은 vault에 워크스페이스 표식 `.aisurada/workspace.json`이 있어야 쓰고, 파일은 같은 폴더의 임시 파일에 쓴 뒤 원자적 이름 바꾸기로 둡니다. 이름 바꾸기가 실패하면(대상 자리에 폴더가 있는 것 같은) 다른 길로 다시 하지 않고 `AIS-500`으로 섭니다. `update_doc`, 메타를 고치는 3개, `update_sub_note`는 직전에 읽은 토큰(`f1`)이 지금 파일 바이트와 맞아야 쓰고, 안 왔거나 어긋나면 아무것도 안 쓰고 `AIS-412`와 종료 코드 5로 답합니다. 편집 도장(`editedBy`, `editedAt`)은 파일 글이 실제로 달라질 때만 새로 찍습니다. `update_doc`의 `agenda=<slug 또는 이름>`은 노트를 다른 아젠다로 옮깁니다. `create_doc`과 같은 차례(slug → 이름(대소문자 무시) → 이름을 slug로 만든 값)로 찾되, 없으면 새로 만들지 않고 아무것도 안 쓴 채 `AIS-404`로 섭니다. 첨부·후속 노트·문답은 폴더째 함께 옮겨지고, 후속 노트 머리의 `agenda:` 줄도 새 slug로 바뀝니다(그 줄만 바꾸고, 못 고친 파일은 `warning`에 적습니다). 옮겼으면 응답에 `agenda`(새 slug)가 붙습니다. AI 호출(MCP `update_doc`)도 옮길 수 있습니다. 도구 표면(AI 호출자)은 기록 층 노트(`tier: record`)를 고칠 수 없습니다. 노트 쓰기, 하위 노트 쓰기, 글 첨부, 휴지통 보내기는 그 노트를 찾자마자 `AIS-409`로 서고, 코멘트·첨부 목록과 휴지통 되살리기도 같고, 휴지통 목록에서는 기록 층 노트의 항목이 보이지 않습니다. 앱 호출자에는 이 제한이 없습니다. `history`는 노트의 git 판과 예전에 남은 스냅숏을 합쳐 조회하고, `commit-vault`는 현재 변경의 커밋 결과를 답합니다. 쓰기 명령은 모두 일하기 전에 vault를 훑어서, 노트로 못 읽는 파일 가운데 훑기를 세우는 것(깨진 YAML 명시 태그, 같은 slug를 다른 id로 쓰는 아젠다 같은 것)이 있으면 아무것도 안 쓰고 종료 코드 1로 섭니다. 4부(Claude 연동) 명령 7개(`get-ask`, `read-session` 등)도 같은 실행 파일에서 돕니다. 자동 커밋은 vault를 쓸 때마다 대기 기록(`vault/.git/aisurada-pending.json`)을 남기고, 마지막 쓰기 뒤 유휴 시간이 지나거나, 계속 써서 유휴가 오지 않아도 처음 쓴 지 15분이 지나거나, 작성자 또는 도구 세션이 바뀌면 커밋합니다. 유휴 타이머는 앱과 MCP 서버(`ais-cli mcp`)만 돌리고, 단발로 부른 `ais-cli`는 기록만 남겨 두었다가 다음에 도는 앱이나 MCP 서버가 커밋합니다. 시험용 환경 변수 `AISURADA_TEST_NOW`(시계. 도장과 평가 시각도 따른다)와 `AISURADA_TEST_RANDOM`(id의 무작위 조각)은 계약이 아니고, 주지 않으면 아무 일도 안 합니다. 도장에 적는 세션 값은 MCP 요청의 세션 표식이나 `AISURADA_SESSION_ID`와 `AISURADA_SESSION_AGENT`로만 받습니다.

## 19장 코멘트

코멘트 읽기 `list_comments`(도구) / `list-comments`(명령)와 명령 전용 쓰기 `create-comment`, `update-comment`, `delete-comment`를 지원합니다. 위 18장까지의 기본 명령 목록에 추가된 범위입니다.

```bash
dist/ais-cli list_comments '{"parent":"nddemo-00000001"}'
dist/ais-cli list-comments nddemo-00000001 sndemo-00000001
dist/ais-cli create-comment nddemo-00000001 'body=확인했습니다'
dist/ais-cli update-comment nddemo-00000001 'comment_id=<코멘트id>' 'body=고친 글'
dist/ais-cli delete-comment nddemo-00000001 'comment_id=<코멘트id>'
```

목록은 `absent`, `loaded`, `unreadable` 봉투를 구별합니다. 하위 노트 코멘트는 직계 컨테이너의 `sub-comments/<하위 id>.json`에서 읽고 씁니다. 쓰기 3개는 못 읽은 파일을 덮지 않고 AIS-409로 서며, f1을 받지 않습니다. 삭제는 코멘트가 달린 본문 마커를 먼저 걷고 코멘트 항목을 지웁니다. 본문이 바뀌었으면 앞뒤 토큰을 답하고, JSON/HTML 본문은 마커가 없어도 거절합니다.

코멘트 파일은 전체를 다시 쓰고 링크는 따라 씁니다. 짝 없는 서로게이트는 파일에서 소문자 탈출로 보존하고 stdout에서는 `?`로 냅니다. 쓰기 크기 상한 부재와 읽기에서 탈락한 항목의 다음 쓰기 소실 등 19.9의 미결정 호환 동작은 유지합니다.

이 구현은 호출마다 새로 훑으므로 옛 색인 행·코멘트 표의 낡음·본문 쓰기 뒤 색인 재삽입을 재현하지 않습니다(8.6). 코멘트 사본 읽기가 세우는 오류가 있으면 쓰기 전에 실패할 수 있습니다. 쓰기 공통 단계에서 작성자와 도구 세션을 기록하고, 작성자 전환 전 자동 커밋을 시도합니다.

Rust 시험 묶음은 `bash test.sh`로 돌립니다. [19장 원문](docs/engine-manual/19-comments.md), [Java 엔진 완료 노트](docs/review-chapter-19-engine-complete.md)를 참고하세요. 이전 [19장 앱 완료 노트](docs/review-chapter-19-complete.md)는 별도 aisurada TypeScript 작업 기록입니다.

## 20장 아젠다

아젠다를 읽고 만들고 고치고 지우는 명령 4개 `list-agendas`, `create-agenda`, `update-agenda`, `delete-agenda`를 지원합니다. 명령 표면(하이픈 이름)에만 있고, `list_agendas` 같은 밑줄 이름은 모르는 명령(종료 코드 2)입니다.

```bash
dist/ais-cli list-agendas
dist/ais-cli create-agenda 'title=광고 플랫폼' 'summary=광고 지표와 입찰을 다룬다.'
dist/ais-cli update-agenda <아젠다 id> 'title=새 이름' 'repo='
dist/ais-cli delete-agenda <아젠다 id>
```

- `list-agendas`는 읽기라 워크스페이스 표식이 없어도 답하고, slug 차례로 냅니다. `repo`와 `project`는 값이 없으면 키째 뺍니다.
- `create-agenda`는 같은 이름이 있어도 늘 새로 만들고 slug만 `-2`부터 `-100`까지 비켜섭니다. 색을 안 주면 12색 가운데 가장 덜 쓰인 색을 고릅니다. `create_doc`이 아젠다를 만들 때도 같은 색 규칙과 바이트를 씁니다.
- `update-agenda`는 준 칸만 바꾸고 파일을 통째로 다시 씁니다. `repo=`는 저장소를 지우고 `project=`는 미분류로 옮깁니다. slug와 파일 이름은 안 바뀝니다.
- `projectRepo=<이름>`은 아젠다가 속한 프로젝트의 저장소 목록(`repos`) 가운데 하나를 가리킵니다. `project`가 있어야 하고 그 프로젝트에 그 이름이 있어야 합니다. 빈 값은 가리킴을 지우고, 아젠다를 다른 프로젝트로 옮기거나 미분류로 보내면 낡은 가리킴도 같이 지웁니다.
- `delete-agenda`는 그 아젠다의 노트가 0건일 때만 파일을 `.aisurada/trash/agendas/<slug>-<아젠다 id>.md`로 옮깁니다. `notes/<slug>/` 폴더는 비어 있어도 안 지웁니다.
- 색인이 없는 판이라 부를 때마다 아젠다 파일을 읽습니다(8.6). 그래서 손으로 지운 아젠다는 목록에서 바로 빠지고, 지금 엔진이 색인에 남기는 행(20.9의 1번)은 재현하지 않습니다.

자세한 것은 [20장 완료 노트](docs/rebuild-p1-chapter20-complete.md)에 있습니다.

## 21장 프로젝트

아젠다를 묶는 프로젝트를 읽고 만들고 고치고 지우는 명령 4개 `list-projects`, `create-project`, `update-project`, `delete-project`를 지원합니다. 아젠다 명령처럼 명령 표면에만 있고, `list_projects` 같은 밑줄 이름은 모르는 명령(종료 코드 2)입니다.

```bash
dist/ais-cli list-projects
dist/ais-cli create-project 'title=광고 플랫폼' 'summary=광고 쪽 아젠다를 묶는다.'
dist/ais-cli update-project <프로젝트 id> 'title=새 이름' hue=teal
dist/ais-cli delete-project <프로젝트 id>
```

- 프로젝트는 `.aisurada/projects/<slug>.md` 파일 1개가 1개입니다. 키는 `id`, `slug`, `title`, `hue` 4개이고 본문이 `summary`입니다. 응답은 `id`, `slug`, `title`, `hue`, `summary` 차례이고 `deepLink`가 없습니다.
- `create-project`는 이름을 `title=`로 받습니다(자리 인자 없음). 같은 이름이 있으면 slug만 `-2`부터 `-100`까지 비켜서고, 색을 안 주면 프로젝트 색만 세어 가장 덜 쓰인 색을 고릅니다.
- `update-project`는 준 칸만 바꾸고 파일을 통째로 다시 씁니다. slug와 파일 이름은 안 바뀝니다.
- `repos=<JSON 배열>`로 프로젝트가 가진 저장소 목록을 통째로 바꿉니다(`create-project`도 받습니다). 항목은 `name`(필수), `path`(이 컴퓨터의 절대 경로), `github`(`owner/name`), `githubId`(양의 정수, `github`와 함께)이고, `path`나 `github` 가운데 하나는 있어야 합니다. 빈 값이나 `[]`는 목록을 지웁니다.
- `delete-project`는 그 slug를 `project`로 가리키는 아젠다가 0개일 때만 파일을 `.aisurada/trash/projects/<slug>-<프로젝트 id>.md`로 옮깁니다. 아젠다를 떼려면 `update-agenda <아젠다 id> project=`를 부릅니다.
- 지금 엔진은 프로젝트를 색인에만 두고 파일을 안 읽지만, 이 판은 색인이 없어 부를 때마다 프로젝트 파일을 설명서 2.2의 규칙으로 읽습니다(8.6). 그래서 손으로 둔 파일도 목록에 나오고, 목록은 id 차례(엔진이 발급한 id끼리는 만든 차례)입니다.

자세한 것은 [21장 완료 노트](docs/rebuild-p1-chapter21-complete.md)에 있습니다.

## 프로젝트 자원: 스킬과 에이전트 정의 (확장)

외부 설명서에 없는 확장입니다. 프로젝트에서 AI에게 작업을 시키기 위한 바탕으로, 스킬(`SKILL.md`가 든 폴더)과 에이전트 정의(마크다운 파일 하나)를 **글로벌**과 **프로젝트** 두 범위로 보관합니다. 프로젝트는 글로벌 자원을 **ID(`sk…`, `ap…`)로 참조**해야 쓸 수 있고, 참조하지 않은 것은 쓰지 않습니다. 명령줄로도, MCP 도구(`list_resources`, `save_resource` 등 10개)로도 쓸 수 있습니다. 쓰기 도구는 노트와 같이 `mcp --write`일 때만 보입니다. **AI가 쓴 변경은 정책에 따라 승인 대기에 올라가고, 사람이 승인한 판만 쓰입니다**(권한 올림·도구 칸 변경·참조 켜기는 늘 승인이 필요합니다).

```bash
dist/ais-cli save-resource kind=skill scope=global 'name=번역 검수' 'description=번역을 검수한다' 'body=# 지침'
dist/ais-cli save-resource kind=agent scope=global 'name=코드 리뷰어' 'description=코드를 리뷰한다' permission=edit 'skills=["<스킬 id>"]'
dist/ais-cli set-resource-ref <프로젝트 slug> <글로벌 자원 id> with-skills=true
dist/ais-cli usable-resources <프로젝트 slug>
dist/ais-cli list-resources scope=global
```

- 명령 8개: `list-resources`, `get-resource`, `save-resource`, `delete-resource`, `restore-resource`, `set-resource-ref`, `usable-resources`, `list-resource-versions`.
- 승인 명령 7개: `list-resource-changes`, `get-resource-change`, `approve-resource-change`, `reject-resource-change`, `resolve-resource-edit`, `get-resource-policy`, `set-resource-policy`. 승인·거절·정책 변경은 사람만 합니다(AI가 부르면 AIS-403).
- 파일은 `.aisurada/resources/` 아래에만 있어서 노트와 기존 명령을 건드리지 않습니다. 고칠 때는 노트처럼 직전에 읽은 `f1` 토큰이 필요하고, 고치기 전의 판은 30판까지 남습니다.
- 자세한 것은 [개발자 가이드](docs/developer-guide.md)의 「프로젝트 자원: 스킬과 에이전트 정의」에 있습니다.

## 태스크 (확장)

외부 설명서에 없는 확장입니다. 사람이 일을 티켓으로 적어 두면 러너가 집어 에이전트에게 맡기는 단위입니다. 태스크를 만들고 상태를 바꾸는 것(1단계)에 더해, 읽기 전용 조사와 수동 개발 실행을 맡는 러너가 있습니다. 앱의 레일에 「태스크」 메뉴가 있어 보드와 목록으로 보고, 태스크를 열어 상태 메뉴로 상태를 바꿉니다. 태스크는 키(`TSK-14`)나 ID(`tk…`)로 부르며, **완료 기준(`criteria`)을 하나 이상 적어야 만들 수 있습니다**. 일이 끝났는지 이 기준으로 확인하기 때문입니다.

```bash
dist/ais-cli create-task 'title=프로젝트를 지울 때 자원 폴더도 정리하기' repo=bnjjong/aisurada 'criteria=["지운 뒤 폴더가 남지 않는다","되살리면 돌아온다"]' type=bug
dist/ais-cli list-tasks status=draft
dist/ais-cli transition-task TSK-1 to=ready-for-develop
dist/ais-cli link-tasks TSK-1 TSK-2 type=blocks
```

- 명령 9개: `create-task`, `get-task`, `list-tasks`, `update-task`, `transition-task`, `link-tasks`, `add-task-comment`, `delete-task`, `restore-task`. MCP에는 읽기 도구 `list_tasks`, `get_task`와 쓰기 도구 `create_task`(Draft 만들기, `--write`일 때만)가 열립니다. 상태를 바꾸는 일은 사람(화면)과 러너만 합니다.
- 상태는 `draft`, `ready-for-develop`, `in-progress`, `waiting-answer`, `in-review`, `done`, `failed`, `cancelled` 8개이고, 누가 어떤 조건에서 바꾸는지는 `transition-task`가 한 곳에서 검사합니다.
- 파일은 `.aisurada/tasks/` 아래에만 있어서 노트와 기존 명령을 건드리지 않습니다. 자세한 것은 [개발자 가이드](docs/developer-guide.md)의 「태스크」에 있습니다.

## 러너 (확장)

Ready For Develop 상태의 태스크에서 **「지금 실행」**을 누르면 글로벌 자원에 저장된 공통 개발 프로세스 오케스트레이터가 개발·검사·리뷰를 맡깁니다. 승인된 변경은 작업 브랜치에 올리고 GitHub PR을 열어 태스크에 링크를 남깁니다. 머지는 사람이 합니다.

- 글로벌 자원에서 오케스트레이터, 개발, QA, PR 리뷰 에이전트를 활성화하고 모델을 설정합니다. 실행은 에이전트마다 설정된 모델과 추론 강도를 사용하고, 호출에 실패하면 다음 순위를 시도합니다.
- GitHub 앱 연결에 저장소 내용 쓰기 및 PR 쓰기 권한이 필요합니다. 검사는 macOS에서 별도 사본을 만들어 네트워크 없이 실행합니다.
- 「Ready가 되면 스스로 집기」를 켜면 Claude가 저장소와 노트를 읽고 조사 결과를 댓글로 남깁니다. 기본값은 꺼짐입니다.
- 태스크 상세의 「실행」에서 진행 기록, 질문과 답변, 중지, PR 링크를 확인할 수 있습니다. 실행 기록은 `~/Library/Application Support/aisurada/runs/`에 저장됩니다.
- [개발자 가이드의 러너 설명](docs/developer-guide.md#러너-runner)을 참고하세요.

## 알림 (확장)

외부 설명서에 없는 확장입니다. 태스크 시스템의 알림으로, 러너가 질문했을 때, 리뷰할 일이 생겼을 때(분석 보고), 실패했을 때, Done이 됐을 때 앱 안과 macOS 알림센터로 알립니다. 설정의 「시험 알림 보내기」로 가는 길을 확인할 수 있습니다. 레일 맨 위의 종 단추가 알림 센터(안 읽음·전체, 모두 읽음, 태스크로 가는 단추)를 엽니다.

- 설정 화면의 「알림」에서 앱 안과 macOS를 각각 끄고 켤 수 있고, 소식별로도 고릅니다(기본은 모두 켜고 Done의 macOS만 끕니다). 앱 안 채널을 꺼도 알림은 알림 센터에 쌓입니다. Telegram은 자리만 있습니다.
- macOS 집중 모드는 macOS가 처리합니다. 집중 모드에서 aisurada를 허용하지 않으면 배너가 막히고 알림 센터에 쌓입니다.
- 자세한 것은 [개발자 가이드](docs/developer-guide.md)의 「알림」에 있습니다.

## 결정 블록 (확장)

외부 설명서에 없는 확장입니다. AI가 사람이 골라야 할 것(「정할 것」)을 노트에 결정 블록으로 쓰면, 사람이 편집기에서 체크하거나 `답:` 줄에 답하고, AI는 그 답을 읽고 다음 일로 넘어갑니다. 채팅으로 `1 - 가`처럼 답하지 않아도 됩니다.

```markdown
> [!decision] 결정 1 · 명령을 실행해도 되나?
> - [ ] 가(추천): 읽기만 한다.
> - [ ] 나: 안 바꾸는 명령만 돌린다.
>
> 답:

> [!decided] 답을 다 적었으면 체크하세요
> - [ ] 답 다 했음
```

- `list_decisions`(도구) / `list-decisions <노트 id> [하위 노트 id]`(명령)가 결정마다 선택지, 체크한 것, 답 줄, 고른 것(`choice`, 답 줄이 체크보다 앞섬), 끝났는지(`finished`)를 냅니다. `pending-decisions`는 vault 전체에서 답을 기다리는 노트를 냅니다(앱의 배지와 알림이 씁니다).
- `wait-decisions`(아래 표)는 사람이 「답 다 했음」을 체크해 저장할 때까지 기다립니다. AI가 쓴 체크(`editedBy: tool`)는 세지 않습니다.
- MCP 서버의 안내문이 결정 블록 형식과 기다리는 법을 알려 줍니다. Claude Code는 백그라운드로 건 `wait-decisions`가 끝나면 세션이 다시 불립니다. Gemini CLI는 `~/.gemini/settings.json`에 `tools.shell.backgroundCompletionBehavior: "inject"`와 `experimental.modelSteering: true`를 **사용자가 켜야** 같은 동작을 합니다(둘 다 기본은 꺼짐). Codex는 쉬는 세션을 깨우지 못해 턴을 연 채 기다립니다.
- 앱의 편집기는 결정 블록에 「답 기다림」/「답함」 표시를 붙이고 한 블록에서 하나만 고르게 합니다. 레일의 「내 차례」 배지, 노트 목록의 「답 기다림」 표시, 알림 센터와 macOS 배너(설정의 「AI가 노트에서 답을 기다릴 때」)로도 알립니다.
- 자세한 것은 [개발자 가이드](docs/developer-guide.md)의 「결정 블록」에 있습니다.

## 22장 사실층

명령 전용 `get-agenda-facts`, `update-agenda-facts`, `facts-brief`를 지원합니다. 사실층은 `.aisurada/facts/<slug>.md`에 두고, AI는 도구 `get_doc`의 `agendaFacts`로 읽습니다.

```bash
dist/ais-cli get-agenda-facts <아젠다 id>
dist/ais-cli update-agenda-facts <아젠다 id> 'facts=지금도 참인 것'
dist/ais-cli facts-brief from=2026-06-16 to=2026-06-25
```

- 저장은 다듬은 글 2,000 UTF-16 단위까지 받고, LF 1개를 붙여 원자적으로 씁니다. `facts=`는 파일을 지웁니다.
- 편집용 읽기는 10,000단위까지 원문을 줍니다. 넘으면 `tooLargeToEdit: true`를 주고, AI용 읽기는 2,000단위를 넘으면 고정 안내 문장을 줍니다.
- 브리핑은 창 안 읽기 층 루트 노트를 셉니다. 사실층이 있으면 1건부터, 없으면 기본 5건부터 실으며, 노트 수 내림차순과 slug UTF-16 순으로 냅니다.
- `facts-brief`는 파일을 바꾸지 않아도 쓰기 깃발을 따라 워크스페이스 표식과 쓰기 전 훑기를 요구하고, 자동 커밋 대기 기록에 `app`으로 남습니다.

22.9의 읽기 버퍼·다듬기 경계 동작을 유지했습니다. 색인을 저장하지 않는 이 구현에서는 새 아젠다가 즉시 보이고, 지운 아젠다의 옛 행은 남지 않습니다. 시험은 `bash test.sh`로 돌립니다. 자세한 내용은 [22장 완료 노트](docs/rebuild-p1-chapter22-complete.md)에 있습니다.

## 23장 init-workspace

vault 폴더 1개를 워크스페이스로 갖추는 `init-workspace`를 지원합니다. 명령 표면에만 있고 `init_workspace`는 모르는 명령(종료 코드 2)입니다.

```bash
AISURADA_VAULT=/path/to/new-vault dist/ais-cli init-workspace name=새-vault 'templates=[]'
```

- 골격 폴더 `.aisurada/agendas`, `.aisurada/templates/doc`, `.aisurada/templates/ritual`, `notes`를 만들고, `templates=`로 받은 템플릿 가운데 그 파일 자리가 빈 것만 심고, 맨 마지막에 `.aisurada/workspace.json`을 없을 때만 씁니다. 아무것도 덮지 않아서 몇 번을 불러도 안전합니다.
- 응답은 `created`(표식을 이번에 썼는가), `seeded`(이번에 쓴 템플릿), `skipped`(자리가 차 있어 건너뛴 템플릿)입니다.
- `templates`는 JSON 배열(64개까지)이고, 항목마다 `category`가 `doc`이면 `id`, `name`, `format`, `desc`, `body`, `type`, `builtin`을, `ritual`이면 `id`, `name`, `format`, `desc`, `body`, `rev`, `icon`을 모두 받습니다. 모르는 키는 거절합니다.
- 쓰기 명령 가운데 이 명령만 워크스페이스 표식을 안 봅니다. 색인 맞추기와 vault git 처리는 다른 쓰기처럼 받고, 그 2가지가 `templates`와 `name` 검사보다 먼저 돕니다.

자세한 것은 [23장 완료 노트](docs/rebuild-p1-chapter23-complete.md)에 있습니다.

## 24장 템플릿

읽기 `list_templates` / `list-templates`, `get_template` / `get-template`에 쓰기 `save_template` / `save-template`, 명령 전용 `delete-doc-template`, `save-ritual-template`를 지원합니다.

```bash
dist/ais-cli save_template '{"name":"조사 노트","type":"research","body":"# 조사\n"}'
dist/ais-cli save-template id=<템플릿 id> 'desc=고친 설명'
dist/ais-cli save-ritual-template t-morning 'name=내 모닝'
dist/ais-cli delete-doc-template <템플릿 id>
```

새 문서 템플릿은 본문과 이름이 필요하고, 기존 템플릿은 준 칸만 바꿉니다. HTML·JSON에서 마크다운으로 고칠 때는 새 파일을 쓴 뒤 옛 확장자 파일을 지웁니다. 빌트인·리추얼 삭제는 거절하고, 리추얼 고치기는 같은 rev가 여러 파일에 있으면 거절합니다. 목록의 `bodies=true`와 못 읽은 파일의 진단도 지원합니다.

[24장 완료 문서](docs/rebuild-p1-chapter24-complete.md)에 파일 바이트, 호환 동작, 검증 결과와 붙인 글에서 해석이 필요한 두 경계를 기록했습니다.

설명서 원문 24장과 다시 맞댄 결과는 [24장 원문 맞대기](docs/rebuild-p1-chapter24-recheck.md)에 있습니다. 원문의 문구와 바이트로 짠 독립 시험 88건이 맞았고, 받아들임 묶음(`fixtures/expect`)에 진짜 묶음과 같은 자리의 템플릿 케이스 9건을 더했습니다.

## 26~30장

휴지통의 보내기·복원·영구 삭제·비우기, 마크다운 내보내기, 샤드 이동과 검증, 하루 플랜, 답 없는 코멘트 점검을 지원합니다. 기준은 제공된 26~30장 PDF이며, 각 장의 미결정 호환 동작을 유지합니다. (25장 회고/데일리 리포트 기능은 제거되었습니다.)

| 장 | 명령과 완료 문서 |
|---|---|
| 26 | `list_trash`, `delete_doc`, `restore_doc`, `purge_doc`와 짝 명령, `empty-trash` — [휴지통](docs/rebuild-p1-chapter26-complete.md) |
| 27 | `export_markdown` / `export-markdown` — [내보내기](docs/rebuild-p1-chapter27-complete.md) |
| 28 | `shard-notes`, `verify-shard` — [샤드](docs/rebuild-p1-chapter28-complete.md) |
| 29 | `plan_day` / `plan-day` — [일일 계획](docs/rebuild-p1-chapter29-complete.md) |
| 30 | `check-notes` — [노트 점검](docs/rebuild-p1-chapter30-complete.md) |

```bash
dist/ais-cli plan-day date=2026-06-22 dryRun=true
dist/ais-cli check-notes date=2026-06-22
```

샤드는 방향을 못박아 계획을 확인한 뒤 적용하고 기준값과 대조합니다. 방향은 환경 변수 `AISURADA_NOTES_SHARD_BY_MONTH`(`true`·`on`·`yes`·`1` / `false`·`off`·`no`·`0`)로 정하고, 계획을 확인하는 호출과 적용하는 호출에 같은 값을 줍니다.

```bash
AISURADA_NOTES_SHARD_BY_MONTH=true dist/ais-cli shard-notes
AISURADA_NOTES_SHARD_BY_MONTH=true dist/ais-cli shard-notes apply=true
dist/ais-cli verify-shard
```

회고·휴지통의 목록 진단, 플랜의 같은 제목 경고, 샤드의 불일치 진단은 stderr에도 냅니다. 시험은 `bash test.sh`로 돌립니다.

## 32장 4부 이음매

4부 명령 7개는 인자 문법과 쓰기 준비를 먼저 검사한 뒤 4부가 없으면 `AIS-409`로 답합니다. 노트 안의 `ask.json`과 `chats/`는 노트 폴더와 함께 이동·보관하고 색인과 마크다운 내보내기에서는 제외합니다. [32장 완료 문서](docs/rebuild-p1-chapter32-complete.md)에 검사 순서와 검증 결과를 기록했습니다.

## 33장 설정과 기기 위 파일

설정은 환경 변수로만 받습니다([환경 변수](#환경-변수) 표). Java 엔진이 받던 명령줄 `--키=값`, JVM 옵션 `-D키=값`, 색인 폴더 `AISURADA_DB_HOME`은 Java와 함께 없어졌습니다. 이 엔진은 영속 SQLite 색인 대신 vault를 다시 훑습니다. 기기 위 파일 가운데 잠금 폴더(`AISURADA_LOCK_DIR`)와 승인 기록 폴더(`AISURADA_APPROVALS_DIR`)는 환경 변수로 자리를 바꿀 수 있습니다. [33장 완료 문서](docs/rebuild-p1-chapter33-complete.md)는 Java 시절의 구현과 검증 범위를 적은 기록이라 지금 코드와 다를 수 있습니다.

## 34장 받아들임 검사

설명서 34장의 받아들임 묶음 `fixtures/expect`를 설명서만 보고 다시 지었습니다. 실행기가 케이스마다 엔진을 셸 없이 불러 종료 코드와 stdout 바이트를 맞댑니다. node 20 이상의 내장 모듈만 씁니다.

```bash
# Rust ais-cli(개발 빌드: cd engine-rs && cargo build -p ais-cli)
AISX_IMPL_BIN="$PWD/engine-rs/target/debug/ais-cli" \
  node fixtures/expect/run.mjs --engine fixtures/aisurada-rust.adapter.json --out /tmp/aisx-run-1
# 기준 Java 엔진
ORACLE="$(bash engine-rs/tools/compat/build-oracle.sh)"   # 태그 java-engine-final에서 지은 기준 jar
AISX_IMPL_JAVA="$(command -v java)" AISX_IMPL_JAR="$ORACLE/ais-engine.jar" \
  node fixtures/expect/run.mjs --engine fixtures/aisurada.adapter.json --out /tmp/aisx-run-2
node fixtures/expect/run.mjs --self-check --mutants --out /tmp/aisx-self-1
node fixtures/runner-test.mjs
```

- 실행기는 34.1의 옵션 문법, 묶음 무결성(sha256), `--out` 겹침(장치와 inode), 거르개, 판정 9가지, `report.json`, 자가 점검과 변형 8가지를 따릅니다. 어댑터 계약은 34.5이고, 이 엔진의 어댑터 `fixtures/aisurada.adapter.json`은 묶음 밖에 둡니다.
- Rust `ais-cli`로도 돌립니다. Rust는 JSON 객체 키를 이름 차례로 내서(Java 엔진은 넣은 차례) 기대값 27건 가운데 16건은 키 차례만 다른데, 어댑터 `fixtures/aisurada-rust.adapter.json`이 `compare.jsonKeyOrder=ignore`를 밝히면 실행기가 키 차례만 빼고 값과 바이트 꼴은 그대로 맞댑니다(설명서에 없는 이 저장소의 확장). 2026-11-16에 27건이 모두 맞았습니다. 돌리는 법은 [개발자 가이드](docs/developer-guide.md) §8에 있습니다.
- 진짜 묶음(읽기 레인 460건, 나무 `fixture.v1`)은 이 저장소에 없습니다. 그래서 레인에는 설명서 원문에 기대값이 글자 그대로 적힌 1부 케이스 27건만 담고, 입력 vault는 다시 짜기 개발 폴더의 픽스처 vault를 담았습니다. 진짜 묶음을 받으면 `lanes/`와 `trees/`를 바꾸고 `node fixtures/build-bundle.mjs`로 목록을 다시 적습니다.

자세한 것은 [34장 완료 노트](docs/rebuild-p1-chapter34-complete.md)에 있습니다.

34장 판 확인이 짚은 것은 [34장 판 확인 후속](docs/rebuild-p1-chapter34-review-fixes.md)에서 고쳤습니다. 33장 설정의 값 없는 칸을 설명서 3.1대로 되돌렸습니다. `append-ask-turn`, `append-chat-turn` 사용법의 줄임표는 부록 A.3대로 `…`로 바꿨습니다. 실행기는 두 실행기의 계약이 갈린 25가지 가운데 24가지를 지금 실행기와 같게 했습니다.

## 1~20장 점검에서 찾은 누락 보완

`reindex-paths`와 공통 쓰기의 자동 커밋·작성자 기록을 구현했습니다. 경로별 재색인은 명령 표면에서만 받고, 옵션 없이 경로 argv를 그대로 보냅니다.

```bash
dist/ais-cli reindex-paths notes/demo/hello/note.md
```

노트 내부 경로는 루트 노트로 접고 중복은 한 번만 엽니다. 200개를 넘겨 `mode: full`이 되어도 템플릿·회고·휴지통은 검사하지 않습니다. 색인이 없는 구현이므로 `indexed`는 이번에 읽은 노트 수, `unchanged`와 `removed`는 0입니다(8.6).

자동 커밋은 `.git` 폴더가 있는 `main` 브랜치에서 동작합니다. `AISURADA_VAULT_AUTOCOMMIT_ENABLED=false`(`off`·`no`·`0`도 같다)로 끄고, `AISURADA_VAULT_AUTOCOMMIT_IDLE_SECONDS`로 유휴 시간을 정합니다(기본 300초). 대기 기록은 `vault/.git/aisurada-pending.json`에 남고 프로세스 사이에 공유합니다. git 잠금·병합 중 상태는 건너뛰고, 실패는 재시도하되 노트 쓰기의 성공 여부를 바꾸지 않습니다. 훅·서명은 끄고 `*.tmp`를 제외한 vault 변경을 담습니다. `history <노트 id> [번호] [commit=<40자 커밋>] [query=<글자>]`는 수동·자동 커밋과 옛 스냅숏을 합쳐 보여 줍니다. `commit-vault`는 즉시 시도한 결과를 `outcome`과 선택적인 `blocked`로 돌려주며, git 실패도 종료 코드 0입니다. 쓰기와 같은 vault 잠금을 잡고 커밋하므로 다른 쪽이 쓰는 중이면 그 쓰기가 끝나기를 기다립니다. 두 명령은 명령 표면에만 있고 워크스페이스 표식이나 색인을 요구하지 않습니다. 본문을 덮기 직전에 옛 바이트를 남기던 스냅숏 쓰기(`AISURADA_VAULT_SNAPSHOT_ENABLED`)는 Rust 이전 때 지웠습니다. 이미 남은 스냅숏은 `history`가 계속 읽습니다. 자세한 계약과 유지한 경계는 [31장 완료 문서](docs/rebuild-p1-chapter31-complete.md)에 있습니다.

자동 커밋 시험은 `bash test.sh`가 실제 임시 git 저장소로 돌립니다. [누락 기능 완료 노트](docs/rebuild-p1-chapters01-20-gaps-complete.md)에 구현과 시험 근거를 기록했습니다.

## 빌드

Rust 툴체인(`cargo`)이 필요합니다. 의존 크레이트는 `Cargo.lock`으로 고정하고, 빌드는 `--locked`로 합니다.

```bash
./build.sh
```

산출물은 `dist/ais-cli` 하나입니다(Apple Silicon 맥 기준). 명령줄, MCP 서버(`ais-cli mcp`), 진단(`doctor`)을 모두 이 파일이 맡습니다. 데스크톱 앱 번들에도 같은 실행 파일을 `bin/ais-cli`로 넣습니다.

예전 Java 엔진의 기준 jar(맞대기용)가 필요하면 태그에서 지어 씁니다. 결과는 `engine-rs/target/oracle/`에 캐시합니다.

```bash
bash engine-rs/tools/compat/build-oracle.sh   # 지은 폴더를 출력한다. JDK 21 이상이 필요하다
```

## 실행

### 단발 호출

vault 폴더의 절대 경로를 환경 변수로 줍니다. 주지 않으면 앱이 지금 연 vault(`~/Library/Application Support/aisurada/current-vault.json`)를 따릅니다.

```bash
export AISURADA_VAULT=/path/to/vault

dist/ais-cli list-docs
dist/ais-cli search-docs 첫
dist/ais-cli get_doc '{"id":"nddemo-00000001"}'
dist/ais-cli list_tags '{}'
```

가장 작은 vault는 아젠다 파일 1개와 노트 1개입니다.

```text
vault/
├── .aisurada/agendas/demo.md
└── notes/demo/hello/note.md
```

```markdown
---
id: ags-00000001
slug: demo
title: 데모
---
```

```markdown
---
id: nddemo-00000001
agenda: demo
type: free
title: 첫 노트
created: '2026-06-16T10:00:00+09:00'
tags: [예시]
---
안녕하세요, 첫 노트입니다.
```

### 엔진 없이 도는 방식과 옛 사이드카

`ais-cli`는 엔진 프로세스를 띄우지 않고 호출마다 그 자리에서 vault를 읽고 씁니다. 소켓도 launchd 등록도 없습니다. 같은 vault에 동시에 쓰는 호출은 잠금 파일(`~/Library/Application Support/aisurada/locks/`, 환경 변수 `AISURADA_LOCK_DIR`로 바꿈)로 한 줄로 세웁니다.

Java 시절에 이 기기에서 `ais-cli`나 앱을 쓴 적이 있다면, vault마다 launchd에 올라간 옛 사이드카(레이블 `co.kcd.aisurada.sidecar.<키>`)가 남아 있을 수 있습니다. 앱은 새 판을 처음 열 때 한 번 정리합니다. 손으로 하려면 이렇게 합니다.

```bash
dist/ais-cli doctor          # 어느 vault를 쓰는지, 옛 사이드카 흔적, 에이전트에 등록된 MCP 서버를 보여 준다
dist/ais-cli sweep --dry-run # 걷어 낼 것만 보여 준다
dist/ais-cli sweep           # 옛 사이드카를 내리고 plist, 상태 파일, 깔아 둔 jar, 로그를 지운다
```

> **launchd를 건드립니다.** `sweep`은 레이블이 `co.kcd.aisurada.sidecar.`로 시작하는 항목만 `launchctl`로 내립니다. vault, 잠금 파일, 다른 프로그램의 launchd 항목은 건드리지 않습니다.

`ais-cli`가 스스로 하는 명령도 있습니다.

| 명령 | 하는 일 |
|---|---|
| `doctor` | 어느 vault를 쓰는지, vault의 git 상태, 이 실행 파일, 에이전트에 등록된 MCP 서버, 걷어 낼 옛 사이드카 흔적을 읽기만 하며 보여 준다. `--prune`은 걷어 내기까지, `--fix`는 낡은 MCP 등록을 다시 등록하는 데까지 한다 |
| `sweep` | 옛 사이드카를 모두 내리고 흔적을 지운다. `--dry-run`은 보여 주기만 한다 |
| `install` | 깔 파일이 없다는 안내만 한다. 옛 jar와 사이드카가 남아 있으면 `sweep`을 권한다 |
| `mcp` | Claude 같은 MCP 클라이언트가 노트를 도구로 쓰도록 MCP 서버로 선다(표준 입출력). 기본은 읽기 도구 20개만 내놓고, `--write`를 주면 쓰기 도구까지 39개를 내놓는다. 영구 삭제(`purge_doc`)는 내놓지 않는다. vault는 `AISURADA_VAULT`가 있으면 그것, 없으면 앱이 지금 연 vault(`~/Library/Application Support/aisurada/current-vault.json`)를 호출마다 따른다. 도구 목록과 쓰는 법은 [MCP 가이드](docs/mcp-guide.md) |
| `claude-mcp` | Claude Code에 위 서버를 등록한다. `status`, `add [--write]`, `remove`는 `claude mcp`를 부르고, `config [--write]`는 Claude Desktop 설정에 붙여 넣을 조각만 출력한다. 등록에는 vault를 박지 않는다 |
| `codex-mcp`, `gemini-mcp` | Codex, Antigravity(`agy`)·Gemini CLI에 같은 서버를 같은 방식으로 등록한다(`status`, `add [--write]`, `remove`) |
| `wait-decisions <노트 id>[/<하위 노트 id>] [timeout=<초>] [interval=<초>]` | 노트의 결정 블록에 사람이 답을 끝낼 때까지 기다린다. 몇 초(기본 5)마다 확인하다가 사람이 「답 다 했음」을 체크해 저장하면 답을 JSON(`result: "done"`)으로 내고 0으로 끝난다. 한도(기본 7200초)가 지나면 `result: "timeout"`과 종료 코드 6. 결정 블록이 없는 노트는 `AIS-400`(종료 코드 3) |
| `--help`, `--list` | 사용법과 이름 목록 |

### 환경 변수

| 이름 | 쓰임 | 기본값 |
|---|---|---|
| `AISURADA_VAULT` | vault 폴더. 없으면 앱이 연 vault를 따른다 | 없음 |
| `AISURADA_VAULT_AUTOCOMMIT_ENABLED` | vault 자동 커밋과 commit-vault 사용(`true`·`on`·`yes`·`1` / `false`·`off`·`no`·`0`, 대소문자 무시. 모르는 값이면 기본값) | true |
| `AISURADA_VAULT_AUTOCOMMIT_IDLE_SECONDS` | 마지막 쓰기부터 커밋까지의 유휴 초 | 300 |
| `AISURADA_NOTES_SHARD_BY_MONTH` | 노트를 월별 폴더(`notes/<아젠다>/YYYY-MM/`)에 두는지(`true`·`on`·`yes`·`1` / `false`·`off`·`no`·`0`) | 없으면 디스크를 보고 판정 |
| `AISURADA_LOCK_DIR` | 쓰기 잠금 파일 폴더 | `~/Library/Application Support/aisurada/locks` |
| `AISURADA_APPROVALS_DIR` | 자원 승인 기록의 바탕 폴더(시험용) | `~/Library/Application Support/aisurada/approvals` |
| `AISURADA_CLAUDE_BIN`, `AISURADA_CODEX_BIN`, `AISURADA_GEMINI_BIN`(또는 `AISURADA_ANTIGRAVITY_BIN`) | `claude-mcp`, `codex-mcp`, `gemini-mcp`가 부를 CLI의 자리 | 홈의 `.local/bin`, Homebrew, `/usr/local/bin`, `PATH` 차례로 찾음 |
| `AISURADA_SQLITE_BIN` | Antigravity 대화 기록을 읽을 `sqlite3`의 자리 | `/usr/bin/sqlite3`, 없으면 `PATH` |
| `AISURADA_TEST_NOW`, `AISURADA_TEST_RANDOM` | 시험용 시계(유닉스 밀리초)와 id 무작위 조각. 계약이 아니고 주지 않으면 아무 일도 안 한다 | 없음 |
| `AISURADA_SESSION_ID` | 이 호출을 보낸 에이전트 세션의 id. `AISURADA_SESSION_AGENT`와 함께 줄 때만 쓴다. `bin/ais-gemini`가 넣는다 | 없음 |
| `AISURADA_SESSION_AGENT` | 그 세션을 연 CLI 이름(`claude`, `codex`, `gemini` …). 소문자 영문으로 시작하고 32자까지 | 없음 |

`ais-cli`는 세션 id를 이 차례로 정합니다. MCP 요청에 Codex가 실은 `_meta.sessionId`, 위의 두 변수, 에이전트 고유 변수(`CLAUDE_CODE_SESSION_ID`, `CODEX_SESSION_ID`, `CODEX_THREAD_ID`, `ANTIGRAVITY_CONVERSATION_ID`, `GEMINI_SESSION_ID`, `AGY_CONVERSATION_ID`). 고유 변수가 둘 이상의 에이전트에서 동시에 보이면 어느 쪽 세션인지 알 수 없어 기록하지 않습니다. claude가 아닌 세션의 노트에는 `sessionAgent` 줄이 `sessionId` 바로 뒤에 붙고, 줄이 없으면 claude입니다. `bin/ais-gemini`는 Gemini CLI를 `--session-id <새 UUID>`로 띄우고 같은 값을 위의 두 변수로 내보내는 래퍼이고, `bin/ais-codex`는 Claude 셸에서 물려받은 세션 변수를 지우고 codex를 띄웁니다.

## 종료 코드

| 코드 | 뜻 |
|---|---|
| 0 | 성공. 답은 stdout에 JSON 하나와 줄바꿈 1개입니다 |
| 1 | 예상 밖 실패. 인자 길이가 1 MiB를 넘는 경우가 이것입니다. stderr에 한 줄이 나옵니다 |
| 2 | vault를 못 찾았거나 모르는 명령입니다 |
| 3 | `AIS-…` 코드가 붙은 실패 (`AIS-404` 노트 없음, `AIS-400` 인자 오류 등). stderr에 한 줄이 나오고 stdout은 비어 있습니다 |
| 5 | `AIS-412`. 읽었을 때의 확인 값(`f1`)이 지금 파일과 어긋나서 쓰지 않고 멈췄습니다 |
| 6 | `wait-decisions`의 한도가 지나 사람의 답 없이 끝났습니다. stdout에는 그때까지의 결정과 `result: "timeout"`이 있습니다 |

Java 시절에 `ais-cli`만 내던 종료 코드 4(엔진에 못 닿음)는 엔진 프로세스가 없어져서 사라졌습니다. `ais-cli mcp`는 표준 입력이 닫히면 종료합니다.

## 소스 구성

Cargo 작업공간 `engine-rs/`에 크레이트 둘이 있습니다. 시험과 맞대기 도구는 각 크레이트의 `tests/`, `examples/`와 `engine-rs/tools/`에 있습니다.

| 자리 | 맡은 일 |
|---|---|
| `crates/ais-core/src/commands/` | 명령 1개당 파일 1개. 읽기와 쓰기 명령의 본체 |
| `crates/ais-core/src/vault/` | 아젠다, 프로젝트, 노트, 템플릿, 회고, 휴지통을 훑어 읽는 부품과 git 이력 읽기 |
| `crates/ais-core/src/write/` | 쓰기 공통 부품. 표식 확인, 원자적 쓰기, 잠금, 폴더 옮기기, 샤드 판정, id와 폴더 이름, 경고, 선결 조건 토큰, 편집 도장, 자동 커밋 |
| `crates/ais-kernel/` | 바닥 크레이트. 글·JSON 규칙(`text`, `json`), 오류 값(`error`), 프론트매터 YAML 읽기와 쓰기(`frontmatter`, SnakeYAML이 하던 일), YAML 방출기(`snake_dump`), 공용 커널(`kernel`). 다른 영역을 모르고 파일·시계에 손대지 않는다 |
| `crates/ais-infra/` | 인프라 크레이트. 바깥 세상(시계, 난수, 환경 변수, 잠금, git, sqlite3, vault 저장소)의 약속 `ports`와 기본 구현 `adapters`. `ais-kernel`만 의존한다 |
| `crates/ais-domain/` | 도메인 크레이트. 노트(`note`), 태스크의 모양·상태 전이·집합체(`task::{format, transitions, aggregate}`), 자원의 모양·승인 규칙·인자와 참조 규칙(`resource::{format, aggregate, rules}`). `ais-kernel`만 의존해서 포트·어댑터·vault 읽기와 쓰기를 쓰면 컴파일이 안 된다 |
| `crates/ais-core/src/{task,resource,session}/` | 태스크, 스킬·에이전트 정의와 승인, 에이전트 세션 읽기 |
| `crates/ais-core/src/engine.rs`, `engine/table.rs` | 엔진 진입점과, 이름·읽기/쓰기·쓰기 범위·요청 종류·처리 함수를 한 줄에 적은 명령 표 |
| `crates/ais-cli/src/` | 명령줄 진입점(`main.rs`, `lib.rs`), 인자 읽기(`parse.rs`, `map.rs`, `tool.rs`), 옛 응답 모양으로 바꾸기(`legacy.rs`), 키 차례를 맞춘 JSON 쓰기(`output.rs`) |
| `crates/ais-cli/src/mcp.rs`, `mcp_tools.rs` | MCP 서버(`ais-cli mcp`)와 도구 목록 |
| `crates/ais-ops/src/{register,agent_cli}.rs` | `claude-mcp`, `codex-mcp`, `gemini-mcp` 등록 |
| `crates/ais-ops/src/{doctor,install,old_sidecar,migrate}.rs` | `doctor`, `sweep`, `install`, 옛 사이드카 걷어 내기와 첫 실행 이주 |
| `crates/ais-ops/src/env.rs` | 프로세스 환경(환경 변수, 홈, 현재 폴더)을 값으로 묶고 vault와 세션 표식을 정하는 규칙 |
| `engine-rs/tools/compat/` | Java 기준 jar를 태그에서 짓는 `build-oracle.sh`와 합성 vault를 만드는 도구들 |

## 검증 요청

검증 절차와 시험 진입점은 [개발자 가이드](docs/developer-guide.md)에 있습니다. Java 시절의 검증 인계 문서 [`docs/handoff-round2.md`](docs/handoff-round2.md)는 기록으로 남아 있습니다.

## 라이선스

[MIT](LICENSE)

## 2부 네이티브 호스트

Tauri 2 호스트의 1~2장 기반은 [native-host](native-host/README.md)에 있습니다. 번들 설정과 권한, main 창·메뉴·딥링크 및 종료 이벤트를 연결했습니다. [1장 구현 기록](docs/rebuild-p2-chapter01-complete.md)과 [2장 구현 기록](docs/rebuild-p2-chapter02-complete.md)에 검증과 후속 범위를 적었습니다.

3~4장의 공통 IPC 규약과 vault 커맨드 네 개도 연결했습니다. [3장 완료 기록](docs/rebuild-p2-chapter03-complete.md)과 [4장 완료 기록](docs/rebuild-p2-chapter04-complete.md)에 실제 Tauri IPC 검증과 경계 동작을 정리했습니다.

5~6장의 `sidecar_call`과 자식 프로세스 실행·환경 정리도 연결했습니다. [5장 완료 기록](docs/rebuild-p2-chapter05-complete.md)과 [6장 완료 기록](docs/rebuild-p2-chapter06-complete.md)은 구현 당시 Java 엔진을 쓰던 때의 기록(Java 엔진 왕복, jar 탐색, 시간 초과 검증)입니다. 지금 호스트는 엔진을 프로세스 안에서 직접 부르므로 jar 탐색은 없습니다([개발자 가이드](docs/developer-guide.md)의 「호출 방식」).

7~8장의 그림 첨부 쓰기·기본 앱 열기·자기 쓰기 알림도 지원합니다. [7장 완료 기록](docs/rebuild-p2-chapter07-complete.md)과 [8장 완료 기록](docs/rebuild-p2-chapter08-complete.md)에 파일 경계와 실제 Tauri IPC 이벤트 검증을 기록했습니다.

9~18장의 외부 Markdown, HTML 첨부 프로토콜, 노트 창 복원·모니터별 배치, 실제 PTY와 비밀 가림까지 구현했습니다. [9장](docs/rebuild-p2-chapter09-complete.md) · [10장](docs/rebuild-p2-chapter10-complete.md) · [11장](docs/rebuild-p2-chapter11-complete.md) · [12장](docs/rebuild-p2-chapter12-complete.md) · [13장](docs/rebuild-p2-chapter13-complete.md) · [14장](docs/rebuild-p2-chapter14-complete.md) · [15장](docs/rebuild-p2-chapter15-complete.md) · [16장](docs/rebuild-p2-chapter16-complete.md) · [17장](docs/rebuild-p2-chapter17-complete.md) · [18장](docs/rebuild-p2-chapter18-complete.md)에 장별 구현과 검증 범위를 적었습니다. 실제 macOS webview의 26개 자동 점검은 `cd native-host && pnpm test:acceptance`로 실행합니다. 4부 Rust 모듈은 포함하지 않습니다. 3부 편집 화면 검사는 native-host의 별도 하네스로 실행합니다.
