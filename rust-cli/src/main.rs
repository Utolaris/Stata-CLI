use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::ffi::OsString;
use std::fs;
use std::io::{self, IsTerminal, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

const COMPILED_REPO_ROOT: &str = env!("STATACLI_REPO_ROOT");
const PROJECT_ROOT_ENV: &str = "STATA_CLI_PROJECT_ROOT";
const STATA_PATH_ENV: &str = "STATA_PATH";

#[derive(Parser, Debug, Clone)]
#[command(name = "stata-cli")]
#[command(about = "A local Rust CLI wrapper for the Python/PyStata backend")]
struct Cli {
    #[arg(long)]
    stata_path: Option<String>,
    #[arg(long)]
    stata_edition: Option<String>,
    #[arg(long)]
    python: Option<PathBuf>,
    #[arg(long)]
    session_id: Option<String>,
    #[arg(long)]
    working_dir: Option<PathBuf>,
    #[arg(long)]
    timeout: Option<u32>,
    #[arg(long)]
    json: bool,
    #[arg(long)]
    quiet: bool,
    #[arg(long, default_value = "WARNING")]
    log_level: String,
    #[arg(long)]
    result_display_mode: Option<String>,
    #[arg(long)]
    max_output_tokens: Option<u32>,
    #[arg(long, conflicts_with = "no_multi_session")]
    multi_session: bool,
    #[arg(long, conflicts_with = "multi_session")]
    no_multi_session: bool,
    #[arg(long)]
    max_sessions: Option<u32>,
    #[arg(long)]
    session_timeout: Option<u32>,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
enum Commands {
    Run {
        #[arg(long)]
        code: String,
    },
    File {
        path: PathBuf,
        #[arg(long)]
        timeout: Option<u32>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        working_dir: Option<PathBuf>,
    },
    Init {
        target_dir: PathBuf,
    },
    Repl,
    Doctor,
    Data {
        #[command(subcommand)]
        command: DataCommands,
    },
}

#[derive(Subcommand, Debug, Clone)]
enum DataCommands {
    View {
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        if_condition: Option<String>,
        #[arg(long, default_value_t = 50)]
        max_rows: u32,
        #[arg(long)]
        input_dta: Option<PathBuf>,
    },
    ExportCsv {
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        input_dta: Option<PathBuf>,
        #[arg(long)]
        session_id: Option<String>,
        #[arg(long)]
        working_dir: Option<PathBuf>,
        #[arg(long)]
        replace: bool,
    },
}

#[derive(Debug, Deserialize, Serialize)]
struct GraphArtifact {
    path: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct ExecutionResult {
    status: String,
    output: String,
    session_id: Option<String>,
    log_file: Option<String>,
    graphs: Vec<GraphArtifact>,
    error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
struct CliConfig {
    project_root: Option<PathBuf>,
    stata_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
struct RepoRootResolution {
    path: PathBuf,
    source: &'static str,
}

#[derive(Debug, Clone)]
struct PythonResolution {
    path: PathBuf,
    source: &'static str,
    version: String,
}

#[derive(Debug, Serialize)]
struct DoctorCheck {
    name: &'static str,
    status: &'static str,
    detail: String,
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    status: &'static str,
    checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StataPathSource {
    CliFlag,
    Environment,
    Config,
    Default,
    Prompt,
}

#[derive(Debug, Clone)]
struct ResolvedStataPath {
    path: Option<PathBuf>,
    source: Option<StataPathSource>,
    save_to_config: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let resolved_stata_path = resolve_effective_stata_path(&cli)?;
    let mut effective_cli = cli.clone();
    if let Some(path) = &resolved_stata_path.path {
        effective_cli.stata_path = Some(path.to_string_lossy().into_owned());
    }

    if matches!(effective_cli.command, Commands::Repl) {
        return repl_command(&effective_cli);
    }

    let repo_root = resolve_repo_root()?;

    match &effective_cli.command {
        Commands::Doctor => doctor_command(&effective_cli, &repo_root, &resolved_stata_path),
        Commands::Run { code } => {
            let python = resolve_python(effective_cli.python.as_deref(), &repo_root.path)?;
            let mut command_args = vec![OsString::from("--code"), OsString::from(code)];
            if let Some(timeout) = effective_cli.timeout {
                command_args.push(OsString::from("--timeout"));
                command_args.push(OsString::from(timeout.to_string()));
            }
            let result = invoke_backend(
                &python.path,
                &repo_root.path,
                &effective_cli,
                "run",
                command_args,
            )?;
            persist_stata_path_if_needed(&resolved_stata_path, &result)?;
            render_result(&result, effective_cli.json, effective_cli.quiet)?;
            Ok(())
        }
        Commands::File {
            path,
            timeout,
            session_id,
            working_dir,
        } => {
            let python = resolve_python(effective_cli.python.as_deref(), &repo_root.path)?;
            let mut file_cli = effective_cli.clone();
            let resolved_path = absolutize_cli_path(path)?;
            if session_id.is_some() {
                file_cli.session_id = session_id.clone();
            }
            if let Some(working_dir) = working_dir {
                file_cli.working_dir = Some(absolutize_cli_path(working_dir)?);
            }
            if timeout.is_some() {
                file_cli.timeout = *timeout;
            }
            let result = invoke_backend(
                &python.path,
                &repo_root.path,
                &file_cli,
                "file",
                vec![resolved_path.as_os_str().to_os_string()],
            )?;
            persist_stata_path_if_needed(&resolved_stata_path, &result)?;
            render_result(&result, file_cli.json, file_cli.quiet)?;
            Ok(())
        }
        Commands::Init { target_dir } => {
            let python = resolve_python(effective_cli.python.as_deref(), &repo_root.path)?;
            let payload = invoke_backend_json(
                &python.path,
                &repo_root.path,
                &effective_cli,
                "init",
                vec![target_dir.as_os_str().to_os_string()],
            )?;
            render_json_payload(&payload, effective_cli.json)
        }
        Commands::Repl => unreachable!("repl is handled before project-root resolution"),
        Commands::Data { command } => {
            let python = resolve_python(effective_cli.python.as_deref(), &repo_root.path)?;
            let (backend_command, backend_args) = data_backend_invocation(command)?;
            let payload = invoke_backend_json(
                &python.path,
                &repo_root.path,
                &effective_cli,
                backend_command,
                backend_args,
            )?;
            persist_stata_path_if_needed_json(&resolved_stata_path, &payload)?;
            render_json_payload(&payload, effective_cli.json)
        }
    }
}

fn home_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    } else {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

fn default_config_path() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|dir| dir.join("stata-cli").join("config.toml"))
    } else {
        home_dir().map(|home| home.join(".config").join("stata-cli").join("config.toml"))
    }
}

fn load_cli_config(path: &Path) -> Result<Option<CliConfig>> {
    if !path.exists() {
        return Ok(None);
    }
    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file at {}", path.display()))?;
    let config = toml::from_str::<CliConfig>(&raw)
        .with_context(|| format!("Failed to parse config file at {}", path.display()))?;
    Ok(Some(config))
}

fn write_cli_config(path: &Path, config: &CliConfig) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create config directory at {}", parent.display())
        })?;
    }
    let serialized = toml::to_string_pretty(config)
        .with_context(|| format!("Failed to serialize config for {}", path.display()))?;
    fs::write(path, serialized)
        .with_context(|| format!("Failed to write config file at {}", path.display()))?;
    Ok(())
}

fn persist_resolved_stata_path(path: &Path) -> Result<()> {
    let config_path = default_config_path().ok_or_else(|| {
        anyhow::anyhow!(
            "Could not determine the Windows config location for saving the Stata path."
        )
    })?;
    let mut config = load_cli_config(&config_path)?.unwrap_or_default();
    config.stata_path = Some(path.to_path_buf());
    write_cli_config(&config_path, &config)
}

fn backend_script(repo_root: &Path) -> PathBuf {
    repo_root.join("src").join("stata_cli_backend.py")
}

fn project_python(repo_root: &Path) -> PathBuf {
    if cfg!(windows) {
        repo_root.join(".venv").join("Scripts").join("python.exe")
    } else {
        repo_root.join(".venv").join("bin").join("python")
    }
}

fn is_repo_root(path: &Path) -> bool {
    path.join("pyproject.toml").exists() && backend_script(path).exists()
}

fn discover_repo_root_from(start: &Path) -> Option<PathBuf> {
    let start_path = if start.is_file() {
        start.parent()?
    } else {
        start
    };
    for candidate in start_path.ancestors() {
        if is_repo_root(candidate) {
            return Some(candidate.to_path_buf());
        }
    }
    None
}

fn normalize_repo_root(path: &Path) -> Option<PathBuf> {
    let candidate = if path.exists() {
        discover_repo_root_from(path)
    } else {
        None
    }?;
    fs::canonicalize(candidate).ok()
}

fn absolutize_cli_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    let cwd = std::env::current_dir()
        .with_context(|| "Failed to resolve the current working directory".to_string())?;
    Ok(cwd.join(path))
}

fn resolve_repo_root_from_executable() -> Option<PathBuf> {
    let exe_path = std::env::current_exe().ok()?;
    normalize_repo_root(&exe_path)
}

fn resolve_repo_root() -> Result<RepoRootResolution> {
    if let Some(value) = std::env::var_os(PROJECT_ROOT_ENV) {
        let candidate = PathBuf::from(value);
        if let Some(path) = normalize_repo_root(&candidate) {
            return Ok(RepoRootResolution {
                path,
                source: "environment",
            });
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        if let Some(path) = normalize_repo_root(&cwd) {
            return Ok(RepoRootResolution {
                path,
                source: "current directory",
            });
        }
    }

    if let Some(path) = resolve_repo_root_from_executable() {
        return Ok(RepoRootResolution {
            path,
            source: "executable location",
        });
    }

    if let Some(config_path) = default_config_path() {
        if let Some(config) = load_cli_config(&config_path)? {
            if let Some(project_root) = config.project_root {
                if let Some(path) = normalize_repo_root(&project_root) {
                    return Ok(RepoRootResolution {
                        path,
                        source: "config file",
                    });
                }
            }
        }
    }

    let compiled_root = PathBuf::from(COMPILED_REPO_ROOT);
    if let Some(path) = normalize_repo_root(&compiled_root) {
        return Ok(RepoRootResolution {
            path,
            source: "compiled fallback",
        });
    }

    let config_hint = default_config_path()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "~/.config/stata-cli/config.toml".to_string());
    bail!(
        "Could not locate the stata-cli project root. Set {} to the repo path, run the command from inside the repo, or create {} with `project_root = \"/absolute/path/to/stata-cli\"`.",
        PROJECT_ROOT_ENV,
        config_hint
    )
}

fn inspect_python_version(python: &Path) -> Result<String> {
    let output = Command::new(python)
        .args([
            "-c",
            "import sys; print(f'{sys.version_info[0]}.{sys.version_info[1]}')",
        ])
        .output()
        .with_context(|| format!("Failed to inspect Python version for {}", python.display()))?;

    if !output.status.success() {
        bail!("Python version check failed for {}", python.display());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn resolve_python(explicit: Option<&Path>, repo_root: &Path) -> Result<PythonResolution> {
    if let Some(path) = explicit {
        if !path.exists() {
            bail!("Explicit --python path does not exist: {}", path.display());
        }
        let version = inspect_python_version(path)?;
        if version != "3.11" {
            bail!(
                "Explicit --python must point to Python 3.11, but {} is Python {}.",
                path.display(),
                version
            );
        }
        return Ok(PythonResolution {
            path: path.to_path_buf(),
            source: "explicit --python",
            version,
        });
    }

    let candidate = project_python(repo_root);
    if !candidate.exists() {
        bail!(
            "No compatible Python 3.11 interpreter found. This CLI expects the uv-managed project environment at {}. Run `uv sync --all-extras --python 3.11` in {}.",
            candidate.display(),
            repo_root.display()
        );
    }

    let version = inspect_python_version(&candidate)?;
    if version != "3.11" {
        bail!(
            "The uv-managed interpreter at {} is Python {}. Run `uv sync --all-extras --python 3.11` in {}.",
            candidate.display(),
            version,
            repo_root.display()
        );
    }

    Ok(PythonResolution {
        path: candidate,
        source: "project .venv",
        version,
    })
}

fn windows_default_stata_path() -> PathBuf {
    PathBuf::from(r"C:\Program Files\Stata18")
}

fn validate_stata_path(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("Stata path does not exist: {}", path.display());
    }
    if !path.is_dir() {
        bail!("Stata path is not a directory: {}", path.display());
    }
    Ok(())
}

fn prompt_for_stata_path(message: &str) -> Result<Option<PathBuf>> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{message}")?;
    writeln!(
        stdout,
        "Enter your Stata installation directory, or press Enter to cancel:"
    )?;
    write!(stdout, "stata-path> ")?;
    stdout.flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(trimmed)))
}

fn resolve_windows_stata_path_with_prompt<F>(
    cli: &Cli,
    config: Option<&CliConfig>,
    interactive: bool,
    mut prompt: F,
) -> Result<ResolvedStataPath>
where
    F: FnMut(&str) -> Result<Option<PathBuf>>,
{
    if let Some(path) = &cli.stata_path {
        let candidate = PathBuf::from(path);
        validate_stata_path(&candidate).with_context(|| {
            "The --stata-path value is invalid. Pass a valid Windows Stata installation directory."
                .to_string()
        })?;
        return Ok(ResolvedStataPath {
            path: Some(candidate),
            source: Some(StataPathSource::CliFlag),
            save_to_config: false,
        });
    }

    if let Some(value) = std::env::var_os(STATA_PATH_ENV) {
        let candidate = PathBuf::from(value);
        validate_stata_path(&candidate).with_context(|| {
            format!(
                "The {} environment variable points to an invalid Stata directory.",
                STATA_PATH_ENV
            )
        })?;
        return Ok(ResolvedStataPath {
            path: Some(candidate),
            source: Some(StataPathSource::Environment),
            save_to_config: false,
        });
    }

    if let Some(saved_path) = config.and_then(|item| item.stata_path.clone()) {
        match validate_stata_path(&saved_path) {
            Ok(()) => {
                return Ok(ResolvedStataPath {
                    path: Some(saved_path),
                    source: Some(StataPathSource::Config),
                    save_to_config: false,
                })
            }
            Err(error) => {
                if !interactive {
                    bail!(
                        "{}. Update {} or pass --stata-path.",
                        error,
                        default_config_path()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "%APPDATA%\\stata-cli\\config.toml".to_string())
                    );
                }

                let mut prompt_message = format!(
                    "{}\nSaved path came from {}.",
                    error,
                    default_config_path()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "%APPDATA%\\stata-cli\\config.toml".to_string())
                );
                loop {
                    match prompt(&prompt_message)? {
                        Some(candidate) => match validate_stata_path(&candidate) {
                            Ok(()) => {
                                return Ok(ResolvedStataPath {
                                    path: Some(candidate),
                                    source: Some(StataPathSource::Prompt),
                                    save_to_config: true,
                                })
                            }
                            Err(prompt_error) => {
                                prompt_message = format!(
                                    "{}\nPlease enter a valid Windows Stata installation directory.",
                                    prompt_error
                                );
                            }
                        },
                        None => bail!(
                            "Stata path is required on Windows. Pass --stata-path or update {}.",
                            default_config_path()
                                .map(|path| path.display().to_string())
                                .unwrap_or_else(|| "%APPDATA%\\stata-cli\\config.toml".to_string())
                        ),
                    }
                }
            }
        }
    }

    let default_path = windows_default_stata_path();
    match validate_stata_path(&default_path) {
        Ok(()) => Ok(ResolvedStataPath {
            path: Some(default_path),
            source: Some(StataPathSource::Default),
            save_to_config: false,
        }),
        Err(error) => {
            if !interactive {
                bail!(
                    "{}. Pass --stata-path or create {} with `stata_path = \"C:\\\\Path\\\\To\\\\Stata\"`.",
                    error,
                    default_config_path()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "%APPDATA%\\stata-cli\\config.toml".to_string())
                );
            }

            let mut prompt_message =
                format!("{}\nWindows defaults to {}.", error, default_path.display());
            loop {
                match prompt(&prompt_message)? {
                    Some(candidate) => match validate_stata_path(&candidate) {
                        Ok(()) => {
                            return Ok(ResolvedStataPath {
                                path: Some(candidate),
                                source: Some(StataPathSource::Prompt),
                                save_to_config: true,
                            })
                        }
                        Err(prompt_error) => {
                            prompt_message = format!(
                                "{}\nPlease enter a valid Windows Stata installation directory.",
                                prompt_error
                            );
                        }
                    },
                    None => bail!(
                        "Stata path is required on Windows. Pass --stata-path or create {}.",
                        default_config_path()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "%APPDATA%\\stata-cli\\config.toml".to_string())
                    ),
                }
            }
        }
    }
}

fn resolve_effective_stata_path(cli: &Cli) -> Result<ResolvedStataPath> {
    if !cfg!(windows) {
        return Ok(ResolvedStataPath {
            path: cli.stata_path.as_ref().map(PathBuf::from),
            source: cli.stata_path.as_ref().map(|_| StataPathSource::CliFlag),
            save_to_config: false,
        });
    }

    let config = if let Some(path) = default_config_path() {
        load_cli_config(&path)?
    } else {
        None
    };

    let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
    resolve_windows_stata_path_with_prompt(cli, config.as_ref(), interactive, prompt_for_stata_path)
}

fn persist_stata_path_if_needed(
    resolved_stata_path: &ResolvedStataPath,
    result: &ExecutionResult,
) -> Result<()> {
    if resolved_stata_path.save_to_config && result.status == "success" {
        if let Some(path) = &resolved_stata_path.path {
            persist_resolved_stata_path(path)?;
        }
    }
    Ok(())
}

fn persist_stata_path_if_needed_json(
    resolved_stata_path: &ResolvedStataPath,
    payload: &Value,
) -> Result<()> {
    if !resolved_stata_path.save_to_config {
        return Ok(());
    }

    let status = payload
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if status == "success" {
        if let Some(path) = &resolved_stata_path.path {
            persist_resolved_stata_path(path)?;
        }
    }
    Ok(())
}

fn base_backend_cli_args(cli: &Cli, json: bool) -> Vec<OsString> {
    let mut args = Vec::new();
    if let Some(path) = &cli.stata_path {
        args.push(OsString::from("--stata-path"));
        args.push(OsString::from(path));
    }
    if let Some(edition) = &cli.stata_edition {
        args.push(OsString::from("--stata-edition"));
        args.push(OsString::from(edition));
    }
    args.push(OsString::from("--log-level"));
    args.push(OsString::from(cli.log_level.clone()));
    if let Some(mode) = &cli.result_display_mode {
        args.push(OsString::from("--result-display-mode"));
        args.push(OsString::from(mode));
    }
    if let Some(tokens) = cli.max_output_tokens {
        args.push(OsString::from("--max-output-tokens"));
        args.push(OsString::from(tokens.to_string()));
    }
    if cli.multi_session {
        args.push(OsString::from("--multi-session"));
    }
    if cli.no_multi_session {
        args.push(OsString::from("--no-multi-session"));
    }
    if let Some(max_sessions) = cli.max_sessions {
        args.push(OsString::from("--max-sessions"));
        args.push(OsString::from(max_sessions.to_string()));
    }
    if let Some(session_timeout) = cli.session_timeout {
        args.push(OsString::from("--session-timeout"));
        args.push(OsString::from(session_timeout.to_string()));
    }
    if json {
        args.push(OsString::from("--json"));
    }
    args
}

fn base_backend_args(repo_root: &Path, cli: &Cli, json: bool) -> Vec<OsString> {
    let mut args = vec![backend_script(repo_root).into_os_string()];
    args.extend(base_backend_cli_args(cli, json));
    args
}

fn session_args(cli: &Cli) -> Vec<OsString> {
    let mut args = Vec::new();
    if let Some(session_id) = &cli.session_id {
        args.push(OsString::from("--session-id"));
        args.push(OsString::from(session_id));
    }
    if let Some(working_dir) = &cli.working_dir {
        args.push(OsString::from("--working-dir"));
        let resolved_working_dir =
            absolutize_cli_path(working_dir).unwrap_or_else(|_| working_dir.clone());
        args.push(resolved_working_dir.as_os_str().to_os_string());
    }
    args
}

fn data_backend_invocation(command: &DataCommands) -> Result<(&'static str, Vec<OsString>)> {
    match command {
        DataCommands::View {
            session_id,
            if_condition,
            max_rows,
            input_dta,
        } => {
            let mut args = vec![
                OsString::from("view"),
                OsString::from("--max-rows"),
                OsString::from(max_rows.to_string()),
            ];
            if let Some(session_id) = session_id {
                args.push(OsString::from("--session-id"));
                args.push(OsString::from(session_id));
            }
            if let Some(if_condition) = if_condition {
                args.push(OsString::from("--if-condition"));
                args.push(OsString::from(if_condition));
            }
            if let Some(input_dta) = input_dta {
                args.push(OsString::from("--input-dta"));
                args.push(absolutize_cli_path(input_dta)?.as_os_str().to_os_string());
            }
            Ok(("data", args))
        }
        DataCommands::ExportCsv {
            output,
            input_dta,
            session_id,
            working_dir,
            replace,
        } => {
            let mut args = vec![
                OsString::from("export-csv"),
                OsString::from("--output"),
                absolutize_cli_path(output)?.as_os_str().to_os_string(),
            ];
            if let Some(input_dta) = input_dta {
                args.push(OsString::from("--input-dta"));
                args.push(absolutize_cli_path(input_dta)?.as_os_str().to_os_string());
            }
            if let Some(session_id) = session_id {
                args.push(OsString::from("--session-id"));
                args.push(OsString::from(session_id));
            }
            if let Some(working_dir) = working_dir {
                args.push(OsString::from("--working-dir"));
                args.push(absolutize_cli_path(working_dir)?.as_os_str().to_os_string());
            }
            if *replace {
                args.push(OsString::from("--replace"));
            }
            Ok(("data", args))
        }
    }
}

fn invoke_backend(
    python: &Path,
    repo_root: &Path,
    cli: &Cli,
    command: &str,
    command_args: Vec<OsString>,
) -> Result<ExecutionResult> {
    let payload = invoke_backend_json(python, repo_root, cli, command, command_args.clone())?;
    serde_json::from_value::<ExecutionResult>(payload)
        .with_context(|| "Backend returned a non-execution payload".to_string())
}

fn invoke_backend_json(
    python: &Path,
    repo_root: &Path,
    cli: &Cli,
    command: &str,
    mut command_args: Vec<OsString>,
) -> Result<Value> {
    let backend = backend_script(repo_root);
    if !backend.exists() {
        bail!(
            "Python backend not found at {}. Reinstall from the project root or update the CLI config.",
            backend.display()
        );
    }

    let mut args = base_backend_args(repo_root, cli, true);
    args.push(OsString::from(command));
    args.append(&mut command_args);
    if command != "init" {
        args.extend(session_args(cli));
    }

    if let Commands::File { .. } = cli.command {
        if let Some(timeout) = cli.timeout {
            args.push(OsString::from("--timeout"));
            args.push(OsString::from(timeout.to_string()));
        }
    }

    let output = Command::new(python)
        .args(&args)
        .current_dir(repo_root)
        .output()
        .with_context(|| format!("Failed to launch backend with {}", python.display()))?;

    if !output.status.success() && output.stdout.is_empty() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("Backend execution failed: {}", stderr.trim());
    }

    serde_json::from_slice::<Value>(&output.stdout).with_context(|| {
        format!(
            "Backend returned invalid JSON: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn render_result(result: &ExecutionResult, emit_json: bool, quiet: bool) -> Result<()> {
    if emit_json {
        println!("{}", serde_json::to_string_pretty(result)?);
        if result.status != "success" {
            bail!(
                "{}",
                result
                    .error
                    .clone()
                    .unwrap_or_else(|| "stata-cli command failed".to_string())
            );
        }
        return Ok(());
    }

    if !result.output.trim().is_empty() {
        println!("{}", result.output);
    }
    if !quiet && !result.graphs.is_empty() {
        println!("\nGraphs:");
        for graph in &result.graphs {
            println!("- {}", graph.path);
        }
    }
    if !quiet {
        if let Some(log_file) = &result.log_file {
            println!("\nLog file: {}", log_file);
        }
        if let Some(session_id) = &result.session_id {
            println!("Session: {}", session_id);
        }
    }
    if result.status != "success" {
        bail!(
            "{}",
            result
                .error
                .clone()
                .unwrap_or_else(|| "stata-cli command failed".to_string())
        );
    }
    Ok(())
}

fn render_json_payload(payload: &Value, _emit_json: bool) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(payload)?);

    let status = payload
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if matches!(
        status,
        "success" | "running" | "idle" | "stop_sent" | "stop_requested" | "not_running"
    ) {
        return Ok(());
    }

    let message = payload
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| payload.get("error").and_then(Value::as_str))
        .unwrap_or("stata-cli command failed");
    bail!("{message}")
}

fn repl_command(cli: &Cli) -> Result<()> {
    let status = if let Some(python) = cli.python.as_deref() {
        spawn_repl_via_module(python, cli)?
    } else if backend_command_available("stata-cli-backend") {
        spawn_repl_via_backend_command("stata-cli-backend", cli)?
    } else if let Ok(repo_root) = resolve_repo_root() {
        let python = resolve_python(cli.python.as_deref(), &repo_root.path)?;
        spawn_repl(&python.path, &repo_root.path, cli)?
    } else {
        bail!(
            "Could not start repl from the current directory. Activate an environment that provides `stata-cli-backend`, pass `--python` to a Python 3.11 interpreter with `stata_cli_backend` installed, or configure {}.",
            PROJECT_ROOT_ENV
        );
    };

    if !status.success() {
        bail!("stata-cli repl exited with status {}", status);
    }
    Ok(())
}

fn backend_command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}

fn spawn_repl(python: &Path, repo_root: &Path, cli: &Cli) -> Result<ExitStatus> {
    let backend = backend_script(repo_root);
    if !backend.exists() {
        bail!("Python backend not found at {}", backend.display());
    }

    let mut args = base_backend_args(repo_root, cli, false);
    args.push(OsString::from("repl"));
    args.extend(session_args(cli));

    Command::new(python)
        .args(&args)
        .current_dir(repo_root)
        .env("STATA_CLI_REPL_MODE", "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| "Failed to launch interactive backend".to_string())
}

fn spawn_repl_via_module(python: &Path, cli: &Cli) -> Result<ExitStatus> {
    let mut args = vec![OsString::from("-m"), OsString::from("stata_cli_backend")];
    args.extend(base_backend_cli_args(cli, false));
    args.push(OsString::from("repl"));
    args.extend(session_args(cli));

    Command::new(python)
        .args(&args)
        .env("STATA_CLI_REPL_MODE", "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to launch repl with {}", python.display()))
}

fn spawn_repl_via_backend_command(command: &str, cli: &Cli) -> Result<ExitStatus> {
    let mut args = base_backend_cli_args(cli, false);
    args.push(OsString::from("repl"));
    args.extend(session_args(cli));

    Command::new(command)
        .args(&args)
        .env("STATA_CLI_REPL_MODE", "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| format!("Failed to launch repl with `{command}`"))
}

fn doctor_command(
    cli: &Cli,
    repo_root: &RepoRootResolution,
    resolved_stata_path: &ResolvedStataPath,
) -> Result<()> {
    let config_path = default_config_path();
    let backend = backend_script(&repo_root.path);
    let mut checks = Vec::new();

    checks.push(DoctorCheck {
        name: "repo_root",
        status: "ok",
        detail: format!(
            "{} (source: {})",
            repo_root.path.display(),
            repo_root.source
        ),
    });

    match &config_path {
        Some(path) if path.exists() => checks.push(DoctorCheck {
            name: "config_file",
            status: "ok",
            detail: format!("Config file found at {}", path.display()),
        }),
        Some(path) => checks.push(DoctorCheck {
            name: "config_file",
            status: "warn",
            detail: format!(
                "No config file at {}. Optional, but useful if the repo is ever moved.",
                path.display()
            ),
        }),
        None => checks.push(DoctorCheck {
            name: "config_file",
            status: "warn",
            detail: "Could not determine a home directory for the optional config file."
                .to_string(),
        }),
    }

    if backend.exists() {
        checks.push(DoctorCheck {
            name: "backend_script",
            status: "ok",
            detail: format!("Found {}", backend.display()),
        });
    } else {
        checks.push(DoctorCheck {
            name: "backend_script",
            status: "error",
            detail: format!("Missing {}", backend.display()),
        });
    }

    if cfg!(windows) {
        match (&resolved_stata_path.path, resolved_stata_path.source) {
            (Some(path), Some(source)) => checks.push(DoctorCheck {
                name: "stata_path",
                status: "ok",
                detail: format!("{} (source: {:?})", path.display(), source),
            }),
            _ => checks.push(DoctorCheck {
                name: "stata_path",
                status: "error",
                detail: "Windows requires a valid Stata installation directory.".to_string(),
            }),
        }
    }

    let python_resolution = match resolve_python(cli.python.as_deref(), &repo_root.path) {
        Ok(resolution) => {
            checks.push(DoctorCheck {
                name: "python",
                status: "ok",
                detail: format!(
                    "{} (source: {}, version: {})",
                    resolution.path.display(),
                    resolution.source,
                    resolution.version
                ),
            });
            Some(resolution)
        }
        Err(error) => {
            checks.push(DoctorCheck {
                name: "python",
                status: "error",
                detail: error.to_string(),
            });
            None
        }
    };

    if let Some(python) = python_resolution {
        match invoke_backend(
            &python.path,
            &repo_root.path,
            cli,
            "run",
            vec![OsString::from("--code"), OsString::from("display 1+1")],
        ) {
            Ok(result) if result.status == "success" => {
                persist_stata_path_if_needed(resolved_stata_path, &result)?;
                checks.push(DoctorCheck {
                    name: "backend_probe",
                    status: "ok",
                    detail: "Backend successfully executed `display 1+1`.".to_string(),
                });
            }
            Ok(result) => checks.push(DoctorCheck {
                name: "backend_probe",
                status: "error",
                detail: result
                    .error
                    .unwrap_or_else(|| "Backend probe failed.".to_string()),
            }),
            Err(error) => checks.push(DoctorCheck {
                name: "backend_probe",
                status: "error",
                detail: error.to_string(),
            }),
        }
    }

    let report_status = if checks.iter().any(|check| check.status == "error") {
        "error"
    } else {
        "ok"
    };
    let report = DoctorReport {
        status: report_status,
        checks,
    };

    if cli.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
    } else {
        for check in &report.checks {
            let label = match check.status {
                "ok" => "ok",
                "warn" => "warn",
                _ => "error",
            };
            println!("[{}] {}: {}", label, check.name, check.detail);
        }
        if report.status == "error" {
            println!("\nDoctor found one or more blocking issues.");
        } else {
            println!("\nDoctor checks completed successfully.");
        }
    }

    if report.status == "error" {
        bail!("stata-cli doctor found one or more blocking issues");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt;

    fn make_repo(dir: &Path) {
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("pyproject.toml"),
            "[project]\nname = 'stata-cli'\n",
        )
        .unwrap();
        fs::write(
            dir.join("src").join("stata_cli_backend.py"),
            "print('ok')\n",
        )
        .unwrap();
    }

    fn windows_like_cli() -> Cli {
        Cli::parse_from(["stata-cli", "doctor"])
    }

    #[test]
    fn parse_run_command() {
        let cli = Cli::parse_from(["stata-cli", "--json", "run", "--code", "display 1+1"]);

        assert!(cli.json);
        match cli.command {
            Commands::Run { code } => assert_eq!(code, "display 1+1"),
            _ => panic!("expected run command"),
        }
    }

    #[test]
    fn parse_doctor_command() {
        let cli = Cli::parse_from(["stata-cli", "doctor"]);
        match cli.command {
            Commands::Doctor => {}
            _ => panic!("expected doctor command"),
        }
    }

    #[test]
    fn parse_init_command() {
        let cli = Cli::parse_from(["stata-cli", "init", "./my-analysis"]);
        match cli.command {
            Commands::Init { target_dir } => assert_eq!(target_dir, PathBuf::from("./my-analysis")),
            _ => panic!("expected init command"),
        }
    }

    #[test]
    fn parse_data_export_command() {
        let temp = std::env::temp_dir();
        let output_path = temp.join("out.csv");
        let input_path = temp.join("input.dta");
        let cli = Cli::parse_from([
            "stata-cli",
            "data",
            "export-csv",
            "--output",
            output_path.to_string_lossy().as_ref(),
            "--input-dta",
            input_path.to_string_lossy().as_ref(),
            "--replace",
        ]);

        match cli.command {
            Commands::Data {
                command:
                    DataCommands::ExportCsv {
                        output,
                        input_dta,
                        replace,
                        ..
                    },
            } => {
                assert_eq!(output, output_path);
                assert_eq!(input_dta, Some(input_path));
                assert!(replace);
            }
            _ => panic!("expected data export-csv command"),
        }
    }

    #[test]
    fn parse_file_command_with_globals() {
        let cli = Cli::parse_from([
            "stata-cli",
            "--stata-path",
            "/Applications/Stata",
            "file",
            "tests/fixtures/test_stata.do",
            "--timeout",
            "60",
            "--working-dir",
            "/tmp",
        ]);

        assert_eq!(cli.stata_path.as_deref(), Some("/Applications/Stata"));
        match cli.command {
            Commands::File {
                path,
                timeout,
                working_dir,
                ..
            } => {
                assert_eq!(path, PathBuf::from("tests/fixtures/test_stata.do"));
                assert_eq!(timeout, Some(60));
                assert_eq!(working_dir, Some(PathBuf::from("/tmp")));
            }
            _ => panic!("expected file command"),
        }
    }

    #[test]
    fn parse_data_view_uses_agent_friendly_default() {
        let cli = Cli::parse_from(["stata-cli", "data", "view"]);
        match cli.command {
            Commands::Data {
                command: DataCommands::View { max_rows, .. },
            } => assert_eq!(max_rows, 50),
            _ => panic!("expected data view command"),
        }
    }

    #[test]
    fn base_backend_cli_args_excludes_backend_script_path() {
        let cli = Cli::parse_from([
            "stata-cli",
            "--stata-path",
            "/Applications/Stata",
            "--log-level",
            "INFO",
            "doctor",
        ]);

        let args = base_backend_cli_args(&cli, true);
        assert!(args.iter().any(|arg| arg == "--stata-path"));
        assert!(args.iter().any(|arg| arg == "--json"));
        assert!(!args
            .iter()
            .any(|arg| arg.to_string_lossy().contains("stata_cli_backend.py")));
    }

    #[test]
    fn discover_repo_root_from_ancestor_directory() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("stata-cli");
        make_repo(&repo);
        let nested = repo.join("rust-cli").join("src");
        fs::create_dir_all(&nested).unwrap();

        let discovered = discover_repo_root_from(&nested).unwrap();
        assert_eq!(discovered, repo);
    }

    #[test]
    fn executable_path_inside_repo_bin_discovers_repo_root() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("stata-cli");
        make_repo(&repo);
        let bin = repo.join("bin");
        fs::create_dir_all(&bin).unwrap();
        let fake_exe = if cfg!(windows) {
            bin.join("stata-cli.exe")
        } else {
            bin.join("stata-cli")
        };
        fs::write(&fake_exe, "placeholder").unwrap();

        let discovered = normalize_repo_root(&fake_exe).unwrap();
        assert_eq!(discovered, fs::canonicalize(repo).unwrap());
    }

    #[test]
    fn load_cli_config_reads_project_root_and_stata_path() {
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("config.toml");
        let project_root = temp.path().join("project");
        let stata_path = temp.path().join("Stata18");
        fs::write(
            &config_path,
            format!(
                "project_root = {:?}\nstata_path = {:?}\n",
                project_root.to_string_lossy(),
                stata_path.to_string_lossy()
            ),
        )
        .unwrap();

        let config = load_cli_config(&config_path).unwrap().unwrap();
        assert_eq!(config.project_root, Some(project_root));
        assert_eq!(config.stata_path, Some(stata_path));
    }

    #[test]
    fn write_cli_config_preserves_fields() {
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("config.toml");
        let config = CliConfig {
            project_root: Some(temp.path().join("repo")),
            stata_path: Some(temp.path().join("Stata18")),
        };

        write_cli_config(&config_path, &config).unwrap();
        let reloaded = load_cli_config(&config_path).unwrap().unwrap();
        assert_eq!(reloaded.project_root, config.project_root);
        assert_eq!(reloaded.stata_path, config.stata_path);
    }

    #[test]
    fn repo_root_override_wins() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        make_repo(&repo);

        std::env::set_var(PROJECT_ROOT_ENV, &repo);
        let resolved = resolve_repo_root().unwrap();
        std::env::remove_var(PROJECT_ROOT_ENV);

        assert_eq!(resolved.path, fs::canonicalize(repo).unwrap());
        assert_eq!(resolved.source, "environment");
    }

    #[test]
    fn inspect_python_version_accepts_mock_interpreter() {
        let dir = tempdir().unwrap();
        let script = if cfg!(windows) {
            dir.path().join("python311-mock.cmd")
        } else {
            dir.path().join("python311-mock")
        };

        if cfg!(windows) {
            fs::write(
                &script,
                "@echo off\r\nif \"%1\"==\"-c\" (\r\n  echo 3.11\r\n) else (\r\n  echo Python 3.11.0\r\n)\r\n",
            )
            .unwrap();
        } else {
            fs::write(
                &script,
                "#!/bin/sh\nif [ \"$1\" = \"-c\" ]; then\n  echo 3.11\nelse\n  echo Python 3.11.0\nfi\n",
            )
            .unwrap();
            #[cfg(unix)]
            {
                let mut perms = fs::metadata(&script).unwrap().permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&script, perms).unwrap();
            }
        }

        assert_eq!(inspect_python_version(&script).unwrap(), "3.11");
    }

    #[test]
    fn resolve_python_uses_uv_managed_environment() {
        let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("rust-cli should live under the repo root");
        let expected = project_python(repo);

        let resolved = resolve_python(None, repo).unwrap();
        assert_eq!(resolved.path, expected);
        assert_eq!(resolved.source, "project .venv");
        assert_eq!(resolved.version, "3.11");
    }

    #[test]
    fn resolve_python_errors_when_uv_environment_is_missing() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("repo");
        make_repo(&repo);

        let error = resolve_python(None, &repo).unwrap_err().to_string();
        assert!(error.contains("uv sync --all-extras --python 3.11"));
    }

    #[test]
    fn session_args_absolutizes_working_dir() {
        let cli = Cli::parse_from(["stata-cli", "--working-dir", ".", "doctor"]);

        let args = session_args(&cli);
        let working_dir = args
            .windows(2)
            .find(|pair| pair[0] == "--working-dir")
            .map(|pair| PathBuf::from(&pair[1]))
            .unwrap();

        assert_eq!(working_dir, std::env::current_dir().unwrap());
    }

    #[test]
    fn data_backend_invocation_absolutizes_relative_paths() {
        let cwd = std::env::current_dir().unwrap();
        let command = DataCommands::ExportCsv {
            output: PathBuf::from("scene/export.csv"),
            input_dta: Some(PathBuf::from("scene/grilic.dta")),
            session_id: Some("abc".to_string()),
            working_dir: Some(PathBuf::from(".")),
            replace: true,
        };

        let (_, args) = data_backend_invocation(&command).unwrap();
        let rendered: Vec<PathBuf> = args
            .iter()
            .filter(|arg| !arg.to_string_lossy().starts_with("--"))
            .map(PathBuf::from)
            .collect();

        assert!(rendered.contains(&cwd.join("scene/export.csv")));
        assert!(rendered.contains(&cwd.join("scene/grilic.dta")));
        assert!(rendered.contains(&cwd));
    }

    #[test]
    fn absolutize_cli_path_uses_process_cwd_for_relative_input() {
        let cwd = std::env::current_dir().unwrap();
        assert_eq!(
            absolutize_cli_path(Path::new("scene/smoke_test.do")).unwrap(),
            cwd.join("scene/smoke_test.do")
        );
    }

    #[test]
    fn windows_config_path_uses_appdata() {
        if !cfg!(windows) {
            return;
        }
        let temp = tempdir().unwrap();
        std::env::set_var("APPDATA", temp.path());
        let path = default_config_path().unwrap();
        std::env::remove_var("APPDATA");
        assert_eq!(path, temp.path().join("stata-cli").join("config.toml"));
    }

    #[test]
    fn resolve_windows_stata_path_prefers_cli_flag() {
        if !cfg!(windows) {
            return;
        }
        let temp = tempdir().unwrap();
        let stata_path = temp.path().join("Stata18");
        fs::create_dir_all(&stata_path).unwrap();
        let cli = Cli::parse_from([
            "stata-cli",
            "--stata-path",
            stata_path.to_string_lossy().as_ref(),
            "doctor",
        ]);

        let resolved = resolve_windows_stata_path_with_prompt(&cli, None, false, |_| {
            panic!("should not prompt")
        })
        .unwrap();

        assert_eq!(resolved.path, Some(stata_path));
        assert_eq!(resolved.source, Some(StataPathSource::CliFlag));
        assert!(!resolved.save_to_config);
    }

    #[test]
    fn resolve_windows_stata_path_uses_saved_config() {
        if !cfg!(windows) {
            return;
        }
        let temp = tempdir().unwrap();
        let stata_path = temp.path().join("Stata18");
        fs::create_dir_all(&stata_path).unwrap();
        let config = CliConfig {
            project_root: None,
            stata_path: Some(stata_path.clone()),
        };

        let resolved = resolve_windows_stata_path_with_prompt(
            &windows_like_cli(),
            Some(&config),
            false,
            |_| panic!("should not prompt"),
        )
        .unwrap();

        assert_eq!(resolved.path, Some(stata_path));
        assert_eq!(resolved.source, Some(StataPathSource::Config));
        assert!(!resolved.save_to_config);
    }

    #[test]
    fn resolve_windows_stata_path_errors_non_interactive_when_saved_path_missing() {
        if !cfg!(windows) {
            return;
        }
        let temp = tempdir().unwrap();
        let config = CliConfig {
            project_root: None,
            stata_path: Some(temp.path().join("MissingStata")),
        };
        let error = resolve_windows_stata_path_with_prompt(
            &windows_like_cli(),
            Some(&config),
            false,
            |_| panic!("should not prompt"),
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("Update"));
    }

    #[test]
    fn resolve_windows_stata_path_prompts_and_marks_for_save() {
        if !cfg!(windows) {
            return;
        }
        let temp = tempdir().unwrap();
        let saved_invalid = temp.path().join("MissingStata");
        let prompted = temp.path().join("PromptedStata");
        fs::create_dir_all(&prompted).unwrap();
        let config = CliConfig {
            project_root: None,
            stata_path: Some(saved_invalid),
        };
        let mut prompt_count = 0usize;

        let resolved = resolve_windows_stata_path_with_prompt(
            &windows_like_cli(),
            Some(&config),
            true,
            |_| {
                prompt_count += 1;
                Ok(Some(prompted.clone()))
            },
        )
        .unwrap();

        assert_eq!(prompt_count, 1);
        assert_eq!(resolved.path, Some(prompted));
        assert_eq!(resolved.source, Some(StataPathSource::Prompt));
        assert!(resolved.save_to_config);
    }
}
