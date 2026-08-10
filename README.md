# stata-cli

中文说明请见 [README.zh.md](/Users/utolaris/Documents/ai/stata-cli/README.zh.md).

`stata-cli` is a local command-line tool for running Stata code, `.do` files, and `.dta` data through a native Rust engine. It loads Stata's own shared library (`libstata-mp.dylib`) directly and calls the same `StataSO_*` C ABI that the official PyStata bridge uses — no Python interpreter, `pystata`, or virtual environment is required.

This repo is designed so an AI agent can quickly understand the project, install the right dependencies, bootstrap an analysis workspace, and run Stata locally without needing VS Code.

This CLI also provides a standalone REPL for human exploration, with syntax highlighting and code completion.

## Install

### 1. Install Stata 18

Install Stata 18 first. On Windows, the default location is best:

```text
C:\Program Files\Stata18
```

If Stata is installed somewhere else, pass `--stata-path` or set it in the CLI config.

### 2. Install the skill package (recommended)

The repo ships a self-contained skill folder at `skill/stata-cli/`: `SKILL.md`,
`bin/`, and `boilerplate/` live in one folder, so the binary and the init
templates travel together. Users do not need to clone the repository.

```bash
./scripts/install_skill.sh            # installs into ~/.codex/skills/stata-cli
./scripts/install_skill.sh --claude   # also installs into ~/.claude/skills/stata-cli
```

Existing skill folders are backed up before being replaced. `stata-cli init`
finds `boilerplate/` next to the binary (or via `STATA_CLI_TEMPLATE_DIR`), so a
clone is never required at runtime.

If you prefer to develop inside the repo, add `skill/stata-cli/bin/` to your
shell `PATH` instead:

```bash
export PATH="/absolute/path/to/stata-cli/skill/stata-cli/bin:$PATH"
```

Put that line in your shell config if you want it to persist.

If you are on a platform that does not already have a matching binary in `skill/stata-cli/bin/`, build one locally and copy it there:

macOS / Linux:

```bash
./scripts/update_repo_bin.sh
```

Windows PowerShell:

```powershell
cargo install cargo-zigbuild --locked
cargo zigbuild --release --target x86_64-pc-windows-gnu --manifest-path rust-cli/Cargo.toml
Copy-Item rust-cli\\target\\x86_64-pc-windows-gnu\\release\\stata-cli.exe skill\\stata-cli\\bin\\stata-cli.exe
```

If you have Bash available on Windows, you can also run:

```bash
bash ./scripts/build_windows_bin.sh
```

### 3. Verify the setup

```bash
stata-cli doctor
```

## Capabilities

`stata-cli` is designed to make local Stata work easier for AI agents and humans:

- Run inline Stata commands with `stata-cli run`
- Execute `.do` files with `stata-cli file`
- Inspect and export `.dta` data with `stata-cli data view` and `stata-cli data export-csv`
- Diagnose the local Stata engine with `stata-cli doctor`
- Bootstrap an AI-friendly project scaffold with `stata-cli init`
- Render real local Stata help text for `help <topic>` in the REPL and `run`
- Use the bundled Stata skill that `stata-cli init` places under `skills/stata-cli/` in each workspace
- Use the standalone `stata-cli repl` for human interactive work, including syntax highlighting and code completion

Non-REPL commands are intentionally AI-friendly: they return structured JSON and avoid dumping unnecessary terminal noise into stdout.

### Initialize an AI-ready workspace

```bash
mkdir my-analysis
cd my-analysis
stata-cli init
```

`stata-cli init` copies the `boilerplate/` scaffold that ships next to the binary into the current directory, giving each analysis project a predictable structure for data, Stata code, outputs, helper scripts, and agent instructions.
The scaffold also includes a local `skills/stata-cli/` reference library for AI agents.

### Run Stata code

```bash
stata-cli run --code 'display 1+1'
```

Use this for short inline commands. The response is structured JSON, so AI agents can reliably inspect status, output, logs, and errors.

### Run a `.do` file

```bash
stata-cli file /absolute/path/to/script.do
```

Use this for substantial Stata analysis. It is the preferred path for agent-driven work because code, logs, and generated files stay inside the project workspace.
The JSON response keeps `output` to the final tail of the Stata log for quick error location; read `log_file` when the full result is needed.

### Start the REPL

```bash
stata-cli repl
```

The REPL is a separate human-oriented interface with a Stata-style prompt, syntax highlighting, code completion, continuation handling, and filtered output.
`help <topic>` renders the real local Stata help text (read from Stata's installed `.sthlp` files) into the terminal. Bare `help`, `search`, and `findit` return a guidance message instead, because those commands open Stata's GUI windows and produce no terminal output. Inside `.do` files, `help` keeps Stata's native behavior.

### Diagnose the local environment

```bash
stata-cli doctor
```

Use `doctor` to confirm that the repo-local Rust CLI can load Stata's shared library and execute a probe command.

### Work with data

```bash
stata-cli data view --input-dta /absolute/path/to/data.dta --max-rows 20
stata-cli data view --input-dta /absolute/path/to/data.dta --if-condition 'iq > 110' --max-rows 10
```

Use `data view` for small previews and schema checks from an explicit `.dta` file. Non-REPL CLI commands do not share session state, so AI agents should not rely on `data view` seeing data loaded by a previous command.

## AI-first workflow

For agent-driven work, start with:

```bash
mkdir my-analysis
cd my-analysis
stata-cli init
```

Then keep the working pattern simple:

- Put substantial Stata logic in `do/analysis.do`
- Include `capture log close` and `set more off`
- Write full text output to `outputs/result.txt`
- Run the analysis with `stata-cli file do/analysis.do`
- Use the JSON response to inspect `status`, `error`, `partial_failure_count`, `partial_failures`, `log_file`, and `graphs`
- Use `data view` for schema checks and small previews, not full table dumps
- Use Python scripts under `scripts/` for final charts saved into `outputs/`
- If the user explicitly wants Stata graphs, write explicit `graph export "outputs/..."` commands in the `.do` file instead of relying on CLI graph capture
- Run `which <command>` before using third-party Stata packages, and ask before installing anything
- Read the workspace-local `skills/stata-cli/` reference library when you need Stata syntax, package guidance, or idiomatic patterns

### Export data to CSV

```bash
stata-cli data export-csv --input-dta /absolute/path/to/data.dta --output /absolute/path/to/out.csv --replace
```

- Convert a `.dta` file to CSV
- Overwrite an existing CSV with `--replace`

## Common failure reasons

- `stata-cli` is not installed or not on `PATH`
- Stata 18 is not installed, or `--stata-path` points to the wrong location
- Stata was not found at `--stata-path`, `STATA_PATH`, or the macOS defaults (`/Applications/StataNow`, `/Applications/Stata`)
- The target `.do` or `.dta` file path does not exist

If setup looks wrong, start with:

```bash
stata-cli doctor
```

## Unsafe FFI

The Rust crate normally forbids `unsafe` code (`unsafe_code = "warn"` since the
project no longer uses Python). There is one deliberate exception:
`rust-cli/src/atom/stata_engine.rs` calls into Stata's shared library through
its exported `StataSO_*` C ABI. Stata does not ship a Rust API, and the local
in-process bridge is the only supported way to drive Stata without a separate
process (the official `pystata` package does the same thing through `ctypes`).

The exception is confined to that one module, which exposes a small safe API:

- `StataEngine::new(stata_home, edition)` – loads
  `libstata-{mp,se,be}.dylib` and initializes the engine (no `-pyexec`, so no
  Python is attached). A process-wide singleton guard rejects a second engine
  in the same process.
- `execute(cmd)` / `run_block(code)` – run one line or a temp-do-file block
  and return `(rc, output)`. Output is drained from Stata's buffer (raised to
  512 MB) until empty, so it survives 2 MB+ runs and user `log`/`capture`
  commands.
- `set_break()` – interrupt a running command from a monitor thread (reserved
  for a future stop/timeout feature). An atomic guard allows at most one
  break per execution; cancellation status comes from that flag, not from
  matching `--Break--` text.
- `shutdown()` – note: this calls Stata's `_sexit` and terminates the current
  process, so it is only used at REPL exit. It is serialized against in-flight
  executions.

Known constraints and risks:

- One Stata engine per OS process (Stata uses process-wide globals), so
  parallel sessions must be separate processes.
- `StataSO_Execute` is not reentrant; calls are serialized with a mutex.
- A crash inside the C engine can take the whole CLI process down.
- `data view` previews are produced via a temporary `export delimited` CSV
  with `nolabel`, converted by the storage types read from `describe` (so
  leading-zero strings stay strings, value labels come back as numeric codes,
  and all-missing columns keep their real dtype). Floating-point values use
  Stata's shortest round-trip text form (about 8 significant digits for float32
  storage, full precision for double), which reconstructs the exact stored
  value; this differs only textually from pandas' float64 widening of float32
  columns, and integer columns are reported as JSON integers.

## License

## License

MIT

## Acknowledgements

This project benefits from prior experimentation in AI-oriented Stata tooling and from reverse-engineering the `StataSO_*` ABI used by the PyStata ecosystem.
