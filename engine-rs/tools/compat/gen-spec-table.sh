#!/bin/bash
# 옛 Java 엔진(기준, 태그 java-engine-final)의 명령 명세를 ais-cli의 spec_table.rs로 다시 만든다. 기준은 build-oracle.sh가 지어 준다.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
ORACLE="$("$ROOT/engine-rs/tools/compat/build-oracle.sh" | tail -1)"
JAR="$ORACLE/ais-engine.jar"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
javac -encoding UTF-8 -cp "$JAR" -d "$WORK" "$ROOT/engine-rs/tools/compat/DumpSpecs.java"
java -cp "$JAR:$WORK" DumpSpecs > "$ROOT/engine-rs/crates/ais-cli/src/spec_table.rs"
echo "wrote engine-rs/crates/ais-cli/src/spec_table.rs"
