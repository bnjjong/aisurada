#!/usr/bin/env bash
# 지운 Java 시험을 진짜 Rust `ais-cli`에 겨눠 돌리고 어긋남을 모은다(이슈 #81). 설계와 읽는 법은 docs/rust-engine-java-test-rerun.md.
#
# 사용: engine-rs/tools/compat/java-on-rust/run.sh [시험 이름...]      (이름이 없으면 전부)
# 환경: AIS_CLI(Rust 실행 파일, 기본 engine-rs/target/debug/ais-cli), AISURADA_ORACLE_DIR(기준 jar 폴더, 없으면 build-oracle.sh로 짓는다)
# 필요: JDK 21, git(태그 java-engine-final), python3
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
ENGINE="$(cd "$HERE/../../.." && pwd)"
AIS_CLI="${AIS_CLI:-$ENGINE/target/debug/ais-cli}"
ORACLE="${AISURADA_ORACLE_DIR:-$(mktemp -d)}"
[[ -x "$AIS_CLI" ]] || { echo "Rust ais-cli가 없다: $AIS_CLI (cd engine-rs && cargo build -p ais-cli)" >&2; exit 1; }
[[ -f "$ORACLE/ais-engine.jar" ]] || AISURADA_ORACLE_DIR="$ORACLE" bash "$ENGINE/tools/compat/build-oracle.sh"
JAR="$ORACLE/ais-engine.jar"

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
(cd "$ENGINE/.." && python3 "$HERE/prepare.py" "$WORK")
mkdir -p "$WORK/classes" "$WORK/run/dist"
ln -s "$JAR" "$WORK/run/dist/ais-engine.jar"        # 시험 몇 개가 상대 경로로 기준 jar를 찾는다(serve를 띄우는 마지막 단계)
ln -s "$ORACLE/ais-cli.jar" "$WORK/run/dist/ais-cli.jar" 2>/dev/null || true
javac -encoding UTF-8 -cp "$JAR" -d "$WORK/classes" \
  "$HERE/shim/aisurada/engine/EngineExecutor.java" "$HERE/shim/aisurada/cli/McpServer.java" "$HERE/shim/aisurada/Failures.java" \
  "$WORK"/src/aisurada/*.java "$WORK"/src/aisurada/cli/*.java 2>&1 | grep -v '^Note' || true

ALL=(ApprovalsTest ResourcesTest TasksTest ProjectReposTest AgentSessionTest SessionCommandsTest CommentCommandsTest AgendaFactsTest Chapter24Test Chapter31Test cli.McpServerRust cli.McpProtocolRust)
TESTS=("$@"); [[ ${#TESTS[@]} -gt 0 ]] || TESTS=("${ALL[@]}")
for t in "${TESTS[@]}"; do
  out="$WORK/$t.out"
  (cd "$WORK/run" && timeout 600 java -Dshim.cli="$AIS_CLI" -cp "$WORK/classes:$JAR" "aisurada.$t" >"$out" 2>&1) || true
  fails=$(grep -c '^FAIL' "$out" || true)
  printf '%-24s FAIL %-3s %s\n' "$t" "$fails" "$({ grep -E 'checks passed|OK \(|통과|all checks' "$out" | tail -1; } || true)"
  { grep '^FAIL' "$out" | sed -E 's/ \| .*//' | sort | uniq -c | sort -rn | sed 's/^/    /'; } || true
  { grep -E '^Exception' "$out" | head -2 | sed 's/^/    ! /'; } || true
done
