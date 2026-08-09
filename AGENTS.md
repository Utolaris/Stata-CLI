# AGENTS.md

`stata-cli` is a local Stata CLI with a native Rust engine (no Python), repo-local binaries, and an init scaffold for AI-driven analysis work.

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
├── rust-cli/                # Rust CLI, REPL, native Stata engine (StataSO FFI)
├── scene/                   # Real smoke-test workspace
├── scripts/                 # Build and maintenance helpers
└── tests/                   # Stata .do fixtures
```

## Working Rules

- Keep the repo-local binaries in `bin/`; repo-root discovery depends on that location.
- Treat `boilerplate/` as the source of truth for files created by `stata-cli init`, including the bundled workspace skill.
- When changing CLI behavior or REPL behavior, verify the Rust tests and run real Stata smoke tests from `scene/` (set `SKIP_STATA_TESTS=1` to skip them).
- Real smoke tests should run from `scene/` and use `scene/grilic.dta`.
- The native Stata engine (`rust-cli/src/atom/stata_engine.rs`) is the only
  module allowed to use `unsafe`; it wraps Stata's `StataSO_*` C ABI behind a
  safe API. See the module docs and README.md ("Unsafe FFI") before touching it.
