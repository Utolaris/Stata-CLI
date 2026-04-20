# stata-cli

[Original project: hanlulong/stata-mcp](https://github.com/hanlulong/stata-mcp)

`stata-cli` is a lightweight local CLI for running Stata through the Python/PyStata backend in this repository.

This repository is based on the original `stata-mcp` project and refocused into `stata-cli`, with the goal of giving AI agents a simpler local CLI for Stata without depending on VS Code.

This repository is now focused on the CLI workflow: install the Python runtime, download the `stata-cli` binary, install the Codex skill, and use the command directly from anywhere on your machine.

## Install

### 1. Prepare the Python backend

`stata-cli` depends on the local Python backend in this repository. Use Python 3.11 because the Stata Python bridge is not compatible with newer runtimes.

On macOS, install dependencies with `brew`.

On Windows, install dependencies with `scoop`.

```bash
uv sync --all-extras --python 3.11
```

### 2. Download and install the CLI binary

Release `v0.0.2` ships a macOS Apple Silicon binary named `stata-cli-darwin-arm64.tar.gz` and a Windows binary named `stata-cli-windows-x86_64.zip`.

Install it into `~/.local/bin`:

```bash
mkdir -p ~/.local/bin
curl -L https://github.com/VO-VOO/stata-cli/releases/download/v0.0.2/stata-cli-darwin-arm64.tar.gz \
  | tar -xz -C ~/.local/bin
```

If `~/.local/bin` is not already on `PATH`, add this to your shell config:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### 3. Point the CLI at this repository

The binary needs to know where the Python backend lives.

On macOS/Linux, create `~/.config/stata-cli/config.toml`.

On Windows, create `%APPDATA%\stata-cli\config.toml`.

Minimal example:

```toml
project_root = "/absolute/path/to/stata-cli"
```

You can also persist a custom Stata location in the same file:

```toml
project_root = "/absolute/path/to/stata-cli"
stata_path = "C:\\Program Files\\Stata18"
```

macOS example:

```toml
project_root = "/Users/utolaris/Documents/ai/stata-cli"
```

Windows example:

```toml
project_root = "C:\\Users\\yourname\\Documents\\stata-cli"
stata_path = "D:\\Stata18"
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

- Run one Stata command at a time in a simple interactive shell

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
```

- Preview rows from the current dataset
- Preview rows directly from a `.dta` file with `--input-dta`
- Filter with `--if-condition`
- Limit rows with `--max-rows`

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
- The repository path in the CLI config file is wrong
- Stata is not installed, or `--stata-path` points to the wrong location
- PyStata or the local Stata Python bridge is unavailable
- The target `.do` or `.dta` file path does not exist

## Windows notes

- `stata-cli` looks for Stata at `C:\Program Files\Stata18` by default.
- If that path does not exist and the command is running in an interactive terminal, `stata-cli` will prompt for a custom Stata installation directory and save it to `%APPDATA%\stata-cli\config.toml` after a successful run.
- In non-interactive Windows environments, pass `--stata-path` explicitly or pre-populate `%APPDATA%\stata-cli\config.toml`.
- Python is resolved strictly from the project `.venv` created by `uv sync --all-extras --python 3.11`.

## Package manager notes

- On macOS, prefer `brew` for system dependencies.
- On Windows, prefer `scoop` for system dependencies.

If setup looks wrong, start with:

```bash
stata-cli doctor
```

## Release

- Current CLI release: `v0.0.2`
- Current binary crate version: `0.0.2`

## License

MIT
