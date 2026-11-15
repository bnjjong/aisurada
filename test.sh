#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
# 쓰기 잠금 파일(vault 밖)이 실제 홈에 쌓이지 않게 시험 동안 폴더를 따로 쓴다. 자식 프로세스도 환경 변수로 물려받는다.
export AISURADA_LOCK_DIR="$(mktemp -d)"
trap 'rm -rf "$AISURADA_LOCK_DIR"' EXIT
# 엔진(ais-core)과 명령줄(ais-cli) 시험 전체. Java와 맞대는 시험은 기준(oracle)이 필요해서 따로 돈다(docs/developer-guide.md).
# 단, 어긋남 0이던 때의 기준 응답을 녹화해 둔 골든 재생(engine-rs/tools/golden)은 Java 없이 이 안에서 돈다(약 1분).
cargo test --manifest-path engine-rs/Cargo.toml --workspace --locked
