use crate::atom::cli_contract::Cli;
use crate::atom::json_contract::ExecutionResult;
use crate::atom::output_filtering::process_output;
use anyhow::{bail, Result};
use serde_json::Value;

pub(crate) fn prepare_execution_result(
    cli: &Cli,
    mut result: ExecutionResult,
    filter_command_echo: bool,
) -> ExecutionResult {
    let display_mode = cli.result_display_mode.as_deref().unwrap_or("full");
    let max_tokens = cli.max_output_tokens.unwrap_or(10_000) as usize;
    result.output = process_output(
        &result.output,
        display_mode,
        max_tokens,
        filter_command_echo,
    );
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
