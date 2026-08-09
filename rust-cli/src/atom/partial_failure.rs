//! Parse non-fatal Stata command failures from execution logs.
//!
//! Port of the previous Python backend's `partial_failure_parser.py`.

use crate::atom::json_contract::PartialFailure;
use regex::Regex;
use std::collections::HashSet;

fn is_wrapper_command(command: &str) -> bool {
    let lowered = command.trim().to_lowercase();
    lowered.starts_with("capture log close")
        || lowered.starts_with("log using ")
        || lowered.starts_with("set seed ")
        || lowered.starts_with("cd ")
}

fn clean_message(lines: &[String]) -> String {
    lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.trim().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

type FailureSignature = (Option<u32>, Option<String>, Option<String>, String);

pub(crate) fn parse_partial_failures(output: &str) -> Vec<PartialFailure> {
    if output.is_empty() {
        return Vec::new();
    }

    let command_echo = Regex::new(r"^\.\s+(?P<command>.+?)\s*$").expect("valid regex");
    let continuation_echo = Regex::new(r"^>\s?(?P<command>.+?)\s*$").expect("valid regex");
    let return_code = Regex::new(r"\br\((?P<code>\d+)\);").expect("valid regex");
    let error_message = Regex::new(
        r"(?i)^\s*(?:command .+ is unrecognized|variable .+ not found|file .+ not found|invalid syntax|type mismatch|no observations|insufficient observations|conformability error|option .+ not allowed)\s*$",
    )
    .expect("valid regex");

    let mut failures: Vec<PartialFailure> = Vec::new();
    let mut seen: HashSet<FailureSignature> = HashSet::new();
    let mut current_command: Option<String> = None;
    let mut current_line: Option<u32> = None;
    let mut current_error_index: Option<usize> = None;
    let mut message_lines: Vec<String> = Vec::new();
    let mut command_line = 0u32;

    for raw_line in output.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        if let Some(captures) = command_echo.captures(raw_line) {
            let command = captures["command"].trim().to_string();
            if is_wrapper_command(&command) {
                current_command = None;
                current_line = None;
            } else {
                command_line += 1;
                current_command = Some(command);
                current_line = Some(command_line);
            }
            current_error_index = None;
            message_lines.clear();
            continue;
        }

        if let Some(captures) = continuation_echo.captures(raw_line) {
            if let Some(prefix) = current_command.take() {
                current_command = Some(format!("{prefix}\n{}", captures["command"].trim()));
            }
            continue;
        }

        if let Some(captures) = return_code.captures(raw_line) {
            if let Some(command) = &current_command {
                let code = format!("r({})", &captures["code"]);
                let message = clean_message(&message_lines);
                if let Some(index) = current_error_index {
                    if index < failures.len() {
                        failures[index].return_code = Some(code);
                    }
                } else {
                    let signature: FailureSignature = (
                        current_line,
                        Some(command.clone()),
                        Some(code.clone()),
                        message.clone(),
                    );
                    if seen.insert(signature) {
                        failures.push(PartialFailure {
                            line: current_line,
                            command: Some(command.clone()),
                            return_code: Some(code),
                            message: if message.is_empty() {
                                raw_line.trim().to_string()
                            } else {
                                message
                            },
                        });
                        current_error_index = Some(failures.len() - 1);
                    }
                }
                message_lines.clear();
            }
            continue;
        }

        if current_command.is_some() && !raw_line.trim().is_empty() {
            if error_message.is_match(raw_line) {
                let message = raw_line.trim().to_string();
                let signature: FailureSignature =
                    (current_line, current_command.clone(), None, message.clone());
                if seen.insert(signature.clone()) {
                    failures.push(PartialFailure {
                        line: current_line,
                        command: current_command.clone(),
                        return_code: None,
                        message,
                    });
                    current_error_index = Some(failures.len() - 1);
                }
            }
            message_lines.push(raw_line.to_string());
        }
    }

    failures
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_return_code_failure() {
        let output = ". display 2+2\n4\n\n. nonexistent_command\ncommand nonexistent_command is unrecognized\nr(199);\n";
        let failures = parse_partial_failures(output);
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].return_code.as_deref(), Some("r(199)"));
        assert_eq!(failures[0].command.as_deref(), Some("nonexistent_command"));
        assert!(failures[0].message.contains("unrecognized"));
    }

    #[test]
    fn skips_wrapper_commands() {
        let output = ". capture log close _all\n. set seed 123\n. display 1\n1\n";
        let failures = parse_partial_failures(output);
        assert!(failures.is_empty());
    }

    #[test]
    fn deduplicates_repeated_error_lines_within_one_command() {
        let output = ". badcmd\ninvalid syntax\ninvalid syntax\nr(198);\n";
        let failures = parse_partial_failures(output);
        assert_eq!(failures.len(), 1);
    }
}
