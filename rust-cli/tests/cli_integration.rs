use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::tempdir;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("rust-cli should live under the repo root")
        .to_path_buf()
}

fn project_python(repo_root: &Path) -> PathBuf {
    if cfg!(windows) {
        repo_root.join(".venv").join("Scripts").join("python.exe")
    } else {
        repo_root.join(".venv").join("bin").join("python")
    }
}

fn base_command() -> Command {
    let repo_root = repo_root();
    let python = project_python(&repo_root);
    let mut command = Command::new(env!("CARGO_BIN_EXE_stata-cli"));
    command.env("STATA_CLI_PROJECT_ROOT", &repo_root);
    command.env("STATA_CLI_BACKEND_TEST_MODE", "1");
    command.arg("--python").arg(python);
    command
}

#[test]
fn run_command_round_trips_through_python_backend() {
    let temp = tempdir().unwrap();
    let output = base_command()
        .arg("--json")
        .arg("--session-id")
        .arg("rust-run")
        .arg("--working-dir")
        .arg(temp.path())
        .arg("--timeout")
        .arg("17")
        .arg("run")
        .arg("--code")
        .arg("display 1+1")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "success");
    assert_eq!(json["session_id"], "rust-run");
    let rendered_output = json["output"].as_str().unwrap();
    assert!(rendered_output.contains("mock-run"));
    assert!(rendered_output.contains("display 1+1"));
    assert!(rendered_output.contains("timeout=17"));
    assert!(rendered_output.contains(temp.path().to_string_lossy().as_ref()));
}

#[test]
fn file_command_returns_structured_python_artifacts() {
    let temp = tempdir().unwrap();
    let do_file = temp.path().join("sample.do");
    std::fs::write(&do_file, "display 1+1\n").unwrap();

    let output = base_command()
        .arg("--json")
        .arg("--session-id")
        .arg("rust-file")
        .arg("--working-dir")
        .arg(temp.path())
        .arg("--timeout")
        .arg("45")
        .arg("file")
        .arg(&do_file)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "success");
    assert_eq!(json["session_id"], "rust-file");
    assert_eq!(json["log_file"], "/tmp/sample.log");
    assert_eq!(json["graphs"][0]["path"], "/tmp/sample.png");
    let rendered_output = json["output"].as_str().unwrap();
    assert!(rendered_output.contains("mock-file"));
    assert!(rendered_output.contains("sample.do"));
    assert!(rendered_output.contains("timeout=45"));
}

#[test]
fn doctor_command_checks_python_backend_probe() {
    let output = base_command().arg("--json").arg("doctor").output().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "ok");
    let checks = json["checks"].as_array().unwrap();
    assert!(checks
        .iter()
        .any(|check| check["name"] == "backend_probe" && check["status"] == "ok"));
}
