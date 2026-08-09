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

fn require_stata() -> PathBuf {
    stata_home()
        .unwrap_or_else(|| panic!("SKIP_STATA_TESTS is unset but no Stata installation was found"))
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
    let stata = require_stata();
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
    let stata = require_stata();
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
    assert!(stderr.contains("--stata-path is not a directory"));
}

#[test]
fn file_command_returns_structured_result_with_log_file() {
    let stata = require_stata();
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
    let stata = require_stata();
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
    let stata = require_stata();
    let temp = tempdir().unwrap();
    let do_file = temp.path().join("browse.do");
    fs::write(&do_file, "browse price\n").unwrap();

    let mut command = base_command();
    command
        .args([
            "--stata-path",
            stata.to_str().unwrap(),
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
        .write_all(b"display 2+2\n:exit\n")
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
    let stata = require_stata();
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
    let stata = require_stata();
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
    let stata = require_stata();
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
    let stata = require_stata();
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
