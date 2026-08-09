//! End-to-end tests against a real Stata installation (macOS default).
//! Skipped when `SKIP_STATA_TESTS` is set.

use serde_json::Value;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;
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
    if let Some(stata) = stata_home() {
        command.arg("--stata-path").arg(stata);
    }
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

#[test]
fn e2e_doctor_reports_engine_probe_ok() {
    let Some(stata) = stata_home() else {
        eprintln!("skipping e2e doctor (no Stata)");
        return;
    };
    let output = base_command()
        .args(["doctor"])
        .output()
        .expect("run doctor");
    let report = parse_success_json(&output);
    assert_eq!(report["status"], "ok", "{report}");
    let checks = report["checks"].as_array().unwrap();
    let probe = checks
        .iter()
        .find(|check| check["name"] == "engine_probe")
        .expect("engine_probe check present");
    assert_eq!(probe["status"], "ok");
    assert!(probe["detail"]
        .as_str()
        .unwrap()
        .contains(stata.to_str().unwrap()));
}

#[test]
fn e2e_file_scene_smoke_keeps_public_contract_clean() {
    let Some(_stata) = stata_home() else {
        eprintln!("skipping e2e file (no Stata)");
        return;
    };
    let smoke = repo_root().join("scene").join("smoke_test.do");
    assert!(smoke.is_file(), "scene/smoke_test.do must exist");
    let output = base_command()
        .args(["file", smoke.to_str().unwrap()])
        .output()
        .expect("run file");
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success", "{result}");
    assert!(result["log_file"].as_str().is_some());
    assert!(result["graphs"].as_array().unwrap().is_empty());
}

#[test]
fn e2e_data_view_uses_scene_fixture() {
    let Some(_stata) = stata_home() else {
        eprintln!("skipping e2e data view (no Stata)");
        return;
    };
    let fixture = repo_root().join("scene").join("grilic.dta");
    let output = base_command()
        .args([
            "data",
            "view",
            "--input-dta",
            fixture.to_str().unwrap(),
            "--max-rows",
            "5",
        ])
        .output()
        .expect("run data view");
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success");
    assert_eq!(result["rows"], 5);
    assert_eq!(result["total_rows"], 758);
    assert_eq!(
        result["source_dta"].as_str().unwrap(),
        fixture.to_str().unwrap()
    );
}

#[test]
fn e2e_export_csv_resolves_relative_output_under_working_dir() {
    let Some(_stata) = stata_home() else {
        eprintln!("skipping e2e export (no Stata)");
        return;
    };
    let temp = tempdir().unwrap();
    let fixture = repo_root().join("scene").join("grilic.dta");
    let output = base_command()
        .args([
            "data",
            "export-csv",
            "--output",
            "out.csv",
            "--input-dta",
            fixture.to_str().unwrap(),
            "--working-dir",
            temp.path().to_str().unwrap(),
            "--replace",
        ])
        .output()
        .expect("run export-csv");
    let result = parse_success_json(&output);
    assert_eq!(result["status"], "success", "{result}");
    assert!(temp.path().join("out.csv").is_file());
    assert_eq!(
        result["output_csv"].as_str().unwrap(),
        temp.path().join("out.csv").to_str().unwrap()
    );
}
