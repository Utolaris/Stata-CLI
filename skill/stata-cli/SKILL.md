---
name: stata-cli
description: |
  Use the local `stata-cli` command for Stata work on this machine. Trigger this skill when the user wants to bootstrap an AI-ready Stata workspace, write or debug `.do` files, inspect `.dta` data, export CSVs, or needs Stata syntax/package guidance while staying inside the local `stata-cli` workflow.
---

# stata-cli

Use `stata-cli` as the default local entrypoint for Stata tasks in AI agent.

This skill now includes a local reference library.
Do not load everything. Read only the 1-3 files relevant to the current task.

## Core workflow

For AI-driven work:

```bash
mkdir my-analysis
cd my-analysis
stata-cli init
stata-cli file do/analysis.do
stata-cli data view --input-dta /absolute/path/to/data.dta --max-rows 20
stata-cli data export-csv --input-dta /absolute/path/to/data.dta --output /absolute/path/to/out.csv --replace
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

### Stata 19 features (verified on Stata 19.5)

- `references/panel-var-xtvar.md` — panel VAR (`xtvar`)
- `references/high-dimensional-fixed-effects.md` — HDFE absorption in `areg`, `xtreg, fe`, `ivregress 2sls`
- `references/cate.md` — conditional average treatment effects (`cate`)
- `references/weak-instruments.md` — weak-instrument-robust inference (`estat weakrobust`)
- `references/control-functions.md` — control-function models (`cfregress`, `cfprobit`)
- `references/svar-with-iv.md` — SVAR via instruments (`ivsvar`)
- `references/iv-local-projections.md` — IV local projections (`ivlpirf`)
- `references/correlated-random-effects.md` — CRE model and Mundlak test (`xtreg, cre`, `estat mundlak`)
- `references/gmm-xtinstruments.md` — GMM with panel-style instruments (`gmm ... xtinstruments()`)

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

### Mata

- `references/mata-introduction.md`
- `references/mata-data-access.md`
- `references/mata-matrix-operations.md`
- `references/mata-programming.md`

### Community packages

- `packages/package-management.md`
- `packages/reghdfe.md`
- `packages/estout.md`
- `packages/coefplot.md`
- `packages/winsor.md`
- `packages/event-study.md`
- `packages/rdrobust.md`
- `packages/synth.md`
- `packages/ivreg2.md`
- `packages/xtabond2.md`
- `packages/binsreg.md`
- `packages/nprobust.md`
- `packages/asdoc.md`
- `packages/data-manipulation.md`
- `packages/graph-schemes.md`

## Practical defaults

- If the task is more than a few lines, initialize a workspace and write `do/analysis.do`.
- If the user already has a project folder, keep outputs inside its `outputs/` directory.
- If charts are needed, prefer exporting tidy results from Stata and plotting with Python.
- If a package is needed but may not be installed, check with `which` before using it.

## When failure 
If setup looks wrong, run:

```bash
stata-cli doctor
```
