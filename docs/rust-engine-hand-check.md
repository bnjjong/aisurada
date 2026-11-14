# Rust 엔진 손 확인 안내

## 요약

**결론:** Rust 엔진이 읽는 명령 27개와 쓰는 명령 43개를 실제 앱 창에서 확인하는 일은 **사람이 눌러야 하는 부분이 남아 있다.** 아래 한 줄로 격리 환경에서 앱을 띄우고 점검표를 따라 누르면 된다. 앱이 어느 명령을 Rust 엔진으로 보냈는지는 앱을 띄운 터미널에 `[engine] 명령 ok 걸린시간ms`로 한 줄씩 찍힌다. 이슈 [#54](https://github.com/bnjjong/aisurada/issues/54), [#61](https://github.com/bnjjong/aisurada/issues/61)이 이 확인의 기록 자리다.

## 1. 띄우는 법

저장소 루트에서 한다.

```bash
./build.sh
pnpm --dir native-host tauri build --debug --bundles app --config '{"identifier":"co.kcd.aisurada.handcheck"}'
engine-rs/tools/hand-check/run.sh --write rust --git
```

| 옵션 | 뜻 |
| --- | --- |
| (없음) | 읽기만 Rust 엔진. 쓰기는 Java 엔진이다 |
| `--write rust` | 쓰기 스위치(`AISURADA_ENGINE_WRITE=rust`)를 켠다. 쓰기 43개가 Rust 엔진으로 간다 |
| `--engine java` | 모든 명령을 Java 엔진으로 보낸다(`AISURADA_ENGINE=java`). Rust와 견줄 때 쓴다 |
| `--git` | 합성 vault에 git 저장소와 노트의 판 이력을 만든다(편집 이력 창, 자동 커밋 확인용) |

스크립트가 하는 일: 합성 vault(진짜 vault 내용 없음)와 가짜 HOME을 `~/ais-rust-check`에 만들고, Java 엔진을 먼저 띄운 뒤(앱이 launchd에 엔진을 등록하지 않게), 앱 바이너리를 격리 환경 변수로 실행한다. Ctrl-C로 앱과 엔진이 함께 내려가고 폴더는 남는다. 이 스크립트가 만들지 않은 폴더는 지우지 않는다.

**식별자를 바꿔 빌드하는 것이 필수다.** 번들 식별자가 설치된 진짜 앱(`co.kcd.aisurada`)과 같으면 가짜 `HOME`을 줘도 웹뷰 저장소가 진짜 앱과 공유된다. 그러면 격리 앱이 진짜 앱이 마지막으로 연 작업 공간을 열고, 쓰기 스위치를 켜면 그 vault에 쓸 수 있다. 스크립트는 식별자가 같으면 앱을 띄우지 않는다. 식별자를 바꾼 앱은 처음에 아무 작업 공간도 열지 않으므로, 앱의 「폴더 열기」에 스크립트가 알려 주는 vault 경로(`~/ais-rust-check/vault`)를 넣는다.

## 2. 점검표

### 읽기(옵션 없이 띄운다)

- [ ] 「폴더 열기」로 vault를 열면 노트 40여 개와 아젠다 4개가 보인다. 터미널에 `[engine] list_docs ok`, `list_agendas ok`, `reindex ok`가 찍힌다
- [ ] 노트를 열면 본문, 코멘트, 첨부, 후속 노트 탭(`모든 필드가 있는 노트`와 부모·자식 노트), 아젠다 사실층이 보인다(`get_doc`, `list_comments`, `list_attachments`, `list_sub_notes`, `get_agenda_facts`)
- [ ] 같은 vault를 `--engine java`로 띄워 노트 수, 순서, 후속 노트, 코멘트가 같은지 견준다

### 읽기: 편집 이력(`--git`)

- [ ] 노트의 이력 창에서 판 목록(시각, 편집자), 글자 찾기, 판 보기가 된다(`[engine] history ok`)
- [ ] 부모 노트 하나의 판이 6개 보이고 스냅숏 판(`snapshot`)이 섞여 있다

### 쓰기(`--write rust`)

각 항목을 하고 `~/ais-rust-check/vault`의 파일을 확인한다.

- [ ] 아젠다 만들기(이름과 색), 고치기(이름, 요약, 저장소), 노트가 없는 아젠다 지우기 → `.aisurada/agendas/<slug>.md`가 생기고 바뀌고 `.aisurada/trash/agendas/`로 옮겨진다(`create_agenda`, `update_agenda`, `delete_agenda`)
- [ ] 프로젝트 만들기(저장소 목록 포함), 고치기, 지우기 → `.aisurada/projects/`(`create_project` 따위)
- [ ] 문서 템플릿 저장(새로, 고침)과 삭제, 회고 템플릿 고치기(`save_template`, `save_ritual_template`, `delete_doc_template`)
- [ ] 아젠다 사실층 저장과 지움 → `.aisurada/facts/<slug>.md`(`update_agenda_facts`)
- [ ] 태스크 화면: 목록과 상세가 보인다(읽기는 옵션 없이도 Rust). 태스크를 만들고(완료 기준을 안 쓰면 거절된다), 제목·본문을 고치고, 완료 기준을 체크하고, 상태를 옮기고, 다른 태스크와 연결하고, 댓글을 달고, 지웠다가 되살린다 → `.aisurada/tasks/TSK-<번호>.md`와 `TSK-<번호>/activity.jsonl`이 생기고 바뀐다. 막은 태스크가 Done 이 되기 전에는 막힌 태스크의 「시작」이 막힌 까닭과 함께 비활성이다(`create_task`, `update_task`, `transition_task`, `link_tasks`, `add_task_comment`, `delete_task`, `restore_task`)
- [ ] 스킬·에이전트 화면(읽기는 옵션 없이도 Rust): 글로벌 스킬과 에이전트를 만들고, 설명·지침·권한·도구 칸을 고치고, 이름을 바꾸고(파일이나 폴더가 새 이름으로 옮겨진다), 프로젝트 화면에서 글로벌 자원 참조를 켜고 끄고, 지웠다가 되살린다 → `.aisurada/resources/global/`과 `projects/<slug>/` 아래 파일이 생기고 바뀐다. 파일 머리말은 `id`, `name`, `description` 순서이고 고칠 때마다 `.aisurada/resources/.history/<id>/`에 고치기 전 판이 쌓인다. 앱 밖의 승인 기록은 `~/ais-rust-check/home/Library/Application Support/aisurada/approvals/<해시>/ledger.json`에 있고 사람이 한 변경은 `"by": "human"`이다(`save_resource`, `delete_resource`, `restore_resource`, `set_resource_ref`)
- [ ] 승인 화면: 자원 파일을 편집기로 직접 고치면 「승인 안 된 직접 수정」으로 보이고 목록과 조회에는 승인한 판이 계속 보인다. 「받아들이기」면 지금 파일이 승인한 판이 되고, 「되돌리기」면 승인한 판으로 돌아온다. 승인 정책 네 칸을 바꾸면 저장된다(`resolve_resource_edit`, `set_resource_policy`). AI 세션이 올리는 대기 변경(승인·거절)은 AI 세션이 있어야 만들어지므로 이 점검표에서는 Rust 통합 시험(`queues_ai_resource_changes_until_a_human_approves`)으로 대신했다
- [ ] 문답과 레일 대화: 노트 폴더에 `ask.json`(`{"v":1,"noteId":"…","sessionId":"s","turns":[…]}`)과 `chats/<대화 id>.json`을 손으로 만들어 두고 노트를 열면 문답 밑줄과 레일의 대화 목록이 보인다(`get_ask`, `list_chats`는 옵션 없이도 Rust). Claude를 쓸 수 있는 환경이면 본문을 드래그해 물어보고 레일에서 물어서 `ask.json`과 `chats/`가 생기고 회차가 붙는지 본다. 깨진 JSON이나 1MB 넘는 대화 파일은 「못 읽은 대화 파일이 있어요: <파일 이름>」 안내로 이름만 나온다(`append_ask_turn`, `append_chat_turn`)
- [ ] 새 작업 공간 만들기(`init_workspace`)
- [ ] 노트의 즐겨찾기 별, 평가 점수, 다른 노트 연결을 바꾼다 → 그 노트 머리에 `favorite`, `evaluation`, `links` 줄이 생기고 `editedBy: app` 도장이 붙는다(`set_favorite`, `evaluate_doc`, `link_docs`)
- [ ] 새 노트를 만든다(아젠다와 제목을 정하고 저장) → `notes/<아젠다>/<연-월>/<날짜>-<시각>-<제목>-<꼬리>/note.md`가 생기고 터미널에 `[engine] create_doc ok`가 찍힌다. 같은 날 새 노트를 연달아 만들면 시각이 5분씩 밀린다. 없는 이름의 아젠다로 만들면 `.aisurada/agendas/`에 새 아젠다 파일도 생긴다
- [ ] 노트를 휴지통으로 보냈다가 되살리고, 다른 노트는 완전 삭제하고, 휴지통을 비운다 → `.aisurada/trash/files/`로 폴더(단일 파일은 확장자가 붙은 이름)가 옮겨지고 `.aisurada/trash/trash.json` 맨 앞에 항목이 생긴다. 되살리면 원래 아젠다의 월별 폴더로 돌아온다(같은 이름이 있으면 `-restored-…`). 즐겨찾기였던 노트를 보내면 「즐겨찾기였어요」 안내가 나온다. 비우면 목록이 `[]`가 된다(`delete_doc`, `restore_doc`, `purge_doc`, `empty_trash`)
- [ ] 노트에서 본문 일부를 드래그해 코멘트를 달고, 고치고, 지운다 → 노트 폴더의 `comments.json`(후속 노트는 부모 폴더의 `sub-comments/<id>.json`)이 생기고 바뀌고, 마지막 코멘트를 지우면 파일이 사라진다. 지울 때 본문에서 그 코멘트의 앵커 표시가 걷힌다(`create_comment`, `update_comment`, `delete_comment`)
- [ ] 노트에서 「새 후속 노트」를 만들고 제목·본문을 고친 뒤 지운다 → `…/sub/<날짜>-<시각>-<제목>-<꼬리>/note.md`가 생기고, 제목을 바꾸면 폴더 이름이 새 제목으로 옮겨지고, 지우면 폴더가 없어진다. 부모 본문에 그 노트의 카드 줄(`[제목](#sub:…)`)이 있었다면 지울 때 그 줄도 없어진다(`create_sub_note`, `update_sub_note`, `delete_sub_note`)
- [ ] 노트 본문을 고치고 0.6초 기다린다(자동 저장) → 터미널에 `[engine] update_doc ok`가 찍히고, 노트 머리의 `editedAt`이 바뀌고, 파일 본문이 새 글이다
- [ ] 노트의 제목(첫 헤딩)을 바꾼다 → 노트 폴더 이름이 새 제목으로 옮겨진다. 후속 노트가 있는 노트(`parent`)는 후속 노트와 첨부가 폴더째 따라온다
- [ ] 노트의 날짜나 시각을 바꾼다 → 폴더 이름의 날짜·시각이 바뀌고, 날짜의 달이 바뀌면 다른 월별 폴더(`2026-12/` 따위)로 옮겨진다
- [ ] 같은 노트 파일을 다른 편집기로 먼저 고친 뒤 앱에서 고친다 → 「다른 쪽이 이 노트를 먼저 고쳤어요」가 나오고 저장이 멈춘다(`AIS-412`)
- [ ] 안내된 오류가 화면에 나온다: 노트가 있는 아젠다 지우기, 이름 없는 아젠다 저장

### 자동 커밋(`--git --write rust`)

- [ ] 쓰기 뒤 5분(유휴 300초) 조용히 두면 `git -C ~/ais-rust-check/vault log -3`에 `ais(app): 노트 …개, 파일 …개` 커밋이 생긴다
- [ ] 앱을 닫고 열 때 커밋이 남는다(`commit_vault`)

### 되돌리기

- [ ] 문제가 있으면 `--engine java`로 띄워 같은 동작이 되는지 본다. Rust 쪽만 틀리다면 이슈에 명령 이름과 화면을 적는다

점검이 끝나면 `rm -rf ~/ais-rust-check`로 지운다.

## 3. 지금까지 확인한 것

- **격리 환경에서 앱이 뜨는 것**(식별자를 바꾼 번들): 시작할 때 엔진 호출이 0건이고 작업 공간을 열지 않는다.
- **실제 창에서 읽기 6개와 `init_workspace`가 Rust 엔진으로 가서 오류 없이 끝난 것**: 식별자를 바꾸기 전의 번들이 설치된 앱의 웹뷰 저장소를 공유해서 개발자의 vault가 한 번 열렸을 때 `init_workspace`, `list_templates`, `list_trash`, `reindex`, `list_projects`, `list_agendas`, `list_docs`가 모두 `ok`로 끝났다. 그 vault는 읽기와 아무것도 바꾸지 않는 `init_workspace`만 받았고 파일은 바뀌지 않았다. 이 일 때문에 위의 식별자 규칙과 스크립트의 거절 장치를 만들었다. 노트를 열기, 쓰기, 이력 창은 못 눌러 봤다.
- 클릭이 필요한 확인은 접근성으로 웹 화면 안이 읽히지 않고 창 이름이 진짜 앱과 같아서 자동화하지 않았다(잘못 눌러 진짜 vault에 쓸 위험).
