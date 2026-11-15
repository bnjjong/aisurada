#!/bin/bash
# 옛 Java MCP 서버(기준, 태그 java-engine-final)의 도구 정의 표를 뽑는다. 기준은 build-oracle.sh가 지어 준다.
# Java 엔진을 지운 뒤로 crates/ais-cli/src/mcp_tools.rs가 원본이라 이 스크립트는 그것을 덮어쓰지 않는다.
# 기본은 engine-rs/target/oracle-tables/mcp_tools.rs에 뽑으니, 현재 표와 diff로 견준다. 출력 자리는 첫 인자로 바꾼다.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
OUT="${1:-$ROOT/engine-rs/target/oracle-tables/mcp_tools.rs}"
mkdir -p "$(dirname "$OUT")"
ORACLE="$("$ROOT/engine-rs/tools/compat/build-oracle.sh" | tail -1)"
CLI_JAR="$ORACLE/ais-cli.jar"
ENGINE_JAR="$ORACLE/ais-engine.jar"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
CP="$CLI_JAR:$ENGINE_JAR"
javac -encoding UTF-8 -cp "$CP" -d "$WORK" "$ROOT/engine-rs/tools/compat/DumpMcpTools.java"
java -cp "$CP:$WORK" aisurada.cli.DumpMcpTools > "$OUT"
echo "$OUT"
