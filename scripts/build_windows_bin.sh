#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
manifest_path="$repo_root/rust-cli/Cargo.toml"
bin_dir="$repo_root/bin"
target_triple="${WINDOWS_TARGET_TRIPLE:-x86_64-pc-windows-gnu}"
target_binary="$repo_root/rust-cli/target/$target_triple/release/stata-cli.exe"
cargo_bin="${CARGO_BIN:-}"
rustup_bin="${RUSTUP_BIN:-}"

mkdir -p "$bin_dir"

if [[ -z "$cargo_bin" ]]; then
  if [[ -x "$HOME/.cargo/bin/cargo" ]]; then
    cargo_bin="$HOME/.cargo/bin/cargo"
  else
    cargo_bin="$(command -v cargo)"
  fi
fi

if [[ -z "$rustup_bin" ]]; then
  if [[ -x "$HOME/.cargo/bin/rustup" ]]; then
    rustup_bin="$HOME/.cargo/bin/rustup"
  else
    rustup_bin="$(command -v rustup)"
  fi
fi

if [[ -d "$HOME/.cargo/bin" ]]; then
  export PATH="$HOME/.cargo/bin:$PATH"
fi

if ! "$cargo_bin" zigbuild --help >/dev/null 2>&1; then
  echo "[build_windows_bin] cargo-zigbuild is required." >&2
  echo "[build_windows_bin] Install it with: cargo install cargo-zigbuild" >&2
  exit 1
fi

if ! "$rustup_bin" target list --installed | grep -qx "$target_triple"; then
  echo "[build_windows_bin] Installing Rust target $target_triple..."
  "$rustup_bin" target add "$target_triple"
fi

echo "[build_windows_bin] Building Rust CLI for Windows target $target_triple..."
"$cargo_bin" zigbuild --release --target "$target_triple" --manifest-path "$manifest_path"

if [[ ! -f "$target_binary" ]]; then
  echo "[build_windows_bin] Expected binary not found: $target_binary" >&2
  exit 1
fi

cp "$target_binary" "$bin_dir/stata-cli.exe"
echo "[build_windows_bin] Updated $bin_dir/stata-cli.exe"
