use crate::atom::cli_contract::{Cli, Commands};
use crate::atom::do_file_scan::{confirm_gui_command_execution, scan_do_file_for_gui_commands};
use crate::atom::json_contract::{DoctorCheck, ResolvedStataPath};
use crate::atom::path_ops::{
    absolutize_cli_path, backend_entry, default_config_path, validate_existing_working_dir,
};
use crate::coordinator::repl_commander::repl_command;
use crate::molecule::backend_client::{
    data_backend_invocation, invoke_backend, invoke_backend_json,
};
use crate::molecule::doctor_report::{
    backend_entry_check, backend_probe_ok_check, config_file_check, error_check, finalize_report,
    python_ok_check, repo_root_check, stata_path_check,
};
use crate::molecule::repo_resolution::{resolve_python, resolve_repo_root};
use crate::molecule::result_render::{
    prepare_execution_result, prepare_json_payload, render_execution_result, render_json_payload,
};
use crate::molecule::stata_path_resolution::{
    clone_with_effective_stata_path, persist_stata_path_if_needed,
    persist_stata_path_if_needed_json, resolve_effective_stata_path,
};
use crate::molecule::workspace_init::init_command;
use anyhow::{bail, Result};
use clap::Parser;
use std::ffi::OsString;

pub(crate) fn run() -> Result<()> {
    let cli = Cli::parse();

    if matches!(cli.command, Commands::Init) {
        let repo_root = resolve_repo_root()?;
        return init_command(&repo_root.path);
    }

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
            let mut run_cli = effective_cli.clone();
            if let Some(working_dir) = &effective_cli.working_dir {
                run_cli.working_dir = Some(validate_existing_working_dir(working_dir)?);
            }
            let command_args = vec![OsString::from("--code"), OsString::from(code)];
            let result =
                invoke_backend(&python.path, &repo_root.path, &run_cli, "run", command_args)?;
            let result = prepare_execution_result(&run_cli, result, false);
            persist_stata_path_if_needed(&resolved_stata_path, &result)?;
            render_execution_result(&result)
        }
        Commands::File {
            path,
            session_id,
            working_dir,
        } => {
            let python = resolve_python(effective_cli.python.as_deref(), &repo_root.path)?;
            let mut file_cli = effective_cli.clone();
            let resolved_path = absolutize_cli_path(path)?;
            let gui_hits = scan_do_file_for_gui_commands(&resolved_path)?;
            if !gui_hits.is_empty() && !confirm_gui_command_execution(&resolved_path, &gui_hits)? {
                bail!("Execution cancelled by user after GUI command warning");
            }
            if session_id.is_some() {
                file_cli.session_id = session_id.clone();
            }
            if let Some(working_dir) = working_dir {
                file_cli.working_dir = Some(absolutize_cli_path(working_dir)?);
            }
            let result = invoke_backend(
                &python.path,
                &repo_root.path,
                &file_cli,
                "file",
                vec![resolved_path.as_os_str().to_os_string()],
            )?;
            let result = prepare_execution_result(&file_cli, result, true);
            persist_stata_path_if_needed(&resolved_stata_path, &result)?;
            render_execution_result(&result)
        }
        Commands::Init => unreachable!("init is handled before runtime resolution"),
        Commands::Repl => unreachable!("repl is handled before project-root resolution"),
        Commands::Data { command } => {
            let python = resolve_python(effective_cli.python.as_deref(), &repo_root.path)?;
            let (backend_command, backend_args) = data_backend_invocation(command)?;
            let mut data_cli = effective_cli.clone();
            if matches!(
                command,
                crate::atom::cli_contract::DataCommands::View { .. }
            ) {
                data_cli.working_dir = None;
            }
            let payload = invoke_backend_json(
                &python.path,
                &repo_root.path,
                &data_cli,
                backend_command,
                backend_args,
            )?;
            let payload = prepare_json_payload(
                &data_cli,
                payload,
                matches!(
                    command,
                    crate::atom::cli_contract::DataCommands::ExportCsv { .. }
                ),
            );
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
