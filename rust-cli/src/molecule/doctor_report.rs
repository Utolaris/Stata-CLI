use crate::atom::json_contract::{DoctorCheck, DoctorReport, RepoRootResolution};
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

pub(crate) fn engine_probe_ok_check(detail: String) -> DoctorCheck {
    DoctorCheck {
        name: "engine_probe",
        status: "ok",
        detail,
    }
}

pub(crate) fn error_check(name: &'static str, detail: String) -> DoctorCheck {
    DoctorCheck {
        name,
        status: "error",
        detail,
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
