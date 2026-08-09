//! Native replacement for the former Python/PyStata backend.
//!
//! All Stata work is done through `StataEngine` (see `atom::stata_engine`).
//! The functions here mirror the previous backend's behavior: log-file output
//! capture, per-session seeding, interactive-command blocking, help guidance,
//! compact output filtering, partial failure parsing, data preview via a
//! temporary CSV export, graph-neutral result contracts, and completion
//! snapshots for the REPL.

use crate::atom::cli_contract::Cli;
use crate::atom::csv_table::{infer_dtype, parse_csv};
use crate::atom::json_contract::{CompletionContextResult, ExecutionResult, PartialFailure};
use crate::atom::output_filtering::{process_file_output, process_output};
use crate::atom::partial_failure::parse_partial_failures;
use crate::atom::stata_engine::{StataEngine, StataOutput};
use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

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
    pub(crate) fn from_cli(cli: &Cli) -> FilterOptions {
        FilterOptions {
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

/// Resolve the Stata installation directory the same way the old backend did:
/// `--stata-path` > `STATA_PATH` > macOS defaults.
pub(crate) fn resolve_stata_home(cli: &Cli) -> Result<PathBuf> {
    if let Some(path) = &cli.stata_path {
        let candidate = PathBuf::from(path);
        if !candidate.is_dir() {
            bail!("--stata-path is not a directory: {}", candidate.display());
        }
        return canonicalize_or(candidate);
    }
    if let Some(path) = env::var_os("STATA_PATH") {
        let candidate = PathBuf::from(path);
        if candidate.is_dir() {
            return canonicalize_or(candidate);
        }
    }
    for default in ["/Applications/StataNow", "/Applications/Stata"] {
        let candidate = PathBuf::from(default);
        if candidate.is_dir() {
            return canonicalize_or(candidate);
        }
    }
    bail!(
        "Could not locate a Stata installation. Pass --stata-path or set STATA_PATH (tried /Applications/StataNow and /Applications/Stata)."
    )
}

fn canonicalize_or(path: PathBuf) -> Result<PathBuf> {
    fs::canonicalize(&path).with_context(|| format!("Failed to resolve {}", path.display()))
}

pub(crate) fn open_engine(cli: &Cli) -> Result<StataEngine> {
    let home = resolve_stata_home(cli)?;
    let edition = cli.stata_edition.as_deref().unwrap_or("mp");
    StataEngine::new(&home, edition)
}

// ---------------------------------------------------------------------------
// Path helpers (ported from the Python backend's `pathing.py`)
// ---------------------------------------------------------------------------

pub(crate) fn join_stata_line_continuations(code: &str) -> String {
    let mut joined_lines: Vec<String> = Vec::new();
    let mut current_line = String::new();
    for raw_line in code.lines() {
        let stripped = raw_line.trim_end();
        if let Some(rest) = stripped.strip_suffix("///") {
            current_line.push_str(rest.trim_end());
            current_line.push(' ');
        } else {
            current_line.push_str(raw_line);
            joined_lines.push(std::mem::take(&mut current_line));
        }
    }
    if !current_line.is_empty() {
        joined_lines.push(current_line);
    }
    joined_lines.join("\n")
}

pub(crate) fn build_selection_for_working_dir(
    selection: &str,
    working_dir: Option<&str>,
) -> String {
    let processed = join_stata_line_continuations(selection);
    match working_dir.filter(|wd| Path::new(wd).is_dir()) {
        Some(wd) => format!("cd \"{}\"\n{processed}", wd.replace('\\', "/")),
        None => processed,
    }
}

fn resolve_output_path(output: &Path, working_dir: Option<&str>) -> PathBuf {
    let output = expand_tilde(output);
    if output.is_absolute() {
        return output;
    }
    let base = working_dir.map(PathBuf::from).unwrap_or_default();
    let base = if base.is_absolute() {
        base
    } else {
        env::current_dir().unwrap_or_default().join(base)
    };
    base.join(output)
}

/// Expand a leading `~/` only; unlike `expand_user` this does not resolve
/// relative paths against the current directory.
fn expand_tilde(path: &Path) -> PathBuf {
    let rendered = path.to_string_lossy();
    if let Some(rest) = rendered.strip_prefix("~/") {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    path.to_path_buf()
}

/// Port of `resolve_do_file_path`: absolute paths are used directly; relative
/// paths are tried against cwd and a shallow (2-level) recursive scan.
pub(crate) fn resolve_do_file_path(file_path: &Path) -> (Option<PathBuf>, Vec<String>) {
    let mut candidates: Vec<PathBuf> = Vec::new();
    let mut tried: Vec<String> = Vec::new();

    if file_path.is_absolute() {
        candidates.push(file_path.to_path_buf());
    } else {
        let cwd = env::current_dir().unwrap_or_default();
        candidates.push(file_path.to_path_buf());
        candidates.push(cwd.join(file_path));
        if let Some(base) = file_path.file_name() {
            candidates.push(cwd.join(base));
        }

        let base_name = file_path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut stack: Vec<(PathBuf, usize)> = vec![(cwd.clone(), 0)];
        while let Some((dir, depth)) = stack.pop() {
            if depth >= 2 {
                continue;
            }
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push((path, depth + 1));
                } else if depth >= 1
                    && path
                        .file_name()
                        .map(|name| name.to_string_lossy() == base_name)
                        .unwrap_or(false)
                {
                    candidates.push(path);
                }
            }
        }
    }

    let mut seen = std::collections::HashSet::new();
    for candidate in candidates {
        let normalized = candidate
            .canonicalize()
            .unwrap_or_else(|_| candidate.clone());
        if !seen.insert(normalized.clone()) {
            continue;
        }
        tried.push(normalized.display().to_string());
        if normalized.is_file()
            && normalized
                .extension()
                .map(|ext| ext.eq_ignore_ascii_case("do"))
                .unwrap_or(false)
        {
            return (Some(normalized), tried);
        }
    }
    (None, tried)
}

pub(crate) fn get_log_file_path(
    do_file_path: &Path,
    base_name: &str,
    session_id: Option<&str>,
) -> PathBuf {
    let dir = do_file_path.parent().unwrap_or_else(|| Path::new("."));
    let suffix = session_id.map(|id| format!("_{id}")).unwrap_or_default();
    dir.join(format!("{base_name}{suffix}_cli.log"))
}

// ---------------------------------------------------------------------------
// Execution helpers
// ---------------------------------------------------------------------------

pub(crate) struct ExecuteOutcome {
    pub(crate) output: String,
    pub(crate) rc: i32,
    pub(crate) cancelled: bool,
}

fn now_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}

fn stata_side_path(path: &Path) -> String {
    path.display().to_string().replace('\\', "/")
}

/// Run a block through a temporary log file, mirroring the old worker's
/// `execute_stata_code`: `log using` wrap, read the log back, deduplicate
/// break messages.
pub(crate) fn execute_code_with_log(
    engine: &StataEngine,
    code: &str,
    seed_prefix: &str,
    session_id: Option<&str>,
) -> ExecuteOutcome {
    let session_tag = session_id.unwrap_or(DEFAULT_SESSION_ID);
    let log_file = engine
        .temp_dir()
        .join(format!("stata_run_{session_tag}_{}.log", now_nanos()));
    let log_stata = stata_side_path(&log_file);
    let wrapped = format!(
        "capture log close _all\nlog using \"{log_stata}\", replace text\n{seed_prefix}{code}\ncapture log close _all\n"
    );
    let result = engine.run_block(&wrapped);
    let mut output = fs::read_to_string(&log_file).unwrap_or_default();
    if output.trim().is_empty() {
        output = result.output;
    }
    let _ = fs::remove_file(&log_file);
    let output = crate::atom::output_filtering::deduplicate_break_messages(&output);
    let cancelled = output.contains("--Break--");
    ExecuteOutcome {
        output,
        rc: result.rc,
        cancelled,
    }
}

/// Run a `.do` file with a persistent log file, mirroring the old worker's
/// `execute_stata_file` (seed embedded, `cd` to the do-file directory).
pub(crate) fn execute_file_with_log(
    engine: &StataEngine,
    code: &str,
    working_dir: Option<&str>,
    log_file: &Path,
) -> ExecuteOutcome {
    let log_stata = stata_side_path(log_file);
    if let Some(parent) = log_file.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let seed = StataEngine::fresh_seed();
    let do_dir = working_dir
        .map(PathBuf::from)
        .unwrap_or_else(|| log_file.parent().map(PathBuf::from).unwrap_or_default())
        .display()
        .to_string()
        .replace('\\', "/");
    let wrapped = format!(
        "capture log close _all\nset seed {seed}\ncd \"{do_dir}\"\nlog using \"{log_stata}\", replace text\n{code}\ncapture log close _all\n"
    );
    let result = engine.run_block(&wrapped);
    let captured = result.output;
    let log_output = fs::read_to_string(log_file).unwrap_or_default();
    let output = if log_output.trim().is_empty() {
        captured
    } else if captured.trim().is_empty() {
        log_output
    } else if captured.contains(&log_output) {
        captured
    } else {
        format!("{log_output}\n{captured}")
    };
    let output = crate::atom::output_filtering::deduplicate_break_messages(&output);
    let cancelled = output.contains("--Break--");
    ExecuteOutcome {
        output,
        rc: result.rc,
        cancelled,
    }
}

// ---------------------------------------------------------------------------
// Selection / file commands
// ---------------------------------------------------------------------------

fn blocked_interactive_prefix(selection: &str) -> Option<String> {
    const BLOCKED: &[&str] = &[
        "browse", "edit", "db", "dialog", "window", "shell", "winexec", "pause",
    ];
    for raw_line in selection.lines() {
        if let Some((command, _)) = parse_stata_command_line(raw_line) {
            if BLOCKED.contains(&command.as_str()) {
                return Some(command);
            }
        }
    }
    None
}

fn parse_stata_command_line(line: &str) -> Option<(String, Vec<String>)> {
    let stripped = line.trim();
    if stripped.is_empty() || stripped.starts_with('*') || stripped.starts_with("//") {
        return None;
    }
    let mut tokens: Vec<String> = stripped
        .split_whitespace()
        .map(normalize_token)
        .filter(|token| !token.is_empty())
        .collect();
    const WRAPPERS: &[&str] = &[
        "capture",
        "cap",
        "quietly",
        "qui",
        "noisily",
        "noi",
        "capturely",
        "captureily",
    ];
    while tokens
        .first()
        .map(|token| WRAPPERS.contains(&token.as_str()))
        .unwrap_or(false)
    {
        tokens.remove(0);
    }
    if tokens.is_empty() {
        return None;
    }
    Some((tokens.remove(0), tokens))
}

fn normalize_token(token: &str) -> String {
    token
        .trim_matches(|c| matches!(c, ':' | ',' | ';' | '(' | ')'))
        .to_lowercase()
}

fn help_topic_guidance(selection: &str, repo_root: Option<&Path>) -> Option<String> {
    let mut parsed_lines = Vec::new();
    for raw_line in selection.lines() {
        if let Some(parsed) = parse_stata_command_line(raw_line) {
            parsed_lines.push(parsed);
        }
    }
    if parsed_lines.len() != 1 {
        return None;
    }
    let (command, args) = &parsed_lines[0];
    if command != "help" {
        return None;
    }
    let topic = args.join(" ").trim().to_string();
    if topic.is_empty() {
        return None;
    }

    let mut message = "`help {topic}` cannot be captured reliably from the local Stata terminal bridge. Read the local `skills/stata-cli/SKILL.md` reference library instead.".replace("{topic}", &topic);
    if let Some(root) = repo_root {
        if let Some(doc) = skill_doc_for_help_topic(root, &topic) {
            message.push_str(&format!(" Start with `{doc}`."));
        }
    }
    Some(message)
}

fn skill_doc_for_help_topic(repo_root: &Path, topic: &str) -> Option<String> {
    let normalized = topic.trim().to_lowercase();
    let aliases: &[(&str, &str)] = &[
        ("esttab", "estout"),
        ("estout", "estout"),
        ("eststo", "estout"),
        ("estadd", "estout"),
    ];
    let mut candidates: Vec<(String, String)> = Vec::new();
    if let Some((_, alias)) = aliases.iter().find(|(name, _)| *name == normalized) {
        candidates.push(("packages".to_string(), (*alias).to_string()));
    }
    candidates.push(("packages".to_string(), normalized.clone()));
    candidates.push(("references".to_string(), normalized));

    for (folder, name) in candidates {
        let relative = Path::new("boilerplate")
            .join("skills")
            .join("stata-cli")
            .join(&folder)
            .join(format!("{name}.md"));
        if repo_root.join(&relative).is_file() {
            return Some(format!("skills/stata-cli/{folder}/{name}.md"));
        }
    }
    Some("skills/stata-cli/SKILL.md".to_string())
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
    if let Some(guidance) = help_topic_guidance(selection, repo_root) {
        return ExecutionResult {
            status: "success".to_string(),
            output: guidance,
            session_id: Some(presented_session_id(session_id)),
            log_file: None,
            graphs: Vec::new(),
            partial_failures: Vec::new(),
            partial_failure_count: 0,
            error: None,
        };
    }

    let code = build_selection_for_working_dir(selection, working_dir);
    let seed_prefix = engine.seed_prefix();
    let outcome = execute_code_with_log(engine, &code, &seed_prefix, session_id);
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
    let log_file = get_log_file_path(&effective_path, &base_name, session_id);
    if let Some(parent) = log_file.parent() {
        let _ = fs::create_dir_all(parent);
    }

    let code = fs::read_to_string(&resolved_path).unwrap_or_default();
    let outcome = execute_file_with_log(engine, &code, working_dir, &log_file);
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
        log_file: Some(stata_side_path(&log_file)),
        graphs: Vec::new(),
        partial_failure_count: partial_failures.len() as u64,
        partial_failures,
        error,
    }
}

// ---------------------------------------------------------------------------
// Data operations
// ---------------------------------------------------------------------------

pub(crate) struct DataTable {
    pub(crate) columns: Vec<String>,
    pub(crate) dtypes: serde_json::Map<String, Value>,
    pub(crate) rows: Vec<Vec<Value>>,
    pub(crate) index: Vec<Value>,
    pub(crate) total_rows: i64,
    pub(crate) displayed_rows: i64,
}

fn parse_obs_count(output: &StataOutput) -> Result<i64> {
    output
        .output
        .lines()
        .find_map(|line| line.trim().parse::<i64>().ok())
        .ok_or_else(|| anyhow::anyhow!("Failed to parse observation count from Stata output"))
}

fn export_csv(engine: &StataEngine) -> Result<PathBuf> {
    let csv_path = engine.temp_dir().join(format!(
        "stata_data_{}_{}.csv",
        std::process::id(),
        now_nanos()
    ));
    let csv_stata = stata_side_path(&csv_path);
    let result = engine.execute(&format!(
        "quietly export delimited using \"{csv_stata}\", replace"
    ));
    if result.rc != 0 {
        bail!(
            "export delimited failed (rc={}): {}",
            result.rc,
            result.output.trim()
        );
    }
    if !csv_path.is_file() {
        bail!("export delimited did not create {}", csv_path.display());
    }
    Ok(csv_path)
}

fn read_table(csv_path: &Path, drop_column: Option<&str>) -> Result<DataTable> {
    let text = fs::read_to_string(csv_path)
        .with_context(|| format!("Failed to read {}", csv_path.display()))?;
    let rows = parse_csv(&text);
    if rows.is_empty() {
        return Ok(DataTable {
            columns: Vec::new(),
            dtypes: serde_json::Map::new(),
            rows: Vec::new(),
            index: Vec::new(),
            total_rows: 0,
            displayed_rows: 0,
        });
    }
    let mut columns: Vec<String> = rows[0]
        .iter()
        .map(|field| field.clone().unwrap_or_default())
        .collect();
    let drop_index = drop_column.and_then(|name| columns.iter().position(|c| c == name));

    let mut data_rows: Vec<Vec<Option<String>>> = Vec::new();
    let mut index_values: Vec<Value> = Vec::new();
    for raw_row in rows.iter().skip(1) {
        let mut row: Vec<Option<String>> = raw_row.clone();
        row.resize(columns.len(), None);
        if let Some(drop) = drop_index {
            let obs = row[drop]
                .as_ref()
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or_default();
            index_values.push(Value::from(obs));
        }
        data_rows.push(row);
    }

    let mut dtypes = serde_json::Map::new();
    let mut json_rows: Vec<Vec<Value>> = Vec::new();
    for (col_index, column) in columns.iter().enumerate() {
        if Some(col_index) == drop_index {
            continue;
        }
        let values: Vec<Option<String>> =
            data_rows.iter().map(|row| row[col_index].clone()).collect();
        let dtype = infer_dtype(&values);
        dtypes.insert(column.clone(), json!(dtype));
    }
    for row in &data_rows {
        let mut json_row = Vec::new();
        for (col_index, column) in columns.iter().enumerate() {
            if Some(col_index) == drop_index {
                continue;
            }
            let dtype = dtypes
                .get(column)
                .and_then(Value::as_str)
                .unwrap_or("object");
            let value = row[col_index].as_ref();
            json_row.push(match (dtype, value) {
                ("int64", Some(text)) => {
                    text.parse::<i64>().map(Value::from).unwrap_or(Value::Null)
                }
                ("float64", Some(text)) => {
                    text.parse::<f64>().map(Value::from).unwrap_or(Value::Null)
                }
                (_, Some(text)) => Value::String(text.clone()),
                _ => Value::Null,
            });
        }
        json_rows.push(json_row);
    }
    if drop_index.is_none() {
        index_values = (0..json_rows.len())
            .map(|i| Value::from(i as i64))
            .collect();
    }
    if let Some(drop) = drop_index {
        columns.remove(drop);
    }
    Ok(DataTable {
        columns,
        dtypes,
        rows: json_rows,
        index: index_values,
        total_rows: 0,
        displayed_rows: 0,
    })
}

/// Read the current dataset as a preview table, mirroring the old worker's
/// GET_DATA command (preserve / keep-if / restore + pandas extraction).
pub(crate) fn get_data(
    engine: &StataEngine,
    if_condition: Option<&str>,
    max_rows: u32,
) -> Result<DataTable, String> {
    let max_rows = max_rows.max(1) as i64;
    let total_obs = parse_obs_count(&engine.execute("display _N")).map_err(|e| e.to_string())?;
    if total_obs == 0 {
        return Ok(DataTable {
            columns: Vec::new(),
            dtypes: serde_json::Map::new(),
            rows: Vec::new(),
            index: Vec::new(),
            total_rows: 0,
            displayed_rows: 0,
        });
    }

    if let Some(condition) = if_condition {
        engine.execute("preserve");
        let filter_result = (|| -> Result<(DataTable, i64)> {
            let gen = engine.execute("quietly gen long _stata_cli_orig_obs = _n - 1");
            if gen.rc != 0 {
                bail!(
                    "failed to generate observation index: {}",
                    gen.output.trim()
                );
            }
            let keep = engine.execute(&format!("quietly keep if {condition}"));
            if keep.rc != 0 {
                bail!("invalid if-condition: {}", keep.output.trim());
            }
            let filtered_obs =
                parse_obs_count(&engine.execute("display _N")).context("count after keep")?;
            if filtered_obs > max_rows {
                let limit = engine.execute(&format!("quietly keep in 1/{max_rows}"));
                if limit.rc != 0 {
                    bail!("row limit failed: {}", limit.output.trim());
                }
            }
            let csv = export_csv(engine)?;
            let mut table =
                read_table(&csv, Some("_stata_cli_orig_obs")).context("parse preview CSV")?;
            let _ = fs::remove_file(&csv);
            table.total_rows = filtered_obs;
            table.displayed_rows = filtered_obs.min(max_rows);
            Ok((table, filtered_obs))
        })();
        engine.execute("restore");
        match filter_result {
            Ok((table, _)) => Ok(table),
            Err(error) => Err(format!("Filter error: {error:#}")),
        }
    } else {
        let mut table = if total_obs > max_rows {
            engine.execute("preserve");
            let limit = engine.execute(&format!("quietly keep in 1/{max_rows}"));
            let csv = export_csv(engine);
            engine.execute("restore");
            match (limit.rc, csv) {
                (0, Ok(csv)) => {
                    let table = read_table(&csv, None).map_err(|e| e.to_string());
                    let _ = fs::remove_file(&csv);
                    table?
                }
                (rc, _) => return Err(format!("row limit failed (rc={rc})")),
            }
        } else {
            let csv = export_csv(engine).map_err(|e| e.to_string())?;
            let table = read_table(&csv, None).map_err(|e| e.to_string())?;
            let _ = fs::remove_file(&csv);
            table
        };
        table.total_rows = total_obs;
        table.displayed_rows = total_obs.min(max_rows);
        Ok(table)
    }
}

pub(crate) fn data_view_command(
    engine: &StataEngine,
    if_condition: Option<&str>,
    max_rows: u32,
    input_dta: Option<&Path>,
    repo_root: Option<&Path>,
    filter: &FilterOptions,
) -> Value {
    let source_dta = input_dta.map(|path| {
        let expanded = expand_user(path);
        expanded.display().to_string()
    });
    if let Some(input_path) = input_dta {
        let input_path = expand_user(input_path);
        if !input_path.is_file() {
            return json!({
                "status": "error",
                "message": format!("Input DTA file not found: {}", input_path.display())
            });
        }
        let load_code = build_selection_for_working_dir(
            &format!(
                "use \"{}\", clear",
                input_path.display().to_string().replace('\\', "/")
            ),
            None,
        );
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

fn expand_user(path: &Path) -> PathBuf {
    let rendered = path.to_string_lossy();
    if let Some(rest) = rendered.strip_prefix("~/") {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        env::current_dir().unwrap_or_default().join(path)
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
        let _ = fs::create_dir_all(parent);
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
        commands.push(format!(
            "use \"{}\", clear",
            input_path.display().to_string().replace('\\', "/")
        ));
    }
    commands.push(format!(
        "export delimited using \"{}\", replace",
        output_path.display().to_string().replace('\\', "/")
    ));
    let code = build_selection_for_working_dir(&commands.join("\n"), working_dir);
    let seed_prefix = engine.seed_prefix();
    let outcome = execute_code_with_log(engine, &code, &seed_prefix, session_id);
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

pub(crate) fn completion_snapshot(engine: &StataEngine) -> CompletionContextResult {
    let variables = match get_data(engine, None, 1) {
        Ok(table) => table.columns,
        Err(_) => Vec::new(),
    };
    let macros = {
        let seed_prefix = engine.seed_prefix();
        let outcome = execute_code_with_log(engine, "macro dir", &seed_prefix, None);
        if outcome.rc == 0 && !outcome.cancelled {
            engine.mark_seed_done();
        }
        if outcome.rc == 0 && !outcome.cancelled {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn joins_line_continuations() {
        let code = "display ///\n2+2\n";
        assert_eq!(join_stata_line_continuations(code), "display 2+2");
    }

    #[test]
    fn builds_selection_with_working_dir() {
        let temp = tempfile::tempdir().unwrap();
        let code = "use auto, clear";
        let wd = temp.path().to_string_lossy().into_owned();
        let selection = build_selection_for_working_dir(code, Some(&wd));
        assert!(selection.starts_with(&format!("cd \"{wd}\"\n")));
    }

    #[test]
    fn detects_blocked_interactive_prefixes() {
        assert!(blocked_interactive_prefix("quietly browse price").is_some());
        assert!(blocked_interactive_prefix("summarize price").is_none());
        assert!(blocked_interactive_prefix("* browse is a comment").is_none());
    }

    #[test]
    fn parses_macro_dir_names() {
        let output = "\n. macro dir\n\nglobal macros:\n  myglob : foo\n  other : bar\n\nlocal macros:\n  myloc : 1\n";
        let names = parse_macro_names(output);
        assert_eq!(names, vec!["myglob", "myloc", "other"]);
    }
}
