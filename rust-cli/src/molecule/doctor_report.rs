use crate::atom::json_contract::{
    format_stata_path_source, DoctorCheck, DoctorReport, PythonResolution, RepoRootResolution,
    ResolvedStataPath,
};
use std::path::Path;

pub(crate) fn repo_root_check(repo_root: &RepoRootResolution) -> DoctorCheck {
    DoctorCheck {
        name: "repo_root",
        status: "ok",
        detail: format!(
            "{} (source: {})",
            repo_root.path.display(),
            repo_root.source
        ),
    }
}

pub(crate) fn config_file_check(config_path: Option<&Path>) -> DoctorCheck {
    match config_path {
        Some(path) if path.exists() => DoctorCheck {
            name: "config_file",
            status: "ok",
            detail: format!("Config file found at {}", path.display()),
        },
        Some(path) => DoctorCheck {
            name: "config_file",
            status: "warn",
            detail: format!(
                "No config file at {}. Optional, but useful if the repo is ever moved.",
                path.display()
            ),
        },
        None => DoctorCheck {
            name: "config_file",
            status: "warn",
            detail: "Could not determine a home directory for the optional config file."
                .to_string(),
        },
    }
}

pub(crate) fn backend_entry_check(backend: &Path) -> DoctorCheck {
    if backend.exists() {
        DoctorCheck {
            name: "backend_script",
            status: "ok",
            detail: format!("Found {}", backend.display()),
        }
    } else {
        DoctorCheck {
            name: "backend_script",
            status: "error",
            detail: format!("Missing {}", backend.display()),
        }
    }
}

pub(crate) fn stata_path_check(resolved_stata_path: &ResolvedStataPath) -> DoctorCheck {
    match (&resolved_stata_path.path, resolved_stata_path.source) {
        (Some(path), source) => DoctorCheck {
            name: "stata_path",
            status: "ok",
            detail: format!(
                "{} (source: {})",
                path.display(),
                format_stata_path_source(source)
            ),
        },
        _ => DoctorCheck {
            name: "stata_path",
            status: "error",
            detail: "Windows requires a valid Stata installation directory.".to_string(),
        },
    }
}

pub(crate) fn python_ok_check(resolution: &PythonResolution) -> DoctorCheck {
    DoctorCheck {
        name: "python",
        status: "ok",
        detail: format!(
            "{} (source: {}, version: {})",
            resolution.path.display(),
            resolution.source,
            resolution.version
        ),
    }
}

pub(crate) fn error_check(name: &'static str, detail: String) -> DoctorCheck {
    DoctorCheck {
        name,
        status: "error",
        detail,
    }
}

pub(crate) fn backend_probe_ok_check() -> DoctorCheck {
    DoctorCheck {
        name: "backend_probe",
        status: "ok",
        detail: "Backend successfully executed `display 1+1`.".to_string(),
    }
}

pub(crate) fn finalize_report(checks: Vec<DoctorCheck>) -> DoctorReport {
    let status = if checks.iter().any(|check| check.status == "error") {
        "error"
    } else {
        "ok"
    };
    DoctorReport { status, checks }
}
