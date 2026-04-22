#![allow(dead_code)]

use regex::Regex;

const CSS: &str = r#"
body {
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif;
  font-size: 14px;
  line-height: 1.55;
  color: var(--vscode-editor-foreground);
  background: var(--vscode-editor-background);
  padding: 16px 24px 40px;
}
.smcl-title { font-size: 15px; font-weight: 600; margin: 22px 0 8px; border-bottom: 1px solid var(--vscode-editorGroup-border, #444); }
.smcl-line { white-space: pre-wrap; font-family: 'SF Mono', Menlo, Consolas, monospace; }
.smcl-p { margin: 6px 0; }
.smcl-cmd { font-family: 'SF Mono', Menlo, Consolas, monospace; font-weight: 600; color: var(--vscode-textLink-foreground); }
.smcl-err { color: var(--vscode-errorForeground, #f44); }
.smcl-res { font-weight: 600; }
.smcl-com { color: var(--vscode-descriptionForeground, #6a9955); font-family: 'SF Mono', Menlo, Consolas, monospace; }
a.smcl-help-link, a.smcl-browse-link { color: var(--vscode-textLink-foreground); text-decoration: none; }
a.smcl-help-link:hover, a.smcl-browse-link:hover { text-decoration: underline; }
hr.smcl-hline { border: none; border-top: 1px solid var(--vscode-editorGroup-border, #444); margin: 6px 0; }
"#;

const JS: &str = r#"
(function() {
  const vscode = typeof acquireVsCodeApi === 'function' ? acquireVsCodeApi() : null;
  document.addEventListener('click', function(event) {
    const helpLink = event.target.closest('a.smcl-help-link');
    if (helpLink) {
      event.preventDefault();
      if (vscode) {
        vscode.postMessage({ command: 'helpNavigate', topic: helpLink.dataset.topic || '' });
      }
      return;
    }
    const browseLink = event.target.closest('a.smcl-browse-link');
    if (browseLink) {
      event.preventDefault();
      if (vscode) {
        vscode.postMessage({ command: 'openExternal', url: browseLink.href });
      } else {
        window.open(browseLink.href, '_blank');
      }
    }
  });
})();
"#;

fn html_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

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

fn render_tag(content: &str) -> String {
    let (name, args, inner) = split_tag_content(content);
    match name.as_str() {
        "bf" => format!(
            "<strong>{}</strong>",
            render_inline(inner.as_deref().unwrap_or(args.as_str()))
        ),
        "it" => format!(
            "<em>{}</em>",
            render_inline(inner.as_deref().unwrap_or(args.as_str()))
        ),
        "cmd" => format!(
            "<span class=\"smcl-cmd\">{}</span>",
            render_inline(inner.as_deref().unwrap_or(args.as_str()))
        ),
        "err" => format!(
            "<span class=\"smcl-err\">{}</span>",
            render_inline(inner.as_deref().unwrap_or(args.as_str()))
        ),
        "res" => format!(
            "<span class=\"smcl-res\">{}</span>",
            render_inline(inner.as_deref().unwrap_or(args.as_str()))
        ),
        "com" => format!(
            "<span class=\"smcl-com\">{}</span>",
            render_inline(inner.as_deref().unwrap_or(args.as_str()))
        ),
        "help" => {
            let topic = args
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .replace(' ', "_");
            let label = inner.unwrap_or_else(|| args.clone());
            format!(
                "<a class=\"smcl-help-link\" href=\"#\" data-topic=\"{}\">{}</a>",
                html_escape(&topic),
                render_inline(&label)
            )
        }
        "browse" => {
            let target = args.trim_matches('"');
            let label = inner.unwrap_or_else(|| target.to_string());
            format!(
                "<a class=\"smcl-browse-link\" href=\"{}\">{}</a>",
                html_escape(target),
                render_inline(&label)
            )
        }
        "c" => html_escape(&resolve_char(&args)),
        "hline" => "<hr class=\"smcl-hline\" />".to_string(),
        _ => {
            if let Some(inner) = inner {
                render_inline(&inner)
            } else {
                html_escape(content)
            }
        }
    }
}

pub(crate) fn render_inline(text: &str) -> String {
    let mut rendered = String::new();
    let mut cursor = 0usize;

    while let Some(relative_start) = text[cursor..].find('{') {
        let start = cursor + relative_start;
        if start > cursor {
            rendered.push_str(&html_escape(&text[cursor..start]));
        }
        if let Some(end) = find_brace(text, start) {
            rendered.push_str(&render_tag(&text[start + 1..end]));
            cursor = end + 1;
        } else {
            rendered.push_str(&html_escape(&text[start..]));
            cursor = text.len();
        }
    }

    if cursor < text.len() {
        rendered.push_str(&html_escape(&text[cursor..]));
    }
    rendered
}

fn title_pattern() -> Regex {
    Regex::new(r"^\s*\{title:(.*)\}\s*$").unwrap()
}

fn hline_pattern() -> Regex {
    Regex::new(r"^\s*\{hline\}\s*$").unwrap()
}

fn p_pattern() -> Regex {
    Regex::new(r"^\s*\{p\}\s*(.*)$").unwrap()
}

pub(crate) fn render_smcl_to_html(text: &str) -> String {
    let mut body = String::new();
    for line in text.replace("\r\n", "\n").replace('\r', "\n").split('\n') {
        if let Some(caps) = title_pattern().captures(line) {
            body.push_str(&format!(
                "<h2 class=\"smcl-title\">{}</h2>\n",
                render_inline(caps.get(1).map(|m| m.as_str()).unwrap_or_default())
            ));
            continue;
        }
        if hline_pattern().is_match(line) {
            body.push_str("<hr class=\"smcl-hline\" />\n");
            continue;
        }
        if let Some(caps) = p_pattern().captures(line) {
            body.push_str(&format!(
                "<p class=\"smcl-p\">{}</p>\n",
                render_inline(caps.get(1).map(|m| m.as_str()).unwrap_or_default())
            ));
            continue;
        }
        if line.trim().is_empty() {
            body.push('\n');
            continue;
        }
        body.push_str(&format!(
            "<div class=\"smcl-line\">{}</div>\n",
            render_inline(line)
        ));
    }

    format!(
        "<!DOCTYPE html><html><head><meta charset=\"utf-8\" /><style>{CSS}</style></head><body>{body}<script>{JS}</script></body></html>"
    )
}

#[cfg(test)]
mod tests {
    use super::{render_inline, render_smcl_to_html};

    #[test]
    fn render_inline_supports_help_links_and_char_codes() {
        let rendered = render_inline(r#"{help regress} {c TLC}"#);
        assert!(rendered.contains("smcl-help-link"));
        assert!(rendered.contains("data-topic=\"regress\""));
        assert!(rendered.contains("┌"));
    }

    #[test]
    fn render_smcl_to_html_wraps_document_and_escapes_html() {
        let html = render_smcl_to_html("{title:Regression}\n{p}Use {cmd:regress} < safely");
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("Regression"));
        assert!(html.contains("smcl-cmd"));
        assert!(html.contains("&lt; safely"));
    }
}
