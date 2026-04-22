use crate::atom::cli_contract::{Cli, Commands};
use crate::atom::json_contract::{DoctorCheck, ResolvedStataPath};
use crate::atom::path_ops::{absolutize_cli_path, backend_entry, default_config_path};
use crate::molecule::backend_client::{
    data_backend_invocation, invoke_backend, invoke_backend_json, render_json_payload,
    render_result,
};
use crate::molecule::doctor_report::{
    backend_entry_check, backend_probe_ok_check, config_file_check, error_check, finalize_report,
    python_ok_check, repo_root_check, stata_path_check,
};
use crate::molecule::repl_launch::repl_command;
use crate::molecule::repo_resolution::{resolve_python, resolve_repo_root};
use crate::molecule::stata_path_resolution::{
    clone_with_effective_stata_path, persist_stata_path_if_needed,
    persist_stata_path_if_needed_json, resolve_effective_stata_path,
};
use anyhow::{bail, Result};
use clap::Parser;
use std::ffi::OsString;

pub(crate) fn run() -> Result<()> {
    let cli = Cli::parse();
    let resolved_stata_path = resolve_effective_stata_path(&cli)?;
    let effective_cli = clone_with_effective_stata_path(&cli, &resolved_stata_path);

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
            render_result(&result)
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
            render_result(&result)
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
            render_json_payload(&payload)
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
            render_json_payload(&payload)
        }
    }
}

fn doctor_command(
    cli: &Cli,
    repo_root: &crate::atom::json_contract::RepoRootResolution,
    resolved_stata_path: &ResolvedStataPath,
) -> Result<()> {
    let config_path = default_config_path();
    let backend = backend_entry(&repo_root.path);
    let mut checks: Vec<DoctorCheck> = vec![
        repo_root_check(repo_root),
        config_file_check(config_path.as_deref()),
        backend_entry_check(&backend),
    ];

    if cfg!(windows) {
        checks.push(stata_path_check(resolved_stata_path));
    }

    let python_resolution = match resolve_python(cli.python.as_deref(), &repo_root.path) {
        Ok(resolution) => {
            checks.push(python_ok_check(&resolution));
            Some(resolution)
        }
        Err(error) => {
            checks.push(error_check("python", error.to_string()));
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
                checks.push(backend_probe_ok_check());
            }
            Ok(result) => checks.push(error_check(
                "backend_probe",
                result
                    .error
                    .unwrap_or_else(|| "Backend probe failed.".to_string()),
            )),
            Err(error) => checks.push(error_check("backend_probe", error.to_string())),
        }
    }

    let report = finalize_report(checks);
    println!("{}", serde_json::to_string_pretty(&report)?);

    if report.status == "error" {
        bail!("stata-cli doctor found one or more blocking issues");
    }
    Ok(())
}
