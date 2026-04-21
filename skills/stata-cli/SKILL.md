---
name: stata-cli
description: |
  Use the local `stata-cli` command for Stata work on this machine. Trigger this skill when the user wants to bootstrap an AI-ready Stata workspace, write or debug `.do` files, inspect `.dta` data, export CSVs, or needs Stata syntax/package guidance while staying inside the local `stata-cli` workflow.
---

# stata-cli

Use `stata-cli` as the default local entrypoint for Stata tasks in Codex.

This skill now includes a local reference library adapted from `dylantmoore/stata-skill`.
Do not load everything. Read only the 1-3 files relevant to the current task.

## Core workflow

For AI-driven work:

```bash
stata-cli init ./my-analysis
stata-cli file do/analysis.do
stata-cli data view --input-dta /absolute/path/to/data.dta --max-rows 20
stata-cli data export-csv --input-dta /absolute/path/to/data.dta --output /absolute/path/to/out.csv --replace
```

For quick human exploration:

```bash
stata-cli repl
```

## Working rules

- Prefer editing `.do` files over passing long Stata snippets to `run --code`.
- Non-REPL commands already return JSON; inspect `status`, `error`, `log_file`, and `graphs`.
- Do not assume cross-command persistent sessions. `stata-cli` is per invocation unless the same REPL stays open.
- Use `data view` only for schema checks and small previews. Keep `--max-rows` at `50` unless the user asks for more.
- Do not dump large tables into chat context.
- Use Stata for data cleaning, modeling, regression, tests, and exports.
- Use Python under `scripts/` for final charts saved into `outputs/`.
- Before using any third-party Stata command, run `which <command>`.
- Do not install third-party Stata packages unless the user explicitly approves it.

## Current commands

```bash
stata-cli doctor
stata-cli init ./my-analysis
stata-cli run --code 'display 1+1'
stata-cli file /absolute/path/to/script.do
stata-cli data view --input-dta /absolute/path/to/data.dta --max-rows 20
stata-cli data export-csv --input-dta /absolute/path/to/data.dta --output /absolute/path/to/out.csv --replace
stata-cli repl
```

## Routing table

Read only the files relevant to the task. Paths are relative to this `SKILL.md`.

### Basics and workflow

- `references/basics-getting-started.md`
- `references/workflow-best-practices.md`
- `references/programming-basics.md`
- `references/advanced-programming.md`

### Data work

- `references/data-management.md`
- `references/data-import-export.md`
- `references/string-functions.md`
- `references/date-time-functions.md`
- `references/variables-operators.md`
- `references/mathematical-functions.md`

### Statistics and econometrics

- `references/descriptive-statistics.md`
- `references/linear-regression.md`
- `references/panel-data.md`
- `references/time-series.md`
- `references/limited-dependent-variables.md`
- `references/bootstrap-simulation.md`
- `references/survey-data-analysis.md`
- `references/missing-data-handling.md`
- `references/maximum-likelihood.md`
- `references/gmm-estimation.md`

### Causal inference and advanced methods

- `references/difference-in-differences.md`
- `references/regression-discontinuity.md`
- `references/matching-methods.md`
- `references/treatment-effects.md`
- `references/sample-selection.md`
- `references/nonparametric-methods.md`
- `references/sem-factor-analysis.md`
- `references/survival-analysis.md`
- `references/spatial-analysis.md`
- `references/machine-learning.md`

### Graphics, tables, and reporting

- `references/graphics.md`
- `references/tables-reporting.md`
- `references/external-tools-integration.md`

### Mata

- `references/mata-introduction.md`
- `references/mata-data-access.md`
- `references/mata-matrix-operations.md`
- `references/mata-programming.md`

### Community packages

- `packages/package-management.md`
- `packages/reghdfe.md`
- `packages/estout.md`
- `packages/outreg2.md`
- `packages/coefplot.md`
- `packages/winsor.md`
- `packages/did.md`
- `packages/event-study.md`
- `packages/rdrobust.md`
- `packages/psmatch2.md`
- `packages/synth.md`
- `packages/ivreg2.md`
- `packages/xtabond2.md`
- `packages/binsreg.md`
- `packages/nprobust.md`
- `packages/tabout.md`
- `packages/asdoc.md`
- `packages/diagnostics.md`
- `packages/data-manipulation.md`
- `packages/graph-schemes.md`

## Practical defaults

- If the task is more than a few lines, initialize a workspace and write `do/analysis.do`.
- If the user already has a project folder, keep outputs inside its `outputs/` directory.
- If charts are needed, prefer exporting tidy results from Stata and plotting with Python.
- If a package is needed but may not be installed, check with `which` before using it.

## Common failure reasons

- `stata-cli` is not on `PATH`
- Python 3.11 is missing or the repo `.venv` is broken
- Stata is not installed or `--stata-path` points to the wrong location
- PyStata is unavailable in the chosen Python environment
- The target `.do` or `.dta` path does not exist

If setup looks wrong, run:

```bash
stata-cli doctor
```
