#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$repo_root"

echo "[run_repo_checks] Ruff"
uv run ruff check .

echo "[run_repo_checks] Mypy"
uv run mypy src

echo "[run_repo_checks] Rust fmt"
cargo fmt --manifest-path rust-cli/Cargo.toml --check

echo "[run_repo_checks] Rust tests"
cargo test --manifest-path rust-cli/Cargo.toml

echo "[run_repo_checks] Python tests"
uv run pytest -q \
  tests/test_cli_backend.py \
  tests/test_platform_paths.py \
  tests/test_compact_filter.py \
  tests/test_session_manager.py \
  tests/test_stop_execution.py \
  tests/test_timeout_direct.py
