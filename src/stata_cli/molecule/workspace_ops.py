#!/usr/bin/env python3
"""Workspace scaffolding for AI-oriented Stata projects."""

from __future__ import annotations

from pathlib import Path

INIT_DIRS = ["data", "do", "outputs", "scripts"]

INIT_FILES = {
    "AGENTS.md": """# AGENTS.md

- Prefer writing `.do` files instead of putting long Stata programs directly into the CLI.
- Keep main Stata analysis in `do/analysis.do`.
- Keep input datasets in `data/`.
- Keep derived text results, exported tables, and generated files in `outputs/`.
- Keep Python plotting or post-processing helpers in `scripts/`.
- Run analysis with `stata-cli file do/analysis.do`.
- Every `.do` file must include `capture log close` and `set more off`.
- Write full text Stata output to `outputs/result.txt`.
- Use CLI JSON only to inspect `status`, `error`, `log_file`, and `graphs`.
- If a run fails, read the JSON error plus `outputs/result.txt` or the log file, edit the `.do` file, and retry.
- Use `data view` only for variable names and small previews. Keep `max_rows` at 50 or less unless the user asks for more.
- Do not dump large datasets into chat context.
- Use Stata by default for cleaning, regression, and statistical tests.
- Use Python by default for final charts and save them into `outputs/`.
- If the user explicitly wants Stata graphs, export them explicitly to `outputs/` with `graph export` and do not rely on CLI graph capture.
- Before using any third-party Stata command, run `which <command>` and ask the user before installing anything.
- Read the local `stata-cli` skill when you need Stata syntax help, package guidance, or idiomatic patterns.
""",
    "do/analysis.do": """capture log close
clear all
set more off

cap mkdir "outputs"
log using "outputs/result.txt", text replace

display "Run started: $S_DATE $S_TIME"
display "Working directory: `c(pwd)'"

* Load data here
* use "data/example.dta", clear

* Inspect the dataset
describe
summarize

* Main analysis
* regress y x1 x2

log close
""",
    "scripts/plot.py": """from pathlib import Path

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns


BASE_DIR = Path(__file__).resolve().parents[1]
OUTPUTS_DIR = BASE_DIR / "outputs"
DATA_DIR = BASE_DIR / "data"


def main() -> None:
    source = OUTPUTS_DIR / "analysis.csv"
    if not source.exists():
        source = DATA_DIR / "analysis.csv"
    if not source.exists():
        raise FileNotFoundError(
            "Add a CSV file at outputs/analysis.csv or data/analysis.csv before plotting."
        )

    OUTPUTS_DIR.mkdir(parents=True, exist_ok=True)

    df = pd.read_csv(source)
    numeric_columns = df.select_dtypes(include="number").columns.tolist()
    if len(numeric_columns) < 2:
        raise ValueError("Need at least two numeric columns to build the template plot.")

    x_col, y_col = numeric_columns[:2]

    sns.set_theme(style="whitegrid")
    fig, ax = plt.subplots(figsize=(8, 5))
    sns.lineplot(data=df, x=x_col, y=y_col, marker="o", ax=ax)
    ax.set_title("Analysis Plot")
    ax.set_xlabel(x_col)
    ax.set_ylabel(y_col)

    fig.tight_layout()
    fig.savefig(OUTPUTS_DIR / "plot.png", dpi=200)


if __name__ == "__main__":
    main()
""",
}


def init_workspace_command(target_dir: str) -> dict:
    root = Path(target_dir).expanduser().resolve()
    planned_dirs = [root / relative for relative in INIT_DIRS]
    planned_files = [root / relative for relative in INIT_FILES]
    conflicts = [str(path) for path in planned_files if path.exists()]
    if conflicts:
        return {
            "status": "error",
            "message": "Refusing to overwrite existing scaffold files.",
            "target_dir": str(root),
            "conflicts": conflicts,
        }

    root.mkdir(parents=True, exist_ok=True)
    created: list[str] = []
    for directory in planned_dirs:
        directory.mkdir(parents=True, exist_ok=True)
        created.append(str(directory))
    for relative_path, content in INIT_FILES.items():
        path = root / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        created.append(str(path))

    return {
        "status": "success",
        "target_dir": str(root),
        "created": created,
        "message": f"Initialized AI-ready Stata workspace at {root}",
    }

