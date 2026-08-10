#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

codex_home="${CODEX_HOME:-$HOME/.codex}"
codex_dest="$codex_home/skills/stata-cli"
install_claude=0

usage() {
  cat <<'EOF'
Usage: ./scripts/install_skill.sh [--claude]

Installs the self-contained skill folder (skill/stata-cli: SKILL.md, bin/,
boilerplate/) into ~/.codex/skills/stata-cli by default. With --claude, also
installs into ~/.claude/skills/stata-cli. Existing skill folders are backed up
under a `.backup/` directory before being replaced.
EOF
}

backup_and_install() {
  local dest="$1"
  if [[ -d "$dest" ]]; then
    local backup_dir="$(dirname "$dest")/.backup"
    mkdir -p "$backup_dir"
    local backup="$backup_dir/$(basename "$dest")-$(date +%Y%m%d%H%M%S)"
    mv "$dest" "$backup"
    echo "[install_skill] Backed up existing skill to $backup"
  fi
  mkdir -p "$dest"
  cp -R "$repo_root/skill/stata-cli/." "$dest/"
  echo "[install_skill] Installed skill package to $dest"
}

for arg in "$@"; do
  case "$arg" in
    --claude) install_claude=1 ;;
    -h|--help) usage; exit 0 ;;
    *) echo "Unknown argument: $arg" >&2; usage >&2; exit 1 ;;
  esac
done

backup_and_install "$codex_dest"
if [[ "$install_claude" == 1 ]]; then
  claude_dest="${CLAUDE_SKILL_DIR:-$HOME/.claude/skills/stata-cli}"
  backup_and_install "$claude_dest"
fi

validator="$codex_home/skills/.system/skill-creator/scripts/quick_validate.py"
if [[ ! -f "$validator" ]]; then
  echo "[install_skill] Warning: skill-creator validator not found at $validator; skipped validation" >&2
  exit 0
fi

if python3 "$validator" "$codex_dest" >/dev/null 2>&1; then
  echo "[install_skill] Skill validation passed"
elif command -v uv >/dev/null 2>&1 \
  && (cd "$codex_dest" && uv run --with pyyaml python "$validator" "$codex_dest" >/dev/null 2>&1); then
  echo "[install_skill] Skill validation passed (uv fallback)"
else
  echo "[install_skill] Warning: skill validation could not run (PyYAML missing)" >&2
fi
