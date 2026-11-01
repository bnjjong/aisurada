# withlog > 1부 vault 엔진 설명서 > 21장 명령 프로젝트 4개

최종 수정: 목요일 10/22 · 22:56

프로젝트는 아젠다를 묶는 상위 바구니다. 20장의 아젠다 명령 4개와 짝을 맞춘 4개 명령으로 다룬다.

- `list-projects`
- `create-project`
- `update-project`
- `delete-project`

아젠다와 달리 노트는 프로젝트를 직접 가리키지 않고, 아젠다를 거쳐서 간접으로만 프로젝트에 묶인다. 프로젝트를 지우려면 소속 아젠다를 모두 떼거나 지워야 한다.

## 21.1 한눈에

| 명령 | 하는 일 | 쓰기 깃발 | 읽는 자리 | 바꾸는 자리 |
|---|---|---|---|---|
| `list-projects` | 프로젝트 목록을 JSON으로 낸다 | 읽기 | 색인 DB의 `projects` 표 | 없음 |
| `create-project` | 프로젝트를 새로 만든다 | 쓰기 | 없음 | `.withlog/projects/<slug>.md`, 색인 DB의 `projects` 표 |
| `update-project` | 이름·색을 고친다 | 쓰기 | 색인 DB의 `projects` 표 | `.withlog/projects/<slug>.md`, 색인 DB의 `projects` 표 |
| `delete-project` | 빈 프로젝트를 지운다 | 쓰기 | 색인 DB의 `projects` 표, `agendas` 표 | `.withlog/projects/<slug>.md`를 `.withlog/trash/projects/`로 이동, 색인 DB의 `projects` 표에서 삭제 |

아젠다의 20.1과 구조가 같다. 차이는 3가지다. 첫째, 프로젝트는 `.withlog/projects/` 아래에 살아서 vault 루트를 더럽히지 않는다. 둘째, `summary`가 없고 본문도 비워 둔다. 셋째, 지울 때 노트를 세지 않고 소속 아젠다를 센다.

## 21.2 부르는 법

| 겉모양 | 쓰는 곳 |
|---|---|
| `wl-cli <명령> [옵션] [인자]` | 터미널에서 1번 부를 때 |
| `wl-cli --serve` 뒤 표준입력으로 `<명령> [옵션] [인자]` 1줄 | 상주 모드로 엔진을 부를 때 |

```bash
wl-cli list-projects
wl-cli create-project <name> [hue=<색>]
wl-cli update-project <id> [title=<새이름>] [hue=<새색>]
wl-cli delete-project <id>
```

한 줄에 한 명령이고, 공백으로 쪼갠다. 실패는 표준에러 1줄로 끝난다(20.2).

| 명령 | 자리 인자 상한 | 받는 옵션 |
|---|---|---|
| `list-projects` | 0개 | 없음 |
| `create-project` | 1개(`<name>`) | `hue=` |
| `update-project` | 1개(`<id>`) | `title=`, `hue=` |
| `delete-project` | 1개(`<id>`) | 없음 |

```bash
$ wl-cli create-project 광고-플랫폼 hue=blue
{"id":"pjmvit0sw0-0a1b2c3d","slug":"광고-플랫폼","title":"광고-플랫폼","hue":"blue"}
```

자리 인자가 모자라거나 넘치면 거절된다. 빈 따옴표 `''`는 빈 문자열 1개로 세어서, `list-projects ''`는 「인자가 없어야 한다」로 거절되고, `create-project ''`는 자리 인자 1개로 통과한 뒤 빈 이름 검증에서 걸린다.

## 21.3 인자와 검증

인자 해석 규칙은 20.3과 같다. `k=v`에서 첫 `=` 앞을 키로 보고, 0.3의 다듬기로 양끝 ASCII 공백(U+0020, `\t`, `\r`, `\n`)을 뗀다.

| 이름 | 다듬기 | 허용 값 | 기본값 | 비고 |
|---|---|---|---|---|
| `<name>` / `title` | 양끝 공백 제거 | 다듬은 글자 수가 1~100자 | 필수 | slug를 여기서 만든다. 기호뿐이면 slug는 `untitled`가 된다 |
| `hue` | 양끝 공백 제거, 소문자화 | 12색 중 하나 | 12색 팔레트에서 자동 배정 | 대소문자 안 가림 (`Blue` -> `blue`) |
| `<id>` | 양끝 공백 제거 | 빈 글이 아님 | 필수 | `update-project`, `delete-project`의 대상 |

12색 팔레트와 slug를 깎는 규칙은 20.3과 완전히 같다. `hue`를 안 주면 그 시점에 색인에 들어 있는 프로젝트 개수를 12로 나눈 나머지로 자동 배정한다.

- 다듬은 이름에서 한글, 영문, 숫자, `-`, `_`만 남기고 나머지는 하이픈으로 바꾼다.
- 하이픈이 거듭되면 1개로 줄이고 양끝 하이픈을 뗀다.
- 남은 글이 없으면 `untitled`가 된다.
- UTF-16 단위로 50자에서 자른다.
- vault 안에 이미 있는 slug면 `-2`, `-3` 식으로 100까지 센다. 100번까지 겹치면 거절된다.

### 검사 차례

1. vault 루트 검사. `.withlog/`가 없으면 `WL-400 vault 를 찾을 수 없다: <경로>`
2. 명령 이름 검사. 4개 중 하나가 아니면 거절
3. 인자 파싱. 자리 인자 수와 옵션 이름을 확인하고, 어기면 `WL-400 ...`
4. 작업공간 검사. `.withlog/projects/` 폴더가 없으면 만들고, 만들기 실패하면 `WL-500 프로젝트 폴더를 만들 수 없다: <경로>`
5. 필드 검증. `name`/`title`의 길이(1~100자), `hue`의 12색 여부를 검사
6. id 조회 (`update`, `delete`). 색인의 `projects` 표에서 id를 찾고, 없으면 `WL-404 프로젝트를 찾을 수 없다: <id>`
7. 업무 규칙 (`delete`). 색인의 `agendas` 표에서 `project = <slug>`인 행을 세고, 1건이라도 있으면 `WL-400 아젠다가 <N>건 있는 프로젝트는 지울 수 없다. 먼저 그 아젠다를 옮긴다: <slug>`
8. 디스크와 색인 반영.

## 21.4 실패

| 차례 | 조건 | 명령 | 종료 | 코드 |
|---|---|---|---|---|
| 1 | vault 루트 없음 | 모두 | 1 | `WL-400 vault 를 찾을 수 없다: <경로>` |
| 2 | 모르는 명령 | - | 1 | `WL-400 알 수 없는 명령이다: <명령>` |
| 3 | 자리 인자 넘침 | 모두 | 1 | `WL-400 <명령> 인자가 너무 많다. 최대 <N>개다` |
| 4 | 자리 인자 모자람 | `create`, `update`, `delete` | 1 | `WL-400 <명령> 인자가 모자란다. <N>개 필요하다` |
| 5 | 모르는 옵션 키 | 모두 | 1 | `WL-400 알 수 없는 옵션이다: <키>` |
| 6 | 옵션에 값 없음 | 모두 | 1 | `WL-400 옵션 값이 비어 있다: <키>` |
| 7 | `.withlog/projects/` 만들기 실패 | `create` | 1 | `WL-500 프로젝트 폴더를 만들 수 없다: <경로>` |
| 8 | `name`/`title`이 빈 글이거나 공백뿐 | `create`, `update` | 1 | `WL-400 프로젝트 이름이 비어 있다` |
| 9 | `name`/`title`이 100자 초과 | `create`, `update` | 1 | `WL-400 프로젝트 이름은 100자 이하여야 한다: <글자수>자` |
| 10 | `hue`가 12색에 없음 | `create`, `update` | 1 | `WL-400 지원하지 않는 색상이다: <값>` |
| 11 | slug 후보 100개가 모두 충돌 | `create` | 1 | `WL-500 프로젝트 slug 를 생성할 수 없다: <기본slug>` |
| 12 | 대상 프로젝트가 색인에 없음 | `update`, `delete` | 2 | `WL-404 프로젝트를 찾을 수 없다: <id>` |
| 13 | 소속 아젠다가 1건 이상 있음 | `delete` | 3 | `WL-400 아젠다가 <N>건 있는 프로젝트는 지울 수 없다. 먼저 그 아젠다를 옮긴다: <slug>` |
| 14 | 파일 쓰기 실패 | `create`, `update` | 1 | `WL-500 프로젝트 파일을 쓸 수 없다: <경로>` |
| 15 | `.withlog/trash/projects/` 만들기 실패 | `delete` | 1 | `WL-500 휴지통 폴더를 만들 수 없다: <경로>` |
| 16 | 휴지통에 같은 파일명이 이미 있음 | `delete` | 1 | `WL-500 휴지통에 이미 파일이 있다: <경로>` |
| 17 | 파일 이동 실패 | `delete` | 1 | `WL-500 프로젝트 파일을 이동할 수 없다: <경로>` |

종료 코드는 0이 성공, 1이 일반 오류, 2가 대상 없음(404), 3이 업무 규칙 위반(아젠다 남음)이다. 아젠다의 `delete-agenda`가 노트 남았을 때 종료 코드 3인 것과 같다(20.4).

`<경로>`는 vault 루트부터의 상대 경로가 아니라 운영체제의 절대 경로로 찍힌다. 실패 메시지는 모두 표준에러 1줄이고 끝에 줄바꿈이 붙는다.

## 21.5 읽고 쓰는 자리

색인 DB `.withlog/index.db`의 `projects` 표를 쓴다(2.2).

| 칸 | 타입 | 설명 |
|---|---|---|
| `id` | `TEXT PRIMARY KEY` | `pj` 접두사 + base32 18자 (`pjmvit0sw0-0a1b2c3d` 모양) |
| `slug` | `TEXT UNIQUE NOT NULL` | 파일 이름(확장자 뺀 것). 1~100자 |
| `title` | `TEXT NOT NULL` | 화면에 표시할 이름 |
| `hue` | `TEXT NOT NULL` | 12색 중 하나 |

`list-projects`는 색인의 `projects` 표를 id 오름차순으로 읽어 JSON 배열로 낸다. 디스크의 `.withlog/projects/` 폴더를 훑지 않는다. 색인 DB가 지워졌거나 깨졌을 때 재색인(reindex)으로 살리는 대상도 아니다. 프로젝트 메타데이터는 오직 이 4개 명령으로만 색인에 들어간다.

`create-project`는 아래 순서로 적는다.

1. slug 결정. 21.3의 규칙으로 겹치지 않는 slug를 찾는다
2. id 채번. `pj` 접두사에 나노초 시각 기반 base32 문자열을 붙인다
3. `.withlog/projects/<slug>.md` 파일 쓰기
4. 색인의 `projects` 표에 INSERT

`update-project`는 색인의 `projects` 표를 UPDATE 한 뒤, `.withlog/projects/<slug>.md` 파일을 새 머리줄로 다시 쓴다. slug는 바뀌지 않고 파일 이름도 그대로다. `title`과 `hue`만 바뀐다. 둘 다 안 주면 아무것도 안 바꾸고 성공 응답을 낸다.

`delete-project`는 아래 순서로 지운다.

1. 색인의 `agendas` 표에서 `project = <slug>`인 행을 센다. 1건이라도 있으면 거절(종료 코드 3)
2. 색인의 `projects` 표에서 해당 행을 DELETE
3. `.withlog/trash/projects/` 폴더가 없으면 생성
4. `.withlog/projects/<slug>.md` 파일을 `.withlog/trash/projects/<slug>-<id>.md`로 이동

프로젝트 파일 `.withlog/projects/<slug>.md`의 내용은 YAML frontmatter만 있고 본문은 비어 있다.

```yaml
---
id: pjmvit0sw0-0a1b2c3d
slug: 광고-플랫폼
title: 광고-플랫폼
hue: blue
---
```

줄바꿈은 LF(`\n`)이고, 마지막 `---` 뒤에도 줄바꿈 1개가 붙는다.

## 21.6 응답

| 키 | 타입 | 설명 |
|---|---|---|
| `id` | 문자열 | 채번된 고유 id |
| `slug` | 문자열 | 고유 슬러그 |
| `title` | 문자열 | 표시 이름 |
| `hue` | 문자열 | 배정된 색상 |

`list-projects`
```json
[
  {
    "id": "pjbulk0000-00000001",
    "slug": "대용량-처리",
    "title": "대용량 처리",
    "hue": "violet"
  },
  {
    "id": "pjmvit0sw0-0a1b2c3d",
    "slug": "광고-플랫폼",
    "title": "광고-플랫폼",
    "hue": "blue"
  }
]
```

`create-project 광고-플랫폼 hue=blue`
```json
{
  "id": "pjmvit0sw0-0a1b2c3d",
  "slug": "광고-플랫폼",
  "title": "광고-플랫폼",
  "hue": "blue"
}
```

`update-project pjmvit0sw0-0a1b2c3d title=광고플랫폼 hue=teal`
```json
{
  "id": "pjmvit0sw0-0a1b2c3d",
  "slug": "광고-플랫폼",
  "title": "광고플랫폼",
  "hue": "teal"
}
```

`delete-project pjmvit0sw0-0a1b2c3d`
```json
{
  "id": "pjmvit0sw0-0a1b2c3d",
  "slug": "광고-플랫폼",
  "archivedPath": ".withlog/trash/projects/광고-플랫폼-pjmvit0sw0-0a1b2c3d.md"
}
```

| 키 | 타입 | 설명 |
|---|---|---|
| `id` | 문자열 | 지워진 프로젝트의 id |
| `slug` | 문자열 | 지워진 프로젝트의 slug |
| `archivedPath` | 문자열 | 이동된 휴지통 경로 (상대 경로) |

## 21.7 모양이 다른 대상

| 대상 | `list-projects` | `create-project` | `update-project` | `delete-project` |
|---|---|---|---|---|
| `.withlog/projects/` 폴더가 없음 | 빈 목록 `[]` 냄 | 폴더를 만들고 프로젝트 생성 | id 못 찾아 `WL-404` | id 못 찾아 `WL-404` |
| 색인 DB 없음 | `WL-500` (DB 열기 실패) | `WL-500` (DB 열기 실패) | `WL-500` (DB 열기 실패) | `WL-500` (DB 열기 실패) |
| 디스크에 파일만 있고 색인에 행 없음 | 목록에 안 나옴 | slug 같으면 충돌 검사에서 `-2` 붙임 | `WL-404` | `WL-404` |
| 색인에 행만 있고 디스크에 파일 없음 | 목록에 정상 출력 | 영향 없음 | 파일 쓰기 실패 시 `WL-500`, 성공 시 파일 재생성 | 행 삭제 후 파일 이동 단계에서 `WL-500` |
| 파일 본문에 내용이 적혀 있음 | 영향 없음 | 해당 없음 | 머리줄만 다시 쓰면서 본문 내용이 사라짐 | 통째로 휴지통으로 이동 |

경계값은 20.7의 slug 및 색 경계와 같다. 이름이 기호뿐이면 slug가 `untitled`가 되고, 같은 slug가 될 이름으로 101번째 만들면 실패하고, 50번째 단위에서 서로게이트 쌍이 갈리는 이름은 `WL-400 경로로 쓸 수 없는 값이다`이고, `hue=Blue`는 거절이고, `update-project ''`는 「인자가 하나 필요하다」다.

## 21.8 변형 명령 create-agenda와 update-agenda의 project

프로젝트 소속은 프로젝트가 아니라 아젠다 쪽에 적힌다. 아젠다 파일의 `project:` 키, 색인 아젠다 표의 프로젝트 slug 칸, 아젠다 응답의 `project` 키가 그 자리다(20.5, 20.6). 아젠다가 프로젝트를 id가 아니라 slug로 가리키는 이유는, 프로젝트 slug가 안 바뀌고, 노트가 이미 아젠다를 slug로 가리켜서 slug 참조가 새 위험을 안 만들고, 사람이 아젠다 파일을 열었을 때 읽을 수 있기 때문이다.

| 명령 | `project=` 값 | 결과 |
|---|---|---|
| `create-agenda` | 안 줌, 빈 글, 공백뿐 | 미분류 |
| `create-agenda` | 그 밖 | 다듬은 값이 slug 모양이면 소속, 아니면 `WL-400 project 값이 안전한 이름이 아니다: <다듬은 값>` |
| `update-agenda` | 안 줌 | 지금 소속을 둔다 |
| `update-agenda` | 빈 글, 공백뿐 | 미분류로 옮긴다. 파일에서 `project:` 줄이 빠진다 |
| `update-agenda` | 그 밖 | 다듬은 값이 slug 모양이면 소속, 아니면 `WL-400 프로젝트 slug 가 안전한 경로 세그먼트가 아니다: <다듬은 값>` |

`update`에서 빈 값을 「미분류」로 읽는 이유는, 뗄 길이 없으면 1번 붙인 아젠다를 영영 못 떼기 때문이다. 2갈래 모두 그 slug의 프로젝트가 색인에 있는지 안 본다. 그래서 없는 프로젝트를 가리키는 아젠다가 생기고, 그 아젠다는 어느 프로젝트에도 안 묶인다.

파일에서 읽은 `project:` 값이 slug 모양이 아니면 그 아젠다를 미분류로 읽는다(2.1). 그 아젠다를 건너뛰면 손편집 오타 1줄 때문에 그 아젠다의 노트가 통째로 색인에서 빠지기 때문이다. 명령은 반대로 거절하는데, 지금 그 값을 보내는 사람이 있어서 조용히 미분류로 만들면 그 사람이 시킨 일이 된 줄 알기 때문이다.

픽스처 vault에서 21.6의 프로젝트를 만든 뒤 `update-agenda agfixture-0000000a project=광고-플랫폼`을 부르면 `serving.md`가 아래처럼 되고, `delete-project pjmvit0sw0-0a1b2c3d`는 종료 코드 3으로 거절된다. `project:`가 맨 뒤에 붙고, `status: active` 줄과 원래 파일 끝의 줄바꿈이 빠졌다.

```yaml
---
id: agfixture-0000000a
slug: serving
title: 서빙
hue: blue
project: 광고-플랫폼
---
서빙 관련 노트를 모은다.
```

```text
WL-400 아젠다가 1건 있는 프로젝트는 지울 수 없다. 먼저 그 아젠다를 옮긴다: 광고-플랫폼
```

`update-agenda agfixture-0000000a project=`로 떼면 `project:` 줄이 빠지고, 그 뒤의 `delete-project pjmvit0sw0-0a1b2c3d`가 된다.

```json
{
  "id": "pjmvit0sw0-0a1b2c3d",
  "slug": "광고-플랫폼",
  "archivedPath": ".withlog/trash/projects/광고-플랫폼-pjmvit0sw0-0a1b2c3d.md"
}
```

## 21.9 받아들임 묶음과 얼어 있는 특이점

묶음은 이 장의 명령 4개를 안 부르고, 픽스처에 프로젝트 파일이 없다. 이 장의 모든 규칙은 「코드 읽기로 확인」이다. 프로젝트 파일을 읽어 색인을 짓는 구현은 아래 1번을 고친 것과 같아서, 지금 엔진과 답이 갈린다.

지금 엔진의 특이점이고, 지킬지 고칠지는 부록 D에서 정한다.

1. 엔진이 프로젝트 파일을 읽지 않는다. 색인 DB가 새로 생기면 `list-projects`가 `[]`이고, `update-project`와 `delete-project`는 `WL-404`다. 그사이 아젠다의 `project`는 그대로 남아서 프로젝트 머리줄 없는 소속이 생긴다. 같은 이름으로 `create-project`를 다시 부르면 같은 slug로 파일을 새 id로 덮고, 그 slug를 가리키던 아젠다가 새 프로젝트에 다시 묶인다. 아젠다 표처럼 파일에서 다시 짓는 길이 아예 없다.
2. `list-projects`는 정렬하지 않고 색인 표의 행 차례로 낸다. 엔진 소스의 주석은 slug 차례라고 적는다.
3. `update-project`는 파일이 아니라 색인 행을 바탕으로 파일을 통째로 다시 써서, 손으로 고친 이름, 색, 본문을 덮는다.
4. 같은 이름의 `create-project` 2개가 serve 모드에서 동시에 돌면, 2번째가 1번째의 파일을 자기 id로 덮은 뒤 색인 행을 넣다가 slug 유일 제약에 걸려 예상 밖 실패(6.6)로 끝난다. 색인은 1번째 id를, 파일은 2번째 id를 든다. 아젠다와 달리 색인 맞추기가 이 표를 안 읽어서 그 뒤의 명령이 막히지는 않는다. 코드 읽기와 SQLite 재현으로 확인했고 엔진을 실행해 보지는 않았다.
5. 아젠다가 남은 프로젝트의 거절이 `WL-409`가 아니라 `WL-400`이다.
6. `delete-project`의 아젠다 수는 색인의 아젠다 행으로 센다. 그래서 파일을 손으로 지운 아젠다(20.9의 1번)가 계속 그 프로젝트를 가리켜 지우기를 막는다. 그 아젠다를 `update-agenda <아젠다 id> project=`로 떼면(아젠다 파일이 다시 생긴다) 풀리고, 노트가 0건이면 `delete-agenda`로 지워도 풀린다. 파일의 `project:`가 slug 모양이 아니어서 미분류로 읽힌 아젠다는 안 센다.
7. 옮길 자리에 이미 파일이 있으면 색인 행만 지워진 채 `WL-500`으로 끝난다. 프로젝트 행은 다시 짓는 길이 없어서, 아젠다(20.9의 10번)와 달리 그 프로젝트는 파일이 `.withlog/projects/`에 남은 채로 목록에서 영영 사라진다.
8. 프로젝트 이름의 50단위 자르기도 서로게이트 쌍을 갈라서, 그런 이름으로는 `WL-400 경로로 쓸 수 없는 값이다`로 프로젝트를 못 만든다(20.9의 11번).

정할 것은 1번을 고쳐 프로젝트도 색인 맞추기에 넣을지다. 넣으면 2번의 차례, 3번의 덮기, 7번의 사라짐이 함께 바뀐다.
