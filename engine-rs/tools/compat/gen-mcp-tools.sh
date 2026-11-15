#!/bin/bash
# 옛 Java MCP 서버(기준, 태그 java-engine-final)의 도구 정의를 ais-cli의 mcp_tools.rs로 다시 만든다. 기준은 build-oracle.sh가 지어 준다.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
ORACLE="$("$ROOT/engine-rs/tools/compat/build-oracle.sh" | tail -1)"
CLI_JAR="$ORACLE/ais-cli.jar"
ENGINE_JAR="$ORACLE/ais-engine.jar"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
CP="$CLI_JAR:$ENGINE_JAR"
javac -encoding UTF-8 -cp "$CP" -d "$WORK" "$ROOT/engine-rs/tools/compat/DumpMcpTools.java"
java -cp "$CP:$WORK" aisurada.cli.DumpMcpTools > "$ROOT/engine-rs/crates/ais-cli/src/mcp_tools.rs"
echo "wrote engine-rs/crates/ais-cli/src/mcp_tools.rs"
