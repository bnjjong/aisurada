#!/bin/bash
# Java MCP 서버의 도구 정의를 ais-cli의 mcp_tools.rs로 다시 만든다. 먼저 ./build.sh로 dist/ais-cli.jar를 지어 둔다.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
CLI_JAR="$ROOT/dist/ais-cli.jar"
ENGINE_JAR="$ROOT/dist/ais-engine.jar"
[ -f "$CLI_JAR" ] && [ -f "$ENGINE_JAR" ] || { echo "dist/ais-cli.jar와 dist/ais-engine.jar가 없다. 먼저 ./build.sh를 돌린다." >&2; exit 1; }
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
CP="$CLI_JAR:$ENGINE_JAR"
javac -encoding UTF-8 -cp "$CP" -d "$WORK" "$ROOT/engine-rs/tools/compat/DumpMcpTools.java"
java -cp "$CP:$WORK" aisurada.cli.DumpMcpTools > "$ROOT/engine-rs/crates/ais-cli/src/mcp_tools.rs"
echo "wrote engine-rs/crates/ais-cli/src/mcp_tools.rs"
