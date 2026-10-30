# aisurada

withlog vault를 읽어 JSON으로 답하고 새 노트를 쓰는 엔진과, 그 엔진을 띄워 두고 부르는 클라이언트 `wl-cli`입니다. Java 21 하나와 SnakeYAML만으로 짓습니다.

이름은 〈신세기 GPX 사이버 포뮬러〉의 아스라다에서 따온 말장난(AI + 아스라다)입니다. 사람 옆에서 달리는 AI라는 자리가 마음에 들었습니다.

## 하는 일

vault 폴더를 부를 때마다 훑어서 답합니다. 색인 파일을 만들지 않습니다. vault에 쓰는 명령은 31개입니다. 19장의 코멘트 쓰기 3개, 20장의 아젠다 쓰기 3개, 21장의 프로젝트 쓰기 3개, 22장의 사실층 쓰기 1개, 23장의 `init-workspace`, 24장의 템플릿 쓰기 3개, 25~30장의 회고·휴지통·내보내기·샤드·플랜 쓰기 8개를 포함합니다. 기본 노트 쓰기는 새 노트를 만드는 `create_doc`, 노트의 제목과 본문을 고치고 정식 자리로 옮기는 `update_doc`, 노트 메타를 고치는 `set_favorite`, `evaluate_doc`, `link_docs`, 노트나 하위 노트에 글 첨부를 쓰는 `write_attachment`, 그리고 하위 노트를 만들고 고치고 지우는 `create_sub_note`, `update_sub_note`, `delete_sub_note`입니다.

엔진은 두 가지 모양으로 돕니다.

- **단발 모드**: 명령 1개를 처리하고 끝납니다.
- **serve 모드**: vault 1개를 맡아 유닉스 도메인 소켓에서 요청을 받습니다. 요청마다 vault를 다시 훑어서, 떠 있는 동안 바뀐 파일도 다음 요청에 보입니다. 10분 동안 요청이 없으면 스스로 내려갑니다.

`wl-cli`는 serve 모드 엔진에 붙어 명령을 보내고 답을 그대로 흘립니다. 떠 있는 엔진이 없으면 macOS launchd에 LaunchAgent로 등록해 띄우고 30초까지 기다립니다.

| 명령 | 답하는 것 |
|---|---|
| `list_docs` | 노트 목록 (유형, 태그, 날짜, 즐겨찾기 필터) |
| `search_docs` | 제목, 메타, 본문, 첨부 글을 찾아 대목과 함께 |
| `list_tags` | 태그와 쓰인 횟수 |
| `get_doc` | 노트 1건 (본문, 도장, 코멘트) |
| `list_sub_notes`, `get_sub_note` | 하위 노트 목록과 1건 |
| `list_attachments` | 노트나 하위 노트의 첨부 |
| `list_templates`, `get_template` | 문서·리추얼 템플릿 |
| `list_reports`, `get_report` | 회고 |
| `list_trash` | 휴지통 |
| `reindex` | vault를 한 번 훑어 읽지 못한 파일이 있는지 알려 준다 |
| `reindex-paths` | 지정한 경로만 훑는다. 받은 자리 인자가 200개를 넘으면 노트 전체로 전환한다 |
| `create_doc` | 새 노트 1건을 쓴다. 없는 아젠다면 아젠다 파일도 만들고, 같은 제목이나 들여쓴 블록 같은 경고 5가지를 함께 답한다 |
| `update_doc` | 제목, 본문, 태그, 템플릿, 날짜, 시각, 소요 시간을 고치고 그 값으로 다시 계산한 자리로 옮긴다. 폴더 노트는 폴더째 옮기고, 옛 단일 파일은 폴더로 올리거나 이름만 바꾼다 |
| `set_favorite` | 즐겨찾기 별과 라벨을 고친다. 값이 그대로면 파일을 안 연다 |
| `evaluate_doc` | 평가(점수, 메모, 시각)를 새기거나 지운다 |
| `link_docs` | 다른 노트로의 연결 1건을 잇거나 끊는다. 없는 노트로 이으면 경고를 싣는다 |
| `write_attachment` | 노트나 하위 노트 폴더의 `attach/`에 글 첨부 1개(svg, md, txt, json 같은 글 확장자 12가지)를 쓴다. 같은 이름이면 덮고, 본문에 붙일 `ref`를 함께 답한다 |
| `create_sub_note` | 루트 노트나 하위 노트의 `sub/` 아래에 하위 노트 폴더와 `note.md`를 쓴다. 부모는 `노트id/하위id/…` 사슬이고 마디마다 직계 자식인지 본다 |
| `update_sub_note` | 하위 노트의 제목, 본문, 태그(명령 표면만)를 고치고 다시 계산한 폴더 이름으로 같은 `sub/` 안에서 폴더째 옮긴다. 본문만 주면 첫 헤딩이 제목이 된다 |
| `delete_sub_note` | 직계 컨테이너 본문에서 그 하위 노트의 카드 줄을 걷고 하위 노트 폴더를 휴지통 없이 통째로 지운다. 카드를 걷었으면 컨테이너의 앞뒤 토큰을 함께 답한다 |

도구 이름(`get_doc`)과 명령 이름(`get-doc`)을 모두 받습니다. `create_doc`은 vault에 워크스페이스 표식 `.withlog/workspace.json`이 있어야 쓰고, 파일은 같은 폴더의 임시 파일에 쓴 뒤 원자적 이름 바꾸기로 둡니다. 이름 바꾸기가 실패하면(대상 자리에 폴더가 있는 것 같은) 다른 길로 다시 하지 않고 `WL-500`으로 섭니다. `update_doc`, 메타를 고치는 3개, `update_sub_note`는 직전에 읽은 토큰(`f1`)이 지금 파일 바이트와 맞아야 쓰고, 안 왔거나 어긋나면 아무것도 안 쓰고 `WL-412`와 종료 코드 5로 답합니다. 편집 도장(`editedBy`, `editedAt`)은 파일 글이 실제로 달라질 때만 새로 찍습니다. `history`는 노트의 git 판과 옛 스냅숏을 합쳐 조회하고, `commit-vault`는 현재 변경의 커밋 결과를 답합니다. 쓰기 명령은 모두 일하기 전에 vault를 훑어서, 노트로 못 읽는 파일 가운데 훑기를 세우는 것(깨진 YAML 명시 태그, 같은 slug를 다른 id로 쓰는 아젠다 같은 것)이 있으면 아무것도 안 쓰고 종료 코드 1로 섭니다. 4부(Claude 연동) 명령 7개(`get-ask`, `read-session` 등)는 이름 인자와 자리 인자 검사를 먼저 거친 뒤 `WL-409 이 판에는 4부(Claude 연동)가 없다.`로 답합니다. serve는 시작·종료·유휴·15분 경과·작성자 또는 도구 세션 전환 때 vault를 자동 커밋합니다. 단발 모드는 자동 커밋하지 않습니다. 시험용 JVM 속성 `withlog.test.now`(시계. 도장과 평가 시각도 따른다)와 `withlog.test.random`(id의 무작위 조각)은 계약이 아니고, 주지 않으면 아무 일도 안 합니다. 도장에 적는 세션 값은 소켓 요청의 세션 표식으로만 받고, 그것을 대신하는 시험용 속성은 없습니다.

## 19장 코멘트

코멘트 읽기 `list_comments`(도구) / `list-comments`(명령)와 명령 전용 쓰기 `create-comment`, `update-comment`, `delete-comment`를 지원합니다. 위 18장까지의 기본 명령 목록에 추가된 범위입니다.

```bash
java -jar dist/wl-engine.jar list_comments '{"parent":"nddemo-00000001"}' --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar list-comments nddemo-00000001 sndemo-00000001 --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar create-comment nddemo-00000001 'body=확인했습니다' --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar update-comment nddemo-00000001 'comment_id=<코멘트id>' 'body=고친 글' --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar delete-comment nddemo-00000001 'comment_id=<코멘트id>' --withlog.vault=/path/to/vault
```

목록은 `absent`, `loaded`, `unreadable` 봉투를 구별합니다. 하위 노트 코멘트는 직계 컨테이너의 `sub-comments/<하위 id>.json`에서 읽고 씁니다. 쓰기 3개는 못 읽은 파일을 덮지 않고 WL-409로 서며, f1을 받지 않습니다. 삭제는 코멘트가 달린 본문 마커를 먼저 걷고 코멘트 항목을 지웁니다. 본문이 바뀌었으면 앞뒤 토큰을 답하고, JSON/HTML 본문은 마커가 없어도 거절합니다.

코멘트 파일은 전체를 다시 쓰고 링크는 따라 씁니다. 짝 없는 서로게이트는 파일에서 소문자 탈출로 보존하고 stdout에서는 `?`로 냅니다. 쓰기 크기 상한 부재와 읽기에서 탈락한 항목의 다음 쓰기 소실 등 19.9의 미결정 호환 동작은 유지합니다.

이 구현은 호출마다 새로 훑으므로 옛 색인 행·코멘트 표의 낡음·본문 쓰기 뒤 색인 재삽입을 재현하지 않습니다(8.6). 코멘트 사본 읽기가 세우는 오류가 있으면 쓰기 전에 실패할 수 있습니다. 쓰기 공통 단계에서 작성자와 도구 세션을 기록하고, 작성자 전환 전 자동 커밋을 시도합니다.

격리된 실제 Java 엔진/파일 및 serve·wl-cli 검증은 `bash test.sh`로 실행합니다. 외부 시험 프레임워크 없이 JDK 21만 사용하며 임시 vault·HOME·소켓만 만들고 지웁니다. [19장 원문](docs/engine-manual/19-comments.md), [Java 엔진 완료 노트](docs/review-chapter-19-engine-complete.md)를 참고하세요. 이전 [19장 앱 완료 노트](docs/review-chapter-19-complete.md)는 별도 withlog TypeScript 작업 기록입니다.

## 20장 아젠다

아젠다를 읽고 만들고 고치고 지우는 명령 4개 `list-agendas`, `create-agenda`, `update-agenda`, `delete-agenda`를 지원합니다. 명령 표면(하이픈 이름)에만 있고, `list_agendas` 같은 밑줄 이름은 모르는 명령(종료 코드 2)입니다.

```bash
java -jar dist/wl-engine.jar list-agendas --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar create-agenda 'title=광고 플랫폼' 'summary=광고 지표와 입찰을 다룬다.' --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar update-agenda <아젠다 id> 'title=새 이름' 'repo=' --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar delete-agenda <아젠다 id> --withlog.vault=/path/to/vault
```

- `list-agendas`는 읽기라 워크스페이스 표식이 없어도 답하고, slug 차례로 냅니다. `repo`와 `project`는 값이 없으면 키째 뺍니다.
- `create-agenda`는 같은 이름이 있어도 늘 새로 만들고 slug만 `-2`부터 `-100`까지 비켜섭니다. 색을 안 주면 12색 가운데 가장 덜 쓰인 색을 고릅니다. `create_doc`이 아젠다를 만들 때도 같은 색 규칙과 바이트를 씁니다.
- `update-agenda`는 준 칸만 바꾸고 파일을 통째로 다시 씁니다. `repo=`는 저장소를 지우고 `project=`는 미분류로 옮깁니다. slug와 파일 이름은 안 바뀝니다.
- `delete-agenda`는 그 아젠다의 노트가 0건일 때만 파일을 `.withlog/trash/agendas/<slug>-<아젠다 id>.md`로 옮깁니다. `notes/<slug>/` 폴더는 비어 있어도 안 지웁니다.
- 색인이 없는 판이라 부를 때마다 아젠다 파일을 읽습니다(8.6). 그래서 손으로 지운 아젠다는 목록에서 바로 빠지고, 지금 엔진이 색인에 남기는 행(20.9의 1번)은 재현하지 않습니다.

자세한 것은 [20장 완료 노트](docs/rebuild-p1-chapter20-complete.md)에 있습니다.

## 21장 프로젝트

아젠다를 묶는 프로젝트를 읽고 만들고 고치고 지우는 명령 4개 `list-projects`, `create-project`, `update-project`, `delete-project`를 지원합니다. 아젠다 명령처럼 명령 표면에만 있고, `list_projects` 같은 밑줄 이름은 모르는 명령(종료 코드 2)입니다.

```bash
java -jar dist/wl-engine.jar list-projects --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar create-project 'title=광고 플랫폼' 'summary=광고 쪽 아젠다를 묶는다.' --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar update-project <프로젝트 id> 'title=새 이름' hue=teal --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar delete-project <프로젝트 id> --withlog.vault=/path/to/vault
```

- 프로젝트는 `.withlog/projects/<slug>.md` 파일 1개가 1개입니다. 키는 `id`, `slug`, `title`, `hue` 4개이고 본문이 `summary`입니다. 응답은 `id`, `slug`, `title`, `hue`, `summary` 차례이고 `deepLink`가 없습니다.
- `create-project`는 이름을 `title=`로 받습니다(자리 인자 없음). 같은 이름이 있으면 slug만 `-2`부터 `-100`까지 비켜서고, 색을 안 주면 프로젝트 색만 세어 가장 덜 쓰인 색을 고릅니다.
- `update-project`는 준 칸만 바꾸고 파일을 통째로 다시 씁니다. slug와 파일 이름은 안 바뀝니다.
- `delete-project`는 그 slug를 `project`로 가리키는 아젠다가 0개일 때만 파일을 `.withlog/trash/projects/<slug>-<프로젝트 id>.md`로 옮깁니다. 아젠다를 떼려면 `update-agenda <아젠다 id> project=`를 부릅니다.
- 지금 엔진은 프로젝트를 색인에만 두고 파일을 안 읽지만, 이 판은 색인이 없어 부를 때마다 프로젝트 파일을 설명서 2.2의 규칙으로 읽습니다(8.6). 그래서 손으로 둔 파일도 목록에 나오고, 목록은 id 차례(엔진이 발급한 id끼리는 만든 차례)입니다.

자세한 것은 [21장 완료 노트](docs/rebuild-p1-chapter21-complete.md)에 있습니다.

## 22장 사실층

명령 전용 `get-agenda-facts`, `update-agenda-facts`, `facts-brief`를 지원합니다. 사실층은 `.withlog/facts/<slug>.md`에 두고, AI는 도구 `get_doc`의 `agendaFacts`로 읽습니다.

```bash
java -jar dist/wl-engine.jar get-agenda-facts <아젠다 id> --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar update-agenda-facts <아젠다 id> 'facts=지금도 참인 것' --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar facts-brief from=2026-06-16 to=2026-06-25 --withlog.vault=/path/to/vault
```

- 저장은 다듬은 글 2,000 UTF-16 단위까지 받고, LF 1개를 붙여 원자적으로 씁니다. `facts=`는 파일을 지웁니다.
- 편집용 읽기는 10,000단위까지 원문을 줍니다. 넘으면 `tooLargeToEdit: true`를 주고, AI용 읽기는 2,000단위를 넘으면 고정 안내 문장을 줍니다.
- 브리핑은 창 안 읽기 층 루트 노트를 셉니다. 사실층이 있으면 1건부터, 없으면 기본 5건부터 실으며, 노트 수 내림차순과 slug UTF-16 순으로 냅니다.
- `facts-brief`는 파일을 바꾸지 않아도 쓰기 깃발을 따라 워크스페이스 표식과 쓰기 전 훑기를 요구하고, serve의 자동 커밋 대기 기록에 `app`으로 남습니다.

22.9의 읽기 버퍼·다듬기 경계 동작을 유지했습니다. 색인을 저장하지 않는 이 구현에서는 새 아젠다가 즉시 보이고, 지운 아젠다의 옛 행은 남지 않습니다. `bash test.sh`가 사실층 파일·UTF-8 경계·serve와 `wl-cli` 왕복을 검증합니다. 자세한 내용은 [22장 완료 노트](docs/rebuild-p1-chapter22-complete.md)에 있습니다.

## 23장 init-workspace

vault 폴더 1개를 워크스페이스로 갖추는 `init-workspace`를 지원합니다. 명령 표면에만 있고 `init_workspace`는 모르는 명령(종료 코드 2)입니다.

```bash
java -jar dist/wl-engine.jar init-workspace name=새-vault 'templates=[]' --withlog.vault=/path/to/new-vault
```

- 골격 폴더 `.withlog/agendas`, `.withlog/templates/doc`, `.withlog/templates/ritual`, `notes`를 만들고, `templates=`로 받은 템플릿 가운데 그 파일 자리가 빈 것만 심고, 맨 마지막에 `.withlog/workspace.json`을 없을 때만 씁니다. 아무것도 덮지 않아서 몇 번을 불러도 안전합니다.
- 응답은 `created`(표식을 이번에 썼는가), `seeded`(이번에 쓴 템플릿), `skipped`(자리가 차 있어 건너뛴 템플릿)입니다.
- `templates`는 JSON 배열(64개까지)이고, 항목마다 `category`가 `doc`이면 `id`, `name`, `format`, `desc`, `body`, `type`, `builtin`을, `ritual`이면 `id`, `name`, `format`, `desc`, `body`, `rev`, `icon`을 모두 받습니다. 모르는 키는 거절합니다.
- 쓰기 명령 가운데 이 명령만 워크스페이스 표식을 안 봅니다. 색인 맞추기와 vault git 처리는 다른 쓰기처럼 받고, 그 2가지가 `templates`와 `name` 검사보다 먼저 돕니다.

자세한 것은 [23장 완료 노트](docs/rebuild-p1-chapter23-complete.md)에 있습니다.

## 24장 템플릿

읽기 `list_templates` / `list-templates`, `get_template` / `get-template`에 쓰기 `save_template` / `save-template`, 명령 전용 `delete-doc-template`, `save-ritual-template`를 지원합니다.

```bash
java -jar dist/wl-engine.jar save_template '{"name":"조사 노트","type":"research","body":"# 조사\n"}' --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar save-template id=<템플릿 id> 'desc=고친 설명' --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar save-ritual-template t-morning 'name=내 모닝' --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar delete-doc-template <템플릿 id> --withlog.vault=/path/to/vault
```

새 문서 템플릿은 본문과 이름이 필요하고, 기존 템플릿은 준 칸만 바꿉니다. HTML·JSON에서 마크다운으로 고칠 때는 새 파일을 쓴 뒤 옛 확장자 파일을 지웁니다. 빌트인·리추얼 삭제는 거절하고, 리추얼 고치기는 같은 rev가 여러 파일에 있으면 거절합니다. 목록의 `bodies=true`와 못 읽은 파일의 진단도 지원합니다.

[24장 완료 문서](docs/rebuild-p1-chapter24-complete.md)에 파일 바이트, 호환 동작, 검증 결과와 붙인 글에서 해석이 필요한 두 경계를 기록했습니다.

설명서 원문 24장과 다시 맞댄 결과는 [24장 원문 맞대기](docs/rebuild-p1-chapter24-recheck.md)에 있습니다. 원문의 문구와 바이트로 짠 독립 시험 88건이 맞았고, 받아들임 묶음(`fixtures/expect`)에 진짜 묶음과 같은 자리의 템플릿 케이스 9건을 더했습니다.

## 25~30장

회고 저장, 휴지통의 보내기·복원·영구 삭제·비우기, 마크다운 내보내기, 샤드 이동과 검증, 하루 플랜, 답 없는 코멘트 점검을 지원합니다. 기준은 제공된 25~30장 PDF이며, 각 장의 미결정 호환 동작을 유지합니다.

| 장 | 명령과 완료 문서 |
|---|---|
| 25 | `list_reports`, `get_report`, `save_report`와 짝 명령 — [회고](docs/rebuild-p1-chapter25-complete.md) |
| 26 | `list_trash`, `delete_doc`, `restore_doc`, `purge_doc`와 짝 명령, `empty-trash` — [휴지통](docs/rebuild-p1-chapter26-complete.md) |
| 27 | `export_markdown` / `export-markdown` — [내보내기](docs/rebuild-p1-chapter27-complete.md) |
| 28 | `shard-notes`, `verify-shard` — [샤드](docs/rebuild-p1-chapter28-complete.md) |
| 29 | `plan_day` / `plan-day` — [일일 계획](docs/rebuild-p1-chapter29-complete.md) |
| 30 | `check-notes` — [노트 점검](docs/rebuild-p1-chapter30-complete.md) |

```bash
java -jar dist/wl-engine.jar save-report kind=evening date=2026-06-16 'body=오늘의 회고' --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar plan-day date=2026-06-22 dryRun=true --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar check-notes date=2026-06-22 --withlog.vault=/path/to/vault
```

샤드는 방향을 못박아 계획을 확인한 뒤 적용하고 기준값과 대조합니다. `wl-cli` 뒤의 설정 옵션은 이미 뜬 엔진의 방향을 바꾸지 않으므로, 방향을 정하는 이관은 단발 모드로 실행합니다.

```bash
java -jar dist/wl-engine.jar shard-notes --withlog.notes.shard.by.month=true --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar shard-notes apply=true --withlog.notes.shard.by.month=true --withlog.vault=/path/to/vault
java -jar dist/wl-engine.jar verify-shard --withlog.vault=/path/to/vault
```

회고·휴지통의 목록 진단, 플랜의 같은 제목 경고, 샤드의 불일치 진단은 소켓 응답의 stderr에도 전달합니다. `bash test.sh`는 장별 시험과 실제 serve·wl-cli 왕복을 함께 실행합니다.

## 32장 4부 이음매

4부 명령 7개는 인자 문법과 쓰기 준비를 먼저 검사한 뒤 4부가 없으면 `WL-409`로 답합니다. 노트 안의 `ask.json`과 `chats/`는 노트 폴더와 함께 이동·보관하고 색인과 마크다운 내보내기에서는 제외합니다. [32장 완료 문서](docs/rebuild-p1-chapter32-complete.md)에 검사 순서와 검증 결과를 기록했습니다.

## 33장 설정과 기기 위 파일

엔진 설정의 우선순위와 값 규칙, vault 키와 기기 위 파일 자리를 지원합니다. 이 엔진은 영속 SQLite 색인 대신 vault를 다시 훑습니다. [33장 완료 문서](docs/rebuild-p1-chapter33-complete.md)에 구현과 검증 범위를 기록했습니다. 같은 키를 명령줄에 여러 번 주면 `=`가 있는 칸의 값만 쉼표로 잇습니다(3.1).

## 34장 받아들임 검사

설명서 34장의 받아들임 묶음 `fixtures/expect`를 설명서만 보고 다시 지었습니다. 실행기가 케이스마다 엔진을 셸 없이 불러 종료 코드와 stdout 바이트를 맞댑니다. node 20 이상의 내장 모듈만 씁니다.

```bash
./build.sh
WLX_IMPL_JAVA="$(command -v java)" WLX_IMPL_JAR="$PWD/dist/wl-engine.jar" \
  node fixtures/expect/run.mjs --engine fixtures/aisurada.adapter.json --out /tmp/wlx-run-1
node fixtures/expect/run.mjs --self-check --mutants --out /tmp/wlx-self-1
node fixtures/runner-test.mjs
```

- 실행기는 34.1의 옵션 문법, 묶음 무결성(sha256), `--out` 겹침(장치와 inode), 거르개, 판정 9가지, `report.json`, 자가 점검과 변형 8가지를 따릅니다. 어댑터 계약은 34.5이고, 이 엔진의 어댑터 `fixtures/aisurada.adapter.json`은 묶음 밖에 둡니다.
- 진짜 묶음(읽기 레인 460건, 나무 `fixture.v1`)은 이 저장소에 없습니다. 그래서 레인에는 설명서 원문에 기대값이 글자 그대로 적힌 1부 케이스 29건만 담고, 입력 vault는 다시 짜기 개발 폴더의 픽스처 vault를 담았습니다. 진짜 묶음을 받으면 `lanes/`와 `trees/`를 바꾸고 `node fixtures/build-bundle.mjs`로 목록을 다시 적습니다.

자세한 것은 [34장 완료 노트](docs/rebuild-p1-chapter34-complete.md)에 있습니다.

34장 판 확인이 짚은 것은 [34장 판 확인 후속](docs/rebuild-p1-chapter34-review-fixes.md)에서 고쳤습니다. 33장 설정의 값 없는 칸을 설명서 3.1대로 되돌렸습니다. `append-ask-turn`, `append-chat-turn` 사용법의 줄임표는 부록 A.3대로 `…`로 바꿨습니다. 실행기는 두 실행기의 계약이 갈린 25가지 가운데 24가지를 지금 실행기와 같게 했습니다.

## 1~20장 점검에서 찾은 누락 보완

`reindex-paths`와 공통 쓰기의 자동 커밋·작성자 기록을 구현했습니다. 경로별 재색인은 명령 표면에서만 받고, 옵션 없이 경로 argv를 그대로 보냅니다.

```bash
java -jar dist/wl-engine.jar reindex-paths notes/demo/hello/note.md --withlog.vault=/path/to/vault
```

노트 내부 경로는 루트 노트로 접고 중복은 한 번만 엽니다. 200개를 넘겨 `mode: full`이 되어도 템플릿·회고·휴지통은 검사하지 않습니다. 색인이 없는 구현이므로 `indexed`는 이번에 읽은 노트 수, `unchanged`와 `removed`는 0입니다(8.6).

자동 커밋은 `.git` 폴더가 있는 `main` 브랜치에서 동작합니다. `WITHLOG_VAULT_AUTOCOMMIT_ENABLED=false`로 끄고, `WITHLOG_VAULT_AUTOCOMMIT_IDLE_SECONDS`로 유휴 시간을 정합니다(기본 300초). git 잠금·병합 중 상태는 건너뛰고, 실패는 재시도하되 노트 쓰기의 성공 여부를 바꾸지 않습니다. 훅·서명은 끄고 `*.tmp`를 제외한 vault 변경을 담습니다. `history <노트 id> [번호] [commit=<40자 커밋>] [query=<글자>]`는 수동·자동 커밋과 옛 스냅숏을 합쳐 보여 줍니다. `commit-vault`는 즉시 시도한 결과를 `outcome`과 선택적인 `blocked`로 돌려주며, git 실패도 종료 코드 0입니다. 두 명령은 명령 표면에만 있고 워크스페이스 표식이나 색인을 요구하지 않습니다. 선택 기능 `WITHLOG_VAULT_SNAPSHOT_ENABLED=true`는 본문을 덮기 직전에 노트별 참조에 옛 바이트를 남깁니다(기본 false). 자세한 계약과 유지한 경계는 [31장 완료 문서](docs/rebuild-p1-chapter31-complete.md)에 있습니다.

`bash test.sh`는 코멘트·경로 재색인·자동 커밋을 실제 임시 파일과 git 저장소로 검증합니다. [누락 기능 완료 노트](docs/rebuild-p1-chapters01-20-gaps-complete.md)에 구현과 시험 근거를 기록했습니다.

## 빌드

JDK 21 이상이 필요합니다. `JAVA_HOME`이 있으면 그것을, 없으면 `PATH`의 `javac`와 `jar`를 씁니다.

```bash
./build.sh
```

SnakeYAML 2.4는 처음 빌드할 때 Maven Central에서 내려받아 체크섬을 확인합니다. 산출물은 3개입니다.

| 파일 | 쓰임 |
|---|---|
| `dist/wl-engine.jar` | 엔진. 단발 모드와 serve 모드 |
| `dist/wl-sidecar.jar` | `wl-engine.jar`와 같은 파일. `wl-cli`가 설치하고 띄우는 이름 |
| `dist/wl-cli.jar` | 클라이언트 |

## 실행

### 단발 모드

vault 폴더의 절대 경로를 환경 변수나 인자로 줍니다.

```bash
export WITHLOG_VAULT=/path/to/vault

java -jar dist/wl-engine.jar list-docs
java -jar dist/wl-engine.jar search-docs 첫
java -jar dist/wl-engine.jar get_doc '{"id":"nddemo-00000001"}'
java -jar dist/wl-engine.jar list_tags '{}'

java -jar dist/wl-engine.jar list-docs --withlog.vault=/path/to/vault
```

가장 작은 vault는 아젠다 파일 1개와 노트 1개입니다.

```text
vault/
├── .withlog/agendas/demo.md
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

### serve 모드와 `wl-cli`

엔진을 직접 띄워 두면 `wl-cli`는 엔진을 띄우려고 launchd를 부르지 않고 그 소켓에 붙습니다.

```bash
export WITHLOG_VAULT=/path/to/vault

# 터미널 1: 엔진을 띄운다. Ctrl-C로 내린다
java -jar dist/wl-engine.jar serve

# 터미널 2: 떠 있는 엔진에 묻는다
java -jar dist/wl-cli.jar list_tags '{}'
java -jar dist/wl-cli.jar list-docs limit=5
java -jar dist/wl-cli.jar doctor
```

떠 있는 엔진이 없을 때 `wl-cli`는 `WITHLOG_SIDECAR_JAR`가 가리키는 jar나 `install`로 깐 jar를 launchd로 띄웁니다.

```bash
export WITHLOG_SIDECAR_JAR=$PWD/dist/wl-sidecar.jar
java -jar dist/wl-cli.jar list_tags '{}'
```

> **launchd를 건드립니다.**
> - `wl-cli`가 엔진을 띄울 때는 `~/Library/LaunchAgents/co.kcd.withlog.sidecar.<키>.plist`를 쓰고 `launchctl`을 부릅니다.
> - 엔진이 동시 상한(기본 3개)을 넘으면 가장 오래 조용한 엔진을 내립니다. 하루 한 번은 24시간 넘게 조용한 엔진도 내립니다.
> - 레이블과 실행 상태 파일이 withlog 앱과 같은 꼴입니다. 그래서 withlog 앱이 깔린 기기에서는 그 앱의 엔진도 내릴 대상이 될 수 있습니다.
> - 이 저장소의 시험은 모두 임시 홈 폴더와 가짜 `launchctl`로 했습니다. 진짜 `launchctl`로는 아직 돌려 보지 않았습니다.
>
> 등록한 엔진을 지우려면 `wl-cli doctor`로 레이블과 plist 자리를 보고, `launchctl bootout gui/$(id -u)/<레이블>` 뒤에 그 plist를 지웁니다.

`wl-cli`가 스스로 하는 명령도 있습니다.

| 명령 | 하는 일 |
|---|---|
| `doctor` | 키, 레이블, 소켓, plist, 실행 상태, 동시 상한, 하루 청소를 진단한다. `--plist`는 쓸 plist를 보여 주고, `--prune`은 vault 폴더가 사라진 LaunchAgent를 지운다 |
| `sweep` | 24시간 넘게 조용한 엔진을 내리고 죽은 흔적을 지운다. `--dry-run`은 보여 주기만 한다 |
| `install` | `wl-cli.jar`와 `wl-sidecar.jar`를 `~/Library/Application Support/withlog/bin/`에 깐다. `--cli`, `--sidecar`로 원본을 고른다 |
| `--help`, `--list` | 사용법과 이름 목록 |

### 환경 변수

| 이름 | 쓰임 | 기본값 |
|---|---|---|
| `WITHLOG_VAULT` | vault 폴더 | 없음 |
| `WITHLOG_SIDECAR_SOCKET` | 소켓 자리. 102바이트 이하 | `~/Library/Caches/withlog/wl-<해시>.sock` |
| `WITHLOG_SIDECAR_JAR` | plist에 실을 엔진 jar | `install`로 깐 jar |
| `WITHLOG_SIDECAR_JAVA` | plist에 실을 `java` | `wl-cli`를 돌리는 JVM의 `java` |
| `WITHLOG_CLI_JAR` | `install`이 깔 `wl-cli.jar` 원본 | 없음 |
| `WITHLOG_READY_TIMEOUT_SECONDS` | 띄운 뒤 붙기를 기다리는 초 | 30 |
| `WITHLOG_SIDECAR_LIMIT` | 동시에 떠 있을 엔진 수 | 3 |
| `WITHLOG_SWEEP_QUIET_HOURS` | 청소가 내릴 만큼 조용한 시간 | 24 |
| `WITHLOG_IDLE_TIMEOUT_MINUTES` | serve 모드가 스스로 내려가기까지 조용한 분 | 10 |
| `WITHLOG_VAULT_AUTOCOMMIT_ENABLED` | vault 자동 커밋과 commit-vault 사용 | true |
| `WITHLOG_VAULT_AUTOCOMMIT_IDLE_SECONDS` | 마지막 쓰기부터 커밋까지의 유휴 초 | 300 |
| `WITHLOG_VAULT_SNAPSHOT_ENABLED` | 본문 덮기 전 옛 스냅숏 저장 | false |

## 종료 코드

| 코드 | 뜻 |
|---|---|
| 0 | 성공. 답은 stdout에 JSON 하나와 줄바꿈 1개입니다 |
| 1 | 예상 밖 실패. stderr에 스택 트레이스가 나옵니다 |
| 2 | vault를 못 찾았거나 모르는 명령입니다 |
| 3 | `WL-…` 코드가 붙은 실패 (`WL-404` 노트 없음, `WL-400` 인자 오류 등). stderr에 한 줄이 나오고 stdout은 비어 있습니다 |
| 4 | `wl-cli`만 냅니다. 엔진에 못 닿았거나, 띄우지 못했거나, 응답 프레임이 어긋났습니다 |

serve 모드 엔진은 유휴로 내려가면 0, `SIGTERM`을 받으면 143으로 끝납니다.

## 소스 구성

`src/withlog/` 아래에 패키지 11개가 있습니다.

| 패키지 | 맡은 일 |
|---|---|
| `cli` | 엔진 진입점, `wl-cli`와 그 명령(`doctor`, `sweep`, `install`), launchctl 부르기 |
| `config` | 설정 이름과 값 읽기 |
| `engine` | 명령 1개를 돌려 상태, stdout, stderr로 돌려줌. 단발 모드와 serve 모드가 같이 씀 |
| `serve` | 소켓, 잠금, 실행 상태 파일, 유휴 종료 |
| `frame` | 소켓 위의 요청 프레임(`WLQ1`)과 응답 프레임(`WLG1`) |
| `args` | 도구 표면의 JSON 인자와 명령 표면의 `key=value` 인자 읽기 |
| `cmd` | 명령 실행기와 이름 목록, 하위 노트 부모 사슬 풀기 |
| `write` | 쓰기 공통 부품. 표식 확인, 원자적 쓰기, 폴더 옮기기, 샤드 판정, id와 폴더 이름, 프론트매터 YAML, 경고, 선결 조건 토큰, 편집 도장, 메타 쓰기, 하위 노트 쓰기, 카드 걷기, 폴더 지우기 |
| `vault` | 아젠다, 프로젝트, 노트, 템플릿, 회고, 휴지통을 읽는 부품 |
| `text` | 다듬기, UTF-16 자르기, URI 등 글 도구 |
| `out` | 정해진 바이트 모양의 JSON 쓰기 |

## 검증 요청

검증을 맡은 분은 [`docs/handoff-round2.md`](docs/handoff-round2.md)를 먼저 보세요. 단발 모드의 빌드부터 묶음 실행까지 순서가 있습니다.

## 라이선스

[MIT](LICENSE)

## 2부 네이티브 호스트

Tauri 2 호스트의 1~2장 기반은 [native-host](native-host/README.md)에 있습니다. 번들 설정과 권한, main 창·메뉴·딥링크 및 종료 이벤트를 연결했습니다. [1장 구현 기록](docs/rebuild-p2-chapter01-complete.md)과 [2장 구현 기록](docs/rebuild-p2-chapter02-complete.md)에 검증과 후속 범위를 적었습니다.

3~4장의 공통 IPC 규약과 vault 커맨드 네 개도 연결했습니다. [3장 완료 기록](docs/rebuild-p2-chapter03-complete.md)과 [4장 완료 기록](docs/rebuild-p2-chapter04-complete.md)에 실제 Tauri IPC 검증과 경계 동작을 정리했습니다.

5~6장의 `sidecar_call`과 자식 프로세스 실행·환경 정리도 연결했습니다. [5장 완료 기록](docs/rebuild-p2-chapter05-complete.md)과 [6장 완료 기록](docs/rebuild-p2-chapter06-complete.md)에 실제 Java 엔진 왕복, jar 탐색과 시간 초과 검증을 기록했습니다.

7~8장의 그림 첨부 쓰기·기본 앱 열기·자기 쓰기 알림도 지원합니다. [7장 완료 기록](docs/rebuild-p2-chapter07-complete.md)과 [8장 완료 기록](docs/rebuild-p2-chapter08-complete.md)에 파일 경계와 실제 Tauri IPC 이벤트 검증을 기록했습니다.

9~18장의 외부 Markdown, HTML 첨부 프로토콜, 노트 창 복원·모니터별 배치, 실제 PTY와 비밀 가림까지 구현했습니다. [9장](docs/rebuild-p2-chapter09-complete.md) · [10장](docs/rebuild-p2-chapter10-complete.md) · [11장](docs/rebuild-p2-chapter11-complete.md) · [12장](docs/rebuild-p2-chapter12-complete.md) · [13장](docs/rebuild-p2-chapter13-complete.md) · [14장](docs/rebuild-p2-chapter14-complete.md) · [15장](docs/rebuild-p2-chapter15-complete.md) · [16장](docs/rebuild-p2-chapter16-complete.md) · [17장](docs/rebuild-p2-chapter17-complete.md) · [18장](docs/rebuild-p2-chapter18-complete.md)에 장별 구현과 검증 범위를 적었습니다. 실제 macOS webview의 26개 자동 점검은 `cd native-host && pnpm test:acceptance`로 실행합니다. 4부 Rust 모듈과 실제 편집 화면은 아직 포함하지 않습니다.
