#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
bin_dir="$repo_root/bin"

mkdir -p "$bin_dir"

if [[ "$OSTYPE" == msys* || "$OSTYPE" == cygwin* || "$OSTYPE" == win32* ]]; then
  cargo build --release --manifest-path "$repo_root/rust-cli/Cargo.toml"
  cp "$repo_root/rust-cli/target/release/stata-cli.exe" "$bin_dir/stata-cli.exe"
else
  cargo build --release --manifest-path "$repo_root/rust-cli/Cargo.toml"
  cp "$repo_root/rust-cli/target/release/stata-cli" "$bin_dir/stata-cli"
fi
