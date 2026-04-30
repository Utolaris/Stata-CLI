# AGENTS.md

`stata-cli` is a local Stata CLI with a Rust frontend, a Python backend, repo-local binaries, and an init scaffold for AI-driven analysis work.

## Project Structure

```text
.
├── AGENTS.md
├── README.md
├── bin/                     # Repo-local CLI binaries
├── boilerplate/             # `stata-cli init` scaffold source
│   ├── AGENTS.md
│   ├── data/
│   ├── do/
│   ├── outputs/
│   ├── scripts/
│   └── skills/stata-cli/    # Workspace-local Stata skill copied by `init`
├── rust-cli/                # Rust CLI, REPL, bootstrap, output handling
├── scene/                   # Real smoke-test workspace
├── scripts/                 # Build and maintenance helpers
├── src/                     # Python backend and worker runtime
└── tests/                   # Python test suite
```

## Working Rules

- Use the uv-managed Python 3.11 environment in `.venv`.
- Keep the repo-local binaries in `bin/`; repo-root discovery depends on that location.
- Treat `boilerplate/` as the source of truth for files created by `stata-cli init`, including the bundled workspace skill.
- When changing CLI behavior or REPL behavior, verify both Rust tests and Python backend tests.
- Real smoke tests should run from `scene/` and use `scene/grilic.dta`.
- Keep macOS and Windows path behavior aligned when changing CLI path handling.
