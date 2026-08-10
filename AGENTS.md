# AGENTS.md

`stata-cli` is a local Stata CLI with a native Rust engine (no Python), repo-local binaries, and an init scaffold for AI-driven analysis work.

## Project Structure

```text
.
├── AGENTS.md
├── README.md
├── skill/stata-cli/         # Self-contained skill package (SKILL.md + bin/ + references/ + boilerplate/)
│   ├── SKILL.md             # Codex/Claude Code skill manifest + routing table
│   ├── bin/                 # Repo-local CLI binaries
│   ├── references/          # Stata reference library (routed from SKILL.md)
│   ├── packages/            # Community package guidance
│   └── boilerplate/         # `stata-cli init` scaffold source
│       ├── AGENTS.md
│       ├── data/
│       ├── do/
│       ├── outputs/
│       └── scripts/
├── rust-cli/                # Rust CLI, REPL, native Stata engine (StataSO FFI)
├── scene/                   # Real smoke-test workspace
└── scripts/                 # Build and maintenance helpers
```

## Working Rules

- Keep the repo-local binaries in `skill/stata-cli/bin/`; repo-root discovery depends on that location.
- Treat `skill/stata-cli/boilerplate/` as the source of truth for files created by `stata-cli init`.
- Treat `skill/stata-cli/` itself as the installed skill package: SKILL.md,
  references/, packages/, and bin/ ship together and are not duplicated into
  workspaces by `init`.
- The `stata-cli.skill` file attached to GitHub releases is a ZIP archive of
  `skill/stata-cli/` (top-level folder `stata-cli/`); unpack it and drop that
  folder into any agent's skills directory (Codex: `~/.codex/skills/`,
  Claude Code: `~/.claude/skills/`).
- When changing CLI behavior or REPL behavior, verify the Rust tests and run real Stata smoke tests from `scene/` (set `SKIP_STATA_TESTS=1` to skip them).
- Real smoke tests should run from `scene/` and use `scene/grilic.dta`.
- The native Stata engine (`rust-cli/src/atom/stata_engine.rs`) is the only
  module allowed to use `unsafe`; it wraps Stata's `StataSO_*` C ABI behind a
  safe API. See the module docs and README.md ("Unsafe FFI") before touching it.
