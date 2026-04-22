# AGENTS.md

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
