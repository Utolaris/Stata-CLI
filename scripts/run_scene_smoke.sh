#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cli_bin="$repo_root/skill/stata-cli/bin/stata-cli"

if [[ -x "$cli_bin" ]]; then
  stata_cli=("$cli_bin")
else
  stata_cli=(cargo run --manifest-path "$repo_root/rust-cli/Cargo.toml" --)
fi

cd "$repo_root/scene"

echo "[run_scene_smoke] doctor"
"${stata_cli[@]}" doctor

echo "[run_scene_smoke] file smoke_test.do"
"${stata_cli[@]}" file smoke_test.do --working-dir .

echo "[run_scene_smoke] data view grilic.dta"
"${stata_cli[@]}" data view --input-dta grilic.dta --max-rows 20

echo "[run_scene_smoke] data export-csv grilic.dta -> outputs/result.csv"
"${stata_cli[@]}" data export-csv \
  --input-dta "$(pwd)/grilic.dta" \
  --working-dir outputs \
  --output result.csv \
  --replace

if [[ ! -f outputs/result.csv ]]; then
  echo "[run_scene_smoke] expected outputs/result.csv to exist" >&2
  exit 1
fi
