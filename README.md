# stata-cli

[Original project: hanlulong/stata-mcp](https://github.com/hanlulong/stata-mcp)

`stata-cli` is a local command-line tool for running Stata code, `.do` files, and `.dta` data through the Python/PyStata backend in this repository.

This repo is designed so an AI agent can quickly understand the project, install the right dependencies, and run Stata locally without needing VS Code.

## Project Structure

```text
.
├── dist/                 # release archives for the CLI
├── rust-cli/             # native Rust CLI source
├── skills/               # Codex skill for local usage
├── src/                  # Python backend and MCP server
├── tests/                # automated tests
└── README.md
```

## Install

### 1. Install Stata 18

Install Stata 18 first. On macOS, the default location is best:

```text
/Applications/Stata
```

If Stata is installed somewhere else, pass `--stata-path` or set it in the CLI config.

### 2. Prepare the Python backend

`stata-cli` depends on the local Python backend in this repository. Use Python 3.11 because the Stata Python bridge is not compatible with newer runtimes.

```bash
uv sync --all-extras --python 3.11
```

### 3. Download and install the CLI binary

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

### 4. Point the CLI at this repository

The binary needs to know where the Python backend lives.

Create `~/.config/stata-cli/config.toml`:

```toml
project_root = "/absolute/path/to/stata-cli"
```

Example:

```toml
project_root = "/Users/utolaris/Documents/ai/stata-cli"
```

### 5. Install the Codex skill

Copy the bundled skill into Codex's local skill directory:

```bash
mkdir -p ~/.codex/skills/stata-cli
cp skills/stata-cli/SKILL.md ~/.codex/skills/stata-cli/SKILL.md
```

### 6. Verify the setup

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
- Stata 18 is not installed, or `--stata-path` points to the wrong location
- PyStata or the local Stata Python bridge is unavailable
- The target `.do` or `.dta` file path does not exist

If setup looks wrong, start with:

```bash
stata-cli doctor
```

## Release

- Current CLI release: `v0.0.2`
- Current binary crate version: `0.0.2`

## License

MIT
