#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$repo_root"

echo "[run_repo_checks] Rust fmt"
cargo fmt --manifest-path rust-cli/Cargo.toml --check

echo "[run_repo_checks] Rust tests"
cargo test --manifest-path rust-cli/Cargo.toml
