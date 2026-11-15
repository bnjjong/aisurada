#!/usr/bin/env bash
# 맞대기의 기준(oracle)인 옛 Java 엔진을 태그 java-engine-final에서 지어 engine-rs/target/oracle/에 둔다.
#
# 5단계에서 Java 엔진은 저장소에서 지워졌지만, Rust 엔진이 같은 입력에 같은 결과를 내는지 견주는 도구(compat_check, cli_compat, mcp_compat,
# lock_interop, 명세 표 생성기)는 이 기준이 필요하다. 이 스크립트가 태그의 `build.sh`와 `src/`만 꺼내 임시 폴더에서 짓고 jar 4개를 옮긴다.
# JDK 21 이상이 필요하다(JAVA_HOME 또는 PATH의 javac, jar). 이미 지어 둔 것이 같은 태그 커밋이면 다시 짓지 않는다.
#
# 사용: engine-rs/tools/compat/build-oracle.sh        # 끝에 jar가 든 폴더를 한 줄로 찍는다
# 환경변수: AISURADA_ORACLE_TAG(기본 java-engine-final), AISURADA_ORACLE_DIR(기본 engine-rs/target/oracle),
#           AISURADA_SNAKEYAML_JAR(이미 받은 snakeyaml-2.4.jar. 없으면 build.sh가 Maven Central에서 받는다)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
TAG="${AISURADA_ORACLE_TAG:-java-engine-final}"
OUT="${AISURADA_ORACLE_DIR:-$ROOT/engine-rs/target/oracle}"

if ! COMMIT="$(git -C "$ROOT" rev-parse --verify --quiet "$TAG^{commit}")"; then
    echo "태그 $TAG가 없다. 먼저 받는다: git fetch origin tag $TAG" >&2
    exit 1
fi
if [[ -f "$OUT/.built-from" && "$(cat "$OUT/.built-from")" == "$COMMIT" && -f "$OUT/ais-engine.jar" ]]; then
    echo "$OUT"
    exit 0
fi

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
git -C "$ROOT" archive "$TAG" build.sh src | tar -x -C "$WORK"
if [[ -n "${AISURADA_SNAKEYAML_JAR:-}" ]]; then
    mkdir -p "$WORK/lib" && cp "$AISURADA_SNAKEYAML_JAR" "$WORK/lib/snakeyaml-2.4.jar"
elif [[ -f "$ROOT/lib/snakeyaml-2.4.jar" ]]; then
    mkdir -p "$WORK/lib" && cp "$ROOT/lib/snakeyaml-2.4.jar" "$WORK/lib/snakeyaml-2.4.jar"
fi
(cd "$WORK" && bash build.sh > "$WORK/build.log" 2>&1) || { cat "$WORK/build.log" >&2; echo "기준 엔진을 못 지었다" >&2; exit 1; }

rm -rf "$OUT"
mkdir -p "$OUT"
cp "$WORK"/dist/*.jar "$OUT/"
echo "$COMMIT" > "$OUT/.built-from"
echo "$OUT"
