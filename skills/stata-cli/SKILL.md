---
name: stata-cli
description: |
  Use the local `stata-cli` command for Stata work on this machine. Trigger this skill when the user wants to run Stata code, execute a `.do` file, preview data from a loaded dataset or a `.dta` file, export a `.dta` or current dataset to CSV, or diagnose the local Stata CLI environment.
---

# stata-cli

Use `stata-cli` as the default local entrypoint for Stata tasks in Codex.

## When to use

- The user wants to run Stata code directly from Codex.
- The user wants to execute a local `.do` file.
- The user wants to inspect a dataset or preview rows from a `.dta` file.
- The user wants to export Stata data to CSV.
- The user wants to check whether the local Stata CLI environment is healthy.

## Core commands

```bash
stata-cli doctor
stata-cli run --code 'display 1+1'
stata-cli file /absolute/path/to/script.do
stata-cli --json data view --input-dta /absolute/path/to/data.dta --max-rows 20
stata-cli data export-csv --input-dta /absolute/path/to/data.dta --output /absolute/path/to/out.csv --replace
```

## Current capabilities

- Run inline Stata code with `run`
- Execute `.do` files with `file`
- Start a minimal interactive shell with `repl`
- Diagnose repo root, Python, and backend health with `doctor`
- Preview dataset rows with `data view`
- Convert a `.dta` file or current dataset to CSV with `data export-csv`

## Limits

- Do not assume cross-command persistent sessions. `stata-cli` is per-invocation.
- Prefer absolute paths for `.do`, `.dta`, and CSV output paths.
- Use `--json` when structured output is easier to consume.

## Common failure reasons

- `stata-cli` is not installed or not on `PATH`
- Python 3.11 is missing or the project `.venv` is broken
- Stata is not installed, or `--stata-path` points to the wrong location
- PyStata / local Stata Python bridge is unavailable
- The target `.do` or `.dta` path does not exist

If setup looks wrong, run:

```bash
stata-cli doctor
```
