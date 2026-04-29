use crate::atom::cli_contract::{Cli, Commands};
use crate::atom::json_contract::ExecutionResult;
use crate::atom::output_filtering::{process_file_output, process_output};
use anyhow::{bail, Result};
use serde_json::Value;

pub(crate) fn prepare_execution_result(
    cli: &Cli,
    mut result: ExecutionResult,
    filter_command_echo: bool,
) -> ExecutionResult {
    let display_mode = cli.result_display_mode.as_deref().unwrap_or("full");
    let max_tokens = cli.max_output_tokens.unwrap_or(10_000) as usize;
    result.output = if matches!(cli.command, Commands::File { .. }) {
        process_file_output(
            &result.output,
            display_mode,
            max_tokens,
            filter_command_echo,
            result.log_file.as_deref(),
        )
    } else {
        process_output(
            &result.output,
            display_mode,
            max_tokens,
            filter_command_echo,
        )
    };
    if result.status == "error" && result.error.as_deref().unwrap_or("").is_empty() {
        result.error = Some(result.output.clone());
    }
    result
}

pub(crate) fn render_execution_result(result: &ExecutionResult) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(result)?);
    if result.status != "success" {
        bail!(
            "{}",
            result
                .error
                .clone()
                .unwrap_or_else(|| "stata-cli command failed".to_string())
        );
    }
    Ok(())
}

pub(crate) fn prepare_json_payload(
    cli: &Cli,
    mut payload: Value,
    filter_command_echo: bool,
) -> Value {
    let display_mode = cli.result_display_mode.as_deref().unwrap_or("full");
    let max_tokens = cli.max_output_tokens.unwrap_or(10_000) as usize;

    if let Some(output) = payload.get("output").and_then(Value::as_str) {
        let rendered = process_output(output, display_mode, max_tokens, filter_command_echo);
        payload["output"] = Value::String(rendered.clone());
        if payload
            .get("status")
            .and_then(Value::as_str)
            .map(|status| status == "error")
            .unwrap_or(false)
            && payload
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("")
                .is_empty()
        {
            payload["error"] = Value::String(rendered);
        }
    }

    payload
}

pub(crate) fn render_json_payload(payload: &Value) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(payload)?);

    let status = payload
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if matches!(
        status,
        "success" | "running" | "idle" | "stop_sent" | "stop_requested" | "not_running"
    ) {
        return Ok(());
    }

    let message = payload
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| payload.get("error").and_then(Value::as_str))
        .unwrap_or("stata-cli command failed");
    bail!("{message}")
}

#[cfg(test)]
mod tests {
    use super::prepare_execution_result;
    use crate::atom::cli_contract::Cli;
    use crate::atom::json_contract::ExecutionResult;
    use clap::Parser;

    #[test]
    fn file_result_keeps_tail_instead_of_full_output() {
        let cli = Cli::parse_from(["stata-cli", "file", "analysis.do"]);
        let output = (1..=120)
            .map(|line| format!("mock-line-{line:03}"))
            .collect::<Vec<_>>()
            .join("\n");
        let result = ExecutionResult {
            status: "success".to_string(),
            output,
            session_id: None,
            log_file: Some("/tmp/analysis.log".to_string()),
            graphs: Vec::new(),
            partial_failures: Vec::new(),
            partial_failure_count: 0,
            error: None,
        };

        let rendered = prepare_execution_result(&cli, result, true);

        assert!(rendered.output.contains("last 80 lines"));
        assert!(!rendered.output.contains("mock-line-001"));
        assert!(!rendered.output.contains("mock-line-040"));
        assert!(rendered.output.contains("mock-line-041"));
        assert!(rendered.output.contains("mock-line-120"));
    }
}
