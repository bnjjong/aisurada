#!/usr/bin/env bash
# Java 엔진의 프론트매터 읽기·쓰기 결과(기준값)를 뽑고, Rust 구현이 같은 결과를 내는지 맞댄다.
#
# 사용: engine-rs/tools/yaml-spike/run.sh <출력 폴더> [--vault 폴더]... [--cases 파일]...
#
# - 출력 폴더에는 기준값(ground-truth.jsonl)이 생긴다. vault의 노트 제목과 본문 머리가 들어가므로 저장소 밖에 둔다.
# - --vault 폴더는 읽기만 한다. 아무것도 고치지 않는다.
# - --cases 파일은 같은 형식(`#### 이름` 구획)의 사례를 더한다. 퍼징 사례를 넣을 때 쓴다.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
TOOLS="$ROOT/engine-rs/tools/yaml-spike"
OUT="${1:?출력 폴더를 주세요}"
shift || true
mkdir -p "$OUT"

# Java 엔진 클래스가 없으면 짓는다
if [[ ! -d "$ROOT/build/classes" ]]; then
    "$ROOT/build.sh" > /dev/null
fi

# 중첩 깊이 경계(엔진은 깊이 50 안팎에서 선다)를 재는 사례를 만든다
nest_map() {
    local n=$1 i
    for ((i = 0; i < n; i++)); do printf '%*sk:\n' $((i * 2)) ''; done
    printf '%*sv: 1\n' $((n * 2)) ''
}
nest_flow() {
    local n=$1 i
    printf 'a: '
    for ((i = 0; i < n; i++)); do printf '['; done
    printf 'x'
    for ((i = 0; i < n; i++)); do printf ']'; done
    printf '\n'
}
{
    for n in 20 45 48 49 50 51 52 55; do
        echo "#### depth-map-$n"
        nest_map "$n"
        echo "#### depth-flow-$n"
        nest_flow "$n"
    done
} > "$OUT/depth-cases.txt"

args=(--cases "$TOOLS/cases.txt" --cases "$OUT/depth-cases.txt" "$@")

java -Dfile.encoding=UTF-8 -cp "$ROOT/build/classes:$ROOT/lib/snakeyaml-2.4.jar" \
    "$TOOLS/YamlGroundTruth.java" "${args[@]}" > "$OUT/ground-truth.jsonl"

echo "기준값 $(wc -l < "$OUT/ground-truth.jsonl" | tr -d ' ')건을 $OUT/ground-truth.jsonl 에 썼다"
