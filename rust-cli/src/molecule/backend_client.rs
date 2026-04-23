use crate::atom::cli_contract::{Cli, DataCommands};
use crate::atom::json_contract::ExecutionResult;
use crate::atom::path_ops::{absolutize_cli_path, backend_entry};
use crate::atom::process_runner::configure_pythonpath;
use crate::atom::progress_feedback::{backend_heartbeat_message, heartbeat_interval};
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::ffi::OsString;
use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Output, Stdio};
use std::thread;
use std::time::Duration;

pub(crate) fn base_backend_cli_args(
    cli: &Cli,
    include_json: bool,
    raw_output: bool,
) -> Vec<OsString> {
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
    if include_json {
        args.push(OsString::from("--json"));
    }
    if raw_output {
        args.push(OsString::from("--raw-output"));
    }
    args
}

pub(crate) fn base_backend_args(cli: &Cli, include_json: bool, raw_output: bool) -> Vec<OsString> {
    let mut args = vec![
        OsString::from("-m"),
        OsString::from("stata_cli.entry.backend_main"),
    ];
    args.extend(base_backend_cli_args(cli, include_json, raw_output));
    args
}

pub(crate) fn session_args(cli: &Cli) -> Vec<OsString> {
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

pub(crate) fn data_backend_invocation(
    command: &DataCommands,
) -> Result<(&'static str, Vec<OsString>)> {
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

pub(crate) fn invoke_backend(
    python: &Path,
    repo_root: &Path,
    cli: &Cli,
    command: &str,
    command_args: Vec<OsString>,
) -> Result<ExecutionResult> {
    let payload = invoke_backend_json(python, repo_root, cli, command, command_args)?;
    serde_json::from_value::<ExecutionResult>(payload)
        .with_context(|| "Backend returned a non-execution payload".to_string())
}

pub(crate) fn invoke_backend_json(
    python: &Path,
    repo_root: &Path,
    cli: &Cli,
    command_name: &str,
    mut command_args: Vec<OsString>,
) -> Result<Value> {
    let backend = backend_entry(repo_root);
    if !backend.exists() {
        bail!(
            "Python backend entrypoint not found at {}. Reinstall from the project root or update the CLI config.",
            backend.display()
        );
    }

    let mut args = base_backend_args(cli, true, true);
    args.push(OsString::from(command_name));
    args.append(&mut command_args);
    args.extend(session_args(cli));

    if let crate::atom::cli_contract::Commands::File { .. } = cli.command {
        if let Some(timeout) = cli.timeout {
            args.push(OsString::from("--timeout"));
            args.push(OsString::from(timeout.to_string()));
        }
    }

    let mut command = Command::new(python);
    command.args(&args).current_dir(repo_root);
    configure_pythonpath(&mut command, repo_root);
    let output = command_output_with_heartbeat(&mut command)
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

fn command_output_with_heartbeat(command: &mut Command) -> Result<Output> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .with_context(|| "Failed to launch child process".to_string())?;
    let mut stdout = child
        .stdout
        .take()
        .context("Backend stdout was not available")?;
    let mut stderr = child
        .stderr
        .take()
        .context("Backend stderr was not available")?;

    let stdout_reader = thread::spawn(move || {
        let mut buffer = Vec::new();
        stdout.read_to_end(&mut buffer).map(|_| buffer)
    });
    let stderr_reader = thread::spawn(move || {
        let mut buffer = Vec::new();
        stderr.read_to_end(&mut buffer).map(|_| buffer)
    });

    let started = std::time::Instant::now();
    let heartbeat = heartbeat_interval();
    let mut next_heartbeat = heartbeat;
    loop {
        if let Some(status) = child.try_wait()? {
            let stdout = stdout_reader
                .join()
                .map_err(|_| anyhow::anyhow!("Backend stdout reader panicked"))??;
            let stderr = stderr_reader
                .join()
                .map_err(|_| anyhow::anyhow!("Backend stderr reader panicked"))??;
            return Ok(Output {
                status,
                stdout,
                stderr,
            });
        }

        let elapsed = started.elapsed();
        if elapsed >= next_heartbeat {
            eprintln!("{}", backend_heartbeat_message(elapsed));
            next_heartbeat += heartbeat;
        }
        thread::sleep(Duration::from_millis(100));
    }
}

pub(crate) fn spawn_bridge_with_project_python(
    python: &Path,
    repo_root: &Path,
    cli: &Cli,
) -> Result<Child> {
    let backend = backend_entry(repo_root);
    if !backend.exists() {
        bail!("Python backend not found at {}", backend.display());
    }

    let mut args = base_backend_args(cli, false, true);
    args.push(OsString::from("bridge"));
    args.extend(session_args(cli));

    let mut command = Command::new(python);
    command
        .args(&args)
        .current_dir(repo_root)
        .env("STATA_CLI_REPL_MODE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    configure_pythonpath(&mut command, repo_root);
    command
        .spawn()
        .with_context(|| "Failed to launch interactive backend bridge".to_string())
}

pub(crate) fn spawn_bridge_via_module(python: &Path, cli: &Cli) -> Result<Child> {
    let mut args = vec![OsString::from("-m"), OsString::from("stata_cli_backend")];
    args.extend(base_backend_cli_args(cli, false, true));
    args.push(OsString::from("bridge"));
    args.extend(session_args(cli));

    Command::new(python)
        .args(&args)
        .env("STATA_CLI_REPL_MODE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("Failed to launch repl bridge with {}", python.display()))
}

pub(crate) fn spawn_bridge_via_backend_command(command_name: &str, cli: &Cli) -> Result<Child> {
    let mut args = base_backend_cli_args(cli, false, true);
    args.push(OsString::from("bridge"));
    args.extend(session_args(cli));

    Command::new(command_name)
        .args(&args)
        .env("STATA_CLI_REPL_MODE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .with_context(|| format!("Failed to launch repl bridge with `{command_name}`"))
}

#[cfg(test)]
pub(crate) fn project_python_for_tests(repo_root: &Path) -> std::path::PathBuf {
    crate::atom::path_ops::project_python(repo_root)
}
