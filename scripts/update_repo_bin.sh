#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

usage() {
  cat <<'EOF'
Usage:
  ./scripts/update_repo_bin.sh [macos|windows|all]

Default behavior:
  - macOS/Linux hosts: build the local macOS binary into bin/
  - Windows-like hosts: build the local Windows binary into bin/

Explicit targets:
  macos    Build the repo-local macOS binary with cargo build --release
  windows  Build the repo-local Windows binary with cargo zigbuild
  all      Build both repo-local binaries
EOF
}

target="${1:-}"

case "$target" in
  "" )
    if [[ "$OSTYPE" == msys* || "$OSTYPE" == cygwin* || "$OSTYPE" == win32* ]]; then
      exec bash "$repo_root/scripts/build_windows_bin.sh"
    fi
    exec bash "$repo_root/scripts/build_macos_bin.sh"
    ;;
  macos )
    exec bash "$repo_root/scripts/build_macos_bin.sh"
    ;;
  windows )
    exec bash "$repo_root/scripts/build_windows_bin.sh"
    ;;
  all )
    bash "$repo_root/scripts/build_macos_bin.sh"
    bash "$repo_root/scripts/build_windows_bin.sh"
    ;;
  -h|--help|help )
    usage
    ;;
  * )
    usage >&2
    exit 1
    ;;
esac
