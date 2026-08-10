use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(name = "stata-cli")]
#[command(about = "A local Stata CLI for AI agents (native Rust engine)")]
pub(crate) struct Cli {
    /// Path to the Stata installation directory
    #[arg(long)]
    pub(crate) stata_path: Option<String>,
    /// Stata edition to load (mp, se, or be)
    #[arg(long)]
    pub(crate) stata_edition: Option<String>,
    #[arg(long, hide = true)]
    pub(crate) session_id: Option<String>,
    /// Working directory for relative paths and execution
    #[arg(long)]
    pub(crate) working_dir: Option<PathBuf>,
    #[arg(long, hide = true)]
    pub(crate) json: bool,
    #[arg(long, hide = true)]
    pub(crate) quiet: bool,
    #[arg(long, default_value = "WARNING")]
    pub(crate) log_level: String,
    /// Output display mode for results (compact or full)
    #[arg(long)]
    pub(crate) result_display_mode: Option<String>,
    /// Maximum output tokens before truncation
    #[arg(long)]
    pub(crate) max_output_tokens: Option<u32>,
    #[arg(long, conflicts_with = "no_multi_session", hide = true)]
    pub(crate) multi_session: bool,
    #[arg(long, conflicts_with = "multi_session", hide = true)]
    pub(crate) no_multi_session: bool,
    #[arg(long, hide = true)]
    pub(crate) max_sessions: Option<u32>,
    #[arg(long, hide = true)]
    pub(crate) session_timeout: Option<u32>,
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
pub(crate) enum Commands {
    /// Run inline Stata commands
    Run {
        /// Stata code to execute
        #[arg(long)]
        code: String,
    },
    /// Run a .do file
    File {
        /// Path to the .do file
        path: PathBuf,
        #[arg(long, hide = true)]
        session_id: Option<String>,
        /// Working directory for the run
        #[arg(long)]
        working_dir: Option<PathBuf>,
    },
    /// Initialize a new workspace from the bundled templates
    Init,
    /// Start the interactive REPL
    Repl,
    /// Diagnose the local Stata engine
    Doctor,
    /// Inspect and export dataset contents
    Data {
        #[command(subcommand)]
        command: DataCommands,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub(crate) enum DataCommands {
    /// Preview dataset rows as JSON
    View {
        /// Restrict preview to rows matching this condition
        #[arg(long)]
        if_condition: Option<String>,
        /// Maximum number of rows to preview
        #[arg(long, default_value_t = 50)]
        max_rows: u32,
        /// Path to the input .dta file
        #[arg(long)]
        input_dta: PathBuf,
    },
    /// Export a dataset to CSV
    ExportCsv {
        /// Output CSV path
        #[arg(long)]
        output: PathBuf,
        /// Path to the input .dta file
        #[arg(long)]
        input_dta: PathBuf,
        /// Working directory for relative output paths
        #[arg(long)]
        working_dir: Option<PathBuf>,
        /// Overwrite an existing output file
        #[arg(long)]
        replace: bool,
    },
}
