#!/usr/bin/env python3
"""Interactive REPL orchestration for the local Python backend."""

from __future__ import annotations

import os
import re
import sys
from pathlib import Path

from prompt_toolkit import PromptSession, print_formatted_text
from prompt_toolkit.buffer import Buffer
from prompt_toolkit.formatted_text import FormattedText
from prompt_toolkit.history import FileHistory
from prompt_toolkit.key_binding import KeyBindings
from prompt_toolkit.lexers import Lexer
from prompt_toolkit.styles import Style

from ..atom.contracts import ExecutionResult
from ..molecule.selection_ops import run_selection_command

REPL_STYLE = Style.from_dict(
    {
        "prompt": "ansicyan bold",
        "command": "ansimagenta bold",
        "addon-command": "ansibrightmagenta bold",
        "keyword": "ansimagenta",
        "function": "ansiblue",
        "string": "ansigreen",
        "comment": "ansibrightblack italic",
        "macro": "ansiyellow",
        "macro-command": "ansiyellow bold",
        "factor": "ansibrightcyan bold",
        "builtin-variable": "ansibrightblue",
        "result-class": "ansibrightgreen",
        "number": "ansiblue",
        "operator": "ansired",
        "echo-prompt": "ansicyan bold",
        "warning": "ansiyellow bold",
        "note": "ansibrightcyan",
        "error": "ansired bold",
        "return-code": "ansired bold",
        "result-number": "ansibrightblue bold",
        "text": "",
    }
)

REPL_COMMANDS = {
    "about", "append", "areg", "assert", "by", "bysort", "capture", "cd", "clear", "collapse",
    "contract", "count", "decode", "describe", "display", "do", "drop", "egen", "encode",
    "estimates", "export", "forvalues", "foreach", "generate", "graph", "gsort", "if", "in",
    "input", "insheet", "import", "ivregress", "keep", "list", "local", "log", "logit", "merge",
    "net", "notes", "predict", "preserve", "probit", "reg", "quietly", "regress", "rename",
    "replace", "reshape", "restore", "save", "scalar", "set", "sort", "summ", "summarize",
    "tabulate", "use", "twoway", "which", "xtdescribe", "xtreg", "xtset", "xtsum",
}
REPL_ADDON_COMMANDS = {
    "boottest", "coefplot", "estout", "esttab", "gcollapse", "gcontract", "gegen", "gisid",
    "glevelsof", "gquantiles", "ivreghdfe", "ivreg2", "outreg", "outreg2", "ppmlhdfe", "reghdfe",
    "winsor2",
}
REPL_CONTROL_KEYWORDS = {"else", "forvalues", "foreach", "if", "in", "while"}
REPL_MACRO_COMMANDS = {"global", "local", "macro", "tempfile", "tempname", "tempvar"}
REPL_BUILTIN_FUNCTIONS = {
    "abs", "ceil", "cond", "exp", "floor", "length", "ln", "log", "lower", "max", "min",
    "missing", "mi", "real", "regexm", "regexr", "regexs", "round", "sqrt", "string", "strpos",
    "subinstr", "substr", "trim", "upper", "word", "wordcount",
}
REPL_RESULT_CLASSES = {"c", "e", "r", "s"}
REPL_BUILTIN_VARIABLES = {"_b", "_coef", "_cons", "_n", "_N", "_rc", "_se"}
REPL_OPERATORS = {
    "!", "!=", "#", "&", "&&", "(", ")", "*", "+", ",", "-", ".", "/", ":", ":=", "<", "<=",
    "=", "==", ">", ">=", "^", "|", "||", "~", "~=",
}
REPL_FACTOR_PREFIX_PATTERN = re.compile(r"(?:[ico]|ib|ibn|bn|b|o|n|[io]?b\d+|[io]?\d+)", re.IGNORECASE)
REPL_NUMBER_PATTERN = re.compile(r"[-+]?(?:\d+\.\d*|\.\d+|\d+)(?:[eE][-+]?\d+)?")
REPL_TOKEN_PATTERN = re.compile(
    r'(\s+|///.*|//.*|/\*.*?\*/|`"(?:[^"\n]|"")*"\'|"(?:[^"\n]|"")*"|`[^\'\n]+\'|\$[A-Za-z_][A-Za-z0-9_]*|>=|<=|==|!=|~=|:=|\|\||&&|[-+]?(?:\d+\.\d*|\.\d+|\d+)(?:[eE][-+]?\d+)?|\b[A-Za-z_][A-Za-z0-9_]*\b|[()\[\],:#=<>+\-*/.&|!^~])'
)
REPL_LOG_INFO_PATTERN = re.compile(r"^\s*(name:|log:|log type:|opened on:|closed on:)\s*", re.IGNORECASE)
REPL_RETURN_CODE_PATTERN = re.compile(r"^\s*r\((\d+)\);\s*$", re.IGNORECASE)
REPL_OUTPUT_NOTE_PATTERN = re.compile(r"^\s*note:", re.IGNORECASE)
REPL_OUTPUT_WARNING_PATTERN = re.compile(r"^\s*warning:", re.IGNORECASE)
REPL_OUTPUT_ERROR_PATTERN = re.compile(
    r"^\s*(?:error\b|invalid syntax\b|no observations\b|file .+ not found\b|type mismatch\b|conformability error\b|command .+ unrecognized\b|insufficient observations\b)",
    re.IGNORECASE,
)
REPL_INLINE_NUMBER_PATTERN = re.compile(r"(?<![\w.])[-+]?(?:\d+\.\d*|\.\d+|\d+)(?:[eE][-+]?\d+)?")


class StataReplLexer(Lexer):
    def lex_document(self, document):
        lines = document.lines

        def get_line(lineno):
            return _lex_stata_line(lines[lineno])

        return get_line


def _repl_history_path() -> Path:
    if os.name == "nt" and os.getenv("APPDATA"):
        base = Path(os.environ["APPDATA"]) / "stata-cli"
    else:
        base = Path.home() / ".stata-cli"
    base.mkdir(parents=True, exist_ok=True)
    return base / "repl_history.txt"


def _next_non_whitespace_token(tokens: list[str], start: int) -> str | None:
    for token in tokens[start:]:
        if token and not token.isspace():
            return token
    return None


def _is_number_token(token: str) -> bool:
    return bool(REPL_NUMBER_PATTERN.fullmatch(token))


def _is_factor_prefix(token: str, next_token: str | None) -> bool:
    return next_token == "." and bool(REPL_FACTOR_PREFIX_PATTERN.fullmatch(token))


def _style_repl_token(token: str, next_token: str | None) -> str:
    lower = token.lower()
    if token.isspace():
        return "class:text"
    if token.startswith("///") or token.startswith("//") or (token.startswith("/*") and token.endswith("*/")):
        return "class:comment"
    if (token.startswith('"') and token.endswith('"')) or (token.startswith('`"') and token.endswith('"\'')):
        return "class:string"
    if token.startswith("`") or token.startswith("$"):
        return "class:macro"
    if lower in REPL_MACRO_COMMANDS:
        return "class:macro-command"
    if lower in REPL_CONTROL_KEYWORDS:
        return "class:keyword"
    if lower in REPL_ADDON_COMMANDS:
        return "class:addon-command"
    if lower in REPL_COMMANDS:
        return "class:command"
    if lower in REPL_BUILTIN_VARIABLES:
        return "class:builtin-variable"
    if _is_factor_prefix(lower, next_token):
        return "class:factor"
    if lower in REPL_RESULT_CLASSES and next_token == "(":
        return "class:result-class"
    if lower in REPL_BUILTIN_FUNCTIONS and next_token == "(":
        return "class:function"
    if token in REPL_OPERATORS:
        return "class:operator"
    if _is_number_token(token):
        return "class:number"
    return "class:text"


def _lex_stata_line(line: str) -> list[tuple[str, str]]:
    stripped = line.lstrip()
    if stripped.startswith("*"):
        return [("class:comment", line)]
    matches = list(REPL_TOKEN_PATTERN.finditer(line))
    if not matches:
        return [("class:text", line)]
    tokens = [match.group(0) for match in matches]
    fragments: list[tuple[str, str]] = []
    cursor = 0
    for index, match in enumerate(matches):
        start, end = match.span()
        if start > cursor:
            fragments.append(("class:text", line[cursor:start]))
        token = match.group(0)
        next_token = _next_non_whitespace_token(tokens, index + 1)
        fragments.append((_style_repl_token(token, next_token), token))
        cursor = end
    if cursor < len(line):
        fragments.append(("class:text", line[cursor:]))
    return fragments


def _delete_before_cursor_if_possible(buffer: Buffer) -> None:
    if buffer.selection_state is not None:
        buffer.cut_selection()
        return
    if buffer.cursor_position > 0:
        buffer.delete_before_cursor(count=1)


def _delete_under_cursor_if_possible(buffer: Buffer) -> None:
    if buffer.selection_state is not None:
        buffer.cut_selection()
        return
    if buffer.cursor_position < len(buffer.text):
        buffer.delete(count=1)


def _move_cursor_left_if_possible(buffer: Buffer) -> None:
    if buffer.cursor_position > 0:
        buffer.cursor_position -= 1


def _move_cursor_to_start(buffer: Buffer) -> None:
    buffer.cursor_position = 0


def _create_repl_key_bindings() -> KeyBindings:
    bindings = KeyBindings()

    @bindings.add("backspace")
    @bindings.add("c-h")
    def _handle_backspace(event) -> None:
        _delete_before_cursor_if_possible(event.app.current_buffer)

    @bindings.add("delete")
    def _handle_delete(event) -> None:
        _delete_under_cursor_if_possible(event.app.current_buffer)

    @bindings.add("left")
    def _handle_left(event) -> None:
        _move_cursor_left_if_possible(event.app.current_buffer)

    @bindings.add("home")
    @bindings.add("c-a")
    def _handle_home(event) -> None:
        _move_cursor_to_start(event.app.current_buffer)

    return bindings


def _create_repl_session() -> PromptSession:
    return PromptSession(
        lexer=StataReplLexer(),
        style=REPL_STYLE,
        history=FileHistory(str(_repl_history_path())),
        key_bindings=_create_repl_key_bindings(),
    )


def _clear_repl_screen() -> None:
    if not sys.stdout.isatty():
        return
    sys.stdout.write("\033[2J\033[H")
    sys.stdout.flush()


def _sanitize_repl_output(text: str) -> str:
    if not text:
        return ""
    lines = text.replace("\r\n", "\n").replace("\r", "\n").split("\n")
    cleaned: list[str] = []
    pending_separator = False
    for line in lines:
        stripped = line.strip()
        if stripped == "-------------------------------------------------------------------------------":
            pending_separator = True
            continue
        if stripped.startswith("> _") and stripped.endswith(".log"):
            pending_separator = False
            continue
        if REPL_LOG_INFO_PATTERN.match(line):
            pending_separator = False
            continue
        if stripped.startswith(". quietly set seed ") or stripped.startswith(". capture log close"):
            pending_separator = False
            continue
        if stripped.startswith("> ") and cleaned and cleaned[-1].startswith(". "):
            cleaned[-1] = f"{cleaned[-1]} {stripped[2:].lstrip()}"
            continue
        if pending_separator and cleaned and stripped:
            cleaned.append("")
            pending_separator = False
        elif pending_separator:
            pending_separator = False
        cleaned.append(line)
    while cleaned and not cleaned[0].strip():
        cleaned.pop(0)
    while cleaned and not cleaned[-1].strip():
        cleaned.pop()
    collapsed: list[str] = []
    previous_blank = False
    for line in cleaned:
        is_blank = not line.strip()
        if is_blank and previous_blank:
            continue
        collapsed.append(line)
        previous_blank = is_blank
    return "\n".join(collapsed)


def _highlight_numbers_in_text(text: str, base_style: str = "class:text") -> list[tuple[str, str]]:
    fragments: list[tuple[str, str]] = []
    position = 0
    for match in REPL_INLINE_NUMBER_PATTERN.finditer(text):
        start, end = match.span()
        if start > position:
            fragments.append((base_style, text[position:start]))
        fragments.append(("class:result-number", text[start:end]))
        position = end
    if position < len(text):
        fragments.append((base_style, text[position:]))
    if not fragments:
        fragments.append((base_style, text))
    return fragments


def _format_repl_output(text: str) -> list[tuple[str, str]]:
    fragments: list[tuple[str, str]] = []
    for raw_line in text.splitlines(keepends=True):
        newline = "\n" if raw_line.endswith("\n") else ""
        line = raw_line[:-1] if newline else raw_line
        stripped = line.strip()
        if not stripped:
            fragments.append(("class:text", raw_line))
            continue
        return_code_match = REPL_RETURN_CODE_PATTERN.match(stripped)
        if line.startswith(". ") or line.startswith("> "):
            prompt, remainder = line[:2], line[2:]
            fragments.append(("class:echo-prompt", prompt))
            fragments.extend(_lex_stata_line(remainder))
        elif return_code_match:
            fragments.append(("class:return-code", line))
        elif REPL_OUTPUT_ERROR_PATTERN.match(stripped):
            fragments.append(("class:error", line))
        elif REPL_OUTPUT_WARNING_PATTERN.match(stripped):
            fragments.append(("class:warning", line))
        elif REPL_OUTPUT_NOTE_PATTERN.match(stripped):
            fragments.append(("class:note", line))
        else:
            fragments.extend(_highlight_numbers_in_text(line))
        if newline:
            fragments.append(("class:text", newline))
    return fragments


def print_repl_result(result: ExecutionResult) -> None:
    text = _sanitize_repl_output(result.output or result.error or "")
    if text:
        try:
            print_formatted_text(FormattedText(_format_repl_output(text)), style=REPL_STYLE)
        except Exception:
            print(text, end="" if text.endswith("\n") else "\n")


def repl_command(session_id: str | None, working_dir: str | None) -> int:
    session = _create_repl_session()
    _clear_repl_screen()
    while True:
        try:
            line = session.prompt([("class:prompt", ". ")])
        except EOFError:
            print()
            return 0
        except KeyboardInterrupt:
            print()
            continue

        stripped = line.strip()
        if not stripped:
            continue
        if stripped in {":exit", ":quit"}:
            return 0

        buffer = [line]
        while stripped.endswith("///"):
            try:
                continuation = session.prompt([("class:prompt", "> ")])
            except EOFError:
                print()
                return 0
            except KeyboardInterrupt:
                print()
                buffer = []
                break
            buffer.append(continuation)
            stripped = continuation.strip()

        if not buffer:
            continue

        result = run_selection_command("\n".join(buffer), session_id, working_dir)
        print_repl_result(result)
