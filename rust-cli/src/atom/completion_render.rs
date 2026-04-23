use crate::atom::completion_catalog::{CompletionCandidate, CompletionKind};
use rustyline::completion::Pair;
use std::collections::HashSet;

fn kind_label(kind: CompletionKind) -> &'static str {
    match kind {
        CompletionKind::Command => "command",
        CompletionKind::AddonCommand => "addon",
        CompletionKind::Function => "function",
        CompletionKind::Keyword => "keyword",
        CompletionKind::Macro => "macro",
        CompletionKind::ResultClass => "result",
        CompletionKind::Variable => "variable",
        CompletionKind::BufferWord => "buffer",
    }
}

pub(crate) fn render_candidates(candidates: Vec<CompletionCandidate>) -> Vec<Pair> {
    let mut seen = HashSet::new();
    let mut rendered = Vec::new();

    for candidate in candidates {
        let key = candidate.text.to_ascii_lowercase();
        if !seen.insert(key) {
            continue;
        }
        rendered.push(Pair {
            display: format!("{} [{}]", candidate.text, kind_label(candidate.kind)),
            replacement: candidate.text,
        });
    }

    rendered
}

#[cfg(test)]
mod tests {
    use super::render_candidates;
    use crate::atom::completion_catalog::{CompletionCandidate, CompletionKind};

    #[test]
    fn render_candidates_deduplicates_case_insensitively() {
        let rendered = render_candidates(vec![
            CompletionCandidate {
                text: "display".to_string(),
                kind: CompletionKind::Command,
            },
            CompletionCandidate {
                text: "Display".to_string(),
                kind: CompletionKind::BufferWord,
            },
        ]);

        assert_eq!(rendered.len(), 1);
        assert_eq!(rendered[0].replacement, "display");
        assert!(rendered[0].display.contains("[command]"));
    }
}
