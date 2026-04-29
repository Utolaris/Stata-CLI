use crate::atom::boilerplate_copy::copy_tree;
use crate::atom::path_ops::boilerplate_dir;
use anyhow::{bail, Context, Result};
use serde_json::json;
use std::env;
use std::path::Path;
use std::process::Command;

pub(crate) fn init_command(repo_root: &Path) -> Result<()> {
    let source = boilerplate_dir(repo_root);
    if !source.exists() {
        bail!("Boilerplate directory not found at {}", source.display());
    }

    let target_dir = env::current_dir()?;
    copy_tree(&source, &target_dir)?;
    maybe_init_git_repo(&target_dir)?;

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "status": "success",
            "target_dir": target_dir,
            "source_dir": source,
            "message": format!("Copied boilerplate from {} into {}", source.display(), target_dir.display()),
        }))?
    );
    Ok(())
}

fn maybe_init_git_repo(target_dir: &Path) -> Result<()> {
    if is_inside_git_repo(target_dir) {
        eprintln!(
            "stata-cli init warning: {} is already inside a Git repository; skipping `git init`.",
            target_dir.display()
        );
        return Ok(());
    }

    if !git_is_available() {
        eprintln!(
            "stata-cli init warning: Git is not installed or not on PATH; skipping `git init`."
        );
        return Ok(());
    }

    let output = Command::new("git")
        .arg("init")
        .current_dir(target_dir)
        .output()
        .with_context(|| format!("Failed to run git init in {}", target_dir.display()))?;
    if output.status.success() {
        return Ok(());
    }

    bail!(
        "git init failed in {}: {}",
        target_dir.display(),
        String::from_utf8_lossy(&output.stderr).trim()
    );
}

fn is_inside_git_repo(path: &Path) -> bool {
    path.ancestors()
        .any(|candidate| candidate.join(".git").exists())
}

fn git_is_available() -> bool {
    Command::new("git")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}
