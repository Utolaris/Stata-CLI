# stata-cli

[中文文档](README.zh.md)

`stata-cli` is an AI-agent-oriented Stata CLI for running Stata code, `.do` files, and `.dta` data through the Python/PyStata backend in this repository.

This repo is designed so an AI agent can quickly understand the project, install the right dependencies, bootstrap an analysis workspace, and run Stata locally without needing VS Code.

This CLI also provides a human-oriented REPL that allows users to run Stata commands from any directory with syntax highlighting.

## Install

### 1. Install Stata 18

Install Stata 18 first.

On Windows, the default location is best:

```text
C:\Program Files\Stata18
```

On macOS, Stata is usually installed in the default location, so no extra path setup is normally required.

If Stata is installed in a custom location, pass `--stata-path` or set it in the CLI config.

### 2. Prepare the Python backend

`stata-cli` depends on the local Python backend in this repository. Use Python 3.11 because the Stata Python bridge is not compatible with newer runtimes.

```bash
uv sync --all-extras --python 3.11
```

### 3. Add the repo-local binary directory to `PATH`

This project ships a repo-local binary under `bin/` because the CLI depends on the Python backend that lives in the same repository.

After cloning the repo, add its `bin/` directory to your `PATH`.

macOS / Linux:

```bash
export PATH="/absolute/path/to/stata-cli/bin:$PATH"
```

Windows PowerShell:

```powershell
$env:Path = "C:\absolute\path\to\stata-cli\bin;$env:Path"
```

Windows Command Prompt:

```bat
set PATH=C:\absolute\path\to\stata-cli\bin;%PATH%
```

Put the command for your platform in your shell/profile config if you want it to persist.

The binary in `bin/` resolves the repository root from its own location, so keeping it inside the repo means you do not need a separate global install step.

If you are on a platform that does not already have a matching binary in `bin/`, build one locally and copy it there:

macOS / Linux:

```bash
./scripts/update_repo_bin.sh
```

Windows PowerShell:

```powershell
cargo build --release --manifest-path rust-cli/Cargo.toml
Copy-Item rust-cli\\target\\release\\stata-cli.exe bin\\stata-cli.exe
```

### 4. Install the Codex skill

Copy the bundled skill into Codex's local skill directory:

```bash
mkdir -p ~/.codex/skills/stata-cli
cp skills/stata-cli/SKILL.md ~/.codex/skills/stata-cli/SKILL.md
```

### 5. Verify the setup

```bash
stata-cli doctor
```

## Capabilities

### Initialize an AI-ready workspace

```bash
stata-cli init ./my-analysis
```

- Create an agent-oriented Stata working directory
- Generate `AGENTS.md`, `data/`, `do/`, `outputs/`, `scripts/`, `do/analysis.do`, `scripts/plot.py`, and `stata-packages.md`
- Fail if scaffold files already exist instead of overwriting them silently

### Run Stata code

```bash
stata-cli run --code 'display 1+1'
```

- Execute inline Stata code
- Pass `--working-dir`, `--timeout`, `--stata-path`, and `--stata-edition` when needed
- Use `--json` for structured output

### Run a `.do` file

```bash
stata-cli file /absolute/path/to/script.do
```

- Execute local `.do` files
- Return output, effective session id, log path, and graph artifacts when available

### Start a minimal REPL

```bash
stata-cli repl
```

- Run one Stata command at a time in a human-oriented interactive shell
- Prefer this for quick manual exploration instead of AI workflows
- Can run from any directory when `stata-cli-backend` is available on `PATH`, or when you pass `--python` to a Python 3.11 environment with the backend installed
- Uses a Stata-style prompt, syntax highlighting, and filtered output without extra CLI log noise

### Diagnose the local environment

```bash
stata-cli doctor
stata-cli --json doctor
```

- Check repo root resolution
- Check backend script presence
- Check the uv-managed Python 3.11 environment
- Run a minimal backend probe

### Preview data

```bash
stata-cli --json data view --input-dta /absolute/path/to/data.dta --max-rows 20
stata-cli --json data view --if-condition 'iq > 110' --max-rows 10
stata-cli --json data view
```

- Preview rows from the current dataset
- Preview rows directly from a `.dta` file with `--input-dta`
- Filter with `--if-condition`
- Limit rows with `--max-rows`
- Default to `50` rows so AI agents do not dump large tables into chat context

## AI-first workflow

For agent-driven work, start with:

```bash
stata-cli init ./my-analysis
```

Then keep the working pattern simple:

- Put substantial Stata logic in `do/analysis.do`
- Include `capture log close` and `set more off`
- Write full text output to `outputs/result.txt`
- Run the analysis with `stata-cli file do/analysis.do --json`
- Use the JSON response only to inspect `status`, `error`, `log_file`, and `graphs`
- Use `data view` for schema checks and small previews, not full table dumps
- Use Python scripts under `scripts/` for final charts saved into `outputs/`
- Run `which <command>` before using third-party Stata packages, and ask before installing anything

### Export data to CSV

```bash
stata-cli data export-csv --input-dta /absolute/path/to/data.dta --output /absolute/path/to/out.csv --replace
```

- Convert a `.dta` file to CSV
- Export the current dataset to CSV
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

This project was inspired by the design of [stata-mcp](https://github.com/hanlulong/stata-mcp). Thanks to the original project for the ideas and structure it provided.
