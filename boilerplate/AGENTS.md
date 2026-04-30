# AGENTS.md

- Prefer writing `.do` files instead of putting long Stata programs directly into the CLI.
- Keep main Stata analysis in `do/analysis.do`.
- Keep input datasets in `data/`.
- Keep derived text results, exported tables, and generated files in `outputs/`.
- Keep Python plotting or post-processing helpers in `scripts/`.
- Run analysis with `stata-cli file do/analysis.do`.
- Write Stata full output to `outputs/result.txt`.
- Use CLI JSON only to inspect `status`, `error`, `log_file`, and `graphs`.
- Use `data view` only for variable names and small previews. Keep `max_rows` at 50 or less.
- Do not dump large datasets into chat context.
- Use Stata by default for cleaning, regression, and statistical tests.
- Use Python by default for final charts and save them into `outputs/`.
- If the user explicitly wants Stata graphs, export them explicitly to `outputs/` with `graph export`
- Do not use Stata GUI-only commands in `.do` files or CLI snippets when they start with `browse`, `edit`, `db`, `dialog`, `window`, `shell`, or `winexec`.
- Do not install third-party commands without the user's permission. You should ask for their consent.
- Use `stata-cli run -code` to execute a one-time command such as `ssc install`
- Read the `skills/stata-cli` skill when you need Stata syntax help, package guidance, or idiomatic patterns.
