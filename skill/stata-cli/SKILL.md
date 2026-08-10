---
name: stata-cli
description: Local Stata CLI (native Rust engine, no Python) for running Stata code and .do files, bootstrapping AI-ready Stata workspaces, inspecting and exporting .dta data, rendering offline Stata help text, and diagnosing the local Stata engine. Use when the user wants to run or debug Stata, initialize a Stata analysis workspace, preview or export dataset contents, or get Stata command help while staying in the local stata-cli workflow.
---

# stata-cli

Drive the locally installed Stata through the `stata-cli` binary. The binary
loads Stata's own engine in-process, so no Python, pystata, or virtual
environment is needed.

## Binary

The binary lives at `bin/stata-cli` relative to this file's folder (for
example `~/.codex/skills/stata-cli/bin/stata-cli`). Call it by absolute path or
add its folder to `PATH` once.

## Core workflow

1. Bootstrap a workspace:

   ```bash
   mkdir my-analysis && cd my-analysis
   stata-cli init
   ```

   `init` copies the `boilerplate/` templates (AGENTS.md, do/, scripts/, and
   the local `skills/stata-cli` reference library) into the current directory.
   The templates are resolved next to the binary, so no repository clone is
   required.

2. Run code or files:

   ```bash
   stata-cli run --code 'display 2+2'
   stata-cli file do/analysis.do
   ```

3. Inspect and export data:

   ```bash
   stata-cli data view --input-dta /abs/path/data.dta --max-rows 20
   stata-cli data export-csv --input-dta /abs/path/data.dta --output out.csv --replace
   ```

4. Diagnose the setup:

   ```bash
   stata-cli doctor
   ```

Non-REPL commands return structured JSON (`status`, `output`, `error`,
`log_file`, ...) so agents can inspect results reliably.

## Interactive REPL

`stata-cli repl` keeps one Stata session alive with a Stata-style prompt.
`help <topic>` renders the real local Stata help text into the terminal;
type `quit` (or the legacy `:exit`) to quit.

## Reference library

Read only the 1-3 files relevant to the current task:

- `boilerplate/skills/stata-cli/references/<topic>.md` - method guidance (for
  example linear-regression, panel-data, time-series)
- `boilerplate/skills/stata-cli/packages/<package>.md` - user-written package
  guidance (estout, reghdfe, coefplot, ...)
- `boilerplate/skills/stata-cli/SKILL.md` - full routing table

## Templates and maintenance

- `boilerplate/` is the init template source; edit it to customize new
  workspaces. Keep `bin/stata-cli` and `boilerplate/` in the same skill folder.
- Replace `bin/stata-cli` by building from the source repo with
  `cargo build --release --manifest-path ../../rust-cli/Cargo.toml` (macOS) or
  `bash ../../scripts/build_windows_bin.sh` (Windows).

