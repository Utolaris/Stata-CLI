use anyhow::{bail, Context, Result};
use clap::{Parser, Subcommand};
use serde::{Deserialize, Serialize};
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

const COMPILED_REPO_ROOT: &str = env!("STATACLI_REPO_ROOT");
const PROJECT_ROOT_ENV: &str = "STATA_CLI_PROJECT_ROOT";

#[derive(Parser, Debug)]
#[command(name = "stata-cli")]
#[command(about = "A local Rust CLI wrapper for the Python/PyStata backend")]
struct Cli {
    #[arg(long)]
    stata_path: Option<String>,
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
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Run {
        #[arg(long)]
        code: String,
    },
    File {
        path: PathBuf,
    },
    Repl,
    Doctor,
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

#[derive(Debug, Deserialize, Serialize)]
struct CliConfig {
    project_root: Option<PathBuf>,
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

fn main() -> Result<()> {
    let cli = Cli::parse();
    let repo_root = resolve_repo_root()?;

    match &cli.command {
        Commands::Doctor => doctor_command(&cli, &repo_root),
        Commands::Run { code } => {
            let python = resolve_python(cli.python.as_deref(), &repo_root.path)?;
            let mut command_args = vec![OsString::from("--code"), OsString::from(code)];
            if let Some(timeout) = cli.timeout {
                command_args.push(OsString::from("--timeout"));
                command_args.push(OsString::from(timeout.to_string()));
            }
            let result = invoke_backend(&python.path, &repo_root.path, &cli, "run", command_args)?;
            render_result(&result, cli.json, cli.quiet)?;
            Ok(())
        }
        Commands::File { path } => {
            let python = resolve_python(cli.python.as_deref(), &repo_root.path)?;
            let result = invoke_backend(
                &python.path,
                &repo_root.path,
                &cli,
                "file",
                vec![path.as_os_str().to_os_string()],
            )?;
            render_result(&result, cli.json, cli.quiet)?;
            Ok(())
        }
        Commands::Repl => {
            let python = resolve_python(cli.python.as_deref(), &repo_root.path)?;
            let status = spawn_repl(&python.path, &repo_root.path, &cli)?;
            if !status.success() {
                bail!("stata-cli repl exited with status {}", status);
            }
            Ok(())
        }
    }
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

fn default_config_path() -> Option<PathBuf> {
    home_dir().map(|home| home.join(".config").join("stata-cli").join("config.toml"))
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
        "Could not locate the stata-mcp project root. Set {} to the repo path, run the command from inside the repo, or create {} with `project_root = \"/absolute/path/to/stata-mcp\"`.",
        PROJECT_ROOT_ENV,
        config_hint
    )
}

fn is_candidate_available(candidate: &Path) -> bool {
    if candidate.components().count() > 1 {
        return candidate.exists();
    }
    Command::new(candidate)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
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
    let mut candidates: Vec<(PathBuf, &'static str)> = Vec::new();
    if let Some(path) = explicit {
        candidates.push((path.to_path_buf(), "explicit --python"));
    }
    candidates.push((project_python(repo_root), "project .venv"));
    candidates.push((PathBuf::from("python3"), "PATH python3"));

    let mut failures: Vec<String> = Vec::new();
    for (candidate, source) in candidates {
        if candidate.as_os_str().is_empty() {
            continue;
        }
        if !is_candidate_available(&candidate) {
            failures.push(format!("{source}: not found"));
            continue;
        }
        match inspect_python_version(&candidate) {
            Ok(version) if version == "3.11" => {
                return Ok(PythonResolution {
                    path: candidate,
                    source,
                    version,
                });
            }
            Ok(version) => failures.push(format!("{source}: found Python {version}")),
            Err(error) => failures.push(format!("{source}: {error}")),
        }
    }

    bail!(
        "No compatible Python 3.11 interpreter found. Try --python, or run `uv sync --all-extras --python 3.11` in {}. Checked: {}",
        repo_root.display(),
        failures.join("; ")
    )
}

fn base_backend_args(repo_root: &Path, cli: &Cli, json: bool) -> Vec<OsString> {
    let mut args = vec![OsString::from(backend_script(repo_root))];

    if let Some(path) = &cli.stata_path {
        args.push(OsString::from("--stata-path"));
        args.push(OsString::from(path));
    }
    args.push(OsString::from("--log-level"));
    args.push(OsString::from(cli.log_level.clone()));
    if json {
        args.push(OsString::from("--json"));
    }
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
        args.push(working_dir.as_os_str().to_os_string());
    }
    args
}

fn invoke_backend(
    python: &Path,
    repo_root: &Path,
    cli: &Cli,
    command: &str,
    mut command_args: Vec<OsString>,
) -> Result<ExecutionResult> {
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
    args.extend(session_args(cli));

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

    serde_json::from_slice::<ExecutionResult>(&output.stdout).with_context(|| {
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
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| "Failed to launch interactive backend".to_string())
}

fn doctor_command(cli: &Cli, repo_root: &RepoRootResolution) -> Result<()> {
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
            Ok(result) if result.status == "success" => checks.push(DoctorCheck {
                name: "backend_probe",
                status: "ok",
                detail: "Backend successfully executed `display 1+1`.".to_string(),
            }),
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
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    fn make_repo(dir: &Path) {
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("pyproject.toml"),
            "[project]\nname = 'stata-mcp'\n",
        )
        .unwrap();
        fs::write(
            dir.join("src").join("stata_cli_backend.py"),
            "print('ok')\n",
        )
        .unwrap();
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
    fn parse_file_command_with_globals() {
        let cli = Cli::parse_from([
            "stata-cli",
            "--stata-path",
            "/Applications/Stata",
            "--timeout",
            "60",
            "file",
            "tests/fixtures/test_stata.do",
        ]);

        assert_eq!(cli.stata_path.as_deref(), Some("/Applications/Stata"));
        assert_eq!(cli.timeout, Some(60));
        match cli.command {
            Commands::File { path } => {
                assert_eq!(path, PathBuf::from("tests/fixtures/test_stata.do"))
            }
            _ => panic!("expected file command"),
        }
    }

    #[test]
    fn discover_repo_root_from_ancestor_directory() {
        let temp = tempdir().unwrap();
        let repo = temp.path().join("stata-mcp");
        make_repo(&repo);
        let nested = repo.join("rust-cli").join("src");
        fs::create_dir_all(&nested).unwrap();

        let discovered = discover_repo_root_from(&nested).unwrap();
        assert_eq!(discovered, repo);
    }

    #[test]
    fn load_cli_config_reads_project_root() {
        let temp = tempdir().unwrap();
        let config_path = temp.path().join("config.toml");
        fs::write(&config_path, "project_root = \"/tmp/stata-mcp\"\n").unwrap();

        let config = load_cli_config(&config_path).unwrap().unwrap();
        assert_eq!(config.project_root, Some(PathBuf::from("/tmp/stata-mcp")));
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
        let script = dir.path().join("python3.11-mock");
        fs::write(
            &script,
            "#!/bin/sh\nif [ \"$1\" = \"-c\" ]; then\n  echo 3.11\nelse\n  echo Python 3.11.0\nfi\n",
        )
        .unwrap();
        let mut perms = fs::metadata(&script).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).unwrap();

        assert_eq!(inspect_python_version(&script).unwrap(), "3.11");
    }
}
