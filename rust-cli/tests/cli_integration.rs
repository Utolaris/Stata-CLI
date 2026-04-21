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
    let fake_stata = env::temp_dir().join("stata-cli-fake-stata");
    std::fs::create_dir_all(&fake_stata).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_stata-cli"));
    command.env("STATA_CLI_PROJECT_ROOT", &repo_root);
    command.env("STATA_CLI_BACKEND_TEST_MODE", "1");
    command.arg("--python").arg(python);
    command.arg("--stata-path").arg(fake_stata);
    command
}

fn normalize_windows_path(path: &Path) -> String {
    let rendered = path.to_string_lossy().to_string();
    if cfg!(windows) {
        rendered
            .strip_prefix(r"\\?\")
            .unwrap_or(&rendered)
            .to_string()
    } else {
        rendered
    }
}

#[test]
fn run_command_round_trips_through_python_backend() {
    let temp = tempdir().unwrap();
    let output = base_command()
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
        .arg("--session-id")
        .arg("rust-file")
        .arg("file")
        .arg(&do_file)
        .arg("--working-dir")
        .arg(temp.path())
        .arg("--timeout")
        .arg("45")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let temp_base = env::temp_dir();
    let expected_log = temp_base.join("sample.log");
    let expected_graph = temp_base.join("sample.png");
    assert_eq!(json["status"], "success");
    assert_eq!(json["session_id"], "rust-file");
    assert_eq!(json["log_file"], expected_log.to_string_lossy().as_ref());
    assert_eq!(
        json["graphs"][0]["path"],
        expected_graph.to_string_lossy().as_ref()
    );
    let rendered_output = json["output"].as_str().unwrap();
    assert!(rendered_output.contains("mock-file"));
    assert!(rendered_output.contains("sample.do"));
    assert!(rendered_output.contains("timeout=45"));
}

#[test]
fn doctor_command_checks_python_backend_probe() {
    let output = base_command().arg("doctor").output().unwrap();

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

#[test]
fn init_command_creates_agent_workspace_scaffold() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("my-analysis");

    let output = base_command().arg("init").arg(&target).output().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let resolved_target = std::fs::canonicalize(&target).unwrap();
    assert_eq!(json["status"], "success");
    assert_eq!(json["target_dir"], normalize_windows_path(&resolved_target));
    assert!(target.join("AGENTS.md").exists());
    assert!(target.join("data").is_dir());
    assert!(target.join("do").join("analysis.do").exists());
    assert!(target.join("outputs").is_dir());
    assert!(target.join("scripts").join("plot.py").exists());
    assert!(!target.join("stata-packages.md").exists());
}

#[test]
fn init_command_errors_on_existing_scaffold_file() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("my-analysis");
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("AGENTS.md"), "existing\n").unwrap();

    let output = base_command().arg("init").arg(&target).output().unwrap();

    assert!(!output.status.success());

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected_conflict = std::fs::canonicalize(target.join("AGENTS.md")).unwrap();
    assert_eq!(json["status"], "error");
    assert_eq!(
        json["conflicts"][0],
        normalize_windows_path(&expected_conflict)
    );
}

#[test]
fn data_commands_round_trip_through_python_backend() {
    let temp = tempdir().unwrap();
    let csv_path = temp.path().join("export.csv");
    let dta_path = temp.path().join("sample.dta");
    std::fs::write(&dta_path, "mock dta content\n").unwrap();

    let view_output = base_command()
        .arg("data")
        .arg("view")
        .arg("--max-rows")
        .arg("250")
        .arg("--input-dta")
        .arg(&dta_path)
        .output()
        .unwrap();

    assert!(
        view_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&view_output.stderr)
    );
    let view_json: Value = serde_json::from_slice(&view_output.stdout).unwrap();
    assert_eq!(view_json["status"], "success");
    assert_eq!(view_json["columns"][0], "x");
    assert_eq!(view_json["max_rows"], 250);
    assert_eq!(view_json["source_dta"], dta_path.to_string_lossy().as_ref());

    let export_output = base_command()
        .arg("data")
        .arg("export-csv")
        .arg("--input-dta")
        .arg(&dta_path)
        .arg("--output")
        .arg(&csv_path)
        .arg("--replace")
        .output()
        .unwrap();

    assert!(
        export_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&export_output.stderr)
    );
    let export_json: Value = serde_json::from_slice(&export_output.stdout).unwrap();
    assert_eq!(export_json["status"], "success");
    assert_eq!(
        export_json["output_csv"],
        csv_path.to_string_lossy().as_ref()
    );
    assert!(csv_path.exists());
}

#[test]
fn data_view_defaults_to_50_rows() {
    let output = base_command().arg("data").arg("view").output().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "success");
    assert_eq!(json["max_rows"], 50);
}
