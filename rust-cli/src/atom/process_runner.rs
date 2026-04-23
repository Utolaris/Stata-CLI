use anyhow::{bail, Context, Result};
use std::path::Path;
use std::process::{Command, Stdio};

pub(crate) fn inspect_python_version(python: &Path) -> Result<String> {
    let output = Command::new(python)
        .args([
            "-c",
            "import sys; print(f'{sys.version_info[0]}.{sys.version_info[1]}')",
        ])
        .output()
        .with_context(|| format!("Failed to inspect Python version for {}", python.display()))?;

    if !output.status.success() {
        bail!("Python version check failed for {}", python.display());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub(crate) fn configure_pythonpath(command: &mut Command, repo_root: &Path) {
    let src_dir = repo_root.join("src");
    let separator = if cfg!(windows) { ";" } else { ":" };
    let mut value = src_dir.to_string_lossy().to_string();
    if let Some(existing) = std::env::var_os("PYTHONPATH") {
        let existing_rendered = existing.to_string_lossy();
        if !existing_rendered.is_empty() {
            value.push_str(separator);
            value.push_str(&existing_rendered);
        }
    }
    command.env("PYTHONPATH", value);
}

pub(crate) fn backend_command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--help")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok()
}
