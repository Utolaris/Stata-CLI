use crate::atom::cli_contract::Cli;
use crate::atom::config_store::{load_cli_config, persist_resolved_stata_path, CliConfig};
use crate::atom::json_contract::{ExecutionResult, ResolvedStataPath, StataPathSource};
use crate::atom::path_ops::{default_config_path, validate_stata_path, windows_default_stata_path};
use anyhow::{bail, Result};
use serde_json::Value;
use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;

pub(crate) const STATA_PATH_ENV: &str = "STATA_PATH";

pub(crate) fn prompt_for_stata_path(message: &str) -> Result<Option<PathBuf>> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "{message}")?;
    writeln!(
        stdout,
        "Enter your Stata installation directory, or press Enter to cancel:"
    )?;
    write!(stdout, "stata-path> ")?;
    stdout.flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    Ok(Some(PathBuf::from(trimmed)))
}

pub(crate) fn resolve_windows_stata_path_with_prompt<F>(
    cli: &Cli,
    config: Option<&CliConfig>,
    interactive: bool,
    mut prompt: F,
) -> Result<ResolvedStataPath>
where
    F: FnMut(&str) -> Result<Option<PathBuf>>,
{
    if let Some(path) = &cli.stata_path {
        let candidate = PathBuf::from(path);
        validate_stata_path(&candidate).map_err(|error| {
            anyhow::anyhow!(
                "The --stata-path value is invalid. Pass a valid Windows Stata installation directory. {error}"
            )
        })?;
        return Ok(ResolvedStataPath {
            path: Some(candidate),
            source: Some(StataPathSource::CliFlag),
            save_to_config: false,
        });
    }

    if let Some(value) = std::env::var_os(STATA_PATH_ENV) {
        let candidate = PathBuf::from(value);
        validate_stata_path(&candidate).map_err(|error| {
            anyhow::anyhow!(
                "The {} environment variable points to an invalid Stata directory. {error}",
                STATA_PATH_ENV
            )
        })?;
        return Ok(ResolvedStataPath {
            path: Some(candidate),
            source: Some(StataPathSource::Environment),
            save_to_config: false,
        });
    }

    if let Some(saved_path) = config.and_then(|item| item.stata_path.clone()) {
        match validate_stata_path(&saved_path) {
            Ok(()) => {
                return Ok(ResolvedStataPath {
                    path: Some(saved_path),
                    source: Some(StataPathSource::Config),
                    save_to_config: false,
                })
            }
            Err(error) => {
                if !interactive {
                    bail!(
                        "{}. Update {} or pass --stata-path.",
                        error,
                        default_config_path()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "%APPDATA%\\stata-cli\\config.toml".to_string())
                    );
                }

                let mut prompt_message = format!(
                    "{}\nSaved path came from {}.",
                    error,
                    default_config_path()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "%APPDATA%\\stata-cli\\config.toml".to_string())
                );
                loop {
                    match prompt(&prompt_message)? {
                        Some(candidate) => match validate_stata_path(&candidate) {
                            Ok(()) => {
                                return Ok(ResolvedStataPath {
                                    path: Some(candidate),
                                    source: Some(StataPathSource::Prompt),
                                    save_to_config: true,
                                })
                            }
                            Err(prompt_error) => {
                                prompt_message = format!(
                                    "{}\nPlease enter a valid Windows Stata installation directory.",
                                    prompt_error
                                );
                            }
                        },
                        None => bail!(
                            "Stata path is required on Windows. Pass --stata-path or update {}.",
                            default_config_path()
                                .map(|path| path.display().to_string())
                                .unwrap_or_else(|| "%APPDATA%\\stata-cli\\config.toml".to_string())
                        ),
                    }
                }
            }
        }
    }

    let default_path = windows_default_stata_path();
    match validate_stata_path(&default_path) {
        Ok(()) => Ok(ResolvedStataPath {
            path: Some(default_path),
            source: Some(StataPathSource::Default),
            save_to_config: false,
        }),
        Err(error) => {
            if !interactive {
                bail!(
                    "{}. Pass --stata-path or create {} with `stata_path = \"C:\\\\Path\\\\To\\\\Stata\"`.",
                    error,
                    default_config_path()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "%APPDATA%\\stata-cli\\config.toml".to_string())
                );
            }

            let mut prompt_message =
                format!("{}\nWindows defaults to {}.", error, default_path.display());
            loop {
                match prompt(&prompt_message)? {
                    Some(candidate) => match validate_stata_path(&candidate) {
                        Ok(()) => {
                            return Ok(ResolvedStataPath {
                                path: Some(candidate),
                                source: Some(StataPathSource::Prompt),
                                save_to_config: true,
                            })
                        }
                        Err(prompt_error) => {
                            prompt_message = format!(
                                "{}\nPlease enter a valid Windows Stata installation directory.",
                                prompt_error
                            );
                        }
                    },
                    None => bail!(
                        "Stata path is required on Windows. Pass --stata-path or create {}.",
                        default_config_path()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "%APPDATA%\\stata-cli\\config.toml".to_string())
                    ),
                }
            }
        }
    }
}

pub(crate) fn resolve_effective_stata_path(cli: &Cli) -> Result<ResolvedStataPath> {
    if !cfg!(windows) {
        return Ok(ResolvedStataPath {
            path: cli.stata_path.as_ref().map(PathBuf::from),
            source: cli.stata_path.as_ref().map(|_| StataPathSource::CliFlag),
            save_to_config: false,
        });
    }

    let config = if let Some(path) = default_config_path() {
        load_cli_config(&path)?
    } else {
        None
    };

    let interactive = io::stdin().is_terminal() && io::stdout().is_terminal();
    resolve_windows_stata_path_with_prompt(cli, config.as_ref(), interactive, prompt_for_stata_path)
}

pub(crate) fn persist_stata_path_if_needed(
    resolved_stata_path: &ResolvedStataPath,
    result: &ExecutionResult,
) -> Result<()> {
    if resolved_stata_path.save_to_config && result.status == "success" {
        if let Some(path) = &resolved_stata_path.path {
            if let Some(config_path) = default_config_path() {
                persist_resolved_stata_path(&config_path, path)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn persist_stata_path_if_needed_json(
    resolved_stata_path: &ResolvedStataPath,
    payload: &Value,
) -> Result<()> {
    if !resolved_stata_path.save_to_config {
        return Ok(());
    }

    let status = payload
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if status == "success" {
        if let Some(path) = &resolved_stata_path.path {
            if let Some(config_path) = default_config_path() {
                persist_resolved_stata_path(&config_path, path)?;
            }
        }
    }
    Ok(())
}

pub(crate) fn clone_with_effective_stata_path(cli: &Cli, resolved: &ResolvedStataPath) -> Cli {
    let mut effective_cli = cli.clone();
    if let Some(path) = &resolved.path {
        effective_cli.stata_path = Some(path.to_string_lossy().into_owned());
    }
    effective_cli
}
