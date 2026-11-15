# aisurada MCP 가이드

Claude 같은 MCP 클라이언트가 aisurada의 노트를 도구로 찾고, 읽고, 만들고, 고치게 하는 방법과 쓸 수 있는 기능을 정리한 문서입니다. 연결하는 사람과 도구를 고치는 사람이 함께 읽도록 썼고, 프로토콜 이야기는 맨 뒤 §7에 모았습니다.

MCP 서버는 앱에 든 `ais-cli`의 한 모드(`ais-cli mcp`)입니다. `ais-cli`는 Rust로 지은 실행 파일 하나라서 별도 프로그램이나 Java를 깔 필요가 없습니다.

## 1. 한눈에

| 항목 | 내용 |
|------|------|
| 서버 이름 | `aisurada` (버전 0.1.0) |
| 전송 | 표준 입출력(stdio). 한 줄에 JSON-RPC 메시지 하나 |
| 내놓는 도구 | 읽기 전용 모드 19개, 쓰기 모드 37개 |
| 기본값 | **읽기 전용.** 노트를 만들고 고치려면 연결할 때 쓰기를 따로 허용해야 합니다 |
| 영구 삭제 | 어느 모드에서도 열지 않습니다(`purge_doc`). 휴지통 비우기는 앱에서 사람이 합니다 |
| 다루는 범위 | 서버 하나가 한 번에 vault 하나를 다룹니다. vault는 **도구를 부를 때마다** 정합니다. `AISURADA_VAULT`가 있으면 그것, 없으면 앱이 지금 열어 둔 vault입니다(§2-4) |

## 2. 연결하기

연결은 세 가지 길이 있고, 모두 같은 서버를 Claude에 등록합니다. Codex와 Antigravity·Gemini CLI에는 같은 방식의 명령이 따로 있습니다(§2-5). 읽기만 허용하는 것이 기본이고, 쓰기를 허용하면 §3의 쓰기 도구까지 열립니다. 등록에는 vault 경로를 박지 않습니다.

### 2-1. 앱의 설정 화면

설정(톱니바퀴)의 **Claude 연결**을 엽니다.

| 상태 | 보이는 것 |
|------|-----------|
| 연결 안 됨 | 「연결되어 있지 않아요」, 버튼 **Claude Code에 연결** |
| 읽기로 연결됨 | 「연결되어 있어요 · 노트를 읽기만 해요」 |
| 쓰기로 연결됨 | 「…노트를 읽고 쓸 수 있어요」 |

- **노트를 만들고 고치는 것도 허용**을 켜고 연결하면 쓰기 모드로 등록됩니다. 이미 연결돼 있으면 버튼이 **이 설정으로 다시 연결**로 바뀌고, 누르면 모드를 바꿔 끼웁니다.
- **연결 끊기**를 누르면 등록을 지웁니다.
- 연결에는 워크스페이스가 박히지 않습니다. 서버가 도구를 부를 때마다 앱이 지금 연 워크스페이스를 읽으므로, 앱에서 워크스페이스를 바꾸면 에이전트가 따라갑니다. 다시 연결할 필요가 없습니다.
- 4부(Claude 연동)를 끈 판에는 이 화면이 없습니다.
- 같은 영역의 **Claude Desktop에 연결하려면**을 펴면 Claude Desktop용 설정 조각이 나옵니다(§2-3).

### 2-2. 터미널의 `ais-cli claude-mcp`

```bash
ais-cli claude-mcp status          # 연결 여부와 모드
ais-cli claude-mcp add             # 읽기 전용으로 연결
ais-cli claude-mcp add --write     # 읽기+쓰기로 연결(이미 있으면 바꿔 끼움)
ais-cli claude-mcp remove          # 연결 끊기
```

- Claude의 설정 파일을 직접 고치지 않고 `claude mcp add-json --scope user`만 부릅니다. 그래서 `claude` 명령(Claude Code)이 있어야 합니다. 못 찾으면 `AISURADA_CLAUDE_BIN`으로 자리를 알려 줍니다.
- 등록되는 것은 이 명령을 돌린 `ais-cli` 실행 파일입니다. 앱의 `bin/ais-cli`로 돌리면 앱에 든 것이 등록됩니다.
- `status`의 출력 예: `{"claudeFound":true,"registered":false,"write":false,"detail":""}`
- 종료 코드: 0은 성공, 1은 `claude`를 못 돌렸거나 실패, 2는 쓰는 법이 틀린 것입니다. `--write`는 `add`와 `config`에서만 받습니다.
- 예전 판은 `AISURADA_VAULT`를 등록에 박았습니다. 그때 만든 등록은 `add`를 다시 하면 바뀝니다(앞의 등록을 지우고 새로 더합니다).
- 앱이 vault를 연 적이 없고 `AISURADA_VAULT`도 없으면 `add`가 끝에 알림을 냅니다. 오류는 아니고, 앱을 열어 vault를 열면 도구가 그 vault를 씁니다.

앱 안의 `ais-cli`를 터미널에서 쓰는 법은 앱의 **연동 가이드**나 [개발자 가이드](developer-guide.md)의 「앱에 든 `ais-cli`」를 봅니다.

### 2-3. 직접 등록하기

`ais-cli claude-mcp config [--write]`는 아무것도 등록하지 않고 Claude Desktop 설정에 붙여 넣을 `mcpServers` 조각만 출력합니다.

```json
{"mcpServers":{"aisurada":{"type":"stdio",
  "command":"<앱 또는 dist>/ais-cli",
  "args":["mcp"]}}}
```

쓰기를 허용하려면 `args` 끝에 `"--write"`를 더합니다. 다른 MCP 클라이언트도 같은 형태(`command`, `args`, `env`)를 받으면 붙일 수 있습니다.

- 등록에는 실행 파일의 **절대 경로**가 들어갑니다. 앱을 옮기거나 지우면 연결이 끊기니 그때는 다시 연결합니다.
- Java 시절에 만든 등록은 지금은 없는 jar 파일을 가리킵니다. `ais-cli doctor`가 낡았다고 알려 주고, `ais-cli doctor --fix`가 같은 모드로 다시 등록합니다. 정식 앱은 새 판을 처음 열 때 한 번 이것을 합니다.

### 2-4. 어느 vault를 쓰나

- **앱이 지금 연 vault를 씁니다.** 앱은 워크스페이스를 열 때마다 그 경로를 `~/Library/Application Support/aisurada/current-vault.json`에 적고, 서버와 `ais-cli`는 도구를 부를 때마다 그 파일을 읽습니다. 서버를 다시 띄우거나 다시 연결하지 않아도, 앱에서 워크스페이스를 바꾸면 다음 호출부터 새 vault를 씁니다.
- **`AISURADA_VAULT`가 있으면 그것이 이깁니다.** 개발이나 손 확인처럼 특정 vault를 고정하고 싶을 때만 씁니다. 이 값은 등록에 자동으로 들어가지 않습니다. 필요하면 서버를 띄우는 환경에 직접 줍니다.
- **vault를 못 정하면 도구가 실패합니다.** 환경변수도 없고 앱이 vault를 연 적도 없으면 `AIS-400 vault를 정하지 못했다…`가 나옵니다. 서버는 죽지 않습니다. 앱을 열어 vault를 열면 다음 호출부터 됩니다.
- **격리된 앱은 진짜 포인터를 건드리지 않습니다.** 손 확인이나 시험이 가짜 HOME으로 앱을 띄우면 그 HOME 아래에 파일을 적습니다. 진짜 HOME의 파일은 진짜 앱만 고칩니다.
- **어떻게 등록됐는지는 `claude mcp get aisurada`로 봅니다.** 출력의 `Environment`에 `AISURADA_VAULT=…`가 있으면 예전 판이 vault를 박은 등록입니다. 그 경로의 vault만 쓰니 `ais-cli claude-mcp add --write`로 다시 등록합니다(`ais-cli claude-mcp status`의 `detail`에도 같은 글이 들어 있습니다).
- **연결한 뒤에는 새 Claude 세션을 엽니다.** 이미 떠 있던 세션은 새로 등록한 서버를 보지 못하고 「aisurada MCP가 연결돼 있지 않다」고 답할 수 있습니다. 같은 까닭으로, 등록을 바꾸기 전부터 떠 있던 서버는 옛 등록(박힌 vault)으로 계속 돕니다.
- **엔진을 따로 띄울 일이 없습니다.** Java 시절에는 엔진을 직접 띄워 쓰는 개발 방식이 있었지만 지금은 서버가 호출마다 엔진을 프로세스 안에서 돌립니다. vault를 고정하려면 서버를 띄우는 환경에 `AISURADA_VAULT`를 줍니다.

### 2-5. Codex와 Antigravity·Gemini CLI

```bash
ais-cli codex-mcp status           # 연결 여부와 모드
ais-cli codex-mcp add --write      # 읽기+쓰기로 연결(이미 있으면 바꿔 끼움)
ais-cli codex-mcp remove
ais-cli gemini-mcp status          # Antigravity(agy)나 Gemini CLI 가운데 먼저 찾은 것
ais-cli gemini-mcp add --write
ais-cli gemini-mcp remove
```

- `claude-mcp`와 같은 방식입니다. 설정 파일을 직접 고치지 않고 각 CLI의 등록 명령(`codex mcp add`, `agy mcp add`, `gemini mcp add -s user`)만 부릅니다. 등록에는 vault를 박지 않고, 등록되는 것은 이 명령을 돌린 `ais-cli` 실행 파일(앱의 `bin/ais-cli`로 돌리면 앱에 든 것)입니다.
- 실행 파일을 못 찾으면 `AISURADA_CODEX_BIN`, `AISURADA_GEMINI_BIN`(또는 `AISURADA_ANTIGRAVITY_BIN`)으로 자리를 알려 줍니다. Gemini 쪽은 `antigravity`, `agy`, `gemini` 차례로 찾고, `status`와 `add`의 응답에 어느 CLI를 썼는지(`cli`)가 들어 있습니다.
- 두 CLI에서 이 서버로 만든 노트에 세션 ID를 남기는 법은 [개발자 가이드](developer-guide.md)의 「세션 에이전트 확장」을 봅니다.
- 설정 화면의 연결 버튼은 Claude 것뿐입니다. Codex와 Gemini는 이 명령으로만 등록합니다.

## 3. 쓸 수 있는 기능

도구는 37개이고(영구 삭제 `purge_doc`는 내놓지 않습니다), 이름은 엔진의 도구 이름과 같습니다. 기본은 읽기 도구 19개만 보입니다. 읽기 도구(R)는 어느 모드에서나 보이고, 쓰기 도구(W)는 `--write`일 때만 보입니다. `!`는 있던 내용을 지우거나 덮어쓸 수 있는 도구입니다. 인자의 `*`는 필수입니다.

### 노트 찾고 읽기

| 도구 | 모드 | 하는 일 | 인자 |
|------|:----:|---------|------|
| `list_docs` | R | 조건에 맞는 노트의 목록(메타만)을 만든 시각 차례로 냅니다. 본문은 `get_doc`으로 읽습니다 | `agenda`, `date`, `dateFrom`, `dateTo`, `type`, `tag`, `favorite`, `favoriteCategory` |
| `search_docs` | R | 제목, 메타, 본문, 첨부 글에서 찾아 일치한 대목과 함께 냅니다 | `query*`, `limit`(기본 20, 최대 200), 그리고 `list_docs`의 거름 조건 8개 |
| `get_doc` | R | 노트 1건을 본문, 도장, 코멘트와 함께 읽습니다. 응답의 `precondition`이 고칠 때 쓰는 토큰입니다 | `id*` |
| `list_tags` | R | 태그와 쓰인 횟수를 냅니다 | `query`, `limit` |

### 노트 만들고 고치기

인자 목록의 `f1`은 선택처럼 보이지만, 마크다운 노트를 고치는 도구(`update_doc`, `set_favorite`, `evaluate_doc`, `link_docs`, `update_sub_note`)는 **`f1` 없이 부르면 거절**됩니다(AIS-412). `get_doc`으로 읽은 `precondition`을 `f1`로 넘깁니다. 방금 `create_doc`이나 `update_doc`을 했다면 그 응답의 `precondition`을 씁니다(§4).

| 도구 | 모드 | 하는 일 | 인자 |
|------|:----:|---------|------|
| `create_doc` | W | 새 노트 1건을 만듭니다. 응답에 `id`와 `precondition`이 들어 있습니다 | `agenda*`, `type*`, `title*`, `body*`, `date`, `time`, `format`, `links`, `tags`, `template` |
| `update_doc` | W! | 제목, 본문, 태그를 고칩니다. 준 칸만 바뀌고, 본문을 주면 통째로 바뀝니다 | `id*`, `body`, `title`, `tags`, `template`, `f1` |
| `set_favorite` | W | 즐겨찾기 별과 라벨을 고칩니다. `favorite`을 안 주면 지금 값을 뒤집습니다 | `id*`, `favorite`, `category`, `f1` |
| `evaluate_doc` | W | 평가 점수(0~100)와 메모를 새깁니다. `clear`를 주면 평가를 지웁니다 | `id*`, `score`, `note`, `clear`, `f1` |
| `link_docs` | W! | 노트에서 다른 노트로 가는 연결 1건을 잇습니다. `remove`를 주면 끊습니다. 종류는 `direct`, `indirect`, `attach` | `from*`, `to*`, `kind`, `remove`, `f1` |

### 휴지통

| 도구 | 모드 | 하는 일 | 인자 |
|------|:----:|---------|------|
| `delete_doc` | W! | 노트를 휴지통으로 보냅니다. 되살릴 수 있습니다 | `id*` |
| `list_trash` | R | 휴지통의 노트 목록을 냅니다. 각 항목의 `trashId`를 `restore_doc`에 씁니다 | (없음) |
| `restore_doc` | W | 휴지통의 노트를 되살립니다 | `trashId*` |

영구 삭제 `purge_doc`은 목록에 나오지 않고, 불러도 「이 도구는 MCP로 열지 않는다」는 오류를 돌려줍니다.

### 하위 노트, 첨부, 코멘트

| 도구 | 모드 | 하는 일 | 인자 |
|------|:----:|---------|------|
| `list_sub_notes` | R | 노트 바로 아래 하위 노트의 목록(메타만)을 냅니다 | `parent*` |
| `get_sub_note` | R | 하위 노트 1건을 읽습니다 | `parent*`, `id*` |
| `create_sub_note` | W | 노트 아래에 하위 노트를 만듭니다 | `parent*`, `title*`, `body*`, `type` |
| `update_sub_note` | W! | 하위 노트의 제목과 본문을 고칩니다. 준 칸만 바뀝니다 | `parent*`, `id*`, `body`, `title`, `f1` |
| `delete_sub_note` | W! | 하위 노트를 지웁니다 | `parent*`, `id*` |
| `list_attachments` | R | 첨부 목록과 본문에 붙일 참조 조각을 냅니다 | `parent*` |
| `write_attachment` | W | 노트나 하위 노트에 글 첨부 1개를 씁니다 | `parent*`, `name*`, `content*` |
| `list_comments` | R | 코멘트 목록을 냅니다 | `parent*` |

`parent`는 노트 id이고, 하위 노트의 첨부와 코멘트는 `노트id/하위노트id` 형태로 줍니다.

### 하루 계획

| 도구 | 모드 | 하는 일 | 인자 |
|------|:----:|---------|------|
| `plan_day` | W | 하루 계획 초안을 짓습니다. `dryRun`을 true로 주면 노트로 저장하지 않고 초안만 냅니다 | `date`, `agenda`, `dryRun` |

### 템플릿

| 도구 | 모드 | 하는 일 | 인자 |
|------|:----:|---------|------|
| `list_templates` | R | 문서 템플릿과 리추얼 템플릿의 목록을 냅니다 | `category`(`doc`·`ritual`), `type` |
| `get_template` | R | 템플릿 1건을 읽습니다 | `id*` |
| `save_template` | W! | 문서 템플릿을 저장합니다. `id`를 주면 그 템플릿을 고칩니다 | `id`, `name`, `type`, `desc`, `body` |

### 스킬과 에이전트 정의(자원)

외부 설명서에 없는 확장입니다. 스킬(`SKILL.md`가 든 폴더)과 에이전트 정의를 **글로벌**과 **프로젝트** 두 범위로 보관합니다. 프로젝트는 글로벌 자원을 ID(`sk…`, `ap…`)로 **참조**해야 쓸 수 있고, **참조하지 않은 것은 쓰지 않습니다**. 쓰기 도구는 노트와 같은 관문(`--write`)을 지납니다. 정의는 특정 AI 도구에 묶이지 않고, 도구별 차이는 `tools` 칸에만 둡니다.

| 도구 | 모드 | 하는 일 | 인자 |
|------|:----:|---------|------|
| `list_resources` | R | 스킬과 에이전트 정의의 관리 목록을 냅니다. 참조하지 않은 글로벌 자원도 나옵니다. `trash`를 true로 주면 휴지통 목록입니다 | `kind`, `scope`, `project`, `bodies`, `trash` |
| `get_resource` | R | 1건을 지침과 `precondition`(`f1:…`)과 함께 읽습니다. `version`을 주면 옛 판을 읽습니다 | `id*`, `version` |
| `usable_resources` | R | 프로젝트가 쓸 수 있는 자원(프로젝트 자원 + 참조한 글로벌 자원)을 냅니다. 깨진 참조(`broken`)와 필요하지만 못 쓰는 스킬(`missingSkills`)도 냅니다 | `project*`, `bodies` |
| `list_resource_versions` | R | 고치기 전의 판 목록(새것이 앞)을 냅니다 | `id*` |
| `list_resource_changes` | R | 승인을 기다리는 스킬·에이전트 변경(pending), 승인 안 된 직접 수정(unapproved), 최근 결정이 난 변경(decided)을 냅니다. 내가 보낸 변경이 적용됐는지 거절됐는지 여기서 봅니다 | (없음) |
| `get_resource_change` | R | 승인 대기 변경 1건을 이전 판(`before`)과 새 판(`after`)과 함께 냅니다. `status`는 pending, approved, rejected, stale(승인 전에 대상이 바뀌어 적용 못 함) 가운데 하나입니다 | `id*` |
| `save_resource` | W! | 만들거나 고칩니다. `id`가 없으면 새로 만들고(`kind*`, `scope`나 `project`, `name*`, `description*`), 있으면 준 칸만 고칩니다(`f1` 필수, 종류·범위·id는 못 바꿈) | `id`, `kind`, `scope`, `project`, `name`, `description`, `body`, `permission`, `skills`, `repo`, `tools`, `f1` |
| `delete_resource` | W! | 휴지통으로 보냅니다(스킬은 폴더째). 응답의 `referencedBy`는 참조하던 프로젝트들입니다 | `id*` |
| `restore_resource` | W | 휴지통의 자원을 되살립니다. 제자리에 같은 이름이 있으면 거절됩니다 | `id*` |
| `set_resource_ref` | W | 프로젝트가 글로벌 자원을 참조하는 것을 켭니다(`remove`가 true면 끕니다). `withSkills`가 true면 에이전트를 켤 때 그 글로벌 스킬도 함께 켭니다 | `project*`, `id*`, `remove`, `withSkills` |

- 에이전트의 `permission`은 `read`, `edit`, `publish` 가운데 하나이고, `skills`는 스킬 id 목록입니다.
- AI가 고치면 `editedSession`(과 Claude가 아닐 때 `editedAgent`)이 남고, 고치기 전의 판은 30판까지 이력에 남습니다. 되돌리려면 `get_resource`의 `version`으로 옛 판을 읽고 그 내용으로 `save_resource`를 부릅니다.
- **지금은 승인 흐름이 없어서** AI가 쓴 변경(참조 켜고 끄기와 권한 포함)이 바로 적용됩니다. 승인 워크플로우는 이슈 [#53](https://github.com/bnjjong/aisurada/issues/53)입니다. 권한 값은 작업 실행기가 없어 아직 어디에도 쓰이지 않습니다.

### 태스크(티켓)

외부 설명서에 없는 확장입니다. 태스크는 사람이 적어 둔 일이고, 나중에 러너가 집어 에이전트에게 맡깁니다. **읽기 도구만 있습니다.** 태스크를 만들고 상태를 바꾸는 일은 사람(화면)과 러너만 하므로, `--write`를 줘도 쓰기 도구는 열리지 않습니다. 상태 이름은 `draft`, `ready-for-develop`, `in-progress`, `waiting-answer`, `in-review`, `done`, `failed`, `cancelled`입니다.

| 도구 | 모드 | 하는 일 | 인자 |
|------|:----:|---------|------|
| `list_tasks` | R | 태스크 목록을 키가 큰(새) 것부터 냅니다. `blocked`가 true면 다른 태스크에 막힌 것만, `trash`가 true면 휴지통 목록입니다 | `status`, `type`, `repo`, `label`, `q`, `blocked`, `bodies`, `trash` |
| `get_task` | R | 태스크 1건을 설명, 완료 기준, 연결한 자원, 연결된 태스크(막고 있음·막혀 있음·관련·중복), Ready For Develop 조건, 활동과 함께 읽습니다 | `id*`(키 `TSK-14` 또는 `tk…`), `activity`(최근 개수, 기본 100) |

### 내보내기

| 도구 | 모드 | 하는 일 | 인자 |
|------|:----:|---------|------|
| `export_markdown` | W | 노트를 마크다운 사본으로 vault 안의 `.aisurada/md-export`에 씁니다. 쓸 때마다 그 폴더를 지웠다 다시 만듭니다 | (없음) |

## 4. 안전장치

| 장치 | 동작 |
|------|------|
| 읽기 전용이 기본 | 쓰기 모드로 등록하지 않으면 쓰기 도구는 목록에 나오지 않습니다. 이름을 알고 불러도 「이 서버는 읽기 전용으로 떠 있어 … 쓸 수 없다」는 오류가 납니다 |
| 영구 삭제 없음 | `purge_doc`은 두 모드 모두에서 열리지 않습니다. 모델이 되돌릴 수 없는 삭제를 하지 못합니다 |
| 쓰기 전 토큰 `f1` | `get_doc`이나 `create_doc`, 쓰기 응답에 든 `precondition` 값(`f1:…`)을 `update_doc` 같은 쓰기 도구의 `f1`로 넘겨야 합니다. 토큰이 없으면 거절되고, 그 사이에 다른 곳(앱, 다른 에이전트)이 노트를 고쳤으면 덮어쓰지 않고 멈춥니다 |
| 모르는 인자 거절 | 도구 스키마에 없는 키를 주면 쓸 수 있는 키를 알려 주며 거절합니다. 오타로 엉뚱한 칸이 바뀌지 않습니다 |
| 휴지통 | `delete_doc`은 휴지통으로 보낼 뿐이고 `restore_doc`으로 되살릴 수 있습니다 |
| 클라이언트에 알리는 성격 | 각 도구에 `readOnlyHint`가 있고, 쓰기 도구에는 `destructiveHint`도 있습니다(§3의 `!`). 확인 창을 띄울지는 클라이언트가 정합니다 |

`f1`은 모델이 그 노트를 방금 읽었다는 증거입니다. 값을 계산해서 만들 수 없고, 읽은 응답의 `precondition`을 그대로 되돌려야 합니다. 서버는 이 규칙을 접속할 때 클라이언트에 `instructions`로 알리고, 각 도구의 `f1` 설명에도 적어 둡니다. json 형식 노트만 이 검사를 면제받습니다.

**쓰기를 허용하기 전에 알아 둘 것.** 쓰기 모드는 모델이 vault의 노트를 만들고, 고치고, 휴지통으로 보낼 수 있다는 뜻입니다. `update_doc`은 본문을 통째로 바꾸고, `save_resource`는 스킬과 에이전트 정의의 지침은 물론 참조와 권한까지 바꿀 수 있습니다(지금은 승인 흐름이 없습니다). 중요한 vault는 git 같은 곳에 이력이 남게 해 두고 켜는 것을 권합니다.

## 5. 모델에게 시키는 법

모델이 도구를 고르는 것이라 말은 평소처럼 하면 됩니다. 예시입니다.

| 하고 싶은 일 | 이렇게 말합니다 | 모델이 부르는 도구 |
|--------------|-----------------|--------------------|
| 찾아서 요약 | 「aisurada에서 `회의` 태그가 붙은 이번 주 노트를 찾아 요약해 줘」 | `list_docs`(`tag`, `dateFrom`) → `get_doc` |
| 키워드 검색 | 「'릴리스'가 들어간 노트를 찾아 줘」 | `search_docs` |
| 새 노트 | 「`mcp-test` 아젠다에 free 유형으로 오늘 회의록 노트를 만들어 줘」 | `create_doc` |
| 기존 노트 고치기 | 「그 노트를 읽고 맨 끝에 결론 절을 더해 줘」 | `get_doc` → `update_doc`(`f1`) |
| 정리 | 「이 노트를 휴지통으로 보내 줘」 | `delete_doc` (되살리기는 `list_trash` → `restore_doc`) |

노트를 만들게 할 때는 아래를 같이 말해 두면 좋습니다.

- **본문 첫 헤딩을 제목과 같게.** 다르면 `create_doc`이 경고를 돌려줍니다. 앱에서 그 노트를 한 글자라도 고치는 순간 제목이 첫 헤딩으로 바뀌고 폴더도 따라 움직이기 때문입니다.
- **노트를 고치기 전에 먼저 읽게.** `update_doc`의 본문은 통째로 바뀌므로, 읽은 전체에 고친 부분을 얹어 보내야 합니다.

## 6. 응답과 오류 읽기

성공하면 `content`에 JSON 문자열 하나가 텍스트로 담깁니다. 출력이 없는 호출은 `(출력 없음)`입니다. 실패하면 `isError: true`이고 텍스트에 엔진의 오류 글과 `(종료 코드 N)`이 붙습니다. 오류가 나도 서버는 죽지 않고 다음 호출을 받습니다.

실제로 받은 응답입니다(id와 경로는 시험 vault의 값입니다).

`create_doc`의 성공 응답:

```json
{
  "id": "ndmw37ezqe-6234ac9a",
  "agenda": "mcp-test",
  "agendaCreated": true,
  "date": "2026-11-05",
  "path": "notes/mcp-test/2026-11-05-1608-mcp로-만든-테스트-노트-ac9a",
  "precondition": "f1:bcef8da9…6747",
  "deepLink": "aisurada://doc/ndmw37ezqe-6234ac9a?ws=…"
}
```

`list_tags`의 성공 응답은 `[{"tag":"mcp","count":1}, …]` 형태이고, `update_doc`의 성공 응답은 새 `precondition`을 돌려줍니다.

| 상황 | 오류 글(앞부분) | 종료 코드 | 어떻게 합니까 |
|------|-----------------|:---------:|----------------|
| 쓰기 도구를 읽기 서버에서 부름 | 이 서버는 읽기 전용으로 떠 있어 create_doc 을 쓸 수 없다 | — | 쓰기를 허용해 다시 연결합니다(§2) |
| `purge_doc` 호출 | 이 도구는 MCP로 열지 않는다. 영구 삭제는 앱에서 사람이 한다 | — | 앱의 휴지통에서 사람이 지웁니다 |
| 모르는 키 | AIS-400 인자 검증 실패(update_doc): 모르는 키다: [bogus]. 쓸 수 있는 것은 [id, body, title, tags, template, f1] 뿐이다 | 3 | 인자 이름을 고칩니다 |
| 필수 키 빠짐 | AIS-400 인자 검증 실패(get_doc): id 가 필요하다 | 3 | 빠진 인자를 채웁니다 |
| `f1` 없음 | AIS-412 선결 조건 토큰이 안 왔어요. 아무것도 쓰지 않았습니다 | 5 | `get_doc`으로 읽고 그 응답의 `precondition`을 `f1`로 실어 다시 부릅니다. 방금 만들거나 고친 노트면 그 쓰기 응답이 이미 다음 토큰을 줍니다 |
| 낡은 `f1` | AIS-412 다른 쪽이 이 노트를 먼저 고쳤어요. 아무것도 쓰지 않았습니다 | 5 | `get_doc`으로 다시 읽고, 그 새 본문 위에 변경을 얹어 새 `precondition`으로 보냅니다. `f1`만 갈아 끼워 다시 보내지 않습니다(남이 쓴 글이 사라집니다) |
| 없는 도구 이름 | JSON-RPC 오류 -32602 「없는 도구다」 | — | 도구 이름을 확인합니다 |

종료 코드는 `ais-cli`와 같은 값입니다.

## 7. 프로토콜 (만드는 사람용)

| 항목 | 내용 |
|------|------|
| 전송 | 표준 입출력. 한 줄에 JSON-RPC 2.0 메시지 하나(UTF-8). 표준 입력이 닫히면 서버가 끝납니다(종료 코드 0) |
| 받는 메서드 | `initialize`, `ping`, `tools/list`, `tools/call`. 알림(`notifications/initialized` 등)은 답하지 않고 받기만 합니다. 그 밖의 메서드는 -32601입니다 |
| 규격 개정 | `2025-06-18`, `2025-03-26`, `2024-11-05`. 클라이언트가 모르는 개정을 청하면 `2025-06-18`로 답합니다 |
| 능력 | `tools`만 있고 `listChanged`는 false입니다. 리소스와 프롬프트는 없습니다 |
| `instructions` | 노트를 고치는 도구에는 `f1`이 꼭 필요하고, 고치기 전에 `get_doc`으로 읽어 `precondition`을 넘기라는 안내를 접속할 때 줍니다 |
| 도구 주석 | 모든 도구에 `annotations.readOnlyHint`, 쓰기 도구에는 `destructiveHint`도 있습니다 |
| 입력 스키마 | `additionalProperties: false`. 키는 엔진이 받는 키와 같고, 시험이 도구 36개를 모두 엔진의 「모르는 키다」 문구와 대조합니다 |
| 오류 | 요청 형식이 틀리면 JSON-RPC 오류(-32700, -32600, -32601, -32602, -32603)이고, 도구가 실패하면 정상 응답에 `isError: true`입니다 |
| 로그 | 서버의 표준 출력에는 프로토콜 메시지만 나옵니다. 시작 메시지(`aisurada MCP 서버 0.1.0: 읽기 전용`)와 진단은 표준 오류로 갑니다 |

도구 호출은 단발 `ais-cli`와 같은 길로, 서버 프로세스 안에서 엔진을 직접 돌립니다. 띄울 엔진이 따로 없으니 첫 호출이 느려지는 일도 없습니다. 서버는 도구를 부를 때마다 vault의 자동 커밋을 지켜봅니다(유휴 시간이 지나면 커밋하는 타이머가 서버 안에서 돕니다).

직접 시험해 보려면 vault를 주고 서버에 줄을 흘려 넣습니다. 이 호출은 진짜 엔진을 돌리므로, 시험용 vault로 하고 격리 방법은 [손 확인 문서](beta-hand-check.md)를 봅니다.

```bash
export AISURADA_VAULT="/absolute/path/to/vault"
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"manual","version":"1"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized"}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"list_tags","arguments":{}}}' \
  | dist/ais-cli mcp
```

도구를 더하거나 인자를 바꾸는 법은 [개발자 가이드](developer-guide.md)의 「MCP 서버」에 있습니다. 인자를 바꾸면 `engine-rs/crates/ais-cli/src/mcp_tools.rs`의 스키마와 명령줄 인자 명세(`spec_table.rs`)도 같이 고칩니다. 둘 다 Java 엔진을 지운 뒤로는 직접 고치는 원본입니다.

## 8. 문제 해결

| 증상 | 확인할 것 |
|------|-----------|
| 연결했는데 도구가 안 보임 | `claude mcp get aisurada`(또는 `ais-cli claude-mcp status`)로 등록과 `✔ Connected`를 봅니다. 서버는 목록 변경 알림을 보내지 않으므로, 연결을 바꾼 뒤에는 Claude Code를 새 세션으로 엽니다(새 세션이 필요한지는 확인하지 못했습니다) |
| 쓰기 도구가 없음 | 읽기 전용으로 연결된 것입니다. 설정의 **노트를 만들고 고치는 것도 허용**을 켜고 **이 설정으로 다시 연결**을 누르거나 `ais-cli claude-mcp add --write`를 합니다 |
| 앱을 옮기거나 지운 뒤 끊김 | 등록에 든 `ais-cli`의 절대 경로가 없어진 것입니다. 다시 연결합니다(`ais-cli doctor --fix`도 됩니다) |
| 도구가 모두 실패함 | `AIS-400 vault를 정하지 못했다…`이면 `AISURADA_VAULT`도 없고 앱이 vault를 연 적도 없는 것입니다. 앱을 열어 vault를 열면 다음 호출부터 됩니다(서버는 죽지 않습니다). 서버가 뜰 때 표준 오류에도 같은 안내를 냅니다 |
| 노트가 엉뚱한 vault에 생김 | 예전 판이 `AISURADA_VAULT`를 박은 등록일 수 있습니다. `claude mcp get aisurada`(Codex는 `codex mcp get aisurada`)의 `Environment`에 `AISURADA_VAULT`가 있으면 `ais-cli claude-mcp add --write`(`codex-mcp`, `gemini-mcp`도 같습니다)로 다시 등록합니다. 앱이 지금 연 vault는 `~/Library/Application Support/aisurada/current-vault.json`에 있습니다 |
| 클라이언트가 곧 끊김 | 서버의 표준 출력에 JSON-RPC가 아닌 글이 섞이면 클라이언트가 끊습니다. 새 출력은 표준 오류(`eprintln!`)로 씁니다([개발자 가이드](developer-guide.md)의 자주 막히는 곳) |
| `claude` 명령을 못 찾음 | Claude Code가 없거나 PATH 밖에 있습니다. `AISURADA_CLAUDE_BIN`에 실행 파일 자리를 줍니다 |

## 9. 확인한 것과 못 한 것

확인한 것(2026-11-05 기준):

- `bash test.sh`(5d에서 178건 통과)의 `crates/ais-cli/tests/mcp_stdio.rs`(진짜 `mcp` 프로세스와 표준 입출력 왕복. 세션 표식이 노트 머리에 남는 것과, AI가 올린 변경이 대기하다 사람의 `ais-cli`로만 승인되는 길까지 본다)와 `register.rs`(가짜 에이전트 CLI로 등록)가 통과합니다. Java 시절의 MCP 서버 시험과 등록 시험 클래스는 Java와 함께 지웠습니다.
- 기준 Java MCP 서버와 JSON-RPC 대화를 맞댄 `mcp_compat`이 어긋남 0으로 끝났습니다(5d, 174건).
- 진짜 Claude Code 2.1.318을 격리 HOME에서 `add`, `--write`로 바꿔 끼우기, `✔ Connected`, `remove`까지 확인했습니다.
- 이 문서의 도구 수(읽기 19, 쓰기 37)는 5d에서 `tools/list`를 직접 불러 세었습니다. 인자 표, §6의 응답과 오류 글, §7의 직접 시험 예시는 격리 vault에 `mcp --write` 서버를 띄워 직접 부르고 얻은 것입니다. 모델이 아니라 사람이 JSON-RPC를 손으로 보냈습니다.
- §3의 인자 표 37행을 `engine-rs/crates/ais-cli/src/mcp_tools.rs`의 입력 스키마와 대조했습니다(2026-11-16, 스크립트로 읽어서 견줌). 도구 이름, 필수 표시 `*`, 읽기·쓰기·`!` 표시, 인자 이름이 모두 맞았습니다. `search_docs`의 `list_docs` 거름 조건 8개는 두 도구의 스키마가 같았고, `limit`의 기본 20과 최대 200, `get_task`의 `activity` 기본 100, `list_templates`의 `category` 값(`doc`·`ritual`)도 스키마 설명과 같았습니다. 표를 고친 곳은 없습니다.
- `f1` 없이 `update_doc`을 부르면 거절되는 것은 직접 불러 확인했습니다. 나머지 네 도구(`set_favorite`, `evaluate_doc`, `link_docs`, `update_sub_note`)가 같은 검사를 받는 것은 2026-11-16에 Rust 엔진 코드를 읽어 확인했습니다. 마크다운 노트는 `update_doc`(`commands/doc_write.rs`), `update_sub_note`(`commands/sub_note_write.rs`), 나머지 셋(`write/meta_writer.rs`)이 모두 `write/precondition.rs`의 `check`를 거치고, 토큰이 없으면 AIS-412로 거절합니다. 하나씩 불러 보지는 않았습니다.

못 한 것:

- **모델이 도구를 골라 부르는 흐름.** Claude Code가 실제로 `create_doc` 등을 부르는 것은 보지 못했습니다. §5의 말투와 도구 대응은 도구 설명과 설계에서 온 것입니다.
- **Claude Desktop 연결.** 설정 조각을 만드는 것까지만 했고, 붙여서 동작하는지는 확인하지 못했습니다.
- **MCP 규격 공식 문서와의 대조.** 규격은 다른 자료로 짰습니다.
- **설정 화면 버튼을 진짜 `claude`로 눌러 보는 것.** 자동 시험은 가짜 `claude`로 했습니다([손 확인 문서](beta-hand-check.md) §4-4).
- **서명, 공증을 거친 배포본.** (Java를 번들하지 않으므로 「Java가 없는 깨끗한 Mac」은 이제 따로 확인할 일이 아니다.)
