# stata-cli

`stata-cli` is a local command-line tool for running Stata code, `.do` files, and `.dta` data through the Python/PyStata backend in this repository.

This repo is designed so an AI agent can quickly understand the project, install the right dependencies, bootstrap an analysis workspace, and run Stata locally without needing VS Code.

This CLI also provides a standalone REPL for human exploration, with syntax highlighting and code completion.

## Install

### 1. Install Stata 18

Install Stata 18 first. On Windows, the default location is best:

```text
C:\Program Files\Stata18
```

If Stata is installed somewhere else, pass `--stata-path` or set it in the CLI config.

### 2. Prepare the Python backend

`stata-cli` depends on the local Python backend in this repository. Use Python 3.11 because the Stata Python bridge is not compatible with newer runtimes.

```bash
uv sync --all-extras --python 3.11
```

### 3. Add the repo-local binary directory to `PATH`

This project ships a repo-local binary under `bin/` because the CLI depends on the Python backend that lives in the same repository.

After cloning the repo, add its `bin/` directory to your shell `PATH`:

```bash
export PATH="/absolute/path/to/stata-cli/bin:$PATH"
```

Put that line in your shell config if you want it to persist.

The binary in `bin/` resolves the repository root from its own location, so keeping it inside the repo means you do not need a separate global install step.

If you are on a platform that does not already have a matching binary in `bin/`, build one locally and copy it there:

macOS / Linux:

```bash
./scripts/update_repo_bin.sh
```

Windows PowerShell:

```powershell
cargo install cargo-zigbuild --locked
cargo zigbuild --release --target x86_64-pc-windows-gnu --manifest-path rust-cli/Cargo.toml
Copy-Item rust-cli\\target\\x86_64-pc-windows-gnu\\release\\stata-cli.exe bin\\stata-cli.exe
```

If you have Bash available on Windows, you can also run:

```bash
bash ./scripts/build_windows_bin.sh
```

### 4. Locate the bundled skill

The bundled Stata skill lives at:

```text
skills/stata-cli/
```

If your AI tool supports installable local skills, point it at that directory and install it using the tool's own workflow.

### 5. Verify the setup

```bash
stata-cli doctor
```

## Capabilities

`stata-cli` is designed to make local Stata work easier for AI agents and humans:

- Run inline Stata commands with `stata-cli run`
- Execute `.do` files with `stata-cli file`
- Inspect and export `.dta` data with `stata-cli data view` and `stata-cli data export-csv`
- Diagnose the local Python/Stata backend with `stata-cli doctor`
- Bootstrap an AI-friendly project scaffold with `stata-cli init`
- Use the bundled `skills/stata-cli/` guidance to help AI agents write safer, more idiomatic Stata code
- Use the standalone `stata-cli repl` for human interactive work, including syntax highlighting and code completion

Non-REPL commands are intentionally AI-friendly: they return structured JSON and avoid dumping unnecessary terminal noise into stdout.

### Initialize an AI-ready workspace

```bash
mkdir my-analysis
cd my-analysis
stata-cli init
```

`stata-cli init` copies the repo-root `boilerplate/` scaffold into the current directory, giving each analysis project a predictable structure for data, Stata code, outputs, helper scripts, and agent instructions.

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

### Diagnose the local environment

```bash
stata-cli doctor
```

Use `doctor` to confirm that the repo-local Rust CLI, Python backend, and Stata installation can talk to each other.

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
- If the local `stata-cli` skill is available, use it for Stata syntax, package guidance, and idiomatic patterns

### Export data to CSV

```bash
stata-cli data export-csv --input-dta /absolute/path/to/data.dta --output /absolute/path/to/out.csv --replace
```

- Convert a `.dta` file to CSV
- Overwrite an existing CSV with `--replace`

## Common failure reasons

- `stata-cli` is not installed or not on `PATH`
- The uv-managed Python 3.11 environment is missing
- The binary was moved away from the repository, so it can no longer locate the Python backend
- Stata 18 is not installed, or `--stata-path` points to the wrong location
- PyStata or the local Stata Python bridge is unavailable
- The target `.do` or `.dta` file path does not exist

If setup looks wrong, start with:

```bash
stata-cli doctor
```

## License

MIT

## Acknowledgements

This project benefits from prior experimentation in AI-oriented Stata tooling and from the broader PyStata ecosystem.
