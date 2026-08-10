//! Stata-source-level helpers: quoting paths for generated commands,
//! sanitizing identifiers that end up in file names, joining `///`
//! continuations, and recognizing interactive/blocked command prefixes.

use anyhow::{bail, Result};
use std::path::Path;

/// Quote a path for embedding in a Stata command line.
///
/// Simple double quotes cover most paths. A path containing a double quote is
/// wrapped in Stata compound double quotes (`` `"..."' ``), which Stata
/// documents as the robust way to pass filenames with unusual characters.
/// NUL/CR/LF cannot appear in a Stata command line and are rejected instead
/// of being silently stripped.
pub(crate) fn stata_quote_path(path: &str) -> Result<String> {
    if path.contains('\0') || path.contains('\r') || path.contains('\n') {
        bail!(
            "Path contains control characters that Stata cannot handle: {:?}",
            path
        );
    }
    if path.contains('"') {
        Ok(format!("`\"{path}\"'"))
    } else {
        Ok(format!("\"{path}\""))
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
) -> Result<String> {
    let processed = join_stata_line_continuations(selection);
    match working_dir.filter(|wd| Path::new(wd).is_dir()) {
        Some(wd) => Ok(format!("cd {}\n{processed}", stata_quote_path(wd)?)),
        None => Ok(processed),
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

/// Parse a selection that must consist of exactly one Stata command line
/// (after `///` continuation joining). Comment and blank lines are ignored.
pub(crate) fn parse_single_command(selection: &str) -> Option<(String, Vec<String>)> {
    let joined = join_stata_line_continuations(selection);
    let mut parsed_lines = Vec::new();
    for raw_line in joined.lines() {
        if let Some(parsed) = parse_stata_command_line(raw_line) {
            parsed_lines.push(parsed);
        }
    }
    if parsed_lines.len() != 1 {
        return None;
    }
    parsed_lines.pop()
}

/// Reduce a raw help argument list to a safe file lookup key: first token,
/// leading manual-section markers (`[R]`/`[TS]`) and trailing punctuation
/// removed, everything outside `[A-Za-z0-9_-]` dropped so the value can never
/// break out of a generated `findfile` command.
pub(crate) fn clean_help_topic(raw: &str) -> String {
    let mut tokens: Vec<&str> = raw.split_whitespace().collect();
    if tokens
        .first()
        .map(|token| token.starts_with('[') && token.ends_with(']'))
        .unwrap_or(false)
    {
        tokens.remove(0);
    }
    let first = tokens.first().copied().unwrap_or("");
    first
        .trim_matches(|ch: char| matches!(ch, ',' | ';' | '(' | ')'))
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-'))
        .collect()
}

/// Look up a reference doc for `topic` inside a skill package root (a
/// directory containing `references/`, `packages/`, and `SKILL.md`).
/// Returns the doc path relative to that root.
fn skill_doc_for_help_topic(skill_root: &Path, topic: &str) -> Option<String> {
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
        let relative = Path::new(&folder).join(format!("{name}.md"));
        if skill_root.join(&relative).is_file() {
            return Some(format!("{folder}/{name}.md"));
        }
    }
    if skill_root.join("SKILL.md").is_file() {
        return Some("SKILL.md".to_string());
    }
    None
}

fn doc_pointer(topic: &str, workspace: Option<&Path>, repo_root: Option<&Path>) -> Option<String> {
    // Workspaces created by older `stata-cli init` versions still carry the
    // skill under `skills/stata-cli/`; keep routing them for compatibility.
    if let Some(workspace) = workspace {
        let legacy_skill = workspace.join("skills").join("stata-cli");
        if let Some(doc) = skill_doc_for_help_topic(&legacy_skill, topic) {
            return Some(format!("skills/stata-cli/{doc}"));
        }
    }
    if let Some(root) = repo_root {
        let skill_package = root.join("skill").join("stata-cli");
        if let Some(doc) = skill_doc_for_help_topic(&skill_package, topic) {
            return Some(format!("skill/stata-cli/{doc}"));
        }
    }
    None
}

/// Guidance text for interactive Stata commands that produce no terminal
/// output in CLI mode: `help` with a missing or unknown topic, `search`, and
/// `findit`.
pub(crate) fn help_guidance_message(
    command: &str,
    topic: Option<&str>,
    workspace: Option<&Path>,
    repo_root: Option<&Path>,
) -> String {
    let mut message = match command {
        "search" | "findit" => format!(
            "`{command}` opens the interactive Stata search window and produces no terminal \
             output in CLI mode. Use `help <topic>` to render local help text instead, or read \
             the `stata-cli` skill's reference library."
        ),
        "help" => match topic {
            None | Some("") => "`help` needs a topic in CLI mode (Stata would open its Viewer \
                window instead of printing to the terminal). Try `help regress`, or read the \
                `stata-cli` skill's reference library."
                .to_string(),
            Some(topic) => format!(
                "No local help file found for `{topic}`. Check the spelling (for example \
                 `help regress`), or read the `stata-cli` skill's reference library."
            ),
        },
        other => format!("`{other}` is not supported in CLI mode."),
    };
    if let Some(topic) = topic.filter(|topic| !topic.is_empty()) {
        if let Some(doc) = doc_pointer(topic, workspace, repo_root) {
            message.push_str(&format!(" Start with `{doc}`."));
        }
    }
    message
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_paths_for_stata() {
        assert_eq!(stata_quote_path("/tmp/a b.do").unwrap(), "\"/tmp/a b.do\"");
    }

    #[test]
    fn quotes_embedded_double_quote_with_compound_quotes() {
        assert_eq!(
            stata_quote_path("/tmp/a\"b.do").unwrap(),
            "`\"/tmp/a\"b.do\"'"
        );
    }

    #[test]
    fn rejects_path_control_characters() {
        assert!(stata_quote_path("/tmp/a\nb.do").is_err());
        assert!(stata_quote_path("/tmp/a\rb.do").is_err());
        assert!(stata_quote_path("/tmp/a\0b.do").is_err());
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
        let selection = build_selection_for_working_dir(code, Some(&wd)).unwrap();
        assert!(selection.starts_with(&format!("cd \"{wd}\"\n")));
    }

    #[test]
    fn detects_blocked_interactive_prefixes() {
        assert!(blocked_interactive_prefix("quietly browse price").is_some());
        assert!(blocked_interactive_prefix("summarize price").is_none());
        assert!(blocked_interactive_prefix("* browse is a comment").is_none());
    }

    #[test]
    fn parses_single_command_selections() {
        assert_eq!(
            parse_single_command("help regress").map(|(command, _)| command),
            Some("help".to_string())
        );
        assert_eq!(
            parse_single_command("capture help regress").map(|(command, _)| command),
            Some("help".to_string())
        );
        assert!(parse_single_command("help regress\nsummarize x").is_none());
        assert!(parse_single_command("help regress ///\nsummarize x").is_some());
        assert!(parse_single_command("* a comment\nhelp regress").is_some());
        assert!(parse_single_command("").is_none());
    }

    #[test]
    fn cleans_help_topics() {
        assert_eq!(clean_help_topic("regress, nodates"), "regress");
        assert_eq!(clean_help_topic("[R] regress"), "regress");
        assert_eq!(clean_help_topic("[TS] tsset, panel"), "tsset");
        assert_eq!(clean_help_topic("summarize"), "summarize");
        assert_eq!(clean_help_topic(""), "");
        assert_eq!(clean_help_topic("regress; drop _all"), "regress");
        assert_eq!(clean_help_topic("regress\" ; findfile x"), "regress");
        assert_eq!(clean_help_topic("esttab, replace"), "esttab");
    }

    #[test]
    fn guidance_mentions_local_reference_library() {
        let message = help_guidance_message("help", None, None, None);
        assert!(message.contains("needs a topic"));
        let message = help_guidance_message("help", Some("bogus"), None, None);
        assert!(message.contains("No local help file"));
        let message = help_guidance_message("search", None, None, None);
        assert!(message.contains("search window"));
        let message = help_guidance_message("findit", None, None, None);
        assert!(message.contains("search window"));
    }

    #[test]
    fn guidance_points_at_workspace_skill_docs() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = temp.path();
        let doc = workspace
            .join("skills")
            .join("stata-cli")
            .join("references");
        std::fs::create_dir_all(&doc).unwrap();
        std::fs::write(doc.join("linear-regression.md"), "x").unwrap();
        let message =
            help_guidance_message("help", Some("linear-regression"), Some(workspace), None);
        assert!(message.contains("skills/stata-cli/references/linear-regression.md"));
    }

    #[test]
    fn guidance_points_at_repo_skill_docs() {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path();
        let doc = root.join("skill").join("stata-cli").join("references");
        std::fs::create_dir_all(&doc).unwrap();
        std::fs::write(doc.join("linear-regression.md"), "x").unwrap();
        let message = help_guidance_message("help", Some("linear-regression"), None, Some(root));
        assert!(message.contains("skill/stata-cli/references/linear-regression.md"));
    }
}
