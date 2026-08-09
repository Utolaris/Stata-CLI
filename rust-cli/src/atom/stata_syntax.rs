//! Stata-source-level helpers: quoting paths for generated commands,
//! sanitizing identifiers that end up in file names, joining `///`
//! continuations, and recognizing interactive/blocked command prefixes.

use std::path::Path;

/// Quote a path for embedding in a Stata command line.
///
/// Stata accepts both `"..."` and `'...'` as string delimiters, so a path
/// containing double quotes is wrapped in single quotes. CR/LF are stripped
/// (they can never be part of a valid single-line Stata command). Paths
/// containing both quote characters are extremely rare and fall back to
/// double-quote wrapping.
pub(crate) fn stata_quote_path(path: &str) -> String {
    let cleaned: String = path
        .chars()
        .filter(|ch| !matches!(ch, '\r' | '\n'))
        .collect();
    if cleaned.contains('"') && !cleaned.contains('\'') {
        format!("'{cleaned}'")
    } else {
        format!("\"{cleaned}\"")
    }
}

/// Restrict a user-supplied session id to characters that are safe in file
/// names (`[A-Za-z0-9_-]`); anything else becomes `_`.
pub(crate) fn sanitize_session_id(id: &str) -> String {
    let sanitized: String = id
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if sanitized.is_empty() {
        "default".to_string()
    } else {
        sanitized
    }
}

pub(crate) fn join_stata_line_continuations(code: &str) -> String {
    let mut joined_lines: Vec<String> = Vec::new();
    let mut current_line = String::new();
    for raw_line in code.lines() {
        let stripped = raw_line.trim_end();
        if let Some(rest) = stripped.strip_suffix("///") {
            current_line.push_str(rest.trim_end());
            current_line.push(' ');
        } else {
            current_line.push_str(raw_line);
            joined_lines.push(std::mem::take(&mut current_line));
        }
    }
    if !current_line.is_empty() {
        joined_lines.push(current_line);
    }
    joined_lines.join("\n")
}

pub(crate) fn build_selection_for_working_dir(
    selection: &str,
    working_dir: Option<&str>,
) -> String {
    let processed = join_stata_line_continuations(selection);
    match working_dir.filter(|wd| Path::new(wd).is_dir()) {
        Some(wd) => format!("cd {}\n{processed}", stata_quote_path(wd)),
        None => processed,
    }
}

fn parse_stata_command_line(line: &str) -> Option<(String, Vec<String>)> {
    const WRAPPERS: &[&str] = &[
        "capture",
        "cap",
        "quietly",
        "qui",
        "noisily",
        "noi",
        "capturely",
        "captureily",
    ];
    let stripped = line.trim();
    if stripped.is_empty() || stripped.starts_with('*') || stripped.starts_with("//") {
        return None;
    }
    let mut tokens: Vec<String> = stripped
        .split_whitespace()
        .map(normalize_token)
        .filter(|token| !token.is_empty())
        .collect();
    while tokens
        .first()
        .map(|token| WRAPPERS.contains(&token.as_str()))
        .unwrap_or(false)
    {
        tokens.remove(0);
    }
    if tokens.is_empty() {
        return None;
    }
    Some((tokens.remove(0), tokens))
}

fn normalize_token(token: &str) -> String {
    token
        .trim_matches(|ch| matches!(ch, ':' | ',' | ';' | '(' | ')'))
        .to_lowercase()
}

pub(crate) fn blocked_interactive_prefix(selection: &str) -> Option<String> {
    const BLOCKED: &[&str] = &[
        "browse", "edit", "db", "dialog", "window", "shell", "winexec", "pause",
    ];
    for raw_line in selection.lines() {
        if let Some((command, _)) = parse_stata_command_line(raw_line) {
            if BLOCKED.contains(&command.as_str()) {
                return Some(command);
            }
        }
    }
    None
}

pub(crate) fn help_topic_guidance(selection: &str, repo_root: Option<&Path>) -> Option<String> {
    let mut parsed_lines = Vec::new();
    for raw_line in selection.lines() {
        if let Some(parsed) = parse_stata_command_line(raw_line) {
            parsed_lines.push(parsed);
        }
    }
    if parsed_lines.len() != 1 {
        return None;
    }
    let (command, args) = &parsed_lines[0];
    if command != "help" {
        return None;
    }
    let topic = args.join(" ").trim().to_string();
    if topic.is_empty() {
        return None;
    }

    let mut message = "`help {topic}` cannot be captured reliably from the local Stata terminal bridge. Read the local `skills/stata-cli/SKILL.md` reference library instead.".replace("{topic}", &topic);
    if let Some(root) = repo_root {
        if let Some(doc) = skill_doc_for_help_topic(root, &topic) {
            message.push_str(&format!(" Start with `{doc}`."));
        }
    }
    Some(message)
}

fn skill_doc_for_help_topic(repo_root: &Path, topic: &str) -> Option<String> {
    let normalized = topic.trim().to_lowercase();
    let aliases: &[(&str, &str)] = &[
        ("esttab", "estout"),
        ("estout", "estout"),
        ("eststo", "estout"),
        ("estadd", "estout"),
    ];
    let mut candidates: Vec<(String, String)> = Vec::new();
    if let Some((_, alias)) = aliases.iter().find(|(name, _)| *name == normalized) {
        candidates.push(("packages".to_string(), (*alias).to_string()));
    }
    candidates.push(("packages".to_string(), normalized.clone()));
    candidates.push(("references".to_string(), normalized));

    for (folder, name) in candidates {
        let relative = Path::new("boilerplate")
            .join("skills")
            .join("stata-cli")
            .join(&folder)
            .join(format!("{name}.md"));
        if repo_root.join(&relative).is_file() {
            return Some(format!("skills/stata-cli/{folder}/{name}.md"));
        }
    }
    Some("skills/stata-cli/SKILL.md".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_paths_for_stata() {
        assert_eq!(stata_quote_path("/tmp/a b.do"), "\"/tmp/a b.do\"");
        assert_eq!(stata_quote_path("/tmp/a\"b.do"), "'/tmp/a\"b.do'");
        assert_eq!(stata_quote_path("/tmp/a\nb.do"), "\"/tmp/ab.do\"");
    }

    #[test]
    fn sanitizes_session_ids() {
        assert_eq!(sanitize_session_id("abc-123_DEF"), "abc-123_DEF");
        assert_eq!(sanitize_session_id("a/b..c"), "a_b__c");
        assert_eq!(sanitize_session_id(""), "default");
    }

    #[test]
    fn joins_line_continuations() {
        let code = "display ///\n2+2\n";
        assert_eq!(join_stata_line_continuations(code), "display 2+2");
    }

    #[test]
    fn builds_selection_with_working_dir() {
        let temp = tempfile::tempdir().unwrap();
        let code = "use auto, clear";
        let wd = temp.path().to_string_lossy().into_owned();
        let selection = build_selection_for_working_dir(code, Some(&wd));
        assert!(selection.starts_with(&format!("cd \"{wd}\"\n")));
    }

    #[test]
    fn detects_blocked_interactive_prefixes() {
        assert!(blocked_interactive_prefix("quietly browse price").is_some());
        assert!(blocked_interactive_prefix("summarize price").is_none());
        assert!(blocked_interactive_prefix("* browse is a comment").is_none());
    }
}
