#!/bin/bash
# Java 엔진의 명령 명세를 ais-cli의 spec_table.rs로 다시 만든다. 먼저 ./build.sh로 dist/ais-engine.jar를 지어 둔다.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
JAR="$ROOT/dist/ais-engine.jar"
[ -f "$JAR" ] || { echo "dist/ais-engine.jar가 없다. 먼저 ./build.sh를 돌린다." >&2; exit 1; }
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
javac -encoding UTF-8 -cp "$JAR" -d "$WORK" "$ROOT/engine-rs/tools/compat/DumpSpecs.java"
java -cp "$JAR:$WORK" DumpSpecs > "$ROOT/engine-rs/crates/ais-cli/src/spec_table.rs"
echo "wrote engine-rs/crates/ais-cli/src/spec_table.rs"
