#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$repo_root/scene"

echo "[run_scene_smoke] doctor"
cargo run --manifest-path ../rust-cli/Cargo.toml -- doctor

echo "[run_scene_smoke] file smoke_test.do"
cargo run --manifest-path ../rust-cli/Cargo.toml -- file smoke_test.do --working-dir .

echo "[run_scene_smoke] data view grilic.dta"
cargo run --manifest-path ../rust-cli/Cargo.toml -- data view --input-dta grilic.dta --max-rows 20
