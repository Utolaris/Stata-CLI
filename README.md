# stata-cli

`stata-cli` is a lightweight local CLI for running Stata through the Python/PyStata backend in this repository.

This repository is now focused on the CLI workflow: install the Python runtime, download the `stata-cli` binary, install the Codex skill, and use the command directly from anywhere on your machine.

## Install

### 1. Prepare the Python backend

`stata-cli` depends on the local Python backend in this repository. Use Python 3.11 because the Stata Python bridge is not compatible with newer runtimes.

```bash
uv sync --all-extras --python 3.11
```

### 2. Download and install the CLI binary

Release `v0.0.1` ships a macOS Apple Silicon binary named `stata-cli-darwin-arm64.tar.gz`.

Install it into `~/.local/bin`:

```bash
mkdir -p ~/.local/bin
curl -L https://github.com/VO-VOO/stata-cli/releases/download/v0.0.1/stata-cli-darwin-arm64.tar.gz \
  | tar -xz -C ~/.local/bin
```

If `~/.local/bin` is not already on `PATH`, add this to your shell config:

```bash
export PATH="$HOME/.local/bin:$PATH"
```

### 3. Point the CLI at this repository

The binary needs to know where the Python backend lives. Create `~/.config/stata-cli/config.toml`:

```toml
project_root = "/absolute/path/to/stata-cli"
```

Example:

```toml
project_root = "/Users/utolaris/Documents/ai/stata-mcp"
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
- Check Python 3.11 resolution
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
- Python 3.11 is missing
- The repository path in `~/.config/stata-cli/config.toml` is wrong
- Stata is not installed, or `--stata-path` points to the wrong location
- PyStata or the local Stata Python bridge is unavailable
- The target `.do` or `.dta` file path does not exist

If setup looks wrong, start with:

```bash
stata-cli doctor
```

## Release

- Current CLI release: `v0.0.1`
- Current binary crate version: `0.0.1`

## License

MIT
