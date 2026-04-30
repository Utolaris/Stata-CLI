use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser, Debug, Clone)]
#[command(name = "stata-cli")]
#[command(about = "A local Rust CLI wrapper for the Python/PyStata backend")]
pub(crate) struct Cli {
    #[arg(long)]
    pub(crate) stata_path: Option<String>,
    #[arg(long)]
    pub(crate) stata_edition: Option<String>,
    #[arg(long)]
    pub(crate) python: Option<PathBuf>,
    #[arg(long, hide = true)]
    pub(crate) session_id: Option<String>,
    #[arg(long)]
    pub(crate) working_dir: Option<PathBuf>,
    #[arg(long, hide = true)]
    pub(crate) json: bool,
    #[arg(long, hide = true)]
    pub(crate) quiet: bool,
    #[arg(long, default_value = "WARNING")]
    pub(crate) log_level: String,
    #[arg(long)]
    pub(crate) result_display_mode: Option<String>,
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
    Run {
        #[arg(long)]
        code: String,
    },
    File {
        path: PathBuf,
        #[arg(long, hide = true)]
        session_id: Option<String>,
        #[arg(long)]
        working_dir: Option<PathBuf>,
    },
    Init,
    Repl,
    Doctor,
    Data {
        #[command(subcommand)]
        command: DataCommands,
    },
}

#[derive(Subcommand, Debug, Clone)]
pub(crate) enum DataCommands {
    View {
        #[arg(long)]
        if_condition: Option<String>,
        #[arg(long, default_value_t = 50)]
        max_rows: u32,
        #[arg(long)]
        input_dta: PathBuf,
    },
    ExportCsv {
        #[arg(long)]
        output: PathBuf,
        #[arg(long)]
        input_dta: PathBuf,
        #[arg(long)]
        working_dir: Option<PathBuf>,
        #[arg(long)]
        replace: bool,
    },
}
