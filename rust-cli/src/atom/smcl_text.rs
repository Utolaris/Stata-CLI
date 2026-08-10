use regex::Regex;

fn resolve_char(code: &str) -> String {
    match code.trim() {
        "S|" => "$".to_string(),
        "'g" => "`".to_string(),
        "-(" => "{".to_string(),
        ")-" => "}".to_string(),
        "-" => "\u{2500}".to_string(),
        "|" => "\u{2502}".to_string(),
        "+" => "\u{253c}".to_string(),
        "TT" => "\u{252c}".to_string(),
        "BT" => "\u{2534}".to_string(),
        "LT" => "\u{251c}".to_string(),
        "RT" => "\u{2524}".to_string(),
        "TLC" => "\u{250c}".to_string(),
        "TRC" => "\u{2510}".to_string(),
        "BRC" => "\u{2518}".to_string(),
        "BLC" => "\u{2514}".to_string(),
        "a'" => "\u{00e1}".to_string(),
        "A'" => "\u{00c1}".to_string(),
        "e'" => "\u{00e9}".to_string(),
        "E'" => "\u{00c9}".to_string(),
        "i'" => "\u{00ed}".to_string(),
        "I'" => "\u{00cd}".to_string(),
        "o'" => "\u{00f3}".to_string(),
        "O'" => "\u{00d3}".to_string(),
        "u'" => "\u{00fa}".to_string(),
        "U'" => "\u{00da}".to_string(),
        "a.." => "\u{00e4}".to_string(),
        "A.." => "\u{00c4}".to_string(),
        "o.." => "\u{00f6}".to_string(),
        "O.." => "\u{00d6}".to_string(),
        "u.." => "\u{00fc}".to_string(),
        "U.." => "\u{00dc}".to_string(),
        other if other.starts_with("0x") || other.starts_with("0X") => {
            u32::from_str_radix(&other[2..], 16)
                .ok()
                .and_then(char::from_u32)
                .map(|ch| ch.to_string())
                .unwrap_or_else(|| other.to_string())
        }
        other => other
            .parse::<u32>()
            .ok()
            .and_then(char::from_u32)
            .map(|ch| ch.to_string())
            .unwrap_or_else(|| other.to_string()),
    }
}

fn find_brace(text: &str, start: usize) -> Option<usize> {
    let mut depth = 1usize;
    for (offset, ch) in text[start + 1..].char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + 1 + offset);
                }
            }
            _ => {}
        }
    }
    None
}

fn split_tag_content(content: &str) -> (String, String, Option<String>) {
    let content = content.trim();
    if let Some((name, rest)) = content.split_once(':') {
        if !name.contains(' ') {
            return (
                name.trim().to_string(),
                String::new(),
                Some(rest.to_string()),
            );
        }
    }
    let mut parts = content.splitn(2, ' ');
    let name = parts.next().unwrap_or_default().trim().to_string();
    let rest = parts.next().unwrap_or_default().trim().to_string();
    (name, rest, None)
}

// ---------------------------------------------------------------------------
// Plain-text rendering (REPL `help` output)
// ---------------------------------------------------------------------------

/// Structural SMCL tags that only make sense in the Stata Viewer and must not
/// leak into plain text.
const STRUCTURAL_TAGS: &[&str] = &[
    "smcl",
    "*",
    "...",
    "viewerdialog",
    "vieweralsosee",
    "vieweralso",
    "viewerjumpto",
    "findalias",
    "marker",
    "synoptset",
    "synhighlight",
    "synoptline",
    "synopthdr",
    "p2colset",
    "p2colreset",
    "p2colclear",
    "INCLUDE",
    "col",
    "break",
    "dlgtab",
    "p_end",
    "bind",
    "syntab",
    "synonym",
    "hline",
    "p",
    "pstd",
    "pmore",
    "pmore2",
    "phang",
    "phang2",
];

fn is_structural_tag(name: &str) -> bool {
    STRUCTURAL_TAGS.contains(&name)
}

/// Strip a `{help topic##|_sub}`-style subentry marker so plain text shows
/// just the topic.
fn strip_subentry(value: &str) -> &str {
    value.split("##|").next().unwrap_or(value)
}

/// Viewer help-link shorthand inside option tags:
/// `(topic##subentry:label)` becomes plain `(label)`.
fn collapse_help_links(text: &str) -> String {
    let pattern = Regex::new(r"\(([^#()]+##[^:():]+):([^{}()]*)\)").unwrap();
    pattern.replace_all(text, "($2)").into_owned()
}

fn render_tag_text(content: &str) -> String {
    let (name, args, inner) = split_tag_content(content);
    match name.as_str() {
        "c" => resolve_char(&args),
        "help" => {
            let (topic, label) = match args.split_once(':') {
                Some((topic, label)) => (topic, label),
                None => (args.as_str(), args.as_str()),
            };
            let label = inner.unwrap_or_else(|| strip_subentry(label).to_string());
            let rendered = render_inline_text(&label);
            if rendered.trim().is_empty() {
                render_inline_text(strip_subentry(topic))
            } else {
                rendered
            }
        }
        "browse" => {
            // URLs contain `:` themselves, so prefer splitting on the `":`
            // sequence that separates the quoted target from the label.
            let (target, label) = match args.split_once("\":") {
                Some((target, label)) => (target, label),
                None => match args.split_once(':') {
                    Some((target, label)) => (target, label),
                    None => (args.as_str(), args.as_str()),
                },
            };
            let target = target.trim_matches('"');
            let label = inner.unwrap_or_else(|| label.to_string());
            let rendered = render_inline_text(&label);
            if rendered.trim().is_empty() {
                render_inline_text(target)
            } else {
                rendered
            }
        }
        "opt" | "opth" => {
            // `{opt nocons:tant}` renders as the concatenated option name
            // (`noconstant`), with the suffix continued inside nested tags.
            let combined = match args.split_once(':') {
                Some((head, tail)) => format!("{head}{tail}"),
                None => args,
            };
            let mut rendered = render_inline_text(&collapse_help_links(&combined));
            if let Some(inner) = inner {
                rendered.push(' ');
                rendered.push_str(&render_inline_text(&inner));
            }
            rendered
        }
        "synopt" => {
            // `{synopt :{opt ...}}...` wraps one option-syntax line; strip the
            // leading `:` and render the wrapped content. The description
            // text follows the tag directly in the file, so keep a trailing
            // space to avoid gluing `noconstant` and `suppress ...` together.
            let content = args.strip_prefix(':').unwrap_or(&args);
            let mut rendered = render_inline_text(inner.as_deref().unwrap_or(content));
            if !rendered.is_empty() {
                rendered.push(' ');
            }
            rendered
        }
        "p2col" => inner
            .map(|inner| render_inline_text(&inner))
            .unwrap_or_default(),
        "mansection" => match args.split_once(':') {
            Some((_, label)) => render_inline_text(label),
            None => String::new(),
        },
        "manhelp" => match args.split_once(':') {
            Some((_, label)) => render_inline_text(label),
            None => String::new(),
        },
        "helpb" => render_inline_text(&args),
        "bf" | "it" | "cmd" | "err" | "res" | "com" | "ul" | "/ul" => {
            render_inline_text(inner.as_deref().unwrap_or(&args))
        }
        "txt" | "inp" | "red" | "grp" | "clean" | "sf" => inner
            .map(|inner| render_inline_text(&inner))
            .unwrap_or_default(),
        _ => {
            if is_structural_tag(&name) {
                String::new()
            } else if let Some(inner) = inner {
                render_inline_text(&inner)
            } else {
                match args.split_once(':') {
                    // `{topic with spaces:label}` is a Viewer help-link
                    // shorthand; plain text shows just the label.
                    Some((_, label)) if !label.trim().is_empty() => render_inline_text(label),
                    _ => {
                        // Unknown placeholder tags such as `{depvar}` render
                        // as their own text in the Stata Viewer.
                        let mut rendered = name;
                        if !args.is_empty() {
                            rendered.push(' ');
                            rendered.push_str(&args);
                        }
                        rendered
                    }
                }
            }
        }
    }
}

pub(crate) fn render_inline_text(text: &str) -> String {
    let mut rendered = String::new();
    let mut cursor = 0usize;

    while let Some(relative_start) = text[cursor..].find('{') {
        let start = cursor + relative_start;
        if start > cursor {
            rendered.push_str(&text[cursor..start]);
        }
        if let Some(end) = find_brace(text, start) {
            rendered.push_str(&render_tag_text(&text[start + 1..end]));
            cursor = end + 1;
        } else {
            rendered.push_str(&text[start..]);
            cursor = text.len();
        }
    }

    if cursor < text.len() {
        rendered.push_str(&text[cursor..]);
    }
    rendered
}

fn title_text_pattern() -> Regex {
    Regex::new(r"^\s*\{title:(.*)\}\s*$").unwrap()
}

fn hline_text_pattern() -> Regex {
    Regex::new(r"^\s*\{hline(?:\s+\d+)?\}\s*$").unwrap()
}

fn paragraph_text_pattern() -> Regex {
    Regex::new(r"^\s*\{p(?:std|more\d?|hang\d?)?(?:\s+\d+){0,3}\}\s*(.*)$").unwrap()
}

/// Render an `.sthlp` file (SMCL markup) as plain terminal text: structural
/// Viewer tags are dropped, paragraphs and titles become plain lines, and
/// inline formatting tags are unwrapped.
pub(crate) fn render_smcl_to_text(smcl: &str) -> String {
    let normalized = smcl.replace("\r\n", "\n").replace('\r', "\n");
    let mut rendered_lines: Vec<String> = Vec::new();
    for line in normalized.split('\n') {
        if line.trim_start().starts_with("INCLUDE ") {
            continue; // bare Viewer include directive (no braces in the file)
        }
        if let Some(caps) = title_text_pattern().captures(line) {
            let title = render_inline_text(caps.get(1).map(|m| m.as_str()).unwrap_or_default());
            rendered_lines.push(title.trim_end().to_string());
            rendered_lines.push(String::new());
            continue;
        }
        if hline_text_pattern().is_match(line) {
            rendered_lines.push("-".repeat(60));
            continue;
        }
        if let Some(caps) = paragraph_text_pattern().captures(line) {
            let rest = caps.get(1).map(|m| m.as_str()).unwrap_or_default();
            let rendered = render_inline_text(rest);
            if !rendered.trim().is_empty() {
                rendered_lines.push(rendered.trim_end().to_string());
            }
            rendered_lines.push(String::new());
            continue;
        }
        let rendered = render_inline_text(line);
        if !rendered.trim().is_empty() {
            rendered_lines.push(rendered.trim_end().to_string());
        }
    }

    let mut output: Vec<String> = Vec::new();
    let mut previous_blank = true;
    for line in rendered_lines {
        let blank = line.trim().is_empty();
        if blank && previous_blank {
            continue;
        }
        previous_blank = blank;
        output.push(line);
    }
    while output
        .last()
        .map(|line| line.trim().is_empty())
        .unwrap_or(false)
    {
        output.pop();
    }
    output.join("\n")
}

#[cfg(test)]
mod tests {
    use super::{render_inline_text, render_smcl_to_text};

    #[test]
    fn render_smcl_to_text_strips_structural_markup() {
        let smcl = "\
{smcl}
{* *! version 1.5.3  11apr2023}{...}
{viewerdialog regress \"dialog regress\"}{...}
{viewerjumpto \"Syntax\" \"regress##syntax\"}
{title:regress -- Linear regression}
{hline}
{p 4 6 2}
{cmd:regress} {depvar} {indepvars} {if} {weight} {options}
{hline}
{pstd}
{bf:Description}
{phang2}
regress fits a linear regression. See {help regress##|_new:regress postestimation}
and {help tsvarlist}. Download {browse \"https://example.com\":external docs}.
";
        let text = render_smcl_to_text(smcl);
        assert!(!text.contains('{'), "{text}");
        assert!(text.contains("regress -- Linear regression"), "{text}");
        assert!(
            text.contains("regress depvar indepvars if weight options"),
            "{text}"
        );
        assert!(text.contains("Description"), "{text}");
        assert!(text.contains("regress postestimation"), "{text}");
        assert!(text.contains("tsvarlist"), "{text}");
        assert!(text.contains("external docs"), "{text}");
        assert!(text.contains("-".repeat(60).as_str()), "{text}");
    }

    #[test]
    fn render_smcl_to_text_expands_option_abbreviations() {
        let text = render_inline_text(
            "{opt nocons:tant} {opt r:obust} {opt vce:({regress##vcetype:vcetype})} {opt plus}",
        );
        assert!(text.contains("noconstant"), "{text}");
        assert!(text.contains("robust"), "{text}");
        assert!(text.contains("vce(vcetype)"), "{text}");
        assert!(text.contains("plus"), "{text}");
    }
}
