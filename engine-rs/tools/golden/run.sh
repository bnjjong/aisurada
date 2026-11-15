#!/usr/bin/env bash
# 골든 묶음(녹화한 기준 응답과 폴더 해시)을 만들거나 기준 엔진(Java) 없이 다시 돌린다.
#
# 사용:
#   engine-rs/tools/golden/run.sh record   # 기준 Java 엔진(JDK 21 필요)과 맞대 어긋남 0일 때만 golden/*.ndjson을 새로 쓴다
#   engine-rs/tools/golden/run.sh replay   # Java 없이 같은 하네스 셋을 녹화본과 견준다(어긋남 0이어야 한다)
#
# 하네스 셋: compat_check(엔진 읽기·쓰기·커밋), cli_compat(명령줄), mcp_compat(MCP 대화). 셋은 서로 독립이라 함께 돌린다.
# 합성 vault는 매번 임시 폴더에 새로 만든다(진짜 vault 내용은 하나도 없다). 시계와 난수는 하네스가 고정하므로 결과가 결정적이다.
set -euo pipefail

MODE="${1:?record 또는 replay를 주세요}"
HERE="$(cd "$(dirname "$0")" && pwd)"
ENGINE="$(cd "$HERE/../.." && pwd)"
GOLDEN="${AISURADA_GOLDEN_DIR:-$ENGINE/golden}"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
VAULT="$WORK/vault"
bash "$ENGINE/tools/compat/make-fixture.sh" "$VAULT" > /dev/null
bash "$ENGINE/tools/compat/add-search-notes.sh" "$VAULT" > /dev/null
bash "$ENGINE/tools/compat/add-git-history.sh" "$VAULT" > /dev/null

# 쓰기 잠금 파일이 실제 홈에 쌓이지 않게 시험 동안 폴더를 따로 쓴다
export AISURADA_LOCK_DIR="$WORK/locks"
cd "$ENGINE"
# 하네스가 명령 수천 건을 돌리므로 최적화한 golden 프로필(Cargo.toml)로 짓는다
cargo build -q --profile golden --workspace --examples

BIN="${CARGO_TARGET_DIR:-$ENGINE/target}/golden/examples"
case "$MODE" in
record)
    ORACLE="$(bash "$ENGINE/tools/compat/build-oracle.sh")"
    mkdir -p "$GOLDEN"
    "$BIN/compat_check" "$ORACLE/ais-engine.jar" "$VAULT" --write-fixture="$VAULT" --commit-fixture="$VAULT" \
        --record="$GOLDEN/compat.ndjson" > "$WORK/compat.log" 2>&1 &
    P1=$!
    "$BIN/cli_compat" "$ORACLE/ais-engine.jar" "$VAULT" --history --record="$GOLDEN/cli.ndjson" > "$WORK/cli.log" 2>&1 &
    P2=$!
    "$BIN/mcp_compat" "$ORACLE" --record="$GOLDEN/mcp.ndjson" > "$WORK/mcp.log" 2>&1 &
    P3=$!
    ;;
replay)
    "$BIN/compat_check" --replay="$GOLDEN/compat.ndjson" "$VAULT" --write-fixture="$VAULT" --commit-fixture="$VAULT" \
        > "$WORK/compat.log" 2>&1 &
    P1=$!
    "$BIN/cli_compat" --replay="$GOLDEN/cli.ndjson" "$VAULT" --history > "$WORK/cli.log" 2>&1 &
    P2=$!
    "$BIN/mcp_compat" --replay="$GOLDEN/mcp.ndjson" > "$WORK/mcp.log" 2>&1 &
    P3=$!
    ;;
*)
    echo "모르는 방식: $MODE (record 또는 replay)" >&2
    exit 2
    ;;
esac

status=0
wait "$P1" || status=1
wait "$P2" || status=1
wait "$P3" || status=1
for name in compat cli mcp; do
    echo "== $name"
    tail -n 12 "$WORK/$name.log"
done
exit "$status"
