use serde_json::Value;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use tempfile::tempdir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

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

fn write_fake_git(dir: &Path) -> PathBuf {
    let path = if cfg!(windows) {
        dir.join("git.cmd")
    } else {
        dir.join("git")
    };

    if cfg!(windows) {
        fs::write(
            &path,
            "@echo off\r\nif \"%1\"==\"--version\" exit /b 0\r\nif \"%1\"==\"init\" (\r\n  mkdir .git >nul 2>nul\r\n  echo Initialized empty Git repository\r\n  exit /b 0\r\n)\r\nexit /b 1\r\n",
        )
        .unwrap();
    } else {
        fs::write(
            &path,
            "#!/bin/sh\nif [ \"$1\" = \"--version\" ]; then\n  exit 0\nfi\nif [ \"$1\" = \"init\" ]; then\n  mkdir -p .git\n  echo Initialized empty Git repository\n  exit 0\nfi\nexit 1\n",
        )
        .unwrap();
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&path).unwrap().permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&path, perms).unwrap();
        }
    }

    path
}

fn prepend_path(dir: &Path) -> std::ffi::OsString {
    let existing = env::var_os("PATH").unwrap_or_default();
    let mut paths = vec![dir.to_path_buf()];
    paths.extend(env::split_paths(&existing));
    env::join_paths(paths).unwrap()
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

fn assert_same_path(actual: &Value, expected: &Path) {
    let actual_path = PathBuf::from(actual.as_str().expect("path value should be a string"));
    let resolved_actual = std::fs::canonicalize(&actual_path).unwrap_or(actual_path);
    let resolved_expected =
        std::fs::canonicalize(expected).unwrap_or_else(|_| expected.to_path_buf());
    assert_eq!(
        normalize_windows_path(&resolved_actual),
        normalize_windows_path(&resolved_expected)
    );
}

fn contract_echo(output: &std::process::Output) -> Value {
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    serde_json::from_str(json["output"].as_str().unwrap()).unwrap()
}

#[test]
fn run_command_round_trips_through_python_backend() {
    let temp = tempdir().unwrap();
    let output = base_command()
        .arg("--session-id")
        .arg("rust-run")
        .arg("--working-dir")
        .arg(temp.path())
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
    assert!(rendered_output.contains(temp.path().to_string_lossy().as_ref()));
    assert!(!rendered_output.contains("timeout="));
}

#[test]
fn run_command_contract_passes_global_and_session_args_to_python_backend() {
    let temp = tempdir().unwrap();
    let output = base_command()
        .env("STATA_CLI_BACKEND_TEST_ECHO_ARGS", "1")
        .arg("--stata-edition")
        .arg("se")
        .arg("--log-level")
        .arg("DEBUG")
        .arg("--result-display-mode")
        .arg("full")
        .arg("--max-output-tokens")
        .arg("321")
        .arg("--multi-session")
        .arg("--max-sessions")
        .arg("7")
        .arg("--session-timeout")
        .arg("42")
        .arg("--session-id")
        .arg("rust-contract")
        .arg("--working-dir")
        .arg(temp.path())
        .arg("run")
        .arg("--code")
        .arg("display 2+2")
        .output()
        .unwrap();

    let echoed = contract_echo(&output);
    assert_eq!(echoed["command"], "run");
    assert_eq!(echoed["code"], "display 2+2");
    assert_eq!(echoed["session_id"], "rust-contract");
    assert_eq!(
        echoed["working_dir"],
        temp.path().to_string_lossy().as_ref()
    );
    assert_eq!(echoed["stata_edition"], "se");
    assert_eq!(echoed["log_level"], "DEBUG");
    assert_eq!(echoed["result_display_mode"], "full");
    assert_eq!(echoed["max_output_tokens"], 321);
    assert_eq!(echoed["multi_session"], true);
    assert_eq!(echoed["max_sessions"], 7);
    assert_eq!(echoed["session_timeout"], 42);
    assert_eq!(echoed["json"], true);
    assert_eq!(echoed["raw_output"], true);
}

#[test]
fn file_command_contract_absolutizes_paths_before_python_backend() {
    let temp = tempdir().unwrap();
    let do_file = temp.path().join("contract.do");
    fs::write(&do_file, "display 1+1\n").unwrap();

    let output = base_command()
        .env("STATA_CLI_BACKEND_TEST_ECHO_ARGS", "1")
        .arg("file")
        .arg(&do_file)
        .arg("--working-dir")
        .arg(temp.path())
        .arg("--session-id")
        .arg("file-contract")
        .output()
        .unwrap();

    let echoed = contract_echo(&output);
    assert_eq!(echoed["command"], "file");
    assert_same_path(&echoed["file_path"], &do_file);
    assert_same_path(&echoed["working_dir"], temp.path());
    assert_eq!(echoed["session_id"], "file-contract");
}

#[test]
fn data_view_contract_passes_structured_args_to_python_backend() {
    let temp = tempdir().unwrap();
    let dta_path = temp.path().join("sample.dta");
    fs::write(&dta_path, "mock dta content\n").unwrap();

    let output = base_command()
        .env("STATA_CLI_BACKEND_TEST_ECHO_ARGS", "1")
        .arg("data")
        .arg("view")
        .arg("--if-condition")
        .arg("iq > 100")
        .arg("--max-rows")
        .arg("25")
        .arg("--input-dta")
        .arg(&dta_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let echoed = &json["contract"];
    assert_eq!(echoed["command"], "data");
    assert_eq!(echoed["data_command"], "view");
    assert_eq!(echoed["if_condition"], "iq > 100");
    assert_eq!(echoed["max_rows"], 25);
    assert_same_path(&echoed["input_dta"], &dta_path);
}

#[test]
fn run_command_errors_when_working_dir_does_not_exist() {
    let missing_dir = env::temp_dir()
        .join("stata-cli-missing-working-dir")
        .join("nope");
    let output = base_command()
        .arg("--working-dir")
        .arg(&missing_dir)
        .arg("run")
        .arg("--code")
        .arg("display 1+1")
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Working directory does not exist"),
        "stderr: {stderr}"
    );
}

#[test]
fn run_command_emits_long_task_heartbeat_on_stderr_without_polluting_stdout() {
    let temp = tempdir().unwrap();
    let output = base_command()
        .env("STATA_CLI_BACKEND_TEST_SLEEP_MS", "260")
        .env("STATA_CLI_PROGRESS_INTERVAL_MS", "100")
        .arg("--session-id")
        .arg("rust-heartbeat")
        .arg("--working-dir")
        .arg(temp.path())
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
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("stata-cli: still running..."),
        "stderr: {stderr}"
    );
}

#[test]
fn file_command_returns_structured_python_result_without_artifacts() {
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
    assert_eq!(json["status"], "success");
    assert_eq!(json["session_id"], "rust-file");
    assert_eq!(json["log_file"], expected_log.to_string_lossy().as_ref());
    assert_eq!(json["graphs"], serde_json::json!([]));
    assert!(json.get("artifacts").is_none());
    assert!(json.get("artifact_count").is_none());
    let rendered_output = json["output"].as_str().unwrap();
    assert!(rendered_output.contains("mock-file"));
    assert!(rendered_output.contains("sample.do"));
    assert!(!rendered_output.contains("timeout="));
}

#[test]
fn file_command_prompts_and_continues_when_gui_command_is_confirmed() {
    let temp = tempdir().unwrap();
    let do_file = temp.path().join("gui.do");
    fs::write(&do_file, "capture browse\n").unwrap();

    let mut child = base_command()
        .arg("file")
        .arg(&do_file)
        .arg("--working-dir")
        .arg(temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .as_mut()
        .expect("stdin should exist")
        .write_all(b"y\n")
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("This command opens an interactive Stata UI"));
    assert!(stderr.contains("Continue anyway? [y/n]:"));
    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "success");
}

#[test]
fn file_command_cancels_when_gui_command_is_rejected() {
    let temp = tempdir().unwrap();
    let do_file = temp.path().join("gui.do");
    fs::write(&do_file, "quietly window manage forward results\n").unwrap();

    let mut child = base_command()
        .arg("file")
        .arg(&do_file)
        .arg("--working-dir")
        .arg(temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    child
        .stdin
        .as_mut()
        .expect("stdin should exist")
        .write_all(b"n\n")
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert!(
        !output.status.success(),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        output.stdout.is_empty(),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("This command opens an interactive Stata UI"));
    assert!(stderr.contains("Execution cancelled by user after GUI command warning"));
}

#[test]
fn file_command_cancels_on_eof_after_gui_command_warning() {
    let temp = tempdir().unwrap();
    let do_file = temp.path().join("gui.do");
    fs::write(&do_file, "shell ls\n").unwrap();

    let mut child = base_command()
        .arg("file")
        .arg(&do_file)
        .arg("--working-dir")
        .arg(temp.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    drop(child.stdin.take());

    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Continue anyway? [y/n]:"));
    assert!(stderr.contains("Execution cancelled by user after GUI command warning"));
}

#[test]
fn file_command_preserves_partial_failures_from_python_backend() {
    let temp = tempdir().unwrap();
    let do_file = temp.path().join("sample.do");
    std::fs::write(&do_file, "display 1+1\n").unwrap();

    let output = base_command()
        .env("STATA_CLI_BACKEND_TEST_PARTIAL_FAILURE", "1")
        .arg("file")
        .arg(&do_file)
        .arg("--working-dir")
        .arg(temp.path())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "success");
    assert_eq!(json["partial_failure_count"], 1);
    let failures = json["partial_failures"].as_array().unwrap();
    assert_eq!(failures.len(), 1);
    assert_eq!(failures[0]["return_code"], "r(199)");
    assert_eq!(failures[0]["message"], "command esttab is unrecognized");
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
    fs::create_dir_all(&target).unwrap();

    let fake_bin = temp.path().join("fake-bin");
    fs::create_dir_all(&fake_bin).unwrap();
    write_fake_git(&fake_bin);

    let mut command = base_command();
    std::process::Command::env(&mut command, "PATH", prepend_path(&fake_bin));
    let output = command.arg("init").current_dir(&target).output().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let resolved_target = std::fs::canonicalize(&target).unwrap();
    assert_eq!(json["status"], "success");
    assert_same_path(&json["target_dir"], &resolved_target);
    assert!(target.join("AGENTS.md").exists());
    let agents_text = fs::read_to_string(target.join("AGENTS.md")).unwrap();
    assert!(
        agents_text.contains("Do not use Stata GUI-only commands in `.do` files or CLI snippets")
    );
    assert!(target
        .join("skills")
        .join("stata-cli")
        .join("SKILL.md")
        .exists());
    assert!(target.join("data").is_dir());
    assert!(target.join("do").join("analysis.do").exists());
    assert!(target.join("outputs").is_dir());
    assert!(target.join("scripts").join("plot.py").exists());
    assert!(target.join(".git").is_dir());
    assert!(!target.join("stata-packages.md").exists());
}

#[test]
fn init_command_warns_when_directory_is_already_in_git_repo() {
    let temp = tempdir().unwrap();
    let repo_root = temp.path().join("repo");
    let target = repo_root.join("nested").join("my-analysis");
    fs::create_dir_all(&target).unwrap();
    fs::create_dir_all(repo_root.join(".git")).unwrap();
    fs::write(target.join("AGENTS.md"), "existing\n").unwrap();

    let fake_bin = temp.path().join("fake-bin");
    fs::create_dir_all(&fake_bin).unwrap();
    write_fake_git(&fake_bin);

    let mut command = base_command();
    std::process::Command::env(&mut command, "PATH", prepend_path(&fake_bin));
    let output = command.arg("init").current_dir(&target).output().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    let resolved_target = std::fs::canonicalize(&target).unwrap();
    assert_eq!(json["status"], "success");
    assert_same_path(&json["target_dir"], &resolved_target);
    let agents_text = fs::read_to_string(target.join("AGENTS.md")).unwrap();
    assert!(agents_text.contains("Keep main Stata analysis in `do/analysis.do`."));
    assert!(
        agents_text.contains("Do not use Stata GUI-only commands in `.do` files or CLI snippets")
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("already inside a Git repository"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn init_command_warns_when_git_is_missing() {
    let temp = tempdir().unwrap();
    let target = temp.path().join("my-analysis");
    fs::create_dir_all(&target).unwrap();

    let missing_path = temp.path().join("missing-git-bin");
    fs::create_dir_all(&missing_path).unwrap();

    let mut command = base_command();
    std::process::Command::env(&mut command, "PATH", &missing_path);
    let output = command.arg("init").current_dir(&target).output().unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "success");
    assert!(target.join("AGENTS.md").exists());
    assert!(!target.join(".git").exists());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Git is not installed"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn repl_command_runs_native_loop_and_exits() {
    let temp = tempdir().unwrap();
    let mut child = base_command()
        .arg("--working-dir")
        .arg(temp.path())
        .arg("repl")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().expect("repl stdin should exist");
        stdin.write_all(b"display 2+3\n:exit\n").unwrap();
    }

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("5"), "stdout: {stdout}");
}

#[test]
fn bridge_command_returns_completion_snapshot_in_test_mode() {
    let repo_root = repo_root();
    let python = project_python(&repo_root);
    let fake_stata = env::temp_dir().join("stata-cli-fake-stata");
    std::fs::create_dir_all(&fake_stata).unwrap();

    let mut child = Command::new(python)
        .current_dir(&repo_root)
        .env("STATA_CLI_BACKEND_TEST_MODE", "1")
        .env("PYTHONPATH", repo_root.join("src"))
        .arg("-m")
        .arg("stata_cli.entry.backend_main")
        .arg("--stata-path")
        .arg(&fake_stata)
        .arg("--raw-output")
        .arg("bridge")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();

    {
        let stdin = child.stdin.as_mut().expect("bridge stdin should exist");
        stdin
            .write_all(b"{\"command\":\"complete_context\"}\n")
            .unwrap();
    }

    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let first_line = stdout.lines().next().unwrap_or_default();
    let json: Value = serde_json::from_str(first_line).unwrap();
    assert_eq!(json["status"], "success");
    assert!(json["variables"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item == "iq"));
    assert!(json["macros"]
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item == "sample_macro"));
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
fn data_export_csv_resolves_relative_output_against_working_dir() {
    let temp = tempdir().unwrap();
    let working_dir = temp.path().join("outputs");
    let dta_path = temp.path().join("sample.dta");
    fs::write(&dta_path, "mock dta content\n").unwrap();

    let export_output = base_command()
        .arg("data")
        .arg("export-csv")
        .arg("--input-dta")
        .arg(&dta_path)
        .arg("--working-dir")
        .arg(&working_dir)
        .arg("--output")
        .arg("result.csv")
        .arg("--replace")
        .output()
        .unwrap();

    assert!(
        export_output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&export_output.stderr)
    );

    let export_json: Value = serde_json::from_slice(&export_output.stdout).unwrap();
    let expected_output = working_dir.join("result.csv");
    assert_eq!(export_json["status"], "success");
    assert_eq!(
        export_json["output_csv"],
        expected_output.to_string_lossy().as_ref()
    );
    assert!(expected_output.exists());
}

#[test]
fn data_view_requires_explicit_input_dta_and_defaults_to_50_rows() {
    let temp = tempdir().unwrap();
    let dta_path = temp.path().join("sample.dta");
    std::fs::write(&dta_path, "mock dta content\n").unwrap();

    let output = base_command()
        .arg("data")
        .arg("view")
        .arg("--input-dta")
        .arg(&dta_path)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let json: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(json["status"], "success");
    assert_eq!(json["max_rows"], 50);
    assert_eq!(json["source_dta"], dta_path.to_string_lossy().as_ref());
}
