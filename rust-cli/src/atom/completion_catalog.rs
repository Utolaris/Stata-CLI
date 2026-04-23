#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum CompletionKind {
    Command,
    AddonCommand,
    Function,
    Keyword,
    Macro,
    ResultClass,
    Variable,
    BufferWord,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CompletionCandidate {
    pub(crate) text: String,
    pub(crate) kind: CompletionKind,
}

const COMMANDS: &[&str] = &[
    "about",
    "append",
    "areg",
    "assert",
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
const ADDON_COMMANDS: &[&str] = &[
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
const FUNCTIONS: &[&str] = &[
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
const KEYWORDS: &[&str] = &["by", "else", "forvalues", "foreach", "if", "in", "while"];
const MACROS: &[&str] = &[
    "S_DATE", "S_FN", "S_LEVEL", "S_OS", "S_TIME", "S_ADO", "S_FLAVOR", "F1", "F2",
];
const RESULT_CLASSES: &[&str] = &["c", "e", "r", "s"];

fn filter_prefix(items: &[&str], prefix: &str, kind: CompletionKind) -> Vec<CompletionCandidate> {
    let lowered = prefix.to_ascii_lowercase();
    items
        .iter()
        .filter(|item| item.to_ascii_lowercase().starts_with(&lowered))
        .map(|item| CompletionCandidate {
            text: (*item).to_string(),
            kind,
        })
        .collect()
}

pub(crate) fn static_candidates(prefix: &str) -> Vec<CompletionCandidate> {
    let mut candidates = Vec::new();
    candidates.extend(filter_prefix(COMMANDS, prefix, CompletionKind::Command));
    candidates.extend(filter_prefix(
        ADDON_COMMANDS,
        prefix,
        CompletionKind::AddonCommand,
    ));
    candidates.extend(filter_prefix(FUNCTIONS, prefix, CompletionKind::Function));
    candidates.extend(filter_prefix(KEYWORDS, prefix, CompletionKind::Keyword));
    candidates.extend(filter_prefix(MACROS, prefix, CompletionKind::Macro));
    candidates.extend(filter_prefix(
        RESULT_CLASSES,
        prefix,
        CompletionKind::ResultClass,
    ));
    candidates
}

#[cfg(test)]
mod tests {
    use super::{static_candidates, CompletionKind};

    #[test]
    fn static_candidates_return_commands_functions_and_macros() {
        let candidates = static_candidates("di");
        assert!(candidates
            .iter()
            .any(|item| item.text == "display" && item.kind == CompletionKind::Command));

        let functions = static_candidates("mi");
        assert!(functions
            .iter()
            .any(|item| item.text == "missing" && item.kind == CompletionKind::Function));

        let macros = static_candidates("S_T");
        assert!(macros
            .iter()
            .any(|item| item.text == "S_TIME" && item.kind == CompletionKind::Macro));
    }
}
