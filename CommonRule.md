# CommonRule (aisurada 프로젝트 공통 규칙)

이 문서는 aisurada 저장소에서 일하는 모든 AI 어시스턴트와 작업자가 지켜야 하는 공통 규칙이다. 작업을 시작하기 전에 읽고, 작업을 마친 뒤에는 §1의 체크리스트를 점검한다.

공통 규칙은 **이 파일 한 곳에서만** 고친다. `CLAUDE.md`는 이 파일을 가져다 쓸 뿐 규칙을 따로 적지 않는다.

---

## 1. 작업 완료 체크리스트 (이 순서대로)

1. **빌드·시험 통과를 확인한다.** 바꾼 범위에 맞춰 돌린다(§6). 시험이 실패했거나 비밀, 토큰, 실제 vault 내용 같은 개인정보가 섞였으면 커밋하지 않고 원인을 먼저 보고한다.
2. **문서 갱신 여부를 점검한다.** §2에 따라 고칠 문서가 있는지 본다.
3. **커밋하고 `main`에 푸시한다.** 따로 확인을 받지 않는다. §4를 따른다.
4. **작업 폴더를 정리한다.** §5를 따른다.
5. **사용자에게 한두 줄로 보고한다.** 커밋·푸시한 것, 정리한 것(지운 worktree와 브랜치), 현재 브랜치와 원격이 같은지를 알린다.

---

## 2. 문서 갱신 규칙

| 문서 | 갱신 기준 |
|------|-----------|
| `docs/developer-guide.md` (원본) | 명령, 환경 변수, 이름, 시험 진입점과 수치, 구조, 한계가 바뀔 때 |
| `docs/developer-guide.html` (사본) | 원본을 고친 뒤 `pnpm --dir native-host docs:guide`로 다시 만든다. HTML을 직접 고치지 않는다 |
| `README.md` | 빌드 산출물, 실행 명령, 환경 변수, 명령 목록이 바뀔 때 |
| `docs/beta-hand-check.md` | 앱을 격리해서 띄우는 방법이나 자동 시험이 재는 범위가 바뀔 때 |

- `docs/rebuild-*`, `docs/review-*` 같은 기록은 그날의 상태를 적은 것이라 고치지 않는다. 장 작업을 끝내면 완료 기록 `docs/rebuild-p<부>-chapter<NN>-complete.md`를 새로 쓴다.
- 문서에 적는 명령과 수치는 직접 돌려 확인한 것만 쓴다. 못 돌린 것은 못 돌렸다고 적는다.

---

## 3. 이름 규칙

- 프로젝트 이름은 `aisurada`, 접두는 `ais`다(`ais-cli`, `ais-engine.jar`, `AIS-409`, `AISURADA_*`, `.aisurada/`).
- 새 코드, 문서, 시험에 옛 이름(`withlog`, `wl`과 그 대문자·카멜 변형)을 쓰지 않는다. 옛 이름이 허용되는 곳은 `docs/` 아래 옛 기록, `docs/developer-guide.md` §10의 옛 이름 대응표, 외부 설명서의 제목과 파일 경로뿐이다.
- 이름이나 문구를 바꿀 때는 **글자 수를 숫자로 박아 둔 코드**(오프셋, 고정 폭 머리)가 없는지 먼저 찾는다. 예: HTML 머리 마커 `<!--aisurada`의 길이, 소켓 프레임 머리 4바이트. 숫자 대신 문자열의 `length()`를 쓴다.
- 일괄 치환은 단어 경계를 지키고(`newline`, `rawLimit` 같은 일반 단어를 건드리지 않는다), 끝난 뒤 남은 옛 이름이 없는지 훑는다. 바이너리 파일과 긴 base64 줄은 치환하지 않는다.

---

## 4. Git 커밋과 푸시

형식: `type(scope): subject [Jira-ID]`

```
type(scope): subject [Jira-ID]

body (필수: 한글, 72자 내외/줄, 비즈니스 맥락과 변경 이유)

footer (선택: BREAKING CHANGE, 해결됨/관련 이슈)
```

- **type**: `feat` | `fix` | `docs` | `style` | `refactor` | `test` | `chore`
- **scope**: 바뀐 모듈. 엔진과 문서는 `aisurada`, 호스트와 화면은 `native-host`.
- **subject**: 영어, 소문자 동사원형, 50자 이내, 마침표 없음.
- **Jira-ID**: 대괄호 필수. 발급받지 않았으면 `[Empty]`.
- 깨지는 변경(이름, 형식, 프로토콜)은 `BREAKING CHANGE:` 푸터에 적는다.

푸시 규칙:

- **작업이 끝나면 항상 커밋하고 `main`에 푸시한다.** 따로 확인을 받지 않는다. 브랜치와 PR은 쓰지 않고, 필요하면 작업용 브랜치에서 짓되 결과는 `main`에 빨리 감기(`git merge --ff-only`)로 올린다.
- 푸시 전에 `git fetch`로 원격이 앞서 있는지 본다. 앞서 있으면 그 위로 옮기고, 같은 파일을 둘 다 고쳤으면 멈추고 보고한다.
- **내가 바꾼 파일만 커밋한다.** 같은 폴더를 다른 세션이 쓰고 있을 수 있으니 `git status`로 남의 미커밋 수정을 확인하고, 그 파일은 건드리지도 커밋하지도 않는다. 파일 경로를 지정해서 커밋한다.
- 커밋은 논리 단위로 나눈다. 이름 변경 같은 기계적 변경과 동작 변경을 한 커밋에 섞지 않는다.
- 이 규칙이 허락하는 것은 `main` 커밋과 푸시뿐이다. 강제 푸시, 이력 재작성, 원격 브랜치 삭제는 사용자가 시킬 때만 한다.

---

## 5. 작업 폴더 정리

메인 폴더(`/Users/df/github/aisurada`)는 늘 `main`에 둔다. 시험이 길거나 다른 세션과 겹칠 수 있으면 별도 worktree에서 짓는다.

```bash
git worktree add -b <브랜치> <경로> main
mkdir -p <경로>/lib && cp lib/snakeyaml-2.4.jar <경로>/lib/          # lib/는 git이 무시한다
cp -cR native-host/src-tauri/target <경로>/native-host/src-tauri/target   # APFS 복제(약 12GB지만 거의 공간을 안 쓴다)
(cd <경로>/native-host && pnpm install --frozen-lockfile --offline)
```

**작업이 끝난 뒤에는 항상 이 순서로 정리한다.**

1. 내가 만든 worktree를 모두 지운다. `git worktree list`로 확인하고 `git worktree remove --force <경로>` 뒤 `git worktree prune`을 한다. 다른 세션이 만든 worktree는 건드리지 않는다.
2. 작업 결과를 `main`에 올린다(§4).
3. 메인 폴더의 브랜치를 `main`으로 돌려 놓는다. `git switch main` 뒤 `git status -sb`로 `origin/main`과 같은지 확인한다. 전환 전에 다른 세션의 미커밋 수정이 있는지 보고, 있으면 버리지 말고 그대로 둔 채 알린다.
4. `main`에 병합이 끝난 작업 브랜치는 로컬에서 `git branch -d`로 지운다(병합 안 된 브랜치는 지우지 않는다).

---

## 6. 시험

| 바꾼 곳 | 돌릴 것 |
|---------|---------|
| 엔진 (`src/`, `test/`) | `bash test.sh` |
| 화면·호스트 (`native-host/`) | `bash test-native.sh` |
| 받아들임 묶음 (`fixtures/`) | `docs/developer-guide.md` §8의 34장 명령 3개 |
| 편집기, 저장, 창, 목록 같은 화면 동작 | 해당 `pnpm --dir native-host test:*:acceptance` |
| 4부 (묻기, 세션) | `pnpm --dir native-host test:part4:acceptance` |
| 이름, 번들, 권한, 호스트 계약 | 위를 모두 + `pnpm --dir native-host tauri build --debug --bundles app` |

- `build.sh`, `test.sh`, `test-native.sh`, `tauri build`, acceptance는 `dist/`와 `build/`를 지웠다 다시 만든다. **같은 폴더에서 둘 이상을 동시에 돌리지 않는다.** 다른 세션이 돌리는 중이면 별도 worktree에서 한다.
- 앱을 직접 띄울 때는 진짜 HOME과 launchd를 건드리지 않게 격리한다(엔진을 먼저 직접 띄우고, `AISURADA_SIDECAR_SOCKET`을 102바이트 이하 경로로 준다). `open 문서.md`는 쓰지 않는다. 번들이 `.md` 기본 편집기라서 앱이 진짜 HOME으로 뜬다.

---

## 7. 참조

- [`docs/developer-guide.md`](docs/developer-guide.md): 구성, 빌드와 시험, 엔진 사용법, 자주 막히는 곳
- [`docs/beta-hand-check.md`](docs/beta-hand-check.md): 앱을 격리해서 띄우는 법과 손 확인 점검표
- [`README.md`](README.md): 엔진 명령, 환경 변수, 종료 코드
