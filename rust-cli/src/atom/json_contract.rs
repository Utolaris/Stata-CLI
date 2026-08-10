use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct GraphArtifact {
    pub(crate) path: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct PartialFailure {
    pub(crate) line: Option<u32>,
    pub(crate) command: Option<String>,
    pub(crate) return_code: Option<String>,
    pub(crate) message: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ExecutionResult {
    pub(crate) status: String,
    pub(crate) output: String,
    pub(crate) session_id: Option<String>,
    pub(crate) log_file: Option<String>,
    #[serde(default)]
    pub(crate) graphs: Vec<GraphArtifact>,
    #[serde(default)]
    pub(crate) partial_failures: Vec<PartialFailure>,
    #[serde(default)]
    pub(crate) partial_failure_count: u64,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub(crate) struct CompletionContextResult {
    pub(crate) status: String,
    pub(crate) variables: Vec<String>,
    pub(crate) macros: Vec<String>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct RepoRootResolution {
    pub(crate) path: PathBuf,
    pub(crate) source: &'static str,
}

#[derive(Debug, Serialize, Clone)]
pub(crate) struct DoctorCheck {
    pub(crate) name: &'static str,
    pub(crate) status: &'static str,
    pub(crate) detail: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DoctorReport {
    pub(crate) status: &'static str,
    pub(crate) checks: Vec<DoctorCheck>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StataPathSource {
    CliFlag,
    Environment,
    Config,
    Default,
    Prompt,
}

#[derive(Debug, Clone)]
pub(crate) struct ResolvedStataPath {
    pub(crate) path: Option<PathBuf>,
    #[allow(dead_code)] // kept for future platform-aware resolution reporting
    pub(crate) source: Option<StataPathSource>,
    pub(crate) save_to_config: bool,
}
