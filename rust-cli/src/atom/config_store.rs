use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub(crate) struct CliConfig {
    pub(crate) project_root: Option<PathBuf>,
    pub(crate) stata_path: Option<PathBuf>,
}

pub(crate) fn load_cli_config(path: &Path) -> Result<Option<CliConfig>> {
    if !path.exists() {
        return Ok(None);
    }

    let raw = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file at {}", path.display()))?;
    let config = toml::from_str::<CliConfig>(&raw)
        .with_context(|| format!("Failed to parse config file at {}", path.display()))?;
    Ok(Some(config))
}

pub(crate) fn write_cli_config(path: &Path, config: &CliConfig) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!("Failed to create config directory at {}", parent.display())
        })?;
    }

    let serialized = toml::to_string_pretty(config)
        .with_context(|| format!("Failed to serialize config for {}", path.display()))?;
    fs::write(path, serialized)
        .with_context(|| format!("Failed to write config file at {}", path.display()))?;
    Ok(())
}

pub(crate) fn persist_resolved_stata_path(config_path: &Path, path: &Path) -> Result<()> {
    let mut config = load_cli_config(config_path)?.unwrap_or_default();
    config.stata_path = Some(path.to_path_buf());
    write_cli_config(config_path, &config)
}
