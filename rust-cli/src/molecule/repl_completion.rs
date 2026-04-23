use crate::atom::completion_cache::CompletionCache;
use crate::atom::completion_catalog::{static_candidates, CompletionCandidate, CompletionKind};
use crate::atom::completion_context::{completion_context, CompletionContextKind};
use crate::atom::completion_render::render_candidates;
use crate::atom::json_contract::CompletionContextResult;
use rustyline::completion::Pair;
use std::collections::HashSet;

fn sort_priority(kind: CompletionKind, context_kind: CompletionContextKind) -> usize {
    match context_kind {
        CompletionContextKind::Command => match kind {
            CompletionKind::Command => 0,
            CompletionKind::AddonCommand => 1,
            CompletionKind::Keyword => 2,
            CompletionKind::BufferWord => 3,
            CompletionKind::Function => 4,
            CompletionKind::Macro => 5,
            CompletionKind::Variable => 6,
            CompletionKind::ResultClass => 7,
        },
        CompletionContextKind::Function => match kind {
            CompletionKind::Function => 0,
            CompletionKind::ResultClass => 1,
            CompletionKind::BufferWord => 2,
            CompletionKind::Command => 3,
            CompletionKind::AddonCommand => 4,
            CompletionKind::Keyword => 5,
            CompletionKind::Macro => 6,
            CompletionKind::Variable => 7,
        },
        CompletionContextKind::Macro => match kind {
            CompletionKind::Macro => 0,
            CompletionKind::BufferWord => 1,
            CompletionKind::Variable => 2,
            CompletionKind::Command => 3,
            CompletionKind::AddonCommand => 4,
            CompletionKind::Function => 5,
            CompletionKind::Keyword => 6,
            CompletionKind::ResultClass => 7,
        },
        CompletionContextKind::General => match kind {
            CompletionKind::Variable => 0,
            CompletionKind::BufferWord => 1,
            CompletionKind::Macro => 2,
            CompletionKind::Command => 3,
            CompletionKind::AddonCommand => 4,
            CompletionKind::Function => 5,
            CompletionKind::Keyword => 6,
            CompletionKind::ResultClass => 7,
        },
    }
}

fn filtered_words(
    words: &HashSet<String>,
    prefix: &str,
    kind: CompletionKind,
) -> Vec<CompletionCandidate> {
    let lowered = prefix.to_ascii_lowercase();
    let mut candidates: Vec<_> = words
        .iter()
        .filter(|word| word.to_ascii_lowercase().starts_with(&lowered))
        .map(|word| CompletionCandidate {
            text: word.clone(),
            kind,
        })
        .collect();
    candidates.sort_by(|left, right| left.text.cmp(&right.text));
    candidates
}

pub(crate) fn completion_pairs(
    line: &str,
    pos: usize,
    buffer_words: &HashSet<String>,
    cache: &CompletionCache,
) -> (usize, Vec<Pair>) {
    let context = completion_context(line, pos);
    if context.prefix.is_empty() {
        return (pos, Vec::new());
    }

    let mut candidates = static_candidates(&context.prefix);
    candidates.extend(filtered_words(
        buffer_words,
        &context.prefix,
        CompletionKind::BufferWord,
    ));

    match context.kind {
        CompletionContextKind::Macro => {
            let macro_words: HashSet<String> = cache.macros().iter().cloned().collect();
            candidates.extend(filtered_words(
                &macro_words,
                &context.prefix,
                CompletionKind::Macro,
            ));
        }
        CompletionContextKind::General => {
            let variable_words: HashSet<String> = cache.variables().iter().cloned().collect();
            let macro_words: HashSet<String> = cache.macros().iter().cloned().collect();
            candidates.extend(filtered_words(
                &variable_words,
                &context.prefix,
                CompletionKind::Variable,
            ));
            candidates.extend(filtered_words(
                &macro_words,
                &context.prefix,
                CompletionKind::Macro,
            ));
        }
        CompletionContextKind::Command | CompletionContextKind::Function => {}
    }

    candidates.sort_by(|left, right| {
        sort_priority(left.kind, context.kind)
            .cmp(&sort_priority(right.kind, context.kind))
            .then_with(|| left.text.cmp(&right.text))
    });

    (context.start, render_candidates(candidates))
}

pub(crate) fn completion_hint(
    line: &str,
    pos: usize,
    buffer_words: &HashSet<String>,
    cache: &CompletionCache,
) -> Option<String> {
    let context = completion_context(line, pos);
    if context.prefix.is_empty() {
        return None;
    }

    let (_start, pairs) = completion_pairs(line, pos, buffer_words, cache);
    let best = pairs.first()?;
    let replacement = &best.replacement;
    if replacement.len() <= context.prefix.len()
        || !replacement
            .to_ascii_lowercase()
            .starts_with(&context.prefix.to_ascii_lowercase())
    {
        return None;
    }

    Some(replacement[context.prefix.len()..].to_string())
}

pub(crate) fn update_cache_from_snapshot(
    cache: &mut CompletionCache,
    snapshot: &CompletionContextResult,
) {
    let mut variables = snapshot.variables.clone();
    variables.sort();
    variables.dedup_by(|left, right| left.eq_ignore_ascii_case(right));

    let mut macros = snapshot.macros.clone();
    macros.sort();
    macros.dedup_by(|left, right| left.eq_ignore_ascii_case(right));

    cache.update(variables, macros);
}

#[cfg(test)]
mod tests {
    use super::{completion_hint, completion_pairs, update_cache_from_snapshot};
    use crate::atom::completion_cache::CompletionCache;
    use crate::atom::json_contract::CompletionContextResult;
    use std::collections::HashSet;

    #[test]
    fn completion_pairs_prioritize_commands_at_line_start() {
        let words = HashSet::from(["displayed".to_string()]);
        let cache = CompletionCache::default();
        let (_start, pairs) = completion_pairs("disp", 4, &words, &cache);
        assert!(!pairs.is_empty());
        assert_eq!(pairs[0].replacement, "display");
    }

    #[test]
    fn completion_pairs_include_variables_in_general_context() {
        let words = HashSet::new();
        let mut cache = CompletionCache::default();
        update_cache_from_snapshot(
            &mut cache,
            &CompletionContextResult {
                status: "success".to_string(),
                variables: vec!["iq".to_string(), "income".to_string()],
                macros: vec!["sample_macro".to_string()],
                error: None,
            },
        );

        let (_start, pairs) = completion_pairs("summ i", 6, &words, &cache);
        assert!(pairs.iter().any(|pair| pair.replacement == "iq"));
        assert!(pairs.iter().any(|pair| pair.replacement == "income"));
    }

    #[test]
    fn completion_pairs_include_macros_in_macro_context() {
        let words = HashSet::new();
        let mut cache = CompletionCache::default();
        update_cache_from_snapshot(
            &mut cache,
            &CompletionContextResult {
                status: "success".to_string(),
                variables: vec!["iq".to_string()],
                macros: vec!["sample_macro".to_string()],
                error: None,
            },
        );

        let (start, pairs) = completion_pairs("display $sam", 12, &words, &cache);
        assert_eq!(start, 9);
        assert!(pairs.iter().any(|pair| pair.replacement == "sample_macro"));
    }

    #[test]
    fn completion_hint_returns_remaining_suffix_for_best_match() {
        let words = HashSet::new();
        let cache = CompletionCache::default();
        let hint = completion_hint("disp", 4, &words, &cache);
        assert_eq!(hint.as_deref(), Some("lay"));
    }
}
