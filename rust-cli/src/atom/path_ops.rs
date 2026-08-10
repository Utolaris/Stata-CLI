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
    repo_root
        .join("skill")
        .join("stata-cli")
        .join("boilerplate")
}

pub(crate) const TEMPLATE_DIR_ENV: &str = "STATA_CLI_TEMPLATE_DIR";

/// Candidate template locations relative to the binary directory. The skill
/// package keeps `bin/` and `boilerplate/` as siblings
/// (`<skill>/bin/stata-cli` next to `<skill>/boilerplate`), and a copy next
/// to the binary itself is accepted as a fallback for local builds.
pub(crate) fn template_dir_candidates(exe_dir: &Path) -> Vec<PathBuf> {
    vec![
        exe_dir.join("..").join("boilerplate"),
        exe_dir.join("boilerplate"),
    ]
}

pub(crate) fn first_existing_dir(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates
        .iter()
        .find(|candidate| candidate.is_dir())
        .cloned()
}

fn canonicalize_or_keep(path: PathBuf) -> PathBuf {
    fs::canonicalize(&path).unwrap_or(path)
}

/// Resolve the init template directory: `STATA_CLI_TEMPLATE_DIR` > locations
/// relative to the executable > legacy repository-root discovery (dev
/// checkouts). The binary no longer depends on a cloned repository.
pub(crate) fn resolve_template_dir() -> Option<PathBuf> {
    if let Some(value) = env::var_os(TEMPLATE_DIR_ENV) {
        let candidate = PathBuf::from(value);
        if candidate.is_dir() {
            return Some(canonicalize_or_keep(candidate));
        }
    }

    if let Ok(exe) = env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            if let Some(found) = first_existing_dir(&template_dir_candidates(exe_dir)) {
                return Some(canonicalize_or_keep(found));
            }
        }
    }

    if let Ok(cwd) = env::current_dir() {
        if let Some(repo_root) = discover_repo_root_from(&cwd) {
            let candidate = boilerplate_dir(&repo_root);
            if candidate.is_dir() {
                return Some(canonicalize_or_keep(candidate));
            }
        }
    }
    if let Ok(exe) = env::current_exe() {
        if let Some(repo_root) = discover_repo_root_from(&exe) {
            let candidate = boilerplate_dir(&repo_root);
            if candidate.is_dir() {
                return Some(canonicalize_or_keep(candidate));
            }
        }
    }

    None
}

pub(crate) fn is_repo_root(path: &Path) -> bool {
    path.join("rust-cli").join("Cargo.toml").exists()
        && path
            .join("skill")
            .join("stata-cli")
            .join("boilerplate")
            .is_dir()
        && path.join("skill").join("stata-cli").join("bin").is_dir()
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
    let program_files = std::env::var_os("ProgramFiles")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files"));
    first_stata_candidate(&program_files)
        .unwrap_or_else(|| PathBuf::from(r"C:\Program Files\Stata19"))
}

/// Pick the installed Stata home from `program_files`, probing
/// `StataNow<version>` (subscription) before `Stata<version>` (classic) and
/// preferring the highest version: e.g. `StataNow19`, `Stata19`,
/// `StataNow18`, `Stata18`, ...
fn first_stata_candidate(program_files: &Path) -> Option<PathBuf> {
    let mut candidates: Vec<(u32, bool, PathBuf)> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(program_files) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(version) = name.strip_prefix("StataNow") {
                if let Ok(version) = version.parse::<u32>() {
                    candidates.push((version, true, program_files.join(&name)));
                }
            } else if let Some(version) = name.strip_prefix("Stata") {
                if let Ok(version) = version.parse::<u32>() {
                    candidates.push((version, false, program_files.join(&name)));
                }
            }
        }
    }
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.cmp(&a.1)));
    candidates.into_iter().map(|(_, _, path)| path).next()
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
        let normalized = normalize_for_external(&candidate);
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
    let mut components = path.components();
    if !matches!(
        components.next(),
        Some(std::path::Component::Normal(name)) if name == "~"
    ) {
        return path.to_path_buf();
    }
    if let Some(home) = home_dir() {
        let rest: PathBuf = components.collect();
        return home.join(rest);
    }
    path.to_path_buf()
}

/// Canonicalize a path for passing to external tools (Stata, logs, output
/// files). Rust's `fs::canonicalize` on Windows returns `\\?\`-prefixed
/// extended-length paths that many external programs reject, so the verbatim
/// prefix is stripped there. Internal identity checks may keep using the raw
/// canonical form; this is the Stata-facing form.
pub(crate) fn normalize_for_external(path: &Path) -> PathBuf {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    #[cfg(target_os = "windows")]
    {
        let rendered = canonical.to_string_lossy();
        if let Some(rest) = rendered.strip_prefix(r"\\?\") {
            if let Some(unc) = rest.strip_prefix("UNC\\") {
                return PathBuf::from(format!(r"\\{unc}"));
            }
            return PathBuf::from(rest);
        }
    }
    canonical
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn template_dir_candidates_check_sibling_then_local() {
        let temp = tempdir().unwrap();
        let exe_dir = temp.path().join("bin");
        let candidates = template_dir_candidates(&exe_dir);
        assert_eq!(
            candidates,
            vec![
                exe_dir.join("..").join("boilerplate"),
                exe_dir.join("boilerplate")
            ]
        );
    }

    #[test]
    fn first_stata_candidate_prefers_now_then_highest_version() {
        let temp = tempdir().unwrap();
        let pf = temp.path();
        std::fs::create_dir_all(pf.join("Stata18")).unwrap();
        std::fs::create_dir_all(pf.join("StataNow18")).unwrap();
        std::fs::create_dir_all(pf.join("Stata19")).unwrap();
        std::fs::create_dir_all(pf.join("StataNow19")).unwrap();
        assert_eq!(first_stata_candidate(pf), Some(pf.join("StataNow19")));
    }

    #[test]
    fn first_stata_candidate_falls_back_to_classic_latest() {
        let temp = tempdir().unwrap();
        let pf = temp.path();
        std::fs::create_dir_all(pf.join("Stata19")).unwrap();
        std::fs::create_dir_all(pf.join("Stata18")).unwrap();
        assert_eq!(first_stata_candidate(pf), Some(pf.join("Stata19")));
    }

    #[test]
    fn first_stata_candidate_ignores_unrelated_dirs() {
        let temp = tempdir().unwrap();
        std::fs::create_dir_all(temp.path().join("StataCorp")).unwrap();
        std::fs::create_dir_all(temp.path().join("StataX")).unwrap();
        assert_eq!(first_stata_candidate(temp.path()), None);
    }

    #[test]
    fn first_existing_dir_picks_first_directory() {
        let temp = tempdir().unwrap();
        let missing = temp.path().join("missing");
        let present = temp.path().join("present");
        fs::create_dir_all(&present).unwrap();
        assert_eq!(
            first_existing_dir(&[missing.clone(), present.clone()]),
            Some(present)
        );
        assert_eq!(first_existing_dir(&[missing]), None);
    }

    #[test]
    fn resolve_template_dir_prefers_env_override() {
        let temp = tempdir().unwrap();
        let template = temp.path().join("boilerplate");
        fs::create_dir_all(&template).unwrap();
        std::env::set_var(TEMPLATE_DIR_ENV, &template);
        let resolved = resolve_template_dir().unwrap();
        std::env::remove_var(TEMPLATE_DIR_ENV);
        assert_eq!(resolved, fs::canonicalize(template).unwrap());
    }

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
            Some(normalize_for_external(&level2.join("target.do")))
        );
        let expected_suffix = Path::new("a").join("b").join("target.do");
        assert!(
            tried
                .iter()
                .any(|p| Path::new(p).ends_with(&expected_suffix)),
            "tried paths: {tried:?}"
        );
    }

    #[test]
    fn expands_tilde_from_home_directory() {
        let temp = tempdir().unwrap();
        let old_home = std::env::var_os("HOME");
        let old_profile = std::env::var_os("USERPROFILE");
        std::env::set_var("HOME", temp.path());
        std::env::set_var("USERPROFILE", temp.path());
        let expanded = expand_user(Path::new("~/data/input.dta"));
        if let Some(value) = old_home {
            std::env::set_var("HOME", value);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(value) = old_profile {
            std::env::set_var("USERPROFILE", value);
        } else {
            std::env::remove_var("USERPROFILE");
        }
        assert_eq!(expanded, temp.path().join("data").join("input.dta"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_tilde_expands_from_userprofile() {
        let temp = tempdir().unwrap();
        let old_home = std::env::var_os("HOME");
        let old_profile = std::env::var_os("USERPROFILE");
        std::env::remove_var("HOME");
        std::env::set_var("USERPROFILE", temp.path());
        let expanded = expand_user(Path::new(r"~\data\input.dta"));
        if let Some(value) = old_home {
            std::env::set_var("HOME", value);
        } else {
            std::env::remove_var("HOME");
        }
        if let Some(value) = old_profile {
            std::env::set_var("USERPROFILE", value);
        } else {
            std::env::remove_var("USERPROFILE");
        }
        assert_eq!(expanded, temp.path().join("data").join("input.dta"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_external_path_has_no_verbatim_prefix() {
        let temp = tempdir().unwrap();
        let normalized = normalize_for_external(temp.path());
        assert!(
            !normalized.to_string_lossy().starts_with(r"\\?\"),
            "{}",
            normalized.display()
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_config_and_history_share_appdata_root() {
        let temp = tempdir().unwrap();
        let old = std::env::var_os("APPDATA");
        std::env::set_var("APPDATA", temp.path());
        let config = default_config_path().unwrap();
        let history = repl_history_path().unwrap();
        if let Some(value) = old {
            std::env::set_var("APPDATA", value);
        } else {
            std::env::remove_var("APPDATA");
        }
        let expected = temp.path().join("stata-cli");
        assert_eq!(config.parent().unwrap(), expected);
        assert_eq!(history.parent().unwrap(), expected);
    }
}
