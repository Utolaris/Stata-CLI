use anyhow::{bail, Context, Result};
use std::env;
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

pub(crate) fn boilerplate_dir(repo_root: &Path) -> PathBuf {
    repo_root.join("boilerplate")
}

pub(crate) fn is_repo_root(path: &Path) -> bool {
    path.join("rust-cli").join("Cargo.toml").exists()
        && path.join("boilerplate").is_dir()
        && path.join("bin").is_dir()
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

pub(crate) fn repl_history_path() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|dir| dir.join("stata-cli").join("repl_history.txt"))
    } else {
        home_dir().map(|home| home.join(".stata-cli").join("repl_history.txt"))
    }
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

pub(crate) fn validate_existing_working_dir(path: &Path) -> Result<PathBuf> {
    let resolved = absolutize_cli_path(path)?;
    if !resolved.exists() {
        bail!("Working directory does not exist: {}", resolved.display());
    }
    if !resolved.is_dir() {
        bail!(
            "Working directory is not a directory: {}",
            resolved.display()
        );
    }
    Ok(resolved)
}

/// Port of the old backend's `resolve_do_file_path`: absolute paths are used
/// directly; relative paths are tried against cwd and a shallow (2-level)
/// recursive scan.
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
        let mut stack: Vec<(PathBuf, usize)> = vec![(cwd, 0)];
        while let Some((dir, depth)) = stack.pop() {
            let Ok(entries) = fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Old backend stops descending past depth 2, but it does
                    // still inspect files at depth 2.
                    if depth < 2 {
                        stack.push((path, depth + 1));
                    }
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

/// Resolve a CLI-provided output path, anchoring relative paths to the
/// working directory (or cwd) and expanding a leading `~`.
pub(crate) fn resolve_output_path(output: &Path, working_dir: Option<&str>) -> PathBuf {
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

/// Expand a leading `~/` and make the path absolute (used for `.dta` inputs).
pub(crate) fn expand_user(path: &Path) -> PathBuf {
    let expanded = expand_tilde(path);
    if expanded.is_absolute() {
        expanded
    } else {
        env::current_dir().unwrap_or_default().join(expanded)
    }
}

/// Expand a leading `~/` only; does not resolve relative paths.
fn expand_tilde(path: &Path) -> PathBuf {
    let rendered = path.to_string_lossy();
    if let Some(rest) = rendered.strip_prefix("~/") {
        if let Ok(home) = env::var("HOME") {
            return PathBuf::from(home).join(rest);
        }
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn relative_do_file_search_reaches_two_levels_but_not_three() {
        let temp = tempdir().unwrap();
        let level1 = temp.path().join("a");
        let level2 = level1.join("b");
        let level3 = level2.join("c");
        fs::create_dir_all(&level3).unwrap();
        fs::write(level2.join("target.do"), "display 1\n").unwrap();
        fs::write(level3.join("target.do"), "display 1\n").unwrap();

        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(temp.path()).unwrap();
        let (found, tried) = resolve_do_file_path(Path::new("target.do"));
        std::env::set_current_dir(original).unwrap();

        // The old backend stops descending past depth 2 but still inspects
        // files at depth 2, so the level-2 file is found.
        assert_eq!(
            found,
            Some(fs::canonicalize(level2.join("target.do")).unwrap())
        );
        assert!(tried.iter().any(|p| p.contains("a/b/target.do")));
    }
}
