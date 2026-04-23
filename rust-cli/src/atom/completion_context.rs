#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CompletionContextKind {
    Command,
    Function,
    Macro,
    General,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompletionContext {
    pub(crate) start: usize,
    pub(crate) prefix: String,
    pub(crate) kind: CompletionContextKind,
}

fn is_identifier_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '.'
}

pub(crate) fn completion_context(line: &str, pos: usize) -> CompletionContext {
    let safe_pos = pos.min(line.len());
    let prefix_slice = &line[..safe_pos];
    let mut start = safe_pos;

    for (index, ch) in prefix_slice.char_indices().rev() {
        if is_identifier_char(ch) {
            start = index;
            continue;
        }
        break;
    }

    let prefix = line[start..safe_pos].to_string();
    let before_prefix = &line[..start];
    let trimmed_before = before_prefix.trim_end();
    let previous_non_space = trimmed_before.chars().last();

    let kind = if matches!(previous_non_space, Some('$') | Some('`')) {
        CompletionContextKind::Macro
    } else if matches!(previous_non_space, Some('(')) {
        CompletionContextKind::Function
    } else if trimmed_before.is_empty() {
        CompletionContextKind::Command
    } else {
        let tokens_before = trimmed_before
            .split_whitespace()
            .filter(|token| !token.is_empty())
            .count();
        if tokens_before == 0 {
            CompletionContextKind::Command
        } else {
            CompletionContextKind::General
        }
    };

    CompletionContext {
        start,
        prefix,
        kind,
    }
}

#[cfg(test)]
mod tests {
    use super::{completion_context, CompletionContextKind};

    #[test]
    fn completion_context_detects_command_prefix() {
        let context = completion_context("disp", 4);
        assert_eq!(context.start, 0);
        assert_eq!(context.prefix, "disp");
        assert_eq!(context.kind, CompletionContextKind::Command);
    }

    #[test]
    fn completion_context_detects_function_and_macro_prefixes() {
        let function_line = "display missing(mi";
        let function_context = completion_context(function_line, function_line.len());
        assert_eq!(function_context.kind, CompletionContextKind::Function);
        assert_eq!(function_context.prefix, "mi");

        let macro_context = completion_context("display $S_T", 12);
        assert_eq!(macro_context.kind, CompletionContextKind::Macro);
        assert_eq!(macro_context.prefix, "S_T");
    }

    #[test]
    fn completion_context_detects_general_variable_position() {
        let context = completion_context("summ iq", 7);
        assert_eq!(context.kind, CompletionContextKind::General);
        assert_eq!(context.prefix, "iq");
    }
}
