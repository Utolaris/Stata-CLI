//! Integration tests for the native (Python-free) stata-cli.
//!
//! Tests that require a real Stata installation are skipped when
//! `SKIP_STATA_TESTS` is set (the CI workflow sets it; local runs use the
//! macOS default `/Applications/Stata`).

use serde_json::Value;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::tempdir;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("rust-cli should live under the repo root")
        .to_path_buf()
}

fn stata_home() -> Option<PathBuf> {
    if env::var_os("SKIP_STATA_TESTS").is_some() {
        return None;
    }
    env::var_os("STATA_PATH").map(PathBuf::from).or_else(|| {
        ["/Applications/StataNow", "/Applications/Stata"]
            .iter()
            .map(PathBuf::from)
            .find(|path| path.is_dir())
    })
}

fn base_command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_stata-cli"));
    command.env("STATA_CLI_PROJECT_ROOT", repo_root());
    command
}

fn parse_success_json(output: &std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

fn parse_json(output: &std::process::Output) -> Value {
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
        panic!(
            "invalid JSON: {error}; stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn run_output(command: &mut Command) -> std::process::Output {
    command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("failed to run stata-cli")
}

#[test]
fn run_command_round_trips_through_native_engine() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping test (no Stata)");
        return;
    };
    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "run",
        "--code",
        "display 2+2",
    ]));
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success");
    assert!(result["output"].as_str().unwrap().contains("4"));
    assert_eq!(result["session_id"], "default");
    assert_eq!(result["error"], Value::Null);
}

#[test]
fn run_command_accepts_non_ascii_output() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping test (no Stata)");
        return;
    };
    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "run",
        "--code",
        r#"display "你好，Stata""#,
    ]));
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success");
    assert!(result["output"].as_str().unwrap().contains("你好，Stata"));
}

#[test]
fn run_command_errors_when_working_dir_does_not_exist() {
    let output = run_output(base_command().args([
        "--working-dir",
        "/definitely/not/a/real/dir",
        "run",
        "--code",
        "display 1",
    ]));
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Working directory does not exist"));
}

#[test]
fn run_command_rejects_invalid_stata_path() {
    let output = run_output(base_command().args([
        "--stata-path",
        "/definitely/not/stata",
        "run",
        "--code",
        "display 1",
    ]));
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The exact sentence differs per platform (macOS validates in the engine
    // resolver, Windows in the path resolver); the stable contract is that
    // the flag itself is reported as invalid.
    assert!(stderr.contains("--stata-path"), "{stderr}");
}

#[test]
fn help_describes_public_commands_and_options() {
    let output = run_output(base_command().arg("--help"));
    assert!(output.status.success());
    let help = String::from_utf8_lossy(&output.stdout);
    for expected in [
        "Run inline Stata commands",
        "Run a .do file",
        "Diagnose the local Stata engine",
        "Path to the Stata installation",
        "Working directory",
    ] {
        assert!(help.contains(expected), "missing {expected:?} in --help");
    }
}

#[test]
fn file_command_returns_structured_result_with_log_file() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping test (no Stata)");
        return;
    };
    let temp = tempdir().unwrap();
    let do_file = temp.path().join("smoke.do");
    fs::write(&do_file, "display 2+2\n").unwrap();

    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "file",
        do_file.to_str().unwrap(),
    ]));
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success");
    assert!(result["output"].as_str().unwrap().contains("4"));
    let log_file = result["log_file"].as_str().unwrap();
    assert!(Path::new(log_file).is_file());
    assert_eq!(result["partial_failure_count"], 0);
}

#[test]
fn file_command_reports_error_with_raw_output_for_failing_do_file() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping test (no Stata)");
        return;
    };
    let temp = tempdir().unwrap();
    let do_file = temp.path().join("partial.do");
    fs::write(&do_file, "display 1\ntotally_unknown_command_xyz\n").unwrap();

    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "file",
        do_file.to_str().unwrap(),
    ]));
    let result = parse_json(&output);
    // The previous pystata backend raised on any failing command, so a
    // do-file with a real error is `error` with the raw output as the message.
    assert_eq!(result["status"], "error");
    assert_eq!(result["output"], "");
    assert!(result["error"]
        .as_str()
        .unwrap()
        .contains("totally_unknown_command_xyz"));
}

#[test]
fn file_command_cancels_when_gui_command_is_rejected() {
    // The GUI-command guard runs before the Stata engine is loaded, so this
    // test works without a real Stata installation.
    let temp = tempdir().unwrap();
    let do_file = temp.path().join("browse.do");
    fs::write(&do_file, "browse price\n").unwrap();

    let mut command = base_command();
    command
        .args([
            "--stata-path",
            "/definitely/not/stata",
            "file",
            do_file.to_str().unwrap(),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child.stdin.as_mut().unwrap().write_all(b"n\n").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Execution cancelled by user"));
}

#[test]
fn doctor_command_reports_native_engine_probe() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping doctor probe test (no Stata)");
        return;
    };
    let output =
        run_output(base_command().args(["--stata-path", stata.to_str().unwrap(), "doctor"]));
    let report = parse_success_json(&output);
    assert_eq!(report["status"], "ok");
    let checks = report["checks"].as_array().unwrap();
    assert!(checks.iter().any(|check| check["name"] == "engine_probe"));
    assert!(!checks.iter().any(|check| check["name"] == "python"));
}

#[test]
fn init_command_creates_agent_workspace_scaffold() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("workspace");
    fs::create_dir_all(&target).unwrap();
    let output = run_output(base_command().args(["init"]).current_dir(&target));
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(target.join("do").is_dir());
    assert!(target.join("outputs").is_dir());
    assert!(target.join("data").is_dir());
    assert!(target
        .join("skills")
        .join("stata-cli")
        .join("SKILL.md")
        .is_file());
}

#[test]
fn init_copies_boilerplate_from_template_dir_env() {
    let workspace = tempdir().unwrap();
    let templates = tempdir().unwrap();
    fs::create_dir_all(templates.path().join("do")).unwrap();
    fs::write(templates.path().join("AGENTS.md"), "agents template\n").unwrap();
    fs::write(
        templates.path().join("do").join("analysis.do"),
        "display 1\n",
    )
    .unwrap();

    let output = run_output(
        base_command()
            .env("STATA_CLI_TEMPLATE_DIR", templates.path())
            .current_dir(workspace.path())
            .args(["init"]),
    );
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success");
    assert_eq!(
        fs::read_to_string(workspace.path().join("AGENTS.md")).unwrap(),
        "agents template\n"
    );
    assert!(workspace.path().join("do").join("analysis.do").is_file());
}

#[test]
fn run_help_regress_renders_local_help_text() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping help render test (no Stata)");
        return;
    };
    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "run",
        "--code",
        "help regress",
    ]));
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success");
    let text = result["output"].as_str().unwrap();
    assert!(text.contains("regress"), "{text}");
    assert!(!text.contains('{'), "{text}");
}

#[test]
fn run_help_without_topic_returns_guidance() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping help guidance test (no Stata)");
        return;
    };
    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "run",
        "--code",
        "help",
    ]));
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success");
    assert!(
        result["output"].as_str().unwrap().contains("needs a topic"),
        "{}",
        result["output"]
    );
}

#[test]
fn run_help_unknown_topic_returns_guidance() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping help guidance test (no Stata)");
        return;
    };
    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "run",
        "--code",
        "help no_such_topic_xyz",
    ]));
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success");
    assert!(
        result["output"]
            .as_str()
            .unwrap()
            .contains("No local help file"),
        "{}",
        result["output"]
    );
}

#[test]
fn run_search_returns_guidance() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping search guidance test (no Stata)");
        return;
    };
    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "run",
        "--code",
        "search regress",
    ]));
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success");
    assert!(
        result["output"].as_str().unwrap().contains("search window"),
        "{}",
        result["output"]
    );
}

#[test]
fn repl_command_runs_native_loop_and_exits() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping repl test (no Stata)");
        return;
    };
    let mut command = base_command();
    command
        .args(["--stata-path", stata.to_str().unwrap(), "repl"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    child
        .stdin
        .as_mut()
        .unwrap()
        .write_all(b"display 2+2\nquit\n")
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("4"));
}

#[test]
fn data_view_requires_explicit_input_dta_and_defaults_to_50_rows() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping test (no Stata)");
        return;
    };
    let fixture = repo_root().join("scene").join("grilic.dta");
    assert!(fixture.is_file(), "scene/grilic.dta must exist");

    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "data",
        "view",
        "--input-dta",
        fixture.to_str().unwrap(),
    ]));
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success");
    assert_eq!(result["max_rows"], 50);
    assert_eq!(result["total_rows"], 758);
    assert_eq!(result["displayed_rows"], 50);
    assert_eq!(result["rows"], 50);
    assert!(result["columns"]
        .as_array()
        .unwrap()
        .contains(&"lnw".into()));
}

#[test]
fn data_view_supports_if_condition_and_row_limit() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping test (no Stata)");
        return;
    };
    let fixture = repo_root().join("scene").join("grilic.dta");
    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "data",
        "view",
        "--input-dta",
        fixture.to_str().unwrap(),
        "--if-condition",
        "iq > 120",
        "--max-rows",
        "3",
    ]));
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success");
    assert_eq!(result["rows"], 3);
    assert_eq!(result["displayed_rows"], 3);
    assert!(result["total_rows"].as_i64().unwrap() > 3);
    let index = result["index"].as_array().unwrap();
    assert_eq!(index.len(), 3);
    // index keeps original observation positions (0-based)
    assert!(index.iter().any(|value| value.as_i64().unwrap() > 0));
}

#[test]
fn data_export_csv_resolves_relative_output_against_working_dir() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping test (no Stata)");
        return;
    };
    let temp = tempdir().unwrap();
    let fixture = repo_root().join("scene").join("grilic.dta");
    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "data",
        "export-csv",
        "--output",
        "out.csv",
        "--input-dta",
        fixture.to_str().unwrap(),
        "--replace",
        "--working-dir",
        temp.path().to_str().unwrap(),
    ]));
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success");
    let csv = temp.path().join("out.csv");
    assert!(csv.is_file());
    let header = fs::read_to_string(&csv).unwrap();
    assert!(header.starts_with("rns,mrt,smsa"));
}

#[test]
fn data_view_ignores_global_working_dir() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping test (no Stata)");
        return;
    };
    let fixture = repo_root().join("scene").join("grilic.dta");
    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "--working-dir",
        "/tmp",
        "data",
        "view",
        "--input-dta",
        fixture.to_str().unwrap(),
        "--max-rows",
        "2",
    ]));
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success");
    assert_eq!(result["rows"], 2);
}

#[test]
fn run_survives_user_log_close_inside_code() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping test (no Stata)");
        return;
    };
    // The user's own `capture log close` must not disable output capture:
    // capture drains Stata's output buffer, not a user-controllable log.
    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "run",
        "--code",
        "capture log close _all\ndisplay \"still alive\"",
    ]));
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success", "{result}");
    assert!(result["output"].as_str().unwrap().contains("still alive"));
}

#[test]
fn file_log_contains_full_output_even_when_do_file_closes_logs() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping test (no Stata)");
        return;
    };
    let temp = tempdir().unwrap();
    let do_file = temp.path().join("self_closing.do");
    fs::write(
        &do_file,
        "capture log close _all\ndisplay \"after-close\"\n",
    )
    .unwrap();

    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "file",
        do_file.to_str().unwrap(),
    ]));
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success", "{result}");
    let log_file = result["log_file"].as_str().unwrap();
    let log_content = fs::read_to_string(log_file).unwrap();
    assert!(log_content.contains("after-close"), "{log_content}");
}

fn make_labeled_dataset(stata: &Path, dta_path: &Path) {
    let code = format!(
        "clear\nset obs 3\ngen byte foreign = _n - 1\n\
         label define origin 0 \"Domestic\" 1 \"Foreign\"\n\
         label values foreign origin\ngen str6 code = \"00123\"\n\
         gen double score = 1.5 in 1\nreplace score = . in 2/3\n\
         save {}, replace\n",
        dta_path.display()
    );
    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "run",
        "--code",
        &code,
    ]));
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success", "{result}");
}

#[test]
fn data_view_uses_numeric_codes_and_source_types_for_labeled_data() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping test (no Stata)");
        return;
    };
    let temp = tempdir().unwrap();
    let dta = temp.path().join("labeled.dta");
    make_labeled_dataset(&stata, &dta);

    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "data",
        "view",
        "--input-dta",
        dta.to_str().unwrap(),
        "--max-rows",
        "10",
    ]));
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success", "{result}");

    let columns = result["columns"].as_array().unwrap();
    let foreign_index = columns.iter().position(|c| c == "foreign").unwrap();
    let code_index = columns.iter().position(|c| c == "code").unwrap();
    let score_index = columns.iter().position(|c| c == "score").unwrap();

    // Value labels are exported as numeric codes (nolabel), matching the old
    // pdataframe_from_data(valuelabel=False) behavior.
    let rows = result["data"].as_array().unwrap();
    assert_eq!(rows[0][foreign_index], 0);
    assert_eq!(rows[1][foreign_index], 1);
    assert_eq!(result["dtypes"][&*"foreign".to_string()], "int64");

    // String columns keep leading zeros instead of being coerced to numbers.
    assert_eq!(rows[0][code_index], "00123");
    assert_eq!(result["dtypes"][&*"code".to_string()], "object");

    // Missing numeric values become null, not "0".
    assert_eq!(rows[1][score_index], Value::Null);
    assert_eq!(result["dtypes"][&*"score".to_string()], "float64");
}

#[test]
fn data_view_preserves_dtypes_when_filter_matches_nothing() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping test (no Stata)");
        return;
    };
    let temp = tempdir().unwrap();
    let dta = temp.path().join("labeled.dta");
    make_labeled_dataset(&stata, &dta);

    let output = run_output(base_command().args([
        "--stata-path",
        stata.to_str().unwrap(),
        "data",
        "view",
        "--input-dta",
        dta.to_str().unwrap(),
        "--if-condition",
        "foreign == 9",
        "--max-rows",
        "10",
    ]));
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success", "{result}");
    assert_eq!(result["rows"], 0);
    assert_eq!(result["total_rows"], 0);
    // Dtypes come from Stata metadata, not from sample values.
    assert_eq!(result["dtypes"][&*"foreign".to_string()], "int64");
    assert_eq!(result["dtypes"][&*"score".to_string()], "float64");
    assert_eq!(result["dtypes"][&*"code".to_string()], "object");
}
