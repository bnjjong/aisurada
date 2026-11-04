# 베타 판정용 손 확인 안내

작성일: 2026-11-04. 기준 판은 `afd4075`이고, 앱은 `pnpm tauri build --debug --bundles app`으로 지었다.

## 지금까지 자동으로 확인한 것과 못 한 것

| 구분 | 상태 |
|---|---|
| 엔진 `bash test.sh` | 전부 통과 |
| 화면·호스트 `bash test-native.sh` | Node 422건, cargo 88건 통과 |
| 실제 WKWebView acceptance (4부를 뺀 판) | host 26, workspace 9, navigation 11, interaction 12, data 12, editor 114, autosave 16 통과. views는 첫 실행에서 `favorites-category-and-graph-accessible-node`가 `timeout: graph`로 실패했고 재실행에서 29/29 통과(간헐 실패, 원인 미상) |
| 4부를 켠 판의 실제 창 acceptance | `test:part4:acceptance` 7/7 통과(묻기 왕복, 전사가 있는 세션 화면 포함). 레일 대화, 세션 입력창, PROJECTS 레일, 굳히기는 안 잰다 |
| 사람이 앱을 직접 조작한 확인 | **0건** |

아래 점검은 위 마지막 두 줄을 메우는 일이다.

## 1. 앱을 안전하게 띄우는 법

번들 앱을 그냥 켜면(`open aisurada.app`) 진짜 HOME을 쓰고, `ais-cli`가 엔진을 띄우려고 `~/Library/LaunchAgents`에 plist를 쓰고 `launchctl`을 부른다. 그 레이블이 앱 설치본과 같은 꼴이라 이미 깔린 앱의 엔진을 내릴 수 있다. 그래서 아래처럼 **엔진을 먼저 직접 띄우고** 앱 바이너리를 환경 변수와 함께 실행한다. 이렇게 하면 `ais-cli`가 launchd를 부르지 않고 그 소켓에 붙는다(README의 serve 모드 절).

- 빌드한 번들은 `.md`/`.markdown`의 기본 편집기로 등록된다(`fileAssociations`). 그래서 `open 문서.md`를 해도 이 앱이 진짜 HOME으로 뜬다. 이 문서도 `open`으로 열지 말고 에디터나 브라우저로 연다. 2026-11-04에 실제로 그렇게 떴다.
- 소켓 경로는 102바이트 이하여야 한다. 길면 엔진이 `소켓 경로가 102 바이트를 넘는다`로 선다. 아래는 홈 바로 아래 짧은 폴더를 쓴다.
- 앱의 Java도 `user.home`을 격리 HOME으로 맞춘다. 안 그러면 엔진이 진짜 `~/.claude/projects`를 읽는다.
- 앱을 띄운 터미널을 닫거나 `Ctrl-C`를 누르면 앱이 내려간다.

터미널 1(엔진). 저장소 루트에서 실행한다.

```bash
export HC_DIR=$HOME/ais-hand-check
/bin/rm -rf "$HC_DIR"; mkdir -p "$HC_DIR/home/.claude/projects" "$HC_DIR/vault"
printf '#!/bin/sh\nexec /usr/bin/java -Duser.home="%s" "$@"\n' "$HC_DIR/home" > "$HC_DIR/java"; chmod 755 "$HC_DIR/java"
printf '#!/bin/sh\ncat >/dev/null\necho "가짜 claude의 답이에요."\n' > "$HC_DIR/fake-claude"; chmod 755 "$HC_DIR/fake-claude"

java -Duser.home="$HC_DIR/home" -jar dist/ais-engine.jar init-workspace name=손확인 'templates=[]' --aisurada.vault="$HC_DIR/vault"
export AISURADA_VAULT="$HC_DIR/vault" AISURADA_SIDECAR_SOCKET="$HC_DIR/engine.sock"
java -Duser.home="$HC_DIR/home" -jar dist/ais-engine.jar serve
```

터미널 2(앱). 같은 `HC_DIR`을 쓴다.

```bash
export HC_DIR=$HOME/ais-hand-check
cd /Users/df/github/aisurada
AISURADA_VAULT="$HC_DIR/vault" AISURADA_SIDECAR_SOCKET="$HC_DIR/engine.sock" \
HOME="$HC_DIR/home" AISURADA_CLI_JAR="$PWD/dist/ais-cli.jar" AISURADA_SIDECAR_JAVA="$HC_DIR/java" \
AISURADA_CLAUDE_BIN="$HC_DIR/fake-claude" \
native-host/src-tauri/target/debug/bundle/macos/aisurada.app/Contents/MacOS/aisurada-host
```

앱이 뜨면 「열기」로 `$HOME/ais-hand-check/vault`를 고르고, 아젠다 하나와 노트 하나를 만든다. 끝나면 `Ctrl-C`로 엔진과 앱을 내리고 `/bin/rm -rf "$HOME/ais-hand-check"`로 지운다. 확인은 `ls ~/Library/LaunchAgents | grep -i aisurada`가 비어 있는지로 한다.

> 이 방법의 엔진 기동과 앱 기동은 이 문서를 쓰면서 한 번 돌려 확인했다. 앱이 8초 뒤에도 떠 있었고 `ais-cli list-docs`가 소켓으로 답했으며 LaunchAgent는 생기지 않았다. 아래 점검 자체는 아직 아무도 하지 않았다.

## 2. 2부 손 확인 7개 (`rebuild-p2-review-2026-10-31.md`)

| 확인 | 격리 환경에서 | 할 일과 기대 |
|---|---|---|
| main·노트 창의 실제 메뉴 클릭 | 가능 | 메뉴 5개를 눌러 본다. 노트 창이 떠 있으면 그 창이 받는다 |
| 허용한 파일을 기본 앱으로 열기 | 가능 | 노트의 첨부(그림)를 열면 기본 앱이 뜬다. 실행 파일이나 `.app` 폴더는 거절된다 |
| 개발 실행의 jar 선택 | 가능 | `AISURADA_CLI_JAR` 없이 띄웠을 때 어느 jar를 고르는지 앱의 오류 문구나 동작으로 본다 |
| 편집 중인 노트 창의 닫기 요청·Cmd+W와 저장 | 가능 | 노트 창에서 글을 고치다 Cmd+W. 저장되거나 「창을 닫으면 사라지는 것이 있어요」 상자가 뜬다 |
| 비밀 표 뜨기와 굳히기 | **불가** | 진짜 `.zshrc`와 Keychain이 있어야 한다. 격리 HOME에서는 못 본다. 진짜 HOME에서 따로 본다 |
| 모니터 조합 변경과 yabai 데스크톱 복원 | **불가** | 외부 모니터를 꽂았다 뽑아야 한다 |
| 번들 앱의 `aisurada://` 스킴, Markdown 파일 연결, 자원 경로 | **불가** | LaunchServices 등록이 필요하다. 시스템 설정에 영향이 있어서 마지막에, 따로 정하고 한다 |

## 3. 3부 터미널 손 확인 (`rebuild-p3-fix-list-complete.md`)

- 터미널 탭(Cmd+Shift+T)에서 한글을 입력하고 조합 중에 Enter를 눌러 본다.
- 붙여넣기 할 때 WebKit의 허락 메뉴가 뜨는지 본다.
- `[?2004h`(브래킷 붙여넣기 표시)가 화면에 그대로 찍히지 않는지 본다.
- 한글과 상자 글자(`─│┌`)가 깨지지 않고 너비가 맞는지 눈으로 본다.

## 4. 4부 흐름 (가짜 claude)

`AISURADA_CLAUDE_BIN`이 가짜 `claude`를 가리키므로 토큰을 쓰지 않는다. 가짜 `claude`는 늘 `가짜 claude의 답이에요.`만 낸다.

### 4-1. 묻기

1. 노트 본문에서 글을 드래그하고 「AI에게 묻기」를 누른다.
2. 묻는 칸에 질문을 쓰고 「묻기」(또는 Cmd+Enter)를 누른다.
3. 기대: 「요청 중」 줄이 잠깐 보이고, 노트 아래 「AI 문답」 칸과 밑줄 카드에 `가짜 claude의 답이에요.`가 나온다. 다른 탭에 있으면 탭에 「AI 답이 왔어요」 점이 뜬다.
4. 터미널에서 `ls "$HOME/ais-hand-check/vault/notes/"*/*/ask.json`이 파일을 보여야 한다.
5. 같은 대목에 이어 묻기를 한 번 더 한다. 같은 밑줄 아래 묶여야 한다.
6. 표 줄과 HTML 상자에서도 묻기가 되는지 본다(해당 노트가 있으면).

### 4-2. 세션 화면과 레일

세션 화면은 `~/.claude/projects`의 전사가 있어야 보인다. 격리 HOME에 가짜 전사를 하나 둔다.

```bash
export HC_DIR=$HOME/ais-hand-check
SID=f1a50000-0000-4000-8000-000000000001
NOW=$(date -u +%Y-%m-%dT%H:%M:%SZ)
mkdir -p "$HC_DIR/home/.claude/projects/-Users-test-vault"
printf '%s\n' \
  "{\"type\":\"user\",\"entrypoint\":\"cli\",\"cwd\":\"/Users/test/vault\",\"timestamp\":\"$NOW\",\"message\":{\"content\":\"세션 점검용 첫 물음\"}}" \
  "{\"type\":\"assistant\",\"timestamp\":\"$NOW\",\"message\":{\"model\":\"claude-opus-4\",\"content\":[{\"type\":\"text\",\"text\":\"세션 점검용 첫 답\"}]}}" \
  "{\"type\":\"custom-title\",\"customTitle\":\"손확인 세션\"}" \
  > "$HC_DIR/home/.claude/projects/-Users-test-vault/$SID.jsonl"
```

1. 레일의 PROJECTS에 「손확인 세션」이 보이는지 본다(앱을 다시 열거나 목록을 새로 읽는다).
2. 눌러 `/session/<id>` 화면으로 간다. 머리 줄, 사용자·어시스턴트 대화, 「이어 가기 명령 복사」 단추를 본다.
3. 하단 입력창에 글을 쓰고 보낸다. 기대: 가짜 `claude`의 답이 붙는다. 세션 입력은 `resume` 인자로 가므로 이 단계에서 `claude -p`가 불린다.
4. 오른쪽 레일의 「관련 노트」 탭이 기본으로 서 있고, CLI 탭을 누르기 전에는 셸이 자동으로 안 뜨는지 본다.
5. CLI 탭을 눌러 터미널이 뜨고 세션을 이어 가는 명령이 실행되는지 본다. 이 PATH의 진짜 `claude`는 격리 HOME에 로그인이 없어서 안내가 나올 수 있다. 이 단계는 명령이 실행되는 데까지만 본다.
6. 「굳히기」로 전사를 노트로 만든다. 기대: 태그 `세션전사`인 새 노트가 생긴다.
7. 세션 칩(`sessionId`)이 달린 노트가 있으면 그 칩으로 노트 옆 세션 레일을 열고, 레일 대화에서 한 번 묻는다. 칩이 달린 노트를 이 환경에서 만들 수 없으면 이 단계는 건너뛰고 건너뛰었다고 알려 달라.
8. 창 폭을 1,100px 이하로 줄이면 레일이 사라지는지 본다.

### 4-3. 진짜 claude 한 번 (선택)

토큰을 쓰고 진짜 `~/.claude`를 쓴다. 가짜로 끝까지 확인한 뒤에만 한다. 격리 HOME 대신 평소 HOME으로 앱을 띄우되, 엔진 소켓과 vault는 같은 방식으로 격리하고 `AISURADA_CLAUDE_BIN`은 빼면 된다. 질문 한 번이면 충분하다.

## 5. 기록 방법

각 항목에 통과, 실패, 이상한 점을 적어 알려 주면 된다. 실패한 항목은 화면의 문구나 동작을 그대로 적어 주면 원인을 찾겠다. 이 결과가 모이면 베타 판정과 4부 acceptance 하네스를 지을지 정할 수 있다.
