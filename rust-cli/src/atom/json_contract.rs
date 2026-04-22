use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct GraphArtifact {
    pub(crate) path: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct ExecutionResult {
    pub(crate) status: String,
    pub(crate) output: String,
    pub(crate) session_id: Option<String>,
    pub(crate) log_file: Option<String>,
    pub(crate) graphs: Vec<GraphArtifact>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
pub(crate) struct BridgeRequest {
    pub(crate) command: String,
    pub(crate) code: Option<String>,
    pub(crate) working_dir: Option<String>,
    pub(crate) timeout: Option<u32>,
}

#[derive(Debug, Clone)]
pub(crate) struct RepoRootResolution {
    pub(crate) path: PathBuf,
    pub(crate) source: &'static str,
}

#[derive(Debug, Clone)]
pub(crate) struct PythonResolution {
    pub(crate) path: PathBuf,
    pub(crate) source: &'static str,
    pub(crate) version: String,
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
    pub(crate) source: Option<StataPathSource>,
    pub(crate) save_to_config: bool,
}
