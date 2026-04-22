use regex::Regex;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

pub(crate) fn deduplicate_break_messages(output: &str) -> String {
    if output.is_empty() || !output.contains("--Break--") {
        return output.to_string();
    }

    let break_pattern =
        Regex::new(r"(--Break--\s*\n\s*r\(1\);\s*\n?)+").expect("valid break regex");
    break_pattern
        .replace_all(output, "--Break--\nr(1);\n")
        .into_owned()
}

fn is_program_define_line(line: &str) -> bool {
    let mut trimmed = line.trim_start();
    if let Some(stripped) = trimmed.strip_prefix('.') {
        trimmed = stripped.trim_start();
    }
    let Some(rest) = trimmed.strip_prefix("program") else {
        return false;
    };
    let mut rest = rest.trim_start();
    if let Some(stripped) = rest.strip_prefix("define") {
        rest = stripped.trim_start();
    }
    let Some(name) = rest.split_whitespace().next() else {
        return false;
    };
    !matches!(name, "version" | "dir" | "drop" | "list" | "describe")
}

pub(crate) fn apply_compact_mode_filter(output: &str, filter_command_echo: bool) -> String {
    if output.is_empty() {
        return String::new();
    }

    let output = output.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = output.split('\n').collect();
    let mut filtered_lines: Vec<String> = Vec::new();

    let command_echo_pattern = Regex::new(r"^\.\s*$|^\.\s+\S").unwrap();
    let numbered_line_pattern = Regex::new(r"^\s*\d+\.\s").unwrap();
    let continuation_pattern = Regex::new(r"^>\s").unwrap();
    let cli_header_pattern =
        Regex::new(r"^>>>\s+\[\d{4}-\d{2}-\d{2}\s+\d{2}:\d{2}:\d{2}\]").unwrap();
    let exec_time_pattern = Regex::new(r"^\*\*\*\s+Execution completed in").unwrap();
    let final_output_pattern = Regex::new(r"^Final output:\s*$").unwrap();
    let log_info_pattern =
        Regex::new(r"^\s*(name:|log:|log type:|opened on:|closed on:|Log file saved to:)").unwrap();
    let capture_log_pattern = Regex::new(r"^\.\s*capture\s+log\s+close").unwrap();

    let program_drop_pattern = Regex::new(
        r"^\s*\.?\s*(capture\s+program\s+drop|cap\s+program\s+drop|cap\s+prog\s+drop|capt\s+program\s+drop|capt\s+prog\s+drop)\s+\w+",
    )
    .unwrap();
    let mata_start_pattern =
        Regex::new(r"^\s*(\d+\.)?\s*\.?\s*mata\s*:?\s*$|^-+\s*mata\s*\(").unwrap();
    let end_pattern = Regex::new(r"^\s*(\d+\.)?\s*[.:]*\s*end\s*$").unwrap();
    let mata_separator_pattern = Regex::new(r"^-{20,}$").unwrap();

    let loop_start_pattern =
        Regex::new(r"^(\s*\d+\.)?\s*\.?\s*(foreach|forvalues|while)\s+.*\{\s*$").unwrap();
    let loop_end_pattern = Regex::new(r"^\s*\d+\.\s*\}\s*$").unwrap();

    let real_changes_pattern = Regex::new(r"^\s*\([\d,]+\s+real\s+changes?\s+made\)\s*$").unwrap();
    let missing_values_pattern =
        Regex::new(r"^\s*\([\d,]+\s+missing\s+values?\s+generated\)\s*$").unwrap();
    let smcl_pattern = Regex::new(
        r"\{(txt|res|err|inp|com|bf|it|sf|hline|c\s+\||\-+|break|col\s+\d+|right|center|ul|/ul)\}",
    )
    .unwrap();
    let var_list_pattern = Regex::new(r"^\s*(\d+\.\s+)?\w+\s+\w+\s+%").unwrap();
    let empty_numbered_line_pattern = Regex::new(r"^\s*\d+\.\s*$").unwrap();

    let mut variable_list_count = 0usize;
    let mut in_variable_list = false;
    let mut in_program_block = false;
    let mut in_mata_block = false;
    let mut in_loop_block = false;
    let mut program_end_depth = 0usize;
    let mut loop_brace_depth = 0usize;
    let mut i = 0usize;

    while i < lines.len() {
        let mut line = lines[i].to_string();

        if in_program_block {
            if mata_start_pattern.is_match(&line) {
                program_end_depth += 1;
            }
            if end_pattern.is_match(&line) {
                if program_end_depth > 0 {
                    program_end_depth -= 1;
                } else {
                    in_program_block = false;
                }
            }
            i += 1;
            continue;
        }

        if in_mata_block {
            if end_pattern.is_match(&line) {
                in_mata_block = false;
                if i + 1 < lines.len() && mata_separator_pattern.is_match(lines[i + 1]) {
                    i += 1;
                }
            }
            i += 1;
            continue;
        }

        if in_loop_block {
            if loop_start_pattern.is_match(&line) {
                loop_brace_depth += 1;
                i += 1;
                continue;
            }

            if loop_end_pattern.is_match(&line) {
                if loop_brace_depth > 0 {
                    loop_brace_depth -= 1;
                } else {
                    in_loop_block = false;
                }
                i += 1;
                continue;
            }

            if command_echo_pattern.is_match(&line)
                || numbered_line_pattern.is_match(&line)
                || continuation_pattern.is_match(&line)
                || real_changes_pattern.is_match(&line)
                || missing_values_pattern.is_match(&line)
            {
                i += 1;
                continue;
            }

            line = smcl_pattern.replace_all(&line, "").into_owned();
            if !line.trim().is_empty() {
                filtered_lines.push(line);
            }
            i += 1;
            continue;
        }

        if loop_start_pattern.is_match(&line) {
            in_loop_block = true;
            loop_brace_depth = 0;
            i += 1;
            continue;
        }
        if program_drop_pattern.is_match(&line) {
            i += 1;
            continue;
        }
        if is_program_define_line(&line) {
            in_program_block = true;
            program_end_depth = 0;
            i += 1;
            continue;
        }
        if mata_start_pattern.is_match(&line) {
            in_mata_block = true;
            i += 1;
            continue;
        }
        if real_changes_pattern.is_match(&line) || missing_values_pattern.is_match(&line) {
            i += 1;
            continue;
        }

        if filter_command_echo
            && (cli_header_pattern.is_match(&line)
                || exec_time_pattern.is_match(&line)
                || final_output_pattern.is_match(&line)
                || log_info_pattern.is_match(&line)
                || capture_log_pattern.is_match(&line)
                || command_echo_pattern.is_match(&line)
                || numbered_line_pattern.is_match(&line)
                || continuation_pattern.is_match(&line))
        {
            i += 1;
            continue;
        }

        line = smcl_pattern.replace_all(&line, "").into_owned();

        if var_list_pattern.is_match(&line) {
            if !in_variable_list {
                in_variable_list = true;
                variable_list_count = 0;
            }
            variable_list_count += 1;
            if variable_list_count > 100 {
                if variable_list_count == 101 {
                    filtered_lines.push(
                        "    ... (output truncated, showing first 100 variables)".to_string(),
                    );
                }
                i += 1;
                continue;
            }
        } else {
            in_variable_list = false;
            variable_list_count = 0;
        }

        filtered_lines.push(line);
        i += 1;
    }

    let mut cleaned_lines = Vec::new();
    for line in filtered_lines {
        if empty_numbered_line_pattern.is_match(&line) {
            continue;
        }
        cleaned_lines.push(line);
    }

    let mut result_lines = Vec::new();
    let mut prev_blank = false;
    for line in cleaned_lines {
        let is_blank = line.trim().is_empty();
        if is_blank {
            if !prev_blank {
                result_lines.push(line);
            }
            prev_blank = true;
        } else {
            result_lines.push(line);
            prev_blank = false;
        }
    }

    while matches!(result_lines.last(), Some(last) if last.trim().is_empty()) {
        result_lines.pop();
    }

    result_lines.join("\n")
}

fn check_token_limit_and_save(output: &str, max_output_tokens: usize) -> String {
    if max_output_tokens == 0 {
        return output.to_string();
    }

    let estimated_tokens = output.len() / 4;
    if estimated_tokens <= max_output_tokens {
        return output.to_string();
    }

    let log_dir = std::env::temp_dir().join("stata_cli_logs");
    let _ = fs::create_dir_all(&log_dir);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let log_path: PathBuf = log_dir.join(format!("stata_output_{timestamp}.log"));

    if fs::write(&log_path, output).is_ok() {
        let preview_chars = output.len().min(1000);
        let mut message = format!(
            "Output exceeded token limit ({} tokens > {} max).\nFull output saved to: {}\n\nPlease investigate the log file for complete results.\nYou can read this file to see the full Stata output.",
            estimated_tokens,
            max_output_tokens,
            log_path.display()
        );
        if preview_chars > 0 {
            let mut preview = output[..preview_chars].to_string();
            if output.len() > preview_chars {
                preview.push_str("\n... [truncated]");
            }
            message.push_str(&format!("\n\n--- Preview ---\n{preview}"));
        }
        return message;
    }

    let max_chars = max_output_tokens.saturating_mul(4);
    let truncated = output.chars().take(max_chars).collect::<String>();
    format!("{truncated}\n\n... [Output truncated at {max_output_tokens} tokens]")
}

pub(crate) fn process_output(
    output: &str,
    result_display_mode: &str,
    max_output_tokens: usize,
    filter_command_echo: bool,
) -> String {
    let mut processed = deduplicate_break_messages(output);
    if result_display_mode == "compact" {
        processed = apply_compact_mode_filter(&processed, filter_command_echo);
    }
    check_token_limit_and_save(&processed, max_output_tokens)
}

#[cfg(test)]
mod tests {
    use super::{apply_compact_mode_filter, deduplicate_break_messages, process_output};

    #[test]
    fn compact_filter_removes_loop_and_program_echo() {
        let output = r#"
. foreach var in mpg price {
  2. summarize `var'
  3. }

    Variable |        Obs        Mean
-------------+----------------------
         mpg |         74     21.2973
"#;

        let filtered = apply_compact_mode_filter(output, true);
        assert!(!filtered.contains("foreach"));
        assert!(!filtered.contains("summarize `var'"));
        assert!(filtered.contains("21.2973"));
    }

    #[test]
    fn compact_filter_removes_smcl_tags() {
        let filtered = apply_compact_mode_filter("{txt}Some {res}result {err}error", false);
        assert_eq!(filtered, "Some result error");
    }

    #[test]
    fn break_messages_are_deduplicated() {
        let output = "--Break--\nr(1);\n--Break--\nr(1);\n";
        assert_eq!(deduplicate_break_messages(output), "--Break--\nr(1);\n");
    }

    #[test]
    fn process_output_truncates_large_content() {
        let large = "x".repeat(5000);
        let rendered = process_output(&large, "full", 10, false);
        assert!(rendered.contains("Output exceeded token limit"));
    }
}
