use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) fn home_dir() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("USERPROFILE").map(PathBuf::from)
    } else {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}

pub(crate) fn default_config_path() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|dir| dir.join("stata-cli").join("config.toml"))
    } else {
        home_dir().map(|home| home.join(".config").join("stata-cli").join("config.toml"))
    }
}

pub(crate) fn backend_entry(repo_root: &Path) -> PathBuf {
    repo_root
        .join("src")
        .join("stata_cli")
        .join("entry")
        .join("backend_main.py")
}

pub(crate) fn project_python(repo_root: &Path) -> PathBuf {
    if cfg!(windows) {
        repo_root.join(".venv").join("Scripts").join("python.exe")
    } else {
        repo_root.join(".venv").join("bin").join("python")
    }
}

pub(crate) fn is_repo_root(path: &Path) -> bool {
    path.join("pyproject.toml").exists() && backend_entry(path).exists()
}

pub(crate) fn discover_repo_root_from(start: &Path) -> Option<PathBuf> {
    let start_path = if start.is_file() {
        start.parent()?
    } else {
        start
    };
    for candidate in start_path.ancestors() {
        if is_repo_root(candidate) {
            return Some(candidate.to_path_buf());
        }
    }
    None
}

pub(crate) fn normalize_repo_root(path: &Path) -> Option<PathBuf> {
    let candidate = if path.exists() {
        discover_repo_root_from(path)
    } else {
        None
    }?;
    fs::canonicalize(candidate).ok()
}

pub(crate) fn absolutize_cli_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() {
        return Ok(path.to_path_buf());
    }

    let cwd = std::env::current_dir()
        .with_context(|| "Failed to resolve the current working directory".to_string())?;
    Ok(cwd.join(path))
}

pub(crate) fn windows_default_stata_path() -> PathBuf {
    PathBuf::from(r"C:\Program Files\Stata18")
}

pub(crate) fn validate_stata_path(path: &Path) -> Result<()> {
    if !path.exists() {
        bail!("Stata path does not exist: {}", path.display());
    }
    if !path.is_dir() {
        bail!("Stata path is not a directory: {}", path.display());
    }
    Ok(())
}
