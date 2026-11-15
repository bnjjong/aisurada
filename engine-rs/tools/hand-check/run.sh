#!/usr/bin/env bash
# 앱을 격리 환경에서 띄워 손으로 확인한다. 진짜 HOME, launchd, 진짜 vault를 건드리지 않는다.
# 점검표는 docs/rust-engine-hand-check.md에 있다.
#
# 사용: engine-rs/tools/hand-check/run.sh [--git]
#   --git          합성 vault에 git 저장소와 노트 판 이력을 만든다(편집 이력 창, 자동 커밋 확인용).
# 환경 변수 HC_DIR로 폴더를 바꿀 수 있다(기본 ~/ais-rust-check).
#
# 앞에 할 것(저장소 루트에서). **식별자를 바꿔 빌드해야 한다.** 번들 식별자가 설치된 진짜 앱과 같으면 가짜 HOME으로 띄워도
# 웹뷰 저장소(마지막으로 연 작업 공간)가 진짜 앱과 공유되어 진짜 vault를 열고, 진짜 vault에 쓸 수 있다.
#   (cd engine-rs && cargo build -p ais-cli) && pnpm --dir native-host tauri build --debug --bundles app --config '{"identifier":"co.kcd.aisurada.handcheck"}'
# 식별자를 바꾼 앱은 처음에 아무 작업 공간도 열지 않는다. 앱의 「폴더 열기」에 이 스크립트가 알려 주는 vault 경로를 넣는다.
# 앱을 띄운 터미널에서 Ctrl-C를 누르면 앱이 내려간다. 확인한 폴더는 지우지 않고 남겨 둔다.
set -euo pipefail

GIT=0
while [[ $# -gt 0 ]]; do
    case "$1" in
        --git) GIT=1; shift ;;
        *) echo "모르는 인자: $1" >&2; exit 2 ;;
    esac
done

REPO="$(cd "$(dirname "$0")/../../.." && pwd)"
HC_DIR="${HC_DIR:-$HOME/ais-rust-check}"
APP="$REPO/native-host/src-tauri/target/debug/bundle/macos/aisurada.app/Contents/MacOS/aisurada-host"
[[ -x "$APP" ]] || { echo "앱을 먼저 짓는다: pnpm --dir native-host tauri build --debug --bundles app" >&2; exit 1; }
BUNDLE_ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$REPO/native-host/src-tauri/target/debug/bundle/macos/aisurada.app/Contents/Info.plist" 2>/dev/null || true)"
if [[ -z "$BUNDLE_ID" || "$BUNDLE_ID" == "co.kcd.aisurada" ]]; then
    echo "이 앱의 번들 식별자가 진짜 앱과 같거나 읽을 수 없다($BUNDLE_ID). 진짜 vault를 열 수 있어서 띄우지 않는다." >&2
    echo "식별자를 바꿔 다시 짓는다: pnpm --dir native-host tauri build --debug --bundles app --config '{\"identifier\":\"co.kcd.aisurada.handcheck\"}'" >&2
    exit 1
fi
CLI="$REPO/engine-rs/target/debug/ais-cli"
[[ -x "$CLI" ]] || { echo "ais-cli를 먼저 짓는다: (cd engine-rs && cargo build -p ais-cli)" >&2; exit 1; }
if [[ -e "$HC_DIR" && ! -f "$HC_DIR/.rust-hand-check" ]]; then
    echo "이 스크립트가 만든 폴더가 아니라서 지우지 않는다: $HC_DIR" >&2
    exit 1
fi
/bin/rm -rf "$HC_DIR"
mkdir -p "$HC_DIR/home/.claude/projects"
touch "$HC_DIR/.rust-hand-check"
printf '#!/bin/sh\ncat >/dev/null\necho "가짜 claude의 답이에요."\n' > "$HC_DIR/fake-claude"; chmod 755 "$HC_DIR/fake-claude"

# 합성 vault(진짜 vault 내용 없음)
bash "$REPO/engine-rs/tools/compat/make-fixture.sh" "$HC_DIR/vault" > /dev/null
bash "$REPO/engine-rs/tools/compat/add-search-notes.sh" "$HC_DIR/vault" > /dev/null
HOME="$HC_DIR/home" AISURADA_VAULT="$HC_DIR/vault" AISURADA_LOCK_DIR="$HC_DIR/locks" "$CLI" \
    init-workspace name=Rust확인 'templates=[]' > /dev/null
if (( GIT )); then
    # add-git-history.sh는 워크스페이스 표식 파일까지 첫 커밋에 담는다
    bash "$REPO/engine-rs/tools/compat/add-git-history.sh" "$HC_DIR/vault" > /dev/null
fi

export AISURADA_VAULT="$HC_DIR/vault" AISURADA_LOCK_DIR="$HC_DIR/locks"
ENV=(HOME="$HC_DIR/home" AISURADA_CLI_BIN="$CLI" AISURADA_CLAUDE_BIN="$HC_DIR/fake-claude" AISURADA_ENGINE_TRACE=1)

echo "번들 식별자: $BUNDLE_ID"
echo "vault: $HC_DIR/vault   <- 앱의 「폴더 열기」에 이 경로를 넣는다(처음에는 아무 작업 공간도 열려 있지 않다)"
echo "앱을 띄운다. 터미널에 [engine] 줄이 찍히면 그 명령이 호출 객체 길(engine_call)로 간 것이다. 끝내려면 Ctrl-C."
env "${ENV[@]}" "$APP"
