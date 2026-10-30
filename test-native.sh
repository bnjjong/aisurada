#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")"
bash build.sh
node --test native-host/scripts/contracts.test.mjs
cargo test --manifest-path native-host/src-tauri/Cargo.toml --locked
