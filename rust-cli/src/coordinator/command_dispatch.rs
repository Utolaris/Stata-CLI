use crate::atom::cli_contract::{Cli, Commands};
use crate::atom::do_file_scan::{confirm_gui_command_execution, scan_do_file_for_gui_commands};
use crate::atom::json_contract::DoctorCheck;
use crate::atom::path_ops::{
    absolutize_cli_path, default_config_path, validate_existing_working_dir,
};
use crate::coordinator::repl_commander::repl_command;
use crate::molecule::doctor_report::{
    config_file_check, engine_probe_ok_check, error_check, finalize_report, repo_root_check,
};
use crate::molecule::native_backend::{
    data_export_csv_command, data_view_command, open_engine, run_file, run_selection, FilterOptions,
};
use crate::molecule::repo_resolution::resolve_repo_root;
use crate::molecule::result_render::{
    prepare_execution_result, prepare_json_payload, render_execution_result, render_json_payload,
};
use crate::molecule::stata_path_resolution::{
    clone_with_effective_stata_path, persist_stata_path_if_needed,
    persist_stata_path_if_needed_json, resolve_effective_stata_path,
};
use crate::molecule::workspace_init::init_command;
use anyhow::Result;
use clap::Parser;

pub(crate) fn run() -> Result<()> {
    let cli = Cli::parse();

    if matches!(cli.command, Commands::Init) {
        let repo_root = resolve_repo_root()?;
        return init_command(&repo_root.path);
    }

    let resolved_stata_path = resolve_effective_stata_path(&cli)?;
    let effective_cli = clone_with_effective_stata_path(&cli, &resolved_stata_path);

    match &effective_cli.command {
        Commands::Repl => repl_command(&effective_cli),
        Commands::Doctor => doctor_command(&effective_cli),
        Commands::Run { code } => {
            let mut run_cli = effective_cli.clone();
            if let Some(working_dir) = &effective_cli.working_dir {
                run_cli.working_dir = Some(validate_existing_working_dir(working_dir)?);
            }
            let engine = open_engine(&effective_cli)?;
            let filter = FilterOptions::from_cli(&run_cli);
            let repo_root = resolve_repo_root().ok().map(|root| root.path);
            let result = run_selection(
                &engine,
                code,
                run_cli.session_id.as_deref(),
                run_cli
                    .working_dir
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .as_deref(),
                repo_root.as_deref(),
                &filter,
            );
            let result = prepare_execution_result(&run_cli, result, false);
            persist_stata_path_if_needed(&resolved_stata_path, &result)?;
            render_execution_result(&result)
        }
        Commands::File {
            path,
            session_id,
            working_dir,
        } => {
            let mut file_cli = effective_cli.clone();
            let resolved_path = absolutize_cli_path(path)?;
            let gui_hits = scan_do_file_for_gui_commands(&resolved_path)?;
            if !gui_hits.is_empty() && !confirm_gui_command_execution(&resolved_path, &gui_hits)? {
                anyhow::bail!("Execution cancelled by user after GUI command warning");
            }
            if session_id.is_some() {
                file_cli.session_id = session_id.clone();
            }
            if let Some(working_dir) = working_dir {
                file_cli.working_dir = Some(absolutize_cli_path(working_dir)?);
            }
            let engine = open_engine(&effective_cli)?;
            let filter = FilterOptions::from_cli(&file_cli);
            let result = run_file(
                &engine,
                &resolved_path,
                file_cli.session_id.as_deref(),
                file_cli
                    .working_dir
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .as_deref(),
                &filter,
            );
            let result = prepare_execution_result(&file_cli, result, true);
            persist_stata_path_if_needed(&resolved_stata_path, &result)?;
            render_execution_result(&result)
        }
        Commands::Init => unreachable!("init is handled before runtime resolution"),
        Commands::Data { command } => {
            let filter = FilterOptions::from_cli(&effective_cli);
            let repo_root = resolve_repo_root().ok().map(|root| root.path);
            let engine = open_engine(&effective_cli)?;
            let payload = match command {
                crate::atom::cli_contract::DataCommands::View {
                    if_condition,
                    max_rows,
                    input_dta,
                } => data_view_command(
                    &engine,
                    if_condition.as_deref(),
                    *max_rows,
                    Some(input_dta),
                    repo_root.as_deref(),
                    &filter,
                ),
                crate::atom::cli_contract::DataCommands::ExportCsv {
                    output,
                    input_dta,
                    working_dir,
                    replace,
                } => {
                    let working_dir_str = working_dir
                        .as_ref()
                        .map(|path| path.to_string_lossy().into_owned());
                    data_export_csv_command(
                        &engine,
                        output,
                        Some(input_dta.as_path()),
                        effective_cli.session_id.as_deref(),
                        working_dir_str.as_deref(),
                        *replace,
                        &filter,
                    )
                }
            };
            let payload = prepare_json_payload(
                &effective_cli,
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

fn doctor_command(cli: &Cli) -> Result<()> {
    let config_path = default_config_path();
    let mut checks: Vec<DoctorCheck> = Vec::new();

    match resolve_repo_root() {
        Ok(repo_root) => checks.push(repo_root_check(&repo_root)),
        Err(error) => checks.push(error_check("repo_root", error.to_string())),
    }
    checks.push(config_file_check(config_path.as_deref()));

    // Probe the native engine: load libstata-*.dylib and run `display 1+1`.
    match crate::molecule::native_backend::resolve_stata_home(cli) {
        Ok(home) => {
            let edition = cli.stata_edition.as_deref().unwrap_or("mp");
            match crate::atom::stata_engine::StataEngine::new(&home, edition) {
                Ok(engine) => {
                    let probe = engine.execute("display 1+1");
                    if probe.output.trim().ends_with("2") {
                        checks.push(engine_probe_ok_check(format!(
                            "Stata engine loaded {} and executed `display 1+1`.",
                            home.display()
                        )));
                    } else {
                        checks.push(error_check(
                            "engine_probe",
                            format!(
                                "Stata engine initialized but probe output was unexpected: {:?}",
                                probe.output
                            ),
                        ));
                    }
                }
                Err(error) => checks.push(error_check("engine_probe", error.to_string())),
            }
        }
        Err(error) => checks.push(error_check("stata_path", error.to_string())),
    }

    let report = finalize_report(checks);
    println!("{}", serde_json::to_string_pretty(&report)?);

    if report.status == "error" {
        anyhow::bail!("stata-cli doctor found one or more blocking issues");
    }
    Ok(())
}
