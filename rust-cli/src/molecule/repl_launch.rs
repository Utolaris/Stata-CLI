use crate::atom::cli_contract::Cli;
use crate::atom::path_ops::backend_entry;
use crate::atom::process_runner::{
    backend_command_available, command_status, configure_pythonpath,
};
use crate::molecule::backend_client::{base_backend_args, base_backend_cli_args, session_args};
use crate::molecule::repo_resolution::{resolve_python, resolve_repo_root, PROJECT_ROOT_ENV};
use anyhow::{bail, Context, Result};
use std::ffi::OsString;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

pub(crate) fn repl_command(cli: &Cli) -> Result<()> {
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

pub(crate) fn spawn_repl(python: &Path, repo_root: &Path, cli: &Cli) -> Result<ExitStatus> {
    let backend = backend_entry(repo_root);
    if !backend.exists() {
        bail!("Python backend not found at {}", backend.display());
    }

    let mut args = base_backend_args(cli, false);
    args.push(OsString::from("repl"));
    args.extend(session_args(cli));

    let mut command = Command::new(python);
    command
        .args(&args)
        .current_dir(repo_root)
        .env("STATA_CLI_REPL_MODE", "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    configure_pythonpath(&mut command, repo_root);
    command_status(&mut command).with_context(|| "Failed to launch interactive backend".to_string())
}

pub(crate) fn spawn_repl_via_module(python: &Path, cli: &Cli) -> Result<ExitStatus> {
    let mut args = vec![OsString::from("-m"), OsString::from("stata_cli_backend")];
    args.extend(base_backend_cli_args(cli, false));
    args.push(OsString::from("repl"));
    args.extend(session_args(cli));

    let mut command = Command::new(python);
    command
        .args(&args)
        .env("STATA_CLI_REPL_MODE", "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    command_status(&mut command)
        .with_context(|| format!("Failed to launch repl with {}", python.display()))
}

pub(crate) fn spawn_repl_via_backend_command(command_name: &str, cli: &Cli) -> Result<ExitStatus> {
    let mut args = base_backend_cli_args(cli, false);
    args.push(OsString::from("repl"));
    args.extend(session_args(cli));

    let mut command = Command::new(command_name);
    command
        .args(&args)
        .env("STATA_CLI_REPL_MODE", "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    command_status(&mut command)
        .with_context(|| format!("Failed to launch repl with `{command_name}`"))
}
