use regex::Regex;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplClass {
    Command,
    AddonCommand,
    Keyword,
    Function,
    String,
    Comment,
    Macro,
    MacroCommand,
    Factor,
    BuiltinVariable,
    ResultClass,
    Number,
    Operator,
    EchoPrompt,
    Warning,
    Note,
    Error,
    ReturnCode,
    ResultNumber,
    Text,
}

const REPL_COMMANDS: &[&str] = &[
    "about",
    "append",
    "areg",
    "assert",
    "by",
    "bysort",
    "capture",
    "cd",
    "clear",
    "collapse",
    "contract",
    "count",
    "decode",
    "describe",
    "display",
    "do",
    "drop",
    "egen",
    "encode",
    "estimates",
    "export",
    "forvalues",
    "foreach",
    "generate",
    "graph",
    "gsort",
    "if",
    "in",
    "input",
    "insheet",
    "import",
    "ivregress",
    "keep",
    "list",
    "local",
    "log",
    "logit",
    "merge",
    "net",
    "notes",
    "predict",
    "preserve",
    "probit",
    "reg",
    "quietly",
    "regress",
    "rename",
    "replace",
    "reshape",
    "restore",
    "save",
    "scalar",
    "set",
    "sort",
    "summ",
    "summarize",
    "tabulate",
    "use",
    "twoway",
    "which",
    "xtdescribe",
    "xtreg",
    "xtset",
    "xtsum",
];
const REPL_ADDON_COMMANDS: &[&str] = &[
    "boottest",
    "coefplot",
    "estout",
    "esttab",
    "gcollapse",
    "gcontract",
    "gegen",
    "gisid",
    "glevelsof",
    "gquantiles",
    "ivreghdfe",
    "ivreg2",
    "outreg",
    "outreg2",
    "ppmlhdfe",
    "reghdfe",
    "winsor2",
];
const REPL_CONTROL_KEYWORDS: &[&str] = &["else", "forvalues", "foreach", "if", "in", "while"];
const REPL_MACRO_COMMANDS: &[&str] = &[
    "global", "local", "macro", "tempfile", "tempname", "tempvar",
];
const REPL_BUILTIN_FUNCTIONS: &[&str] = &[
    "abs",
    "ceil",
    "cond",
    "exp",
    "floor",
    "length",
    "ln",
    "log",
    "lower",
    "max",
    "min",
    "missing",
    "mi",
    "real",
    "regexm",
    "regexr",
    "regexs",
    "round",
    "sqrt",
    "string",
    "strpos",
    "subinstr",
    "substr",
    "trim",
    "upper",
    "word",
    "wordcount",
];
const REPL_RESULT_CLASSES: &[&str] = &["c", "e", "r", "s"];
const REPL_BUILTIN_VARIABLES: &[&str] = &["_b", "_coef", "_cons", "_n", "_N", "_rc", "_se"];
const REPL_OPERATORS: &[&str] = &[
    "!", "!=", "#", "&", "&&", "(", ")", "*", "+", ",", "-", ".", "/", ":", ":=", "<", "<=", "=",
    "==", ">", ">=", "^", "|", "||", "~", "~=",
];

fn token_pattern() -> Regex {
    Regex::new(
        r#"(\s+|///.*|//.*|/\*.*?\*/|`"(?:[^"\n]|"")*"'|"(?:[^"\n]|"")*"|`[^'\n]+'|\$[A-Za-z_][A-Za-z0-9_]*|>=|<=|==|!=|~=|:=|\|\||&&|[-+]?(?:\d+\.\d*|\.\d+|\d+)(?:[eE][-+]?\d+)?|\b[A-Za-z_][A-Za-z0-9_]*\b|[()\[\],:#=<>+\-*/.&|!^~])"#,
    )
    .unwrap()
}

fn factor_prefix_pattern() -> Regex {
    Regex::new(r"(?i)^(?:[ico]|ib|ibn|bn|b|o|n|[io]?b\d+|[io]?\d+)$").unwrap()
}

fn number_pattern() -> Regex {
    Regex::new(r"^[-+]?(?:\d+\.\d*|\.\d+|\d+)(?:[eE][-+]?\d+)?$").unwrap()
}

fn return_code_pattern() -> Regex {
    Regex::new(r"(?i)^\s*r\((\d+)\);\s*$").unwrap()
}

fn log_info_pattern() -> Regex {
    Regex::new(r"(?i)^\s*(name:|log:|log type:|opened on:|closed on:)\s*").unwrap()
}

fn note_pattern() -> Regex {
    Regex::new(r"(?i)^\s*note:").unwrap()
}

fn warning_pattern() -> Regex {
    Regex::new(r"(?i)^\s*warning:").unwrap()
}

fn error_pattern() -> Regex {
    Regex::new(
        r"(?i)^\s*(?:error\b|invalid syntax\b|no observations\b|file .+ not found\b|type mismatch\b|conformability error\b|command .+ unrecognized\b|insufficient observations\b)",
    )
    .unwrap()
}

fn is_number_prefix_boundary(byte: Option<u8>) -> bool {
    byte.map(|value| {
        let ch = value as char;
        !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
    })
    .unwrap_or(true)
}

fn is_number_suffix_boundary(byte: Option<u8>) -> bool {
    byte.map(|value| {
        let ch = value as char;
        !(ch.is_ascii_alphanumeric() || ch == '_' || ch == '.')
    })
    .unwrap_or(true)
}

fn match_number_span(text: &str, start: usize) -> Option<usize> {
    let bytes = text.as_bytes();
    if start >= bytes.len()
        || !is_number_prefix_boundary(start.checked_sub(1).map(|idx| bytes[idx]))
    {
        return None;
    }

    let mut index = start;
    if matches!(bytes[index], b'+' | b'-') {
        index += 1;
        if index >= bytes.len() {
            return None;
        }
    }

    let integer_start = index;
    while index < bytes.len() && bytes[index].is_ascii_digit() {
        index += 1;
    }
    let integer_digits = index > integer_start;

    let fraction_digits;
    if index < bytes.len() && bytes[index] == b'.' {
        let dot_index = index;
        index += 1;
        let fraction_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        fraction_digits = index > fraction_start;
        if !integer_digits && !fraction_digits {
            return None;
        }
        if !fraction_digits {
            index = dot_index + 1;
        }
    } else if !integer_digits {
        return None;
    }

    if index < bytes.len() && matches!(bytes[index], b'e' | b'E') {
        let exponent_start = index;
        index += 1;
        if index < bytes.len() && matches!(bytes[index], b'+' | b'-') {
            index += 1;
        }
        let exponent_digits_start = index;
        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }
        if index == exponent_digits_start {
            index = exponent_start;
        }
    }

    if !is_number_suffix_boundary(bytes.get(index).copied()) {
        return None;
    }

    Some(index)
}

fn is_factor_prefix(token: &str, next_token: Option<&str>) -> bool {
    next_token == Some(".") && factor_prefix_pattern().is_match(token)
}

fn is_number_token(token: &str) -> bool {
    number_pattern().is_match(token)
}

fn style_token(token: &str, next_token: Option<&str>) -> ReplClass {
    let lower = token.to_ascii_lowercase();
    if token.trim().is_empty() {
        return ReplClass::Text;
    }
    if token.starts_with("///")
        || token.starts_with("//")
        || (token.starts_with("/*") && token.ends_with("*/"))
    {
        return ReplClass::Comment;
    }
    if (token.starts_with('"') && token.ends_with('"'))
        || (token.starts_with("`\"") && token.ends_with("\"'"))
    {
        return ReplClass::String;
    }
    if token.starts_with('`') || token.starts_with('$') {
        return ReplClass::Macro;
    }
    if REPL_MACRO_COMMANDS.contains(&lower.as_str()) {
        return ReplClass::MacroCommand;
    }
    if REPL_CONTROL_KEYWORDS.contains(&lower.as_str()) {
        return ReplClass::Keyword;
    }
    if REPL_ADDON_COMMANDS.contains(&lower.as_str()) {
        return ReplClass::AddonCommand;
    }
    if REPL_COMMANDS.contains(&lower.as_str()) {
        return ReplClass::Command;
    }
    if REPL_BUILTIN_VARIABLES.contains(&lower.as_str()) {
        return ReplClass::BuiltinVariable;
    }
    if is_factor_prefix(&lower, next_token) {
        return ReplClass::Factor;
    }
    if REPL_RESULT_CLASSES.contains(&lower.as_str()) && next_token == Some("(") {
        return ReplClass::ResultClass;
    }
    if REPL_BUILTIN_FUNCTIONS.contains(&lower.as_str()) && next_token == Some("(") {
        return ReplClass::Function;
    }
    if REPL_OPERATORS.contains(&token) {
        return ReplClass::Operator;
    }
    if is_number_token(token) {
        return ReplClass::Number;
    }
    ReplClass::Text
}

pub(crate) fn lex_stata_line(line: &str) -> Vec<(ReplClass, String)> {
    let stripped = line.trim_start();
    if stripped.starts_with('*') {
        return vec![(ReplClass::Comment, line.to_string())];
    }

    let pattern = token_pattern();
    let matches: Vec<_> = pattern.find_iter(line).collect();
    if matches.is_empty() {
        return vec![(ReplClass::Text, line.to_string())];
    }

    let tokens: Vec<&str> = matches.iter().map(|m| m.as_str()).collect();
    let mut fragments = Vec::new();
    let mut cursor = 0usize;

    for (index, matched) in matches.iter().enumerate() {
        let start = matched.start();
        let end = matched.end();
        if start > cursor {
            fragments.push((ReplClass::Text, line[cursor..start].to_string()));
        }
        let token = matched.as_str();
        let next_token = tokens[index + 1..]
            .iter()
            .find(|token| !token.trim().is_empty())
            .copied();
        fragments.push((style_token(token, next_token), token.to_string()));
        cursor = end;
    }

    if cursor < line.len() {
        fragments.push((ReplClass::Text, line[cursor..].to_string()));
    }
    fragments
}

fn wrap_style(class: ReplClass, text: &str, colorize: bool) -> String {
    if !colorize || text.is_empty() {
        return text.to_string();
    }
    let code = match class {
        ReplClass::EchoPrompt => "\x1b[1;36m",
        ReplClass::Command => "\x1b[1;35m",
        ReplClass::AddonCommand => "\x1b[1;95m",
        ReplClass::Keyword => "\x1b[35m",
        ReplClass::Function => "\x1b[34m",
        ReplClass::String => "\x1b[32m",
        ReplClass::Comment => "\x1b[3;90m",
        ReplClass::Macro => "\x1b[33m",
        ReplClass::MacroCommand => "\x1b[1;33m",
        ReplClass::Factor => "\x1b[1;96m",
        ReplClass::BuiltinVariable => "\x1b[94m",
        ReplClass::ResultClass => "\x1b[92m",
        ReplClass::Number => "\x1b[34m",
        ReplClass::Operator => "\x1b[31m",
        ReplClass::Warning => "\x1b[1;33m",
        ReplClass::Note => "\x1b[96m",
        ReplClass::Error | ReplClass::ReturnCode => "\x1b[1;31m",
        ReplClass::ResultNumber => "\x1b[1;94m",
        ReplClass::Text => "",
    };
    if code.is_empty() {
        text.to_string()
    } else {
        format!("{code}{text}\x1b[0m")
    }
}

pub(crate) fn highlight_input_line(line: &str, colorize: bool) -> String {
    let mut rendered = String::new();
    for (class, fragment) in lex_stata_line(line) {
        rendered.push_str(&wrap_style(class, &fragment, colorize));
    }
    rendered
}

pub(crate) fn sanitize_repl_output(text: &str) -> String {
    if text.is_empty() {
        return String::new();
    }
    let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = normalized.split('\n').collect();
    let mut cleaned: Vec<String> = Vec::new();
    let mut pending_separator = false;

    for line in lines {
        let stripped = line.trim();
        if stripped
            == "-------------------------------------------------------------------------------"
        {
            pending_separator = true;
            continue;
        }
        if stripped.starts_with("> _") && stripped.ends_with(".log") {
            pending_separator = false;
            continue;
        }
        if log_info_pattern().is_match(line) {
            pending_separator = false;
            continue;
        }
        if line.starts_with(". ") || line.starts_with("> ") {
            pending_separator = false;
            continue;
        }
        if stripped.starts_with(". quietly set seed ")
            || stripped.starts_with(". capture log close")
        {
            pending_separator = false;
            continue;
        }
        if line.starts_with("> ")
            && cleaned
                .last()
                .map(|last| last.starts_with(". "))
                .unwrap_or(false)
        {
            if let Some(last) = cleaned.last_mut() {
                *last = format!("{last} {}", line[2..].trim_start());
            }
            continue;
        }
        if pending_separator && !cleaned.is_empty() && !stripped.is_empty() {
            cleaned.push(String::new());
            pending_separator = false;
        } else if pending_separator {
            pending_separator = false;
        }
        cleaned.push(line.to_string());
    }

    while matches!(cleaned.first(), Some(line) if line.trim().is_empty()) {
        cleaned.remove(0);
    }
    while matches!(cleaned.last(), Some(line) if line.trim().is_empty()) {
        cleaned.pop();
    }

    let mut collapsed = Vec::new();
    let mut previous_blank = false;
    for line in cleaned {
        let is_blank = line.trim().is_empty();
        if is_blank && previous_blank {
            continue;
        }
        collapsed.push(line);
        previous_blank = is_blank;
    }
    collapsed.join("\n")
}

fn highlight_numbers(text: &str, colorize: bool) -> String {
    let mut rendered = String::new();
    let mut cursor = 0usize;
    let bytes = text.as_bytes();

    while cursor < bytes.len() {
        if let Some(end) = match_number_span(text, cursor) {
            rendered.push_str(&wrap_style(
                ReplClass::ResultNumber,
                &text[cursor..end],
                colorize,
            ));
            cursor = end;
            continue;
        }
        let next = text[cursor..]
            .chars()
            .next()
            .expect("cursor always points to a valid character boundary");
        rendered.push(next);
        cursor += next.len_utf8();
    }
    rendered
}

pub(crate) fn format_repl_output(text: &str, colorize: bool) -> String {
    let mut rendered = String::new();
    for raw_line in text.split_inclusive('\n') {
        let (line, newline) = if let Some(stripped) = raw_line.strip_suffix('\n') {
            (stripped, "\n")
        } else {
            (raw_line, "")
        };
        let trimmed = line.trim();
        if trimmed.is_empty() {
            rendered.push_str(raw_line);
            continue;
        }
        if line.starts_with(". ") || line.starts_with("> ") {
            let prompt = &line[..2];
            let rest = &line[2..];
            rendered.push_str(&wrap_style(ReplClass::EchoPrompt, prompt, colorize));
            rendered.push_str(&highlight_input_line(rest, colorize));
        } else if return_code_pattern().is_match(trimmed) {
            rendered.push_str(&wrap_style(ReplClass::ReturnCode, line, colorize));
        } else if error_pattern().is_match(trimmed) {
            rendered.push_str(&wrap_style(ReplClass::Error, line, colorize));
        } else if warning_pattern().is_match(trimmed) {
            rendered.push_str(&wrap_style(ReplClass::Warning, line, colorize));
        } else if note_pattern().is_match(trimmed) {
            rendered.push_str(&wrap_style(ReplClass::Note, line, colorize));
        } else {
            rendered.push_str(&highlight_numbers(line, colorize));
        }
        rendered.push_str(newline);
    }
    rendered
}

#[cfg(test)]
mod tests {
    use super::{
        format_repl_output, highlight_input_line, lex_stata_line, sanitize_repl_output, ReplClass,
    };

    #[test]
    fn lex_stata_line_highlights_basic_tokens() {
        let fragments = lex_stata_line("regress y x1 if x1 >= 1 // note");
        assert!(fragments
            .iter()
            .any(|(class, text)| *class == ReplClass::Command && text == "regress"));
        assert!(fragments
            .iter()
            .any(|(class, text)| *class == ReplClass::Keyword && text == "if"));
        assert!(fragments
            .iter()
            .any(|(class, text)| *class == ReplClass::Operator && text == ">="));
        assert!(fragments
            .iter()
            .any(|(class, text)| *class == ReplClass::Number && text == "1"));
        assert!(fragments
            .iter()
            .any(|(class, text)| *class == ReplClass::Comment && text == "// note"));
    }

    #[test]
    fn sanitize_repl_output_removes_internal_wrapper_noise() {
        let raw_output = "-------------------------------------------------------------------------------\n  name:  <unnamed>\n   log:  /tmp/stata.log\n> _1776689348098.log\n. quietly set seed 1\n\n. display 2+3\n5\n\n. capture log close _all\n";
        let cleaned = sanitize_repl_output(raw_output);
        assert!(!cleaned.contains("quietly set seed"));
        assert!(!cleaned.contains("capture log close"));
        assert!(!cleaned.contains("> _1776689348098.log"));
        assert!(cleaned.contains("5"));
    }

    #[test]
    fn sanitize_repl_output_removes_echoed_commands_and_continuations() {
        let raw_output = ". clear all\n. cd /tmp/project\n/tmp/project\n> legend(off)\n";
        let cleaned = sanitize_repl_output(raw_output);
        assert!(!cleaned.contains(". clear all"));
        assert!(!cleaned.contains(". cd /tmp/project"));
        assert!(!cleaned.contains("> legend(off)"));
        assert_eq!(cleaned.trim(), "/tmp/project");
    }

    #[test]
    fn sanitize_repl_output_strips_trailing_stata_prompt_echo() {
        // The engine output ends with Stata's own prompt echo (". " with a
        // trailing space); it must not leak into the REPL output as a phantom
        // line.
        let raw_output = "4\n\n. ";
        assert_eq!(sanitize_repl_output(raw_output), "4");
        let raw_output = "summarize lnw\n\n    Variable |        Obs        Mean\n. ";
        let cleaned = sanitize_repl_output(raw_output);
        assert!(cleaned.ends_with("Mean"));
        assert!(!cleaned.contains(". "));
    }

    #[test]
    fn format_repl_output_preserves_plain_text_when_not_colorizing() {
        let rendered =
            format_repl_output(". display 2+3\n5\nwarning: file will be replaced\n", false);
        assert!(rendered.contains(". display 2+3"));
        assert!(rendered.contains("5"));
        assert!(rendered.contains("warning: file will be replaced"));
    }

    #[test]
    fn highlight_input_line_adds_ansi_sequences_when_enabled() {
        let rendered = highlight_input_line("display 2+3", true);
        assert!(rendered.contains("\u{1b}["));
    }
}
