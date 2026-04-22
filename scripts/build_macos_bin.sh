#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest_path="$repo_root/rust-cli/Cargo.toml"
bin_dir="$repo_root/bin"
target_binary="$repo_root/rust-cli/target/release/stata-cli"

mkdir -p "$bin_dir"

echo "[build_macos_bin] Building Rust CLI for macOS..."
cargo build --release --manifest-path "$manifest_path"

if [[ ! -f "$target_binary" ]]; then
  echo "[build_macos_bin] Expected binary not found: $target_binary" >&2
  exit 1
fi

cp "$target_binary" "$bin_dir/stata-cli"
chmod +x "$bin_dir/stata-cli"
echo "[build_macos_bin] Updated $bin_dir/stata-cli"
