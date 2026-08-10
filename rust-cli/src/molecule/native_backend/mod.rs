//! Native replacement for the former Python/PyStata backend.
//!
//! All Stata work goes through `StataEngine` (see `atom::stata_engine`).
//! Output capture drains Stata's own output buffer after every execution, so
//! it cannot be broken by user `log`/`capture` commands; `log_file` results
//! are written by Rust from the captured output. Data previews are driven by
//! Stata metadata (see `data_preview`).

mod data_preview;

use crate::atom::cli_contract::Cli;
use crate::atom::json_contract::{CompletionContextResult, ExecutionResult, PartialFailure};
use crate::atom::output_filtering::{process_file_output, process_output};
use crate::atom::partial_failure::parse_partial_failures;
use crate::atom::path_ops::{
    expand_user, get_log_file_path, normalize_for_external, resolve_do_file_path,
    resolve_output_path,
};
use crate::atom::smcl_text::render_smcl_to_text;
use crate::atom::stata_engine::StataEngine;
use crate::atom::stata_syntax::{
    blocked_interactive_prefix, build_selection_for_working_dir, clean_help_topic,
    help_guidance_message, parse_single_command, sanitize_session_id, stata_quote_path,
};
use anyhow::{bail, Result};
use data_preview::{get_data, simple_variable_names};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const DEFAULT_SESSION_ID: &str = "default";

/// Backend-side filter configuration. The previous backend filtered with
/// `compact` / 10000 tokens by default, and the Rust frontend re-filters the
/// result; we keep that exact two-stage behavior.
#[derive(Debug, Clone)]
pub(crate) struct FilterOptions {
    pub(crate) mode: String,
    pub(crate) max_tokens: usize,
}

impl FilterOptions {
    pub(crate) fn from_cli(cli: &Cli) -> Self {
        Self {
            mode: cli
                .result_display_mode
                .clone()
                .unwrap_or_else(|| "compact".to_string()),
            max_tokens: cli.max_output_tokens.unwrap_or(10_000) as usize,
        }
    }

    fn apply(&self, output: &str, filter_command_echo: bool) -> String {
        process_output(output, &self.mode, self.max_tokens, filter_command_echo)
    }

    fn apply_file(&self, output: &str, log_file: Option<&str>) -> String {
        process_file_output(output, &self.mode, self.max_tokens, true, log_file)
    }
}

pub(crate) fn presented_session_id(session_id: Option<&str>) -> String {
    session_id
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| DEFAULT_SESSION_ID.to_string())
}

pub(crate) fn render_error(message: &str, session_id: Option<&str>) -> ExecutionResult {
    ExecutionResult {
        status: "error".to_string(),
        output: String::new(),
        session_id: Some(presented_session_id(session_id)),
        log_file: None,
        graphs: Vec::new(),
        partial_failures: Vec::new(),
        partial_failure_count: 0,
        error: Some(message.to_string()),
    }
}

/// Resolve the Stata installation directory: `--stata-path` > `STATA_PATH` >
/// macOS defaults.
pub(crate) fn resolve_stata_home(cli: &Cli) -> Result<PathBuf> {
    if let Some(path) = &cli.stata_path {
        let candidate = PathBuf::from(path);
        if !candidate.is_dir() {
            bail!("--stata-path is not a directory: {}", candidate.display());
        }
        return canonicalize_or(&candidate);
    }
    if let Some(path) = env::var_os("STATA_PATH") {
        let candidate = PathBuf::from(path);
        if candidate.is_dir() {
            return canonicalize_or(&candidate);
        }
    }
    for default in ["/Applications/StataNow", "/Applications/Stata"] {
        let candidate = PathBuf::from(default);
        if candidate.is_dir() {
            return canonicalize_or(&candidate);
        }
    }
    bail!(
        "Could not locate a Stata installation. Pass --stata-path or set STATA_PATH (tried /Applications/StataNow and /Applications/Stata)."
    )
}

fn canonicalize_or(path: &Path) -> Result<PathBuf> {
    Ok(normalize_for_external(path))
}

pub(crate) fn open_engine(cli: &Cli) -> Result<StataEngine> {
    let home = resolve_stata_home(cli)?;
    let edition = cli.stata_edition.as_deref().unwrap_or("mp");
    if !matches!(edition, "mp" | "se" | "be") {
        bail!("Unknown Stata edition: {edition}. Expected one of mp, se, be.");
    }
    StataEngine::new(&home, edition)
}

// ---------------------------------------------------------------------------
// Execution
// ---------------------------------------------------------------------------

pub(crate) struct ExecuteOutcome {
    pub(crate) output: String,
    pub(crate) rc: i32,
    pub(crate) cancelled: bool,
}

/// Run a block through a temporary do-file and capture everything from
/// Stata's own output buffer (drained to exhaustion, so it survives 2 MB+
/// output and user `log`/`capture` commands).
pub(crate) fn execute_code(engine: &StataEngine, code: &str, seed_prefix: &str) -> ExecuteOutcome {
    let seed_section = if seed_prefix.is_empty() {
        String::new()
    } else {
        seed_prefix.to_string()
    };
    let wrapped = format!("{seed_section}{code}\n");
    let result = engine.run_block(&wrapped);
    let output = crate::atom::output_filtering::deduplicate_break_messages(&result.output);
    let cancelled = engine.break_requested() && output.contains("--Break--");
    ExecuteOutcome {
        output,
        rc: result.rc,
        cancelled,
    }
}

/// Run a `.do` file: `cd` to its directory, capture the full output buffer,
/// and write the complete raw output to `log_file` (Rust-side), so the log is
/// never truncated and cannot be closed by the user's own Stata code.
pub(crate) fn execute_file(
    engine: &StataEngine,
    code: &str,
    working_dir: Option<&str>,
    log_file: &Path,
) -> ExecuteOutcome {
    if let Some(parent) = log_file.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            return ExecuteOutcome {
                output: format!("failed to create log directory: {error}"),
                rc: -1,
                cancelled: false,
            };
        }
    }
    let do_dir = working_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| log_file.parent().map(PathBuf::from).unwrap_or_default())
        .display()
        .to_string();
    let quoted_do_dir = match stata_quote_path(&do_dir) {
        Ok(quoted) => quoted,
        Err(error) => {
            return ExecuteOutcome {
                output: format!("failed to quote working directory: {error:#}"),
                rc: -1,
                cancelled: false,
            };
        }
    };
    let wrapped = format!(
        "set seed {}\ncd {}\n{code}\n",
        StataEngine::fresh_seed(),
        quoted_do_dir
    );
    let result = engine.run_block(&wrapped);
    let output = crate::atom::output_filtering::deduplicate_break_messages(&result.output);
    if let Err(error) = fs::write(log_file, &output) {
        return ExecuteOutcome {
            output: format!("failed to write log file {}: {error}", log_file.display()),
            rc: -1,
            cancelled: false,
        };
    }
    let cancelled = engine.break_requested() && output.contains("--Break--");
    ExecuteOutcome {
        output,
        rc: result.rc,
        cancelled,
    }
}

/// Map an execution outcome to (status, error) the same way the previous
/// pystata backend did:
/// - interrupted runs become `error` with "Execution cancelled" and the
///   filtered break output;
/// - any nonzero Stata return code becomes `error` carrying the raw output as
///   the message and an empty `output` field;
/// - everything else is `success`.
fn outcome_status(raw: &str, rc: i32, cancelled: bool) -> (String, Option<String>) {
    if cancelled {
        ("error".to_string(), Some("Execution cancelled".to_string()))
    } else if rc != 0 {
        ("error".to_string(), Some(raw.to_string()))
    } else {
        ("success".to_string(), None)
    }
}

pub(crate) fn run_selection(
    engine: &StataEngine,
    selection: &str,
    session_id: Option<&str>,
    working_dir: Option<&str>,
    repo_root: Option<&Path>,
    filter: &FilterOptions,
) -> ExecutionResult {
    if let Some(blocked) = blocked_interactive_prefix(selection) {
        return render_error(
            &format!(
                "This command opens an interactive Stata UI or waits for input ({blocked}) and is not suitable for CLI execution."
            ),
            session_id,
        );
    }
    if let Some(result) =
        help_or_window_command_result(engine, selection, session_id, repo_root, filter)
    {
        return result;
    }

    let code = match build_selection_for_working_dir(selection, working_dir) {
        Ok(code) => code,
        Err(error) => {
            return render_error(&format!("{error:#}"), session_id);
        }
    };
    let seed_prefix = engine.seed_prefix();
    let outcome = execute_code(engine, &code, &seed_prefix);
    let raw = outcome.output;
    let (status, error) = outcome_status(&raw, outcome.rc, outcome.cancelled);
    if status == "success" {
        engine.mark_seed_done();
    }
    let output = if status == "error" && !outcome.cancelled {
        String::new()
    } else {
        filter.apply(&raw, false)
    };
    ExecutionResult {
        status,
        output,
        session_id: Some(presented_session_id(session_id)),
        log_file: None,
        graphs: Vec::new(),
        partial_failures: Vec::new(),
        partial_failure_count: 0,
        error,
    }
}

/// Build the Stata block that resolves a help topic to its `.sthlp` file.
/// `findfile` searches the whole ado-path (base + user PLUS directories) and
/// leaves the resolved path in `r(fn)`; the marker line makes parsing robust
/// against command echoes.
fn help_findfile_block(topic: &str) -> String {
    format!("findfile {topic}.sthlp\ndisplay \"STATA_CLI_HELP_PATH=[`r(fn)']\"")
}

fn parse_help_path(output: &str) -> Option<PathBuf> {
    const MARKER: &str = "STATA_CLI_HELP_PATH=[";
    for line in output.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with(". ") || trimmed.starts_with("> ") {
            continue; // Stata command echoes carry the marker text too
        }
        if let Some(start) = line.find(MARKER) {
            let rest = &line[start + MARKER.len()..];
            if let Some(end) = rest.find(']') {
                let path = rest[..end].trim();
                if !path.is_empty() && !path.contains('`') && !path.contains('\'') {
                    return Some(PathBuf::from(path));
                }
            }
        }
    }
    None
}

fn success_result(output: String, session_id: Option<&str>) -> ExecutionResult {
    ExecutionResult {
        status: "success".to_string(),
        output,
        session_id: Some(presented_session_id(session_id)),
        log_file: None,
        graphs: Vec::new(),
        partial_failures: Vec::new(),
        partial_failure_count: 0,
        error: None,
    }
}

/// Intercept single-line `help`/`search`/`findit` selections. `help <topic>`
/// renders the real local Stata help text; the window-only commands and
/// unresolvable help topics return guidance instead of silently producing
/// nothing.
fn help_or_window_command_result(
    engine: &StataEngine,
    selection: &str,
    session_id: Option<&str>,
    repo_root: Option<&Path>,
    filter: &FilterOptions,
) -> Option<ExecutionResult> {
    let (command, args) = parse_single_command(selection)?;
    let workspace = std::env::current_dir().ok();
    match command.as_str() {
        "search" | "findit" => Some(success_result(
            help_guidance_message(&command, None, workspace.as_deref(), repo_root),
            session_id,
        )),
        "help" => {
            let raw_topic = args.join(" ").trim().to_string();
            let topic = clean_help_topic(&raw_topic);
            if topic.is_empty() {
                return Some(success_result(
                    help_guidance_message("help", None, workspace.as_deref(), repo_root),
                    session_id,
                ));
            }

            let probe = execute_code(engine, &help_findfile_block(&topic), &engine.seed_prefix());
            if probe.rc == 0 && !probe.cancelled {
                if let Some(path) = parse_help_path(&probe.output) {
                    if let Ok(smcl) = fs::read_to_string(&path) {
                        let text = render_smcl_to_text(&smcl);
                        if !text.trim().is_empty() {
                            engine.mark_seed_done();
                            return Some(success_result(filter.apply(&text, false), session_id));
                        }
                    }
                }
            }

            Some(success_result(
                help_guidance_message("help", Some(&topic), workspace.as_deref(), repo_root),
                session_id,
            ))
        }
        _ => None,
    }
}

pub(crate) fn run_file(
    engine: &StataEngine,
    file_path: &Path,
    session_id: Option<&str>,
    working_dir: Option<&str>,
    filter: &FilterOptions,
) -> ExecutionResult {
    let (resolved_path, tried_paths) = resolve_do_file_path(file_path);
    let effective_path = resolved_path
        .clone()
        .unwrap_or_else(|| file_path.to_path_buf());
    let Some(resolved_path) = resolved_path else {
        let tried_display = if tried_paths.is_empty() {
            effective_path.display().to_string()
        } else {
            tried_paths.join(", ")
        };
        return render_error(
            &format!(
                "File not found: {}. Tried these paths: {tried_display}",
                file_path.display()
            ),
            session_id,
        );
    };

    let base_name = effective_path
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default();
    let log_file = get_log_file_path(
        &effective_path,
        &base_name,
        session_id.map(sanitize_session_id).as_deref(),
    );

    let code = match fs::read_to_string(&resolved_path) {
        Ok(code) => code,
        Err(error) => {
            return render_error(
                &format!(
                    "Failed to read do-file {}: {error}",
                    resolved_path.display()
                ),
                session_id,
            );
        }
    };
    let outcome = execute_file(engine, &code, working_dir, &log_file);
    let raw = outcome.output;
    let partial_failures: Vec<PartialFailure> = parse_partial_failures(&raw);
    let (status, error) = outcome_status(&raw, outcome.rc, outcome.cancelled);
    if status == "success" {
        engine.mark_seed_done();
    }
    let filtered = if status == "error" && !outcome.cancelled {
        String::new()
    } else {
        filter.apply_file(&raw, Some(&log_file.display().to_string()))
    };
    ExecutionResult {
        status,
        output: filtered,
        session_id: Some(presented_session_id(session_id)),
        log_file: Some(log_file.display().to_string()),
        graphs: Vec::new(),
        partial_failure_count: partial_failures.len() as u64,
        partial_failures,
        error,
    }
}

// ---------------------------------------------------------------------------
// Data commands
// ---------------------------------------------------------------------------

pub(crate) fn data_view_command(
    engine: &StataEngine,
    if_condition: Option<&str>,
    max_rows: u32,
    input_dta: Option<&Path>,
    repo_root: Option<&Path>,
    filter: &FilterOptions,
) -> Value {
    let source_dta = input_dta.map(|path| expand_user(path).display().to_string());
    if let Some(input_path) = input_dta {
        let input_path = expand_user(input_path);
        if !input_path.is_file() {
            return json!({
                "status": "error",
                "message": format!("Input DTA file not found: {}", input_path.display())
            });
        }
        let quoted_input = match stata_quote_path(&input_path.display().to_string()) {
            Ok(quoted) => quoted,
            Err(error) => {
                return json!({
                    "status": "error",
                    "message": format!("Failed to quote input path: {error:#}")
                });
            }
        };
        let load_code =
            match build_selection_for_working_dir(&format!("use {quoted_input}, clear"), None) {
                Ok(code) => code,
                Err(error) => {
                    return json!({
                        "status": "error",
                        "message": format!("{error:#}")
                    });
                }
            };
        let load_result = run_selection(engine, &load_code, None, None, repo_root, filter);
        if load_result.status != "success" {
            return json!({
                "status": "error",
                "message": load_result.error.unwrap_or_else(|| {
                    format!("Failed to load DTA file: {}", input_path.display())
                })
            });
        }
    }

    match get_data(engine, if_condition, max_rows) {
        Ok(table) => json!({
            "status": "success",
            "data": table.rows,
            "columns": table.columns,
            "dtypes": table.dtypes,
            "rows": table.rows.len(),
            "index": table.index,
            "total_rows": table.total_rows,
            "displayed_rows": table.displayed_rows,
            "max_rows": max_rows,
            "source_dta": source_dta,
        }),
        Err(error) => json!({
            "status": "error",
            "message": error,
        }),
    }
}

pub(crate) fn data_export_csv_command(
    engine: &StataEngine,
    output: &Path,
    input_dta: Option<&Path>,
    session_id: Option<&str>,
    working_dir: Option<&str>,
    replace: bool,
    filter: &FilterOptions,
) -> Value {
    let output_path = resolve_output_path(output, working_dir);
    if let Some(parent) = output_path.parent() {
        if let Err(error) = fs::create_dir_all(parent) {
            return json!({
                "status": "error",
                "message": format!(
                    "Failed to create output directory {}: {error}",
                    parent.display()
                )
            });
        }
    }
    if output_path.exists() && !replace {
        return json!({
            "status": "error",
            "message": format!(
                "Output file already exists: {}. Use --replace to overwrite it.",
                output_path.display()
            )
        });
    }

    let mut commands: Vec<String> = Vec::new();
    if let Some(input_path) = input_dta {
        let input_path = expand_user(input_path);
        if !input_path.is_file() {
            return json!({
                "status": "error",
                "message": format!("Input DTA file not found: {}", input_path.display())
            });
        }
        let quoted_input = match stata_quote_path(&input_path.display().to_string()) {
            Ok(quoted) => quoted,
            Err(error) => {
                return json!({
                    "status": "error",
                    "message": format!("Failed to quote input path: {error:#}")
                });
            }
        };
        commands.push(format!("use {quoted_input}, clear"));
    }
    let quoted_output = match stata_quote_path(&output_path.display().to_string()) {
        Ok(quoted) => quoted,
        Err(error) => {
            return json!({
                "status": "error",
                "message": format!("Failed to quote output path: {error:#}")
            });
        }
    };
    commands.push(format!("export delimited using {quoted_output}, replace"));
    let code = match build_selection_for_working_dir(&commands.join("\n"), working_dir) {
        Ok(code) => code,
        Err(error) => {
            return json!({
                "status": "error",
                "message": format!("{error:#}")
            });
        }
    };
    let seed_prefix = engine.seed_prefix();
    let outcome = execute_code(engine, &code, &seed_prefix);
    let raw = outcome.output.replace("\\n", "\n");
    let (status, error) = outcome_status(&raw, outcome.rc, outcome.cancelled);
    if status == "success" {
        engine.mark_seed_done();
    }
    let filtered = if status == "error" && !outcome.cancelled {
        String::new()
    } else {
        filter.apply(&raw, false)
    };
    json!({
        "status": status,
        "output": filtered,
        "output_csv": output_path.display().to_string(),
        "session_id": presented_session_id(session_id),
        "error": error,
    })
}

// ---------------------------------------------------------------------------
// Completion snapshot (REPL bridge)
// ---------------------------------------------------------------------------

fn parse_macro_names(output: &str) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for raw_line in output.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with(". ") || line.starts_with("> ") {
            continue;
        }
        if line.to_lowercase().starts_with("global macros")
            || line.to_lowercase().starts_with("local macros")
        {
            continue;
        }
        let mut parts = line.splitn(2, ':');
        if let (Some(name), Some(_)) = (parts.next(), parts.next()) {
            let name = name.trim();
            if !name.is_empty()
                && name
                    .chars()
                    .next()
                    .map(|ch| ch.is_ascii_alphabetic() || ch == '_')
                    .unwrap_or(false)
            {
                names.push(name.to_string());
            }
        }
    }
    names.sort();
    names.dedup();
    names
}

/// Completion snapshot: read-only metadata queries only. It never modifies
/// the user's dataset (no preserve/keep/export), so REPL state is preserved.
pub(crate) fn completion_snapshot(engine: &StataEngine) -> CompletionContextResult {
    let variables = simple_variable_names(engine);
    let macros = {
        let seed_prefix = engine.seed_prefix();
        let outcome = execute_code(engine, "macro dir", &seed_prefix);
        if outcome.rc == 0 && !outcome.cancelled {
            engine.mark_seed_done();
            parse_macro_names(&outcome.output)
        } else {
            Vec::new()
        }
    };
    CompletionContextResult {
        status: "success".to_string(),
        variables,
        macros,
        error: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::atom::stata_syntax::join_stata_line_continuations;

    #[test]
    fn parses_macro_dir_names() {
        let output = "\n. macro dir\n\nglobal macros:\n  myglob : foo\n  other : bar\n\nlocal macros:\n  myloc : 1\n";
        let names = parse_macro_names(output);
        assert_eq!(names, vec!["myglob", "myloc", "other"]);
    }

    #[test]
    fn outcome_maps_cancelled_and_rc_errors() {
        let (status, error) = outcome_status("boom", 111, false);
        assert_eq!(status, "error");
        assert_eq!(error.as_deref(), Some("boom"));

        let (status, error) = outcome_status("out", 0, true);
        assert_eq!(status, "error");
        assert_eq!(error.as_deref(), Some("Execution cancelled"));

        let (status, error) = outcome_status("out", 0, false);
        assert_eq!(status, "success");
        assert!(error.is_none());
    }

    #[test]
    fn join_continuations_helper_is_reachable() {
        assert_eq!(join_stata_line_continuations("a ///\nb"), "a b");
    }

    #[test]
    fn parses_help_path_marker() {
        let output = "\n. findfile regress.sthlp\n/Applications/Stata/ado/base/r/regress.sthlp\n\n. display \"STATA_CLI_HELP_PATH=[`r(fn)']\"\nSTATA_CLI_HELP_PATH=[/Applications/Stata/ado/base/r/regress.sthlp]\n\n. ";
        assert_eq!(
            parse_help_path(output),
            Some(PathBuf::from(
                "/Applications/Stata/ado/base/r/regress.sthlp"
            ))
        );
        assert_eq!(parse_help_path("no marker here"), None);
        assert_eq!(parse_help_path("STATA_CLI_HELP_PATH=[]"), None);
    }

    #[test]
    fn help_findfile_block_uses_cleaned_topic() {
        assert_eq!(
            help_findfile_block("regress"),
            "findfile regress.sthlp\ndisplay \"STATA_CLI_HELP_PATH=[`r(fn)']\""
        );
    }
}
