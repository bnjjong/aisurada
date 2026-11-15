#!/bin/bash
# 옛 Java 엔진(기준, 태그 java-engine-final)의 명령 명세 표를 뽑는다. 기준은 build-oracle.sh가 지어 준다.
# Java 엔진을 지운 뒤로 crates/ais-cli/src/spec_table.rs가 원본이라 이 스크립트는 그것을 덮어쓰지 않는다.
# 기본은 engine-rs/target/oracle-tables/spec_table.rs에 뽑으니, 현재 표와 diff로 견준다. 출력 자리는 첫 인자로 바꾼다.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
OUT="${1:-$ROOT/engine-rs/target/oracle-tables/spec_table.rs}"
mkdir -p "$(dirname "$OUT")"
ORACLE="$("$ROOT/engine-rs/tools/compat/build-oracle.sh" | tail -1)"
JAR="$ORACLE/ais-engine.jar"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
javac -encoding UTF-8 -cp "$JAR" -d "$WORK" "$ROOT/engine-rs/tools/compat/DumpSpecs.java"
java -cp "$JAR:$WORK" DumpSpecs > "$OUT"
echo "$OUT"
