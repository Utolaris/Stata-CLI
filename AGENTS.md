# AGENTS.md

## Purpose

`stata-cli` is a local Stata CLI project with:

- a Rust frontend CLI in `rust-cli/`
- a Python backend implementation in `src/`
- automated tests in `tests/`
- repo-local binaries in `bin/`
- a real test scene in `scene/`

## Project Structure

```text
.
├── AGENTS.md                # Project guidance for coding agents
├── README.md                # User-facing project documentation
├── pyproject.toml           # Python package, Ruff, pytest, mypy config
├── uv.lock                  # Python lockfile
├── bin/                     # Repo-local built CLI binaries
├── boilerplate/             # Workspace scaffold copied by `stata-cli init`
├── dist/                    # Release artifacts
├── logs/                    # Runtime logs and sample outputs
├── rust-cli/                # Rust CLI, REPL, rendering, and workspace bootstrap
│   ├── Cargo.toml
│   ├── build.rs
│   ├── src/main.rs
│   └── tests/
├── scene/                   # Real local smoke-test scene
│   └── grilic.dta
├── scripts/                 # Helper scripts for local maintenance
├── skills/                  # Codex skill content
├── src/                     # Python backend, worker, and compatibility shims
│   ├── api_models.py
│   ├── output_filter.py
│   ├── session_manager.py
│   ├── smcl_parser.py
│   ├── stata_cli/
│   ├── stata_cli_backend.py
│   ├── stata_worker.py
│   └── utils.py
└── tests/                   # Python tests, fixtures, and integration helpers
```

## Working Rules

- Use the uv-managed Python 3.11 environment from the repo `.venv`.
- Keep the Rust binary in `bin/` so repo-root discovery continues to work.
- Prefer non-destructive verification first: `ruff check .`, `cargo fmt --check`, `cargo test`.
- Real CLI smoke tests should run from `scene/` and use `scene/grilic.dta`.
- When changing the REPL or CLI contract, verify both Python backend tests and Rust CLI tests.
- `stata-cli init` now copies from repo-root `boilerplate/` into the current directory; treat `boilerplate/` as the scaffold source of truth.
- Rust owns the public REPL, CLI output filtering, SMCL HTML rendering, and workspace bootstrap behavior.
- This project needs to support both Windows and macOS; pay attention to differences in path separators and system commands.
