#!/usr/bin/env bash
set -euo pipefail

# Rust `ais-cli`(release)를 지어 dist/ais-cli에 둔다. Rust 1.85 이상(edition 2024)이 필요하다.
# Java 엔진은 5단계에서 저장소에서 지웠다. 맞대기의 기준이 필요하면 engine-rs/tools/compat/build-oracle.sh를 쓴다.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

cargo build --release --locked --manifest-path engine-rs/Cargo.toml -p ais-cli
mkdir -p dist
cp engine-rs/target/release/ais-cli dist/ais-cli
echo "dist/ais-cli"
