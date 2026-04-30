use anyhow::{Context, Result};
use std::collections::BTreeSet;
use std::fs;
use std::io::{self, BufRead, Write};
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GuiCommandHit {
    pub(crate) line_number: usize,
    pub(crate) command: String,
}

const BLOCKED_COMMANDS: [&str; 7] = [
    "browse", "edit", "db", "dialog", "window", "shell", "winexec",
];
const WRAPPER_PREFIXES: [&str; 8] = [
    "capture",
    "cap",
    "quietly",
    "qui",
    "noisily",
    "noi",
    "capturely",
    "captureily",
];

pub(crate) fn scan_do_file_for_gui_commands(path: &Path) -> Result<Vec<GuiCommandHit>> {
    let source = fs::read_to_string(path).with_context(|| {
        format!(
            "Failed to read do-file for GUI command scan: {}",
            path.display()
        )
    })?;
    Ok(scan_do_source_for_gui_commands(&source))
}

fn scan_do_source_for_gui_commands(source: &str) -> Vec<GuiCommandHit> {
    let mut hits = Vec::new();
    let mut in_block_comment = false;

    for (index, raw_line) in source.lines().enumerate() {
        let sanitized = strip_comments(raw_line, &mut in_block_comment);
        let Some(command) = detect_gui_command_in_line(&sanitized) else {
            continue;
        };
        hits.push(GuiCommandHit {
            line_number: index + 1,
            command: command.to_string(),
        });
    }

    hits
}

fn strip_comments(line: &str, in_block_comment: &mut bool) -> String {
    let chars: Vec<char> = line.chars().collect();
    let mut out = String::new();
    let mut idx = 0usize;
    let mut in_single_quote = false;
    let mut in_double_quote = false;

    while idx < chars.len() {
        let ch = chars[idx];
        let next = chars.get(idx + 1).copied();

        if *in_block_comment {
            if ch == '*' && next == Some('/') {
                *in_block_comment = false;
                idx += 2;
            } else {
                idx += 1;
            }
            continue;
        }

        if !in_single_quote && !in_double_quote {
            if ch == '/' && next == Some('*') {
                *in_block_comment = true;
                idx += 2;
                continue;
            }
            if ch == '/' && next == Some('/') {
                break;
            }
            if ch == '*' && out.trim().is_empty() {
                break;
            }
        }

        if ch == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
        } else if ch == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
        }

        out.push(ch);
        idx += 1;
    }

    out
}

fn detect_gui_command_in_line(line: &str) -> Option<&'static str> {
    let trimmed = line.trim_start();
    if trimmed.is_empty() {
        return None;
    }

    let mut tokens = trimmed
        .split_whitespace()
        .map(normalize_token)
        .filter(|token| !token.is_empty());

    let mut token = tokens.next()?;
    for _ in 0..3 {
        if let Some(command) = blocked_command(token.as_str()) {
            return Some(command);
        }
        if WRAPPER_PREFIXES.contains(&token.as_str()) {
            token = tokens.next()?;
            continue;
        }
        return None;
    }

    blocked_command(token.as_str())
}

fn normalize_token(token: &str) -> String {
    token
        .trim_matches(|ch: char| matches!(ch, ':' | ',' | ';' | '(' | ')'))
        .to_ascii_lowercase()
}

fn blocked_command(token: &str) -> Option<&'static str> {
    BLOCKED_COMMANDS
        .iter()
        .copied()
        .find(|command| *command == token)
}

pub(crate) fn confirm_gui_command_execution(path: &Path, hits: &[GuiCommandHit]) -> Result<bool> {
    let stdin = io::stdin();
    let stderr = io::stderr();
    confirm_gui_command_execution_with_io(path, hits, stdin.lock(), stderr.lock())
}

fn confirm_gui_command_execution_with_io<R: BufRead, W: Write>(
    path: &Path,
    hits: &[GuiCommandHit],
    mut reader: R,
    mut writer: W,
) -> Result<bool> {
    let commands = hits
        .iter()
        .map(|hit| hit.command.as_str())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>()
        .join(", ");
    writeln!(
        writer,
        "Warning: {} contains GUI-only Stata commands on line(s): {}.",
        path.display(),
        hits.iter()
            .map(|hit| hit.line_number.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    )?;
    writeln!(writer, "Detected command prefix(es): {commands}")?;
    writeln!(
        writer,
        "This command opens a Stata GUI dialog and is not suitable for CLI execution."
    )?;
    write!(writer, "Continue anyway? [y/n]: ")?;
    writer.flush()?;

    let mut input = String::new();
    if reader.read_line(&mut input)? == 0 {
        return Ok(false);
    }
    Ok(matches!(input.trim(), "y" | "Y"))
}

#[cfg(test)]
mod tests {
    use super::{
        confirm_gui_command_execution_with_io, detect_gui_command_in_line,
        scan_do_source_for_gui_commands, GuiCommandHit,
    };
    use std::io::Cursor;
    use std::path::Path;

    #[test]
    fn detects_direct_gui_commands() {
        let hits = scan_do_source_for_gui_commands("browse\n  edit price\n");
        assert_eq!(
            hits,
            vec![
                GuiCommandHit {
                    line_number: 1,
                    command: "browse".to_string(),
                },
                GuiCommandHit {
                    line_number: 2,
                    command: "edit".to_string(),
                },
            ]
        );
    }

    #[test]
    fn detects_wrapped_gui_commands() {
        let hits = scan_do_source_for_gui_commands(
            "capture browse\nquietly window manage forward results\n",
        );
        assert_eq!(hits[0].command, "browse");
        assert_eq!(hits[1].command, "window");
    }

    #[test]
    fn ignores_comments_and_strings() {
        let source = r#"
* browse
// edit price
display "browse the data"
/* dialog summarize */
local note "window"
"#;
        assert!(scan_do_source_for_gui_commands(source).is_empty());
    }

    #[test]
    fn tolerates_case_and_colon_wrappers() {
        assert_eq!(
            detect_gui_command_in_line("  QUIETLY: Browse"),
            Some("browse")
        );
        assert_eq!(
            detect_gui_command_in_line("NoIsIlY shell ls"),
            Some("shell")
        );
    }

    #[test]
    fn prompt_accepts_yes() {
        let mut output = Vec::new();
        let confirmed = confirm_gui_command_execution_with_io(
            Path::new("/tmp/test.do"),
            &[GuiCommandHit {
                line_number: 3,
                command: "browse".to_string(),
            }],
            Cursor::new(b"y\n"),
            &mut output,
        )
        .unwrap();
        assert!(confirmed);
        let rendered = String::from_utf8(output).unwrap();
        assert!(rendered.contains("This command opens a Stata GUI dialog"));
    }

    #[test]
    fn prompt_rejects_non_yes_and_eof() {
        let rejected = confirm_gui_command_execution_with_io(
            Path::new("/tmp/test.do"),
            &[GuiCommandHit {
                line_number: 1,
                command: "browse".to_string(),
            }],
            Cursor::new(b"n\n"),
            Vec::new(),
        )
        .unwrap();
        assert!(!rejected);

        let eof = confirm_gui_command_execution_with_io(
            Path::new("/tmp/test.do"),
            &[GuiCommandHit {
                line_number: 1,
                command: "browse".to_string(),
            }],
            Cursor::new(Vec::<u8>::new()),
            Vec::new(),
        )
        .unwrap();
        assert!(!eof);
    }
}
