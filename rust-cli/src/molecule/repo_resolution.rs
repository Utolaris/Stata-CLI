use crate::atom::config_store::load_cli_config;
use crate::atom::json_contract::RepoRootResolution;
use crate::atom::path_ops::{default_config_path, normalize_repo_root};
use anyhow::{bail, Result};
use std::path::PathBuf;

pub(crate) const PROJECT_ROOT_ENV: &str = "STATA_CLI_PROJECT_ROOT";

pub(crate) fn resolve_repo_root_from_executable() -> Option<PathBuf> {
    let exe_path = std::env::current_exe().ok()?;
    normalize_repo_root(&exe_path)
}

pub(crate) fn resolve_repo_root() -> Result<RepoRootResolution> {
    if let Some(value) = std::env::var_os(PROJECT_ROOT_ENV) {
        let candidate = PathBuf::from(value);
        if let Some(path) = normalize_repo_root(&candidate) {
            return Ok(RepoRootResolution {
                path,
                source: "environment",
            });
        }
    }

    if let Ok(cwd) = std::env::current_dir() {
        if let Some(path) = normalize_repo_root(&cwd) {
            return Ok(RepoRootResolution {
                path,
                source: "current directory",
            });
        }
    }

    if let Some(path) = resolve_repo_root_from_executable() {
        return Ok(RepoRootResolution {
            path,
            source: "executable location",
        });
    }

    if let Some(config_path) = default_config_path() {
        if let Some(config) = load_cli_config(&config_path)? {
            if let Some(project_root) = config.project_root {
                if let Some(path) = normalize_repo_root(&project_root) {
                    return Ok(RepoRootResolution {
                        path,
                        source: "config file",
                    });
                }
            }
        }
    }

    let config_hint = default_config_path()
        .map(|path| path.display().to_string())
        .unwrap_or_else(|| "~/.config/stata-cli/config.toml".to_string());
    bail!(
        "Could not locate the stata-cli project root. Set {} to the repo path, run the command from inside the repo, or create {} with `project_root = \"/absolute/path/to/stata-cli\"`.",
        PROJECT_ROOT_ENV,
        config_hint
    )
}
