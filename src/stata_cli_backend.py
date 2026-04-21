#!/usr/bin/env python3
"""
Local Python backend for the Rust stata-cli wrapper.

This module intentionally bypasses MCP and exposes a small JSON contract for:
- run: execute a snippet of Stata code
- file: execute a .do file
- repl: a minimal interactive loop that keeps one backend process alive
"""

from __future__ import annotations

import argparse
import asyncio
import json
import logging
import os
import re
import sys
import tempfile
from pathlib import Path
from typing import Any, cast

if os.getenv("STATA_CLI_REPL_MODE", "").strip().lower() in {"1", "true", "yes", "on"}:
    logging.basicConfig(level=logging.ERROR, force=True)

from prompt_toolkit import PromptSession, print_formatted_text
from prompt_toolkit.buffer import Buffer
from prompt_toolkit.formatted_text import FormattedText
from prompt_toolkit.history import FileHistory
from prompt_toolkit.key_binding import KeyBindings
from prompt_toolkit.lexers import Lexer
from prompt_toolkit.styles import Style

import stata_mcp
import stata_mcp_server as legacy
from api_models import ExecutionResult, GraphArtifact
from session_manager import SessionManager

TEST_MODE_ENV = "STATA_CLI_BACKEND_TEST_MODE"
DEFAULT_DATA_VIEW_MAX_ROWS = 50

INIT_DIRS = ["data", "do", "outputs", "scripts"]

INIT_FILES = {
    "AGENTS.md": """# AGENTS.md

- Prefer writing `.do` files instead of putting long Stata programs directly into the CLI.
- Keep main Stata analysis in `do/analysis.do`.
- Keep input datasets in `data/`.
- Keep derived text results, exported tables, and generated files in `outputs/`.
- Keep Python plotting or post-processing helpers in `scripts/`.
- Run analysis with `stata-cli file do/analysis.do`.
- Every `.do` file must include `capture log close` and `set more off`.
- Write full text Stata output to `outputs/result.txt`.
- Use CLI JSON only to inspect `status`, `error`, `log_file`, and `graphs`.
- If a run fails, read the JSON error plus `outputs/result.txt` or the log file, edit the `.do` file, and retry.
- Use `data view` only for variable names and small previews. Keep `max_rows` at 50 or less unless the user asks for more.
- Do not dump large datasets into chat context.
- Use Stata by default for cleaning, regression, and statistical tests.
- Use Python by default for final charts and save them into `outputs/`.
- Before using any third-party Stata command, run `which <command>` and ask the user before installing anything.
- Read the local `stata-cli` skill when you need Stata syntax help, package guidance, or idiomatic patterns.
""",
    "do/analysis.do": """capture log close
clear all
set more off

cap mkdir "outputs"
log using "outputs/result.txt", text replace

display "Run started: $S_DATE $S_TIME"
display "Working directory: `c(pwd)'"

* Load data here
* use "data/example.dta", clear

* Inspect the dataset
describe
summarize

* Main analysis
* regress y x1 x2

log close
""",
    "scripts/plot.py": """from pathlib import Path

import matplotlib.pyplot as plt
import pandas as pd
import seaborn as sns


BASE_DIR = Path(__file__).resolve().parents[1]
OUTPUTS_DIR = BASE_DIR / "outputs"
DATA_DIR = BASE_DIR / "data"


def main() -> None:
    source = OUTPUTS_DIR / "analysis.csv"
    if not source.exists():
        source = DATA_DIR / "analysis.csv"
    if not source.exists():
        raise FileNotFoundError(
            "Add a CSV file at outputs/analysis.csv or data/analysis.csv before plotting."
        )

    OUTPUTS_DIR.mkdir(parents=True, exist_ok=True)

    df = pd.read_csv(source)
    numeric_columns = df.select_dtypes(include="number").columns.tolist()
    if len(numeric_columns) < 2:
        raise ValueError("Need at least two numeric columns to build the template plot.")

    x_col, y_col = numeric_columns[:2]

    sns.set_theme(style="whitegrid")
    fig, ax = plt.subplots(figsize=(8, 5))
    sns.lineplot(data=df, x=x_col, y=y_col, marker="o", ax=ax)
    ax.set_title("Analysis Plot")
    ax.set_xlabel(x_col)
    ax.set_ylabel(y_col)

    fig.tight_layout()
    fig.savefig(OUTPUTS_DIR / "plot.png", dpi=200)


if __name__ == "__main__":
    main()
""",
}

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
}

REPL_ADDON_COMMANDS = {
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
}

REPL_CONTROL_KEYWORDS = {
    "else",
    "forvalues",
    "foreach",
    "if",
    "in",
    "while",
}

REPL_MACRO_COMMANDS = {
    "global",
    "local",
    "macro",
    "tempfile",
    "tempname",
    "tempvar",
}

REPL_BUILTIN_FUNCTIONS = {
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
}

REPL_RESULT_CLASSES = {"c", "e", "r", "s"}
REPL_BUILTIN_VARIABLES = {"_b", "_coef", "_cons", "_n", "_N", "_rc", "_se"}
REPL_OPERATORS = {
    "!",
    "!=",
    "#",
    "&",
    "&&",
    "(",
    ")",
    "*",
    "+",
    ",",
    "-",
    ".",
    "/",
    ":",
    ":=",
    "<",
    "<=",
    "=",
    "==",
    ">",
    ">=",
    "^",
    "|",
    "||",
    "~",
    "~=",
}
REPL_FACTOR_PREFIX_PATTERN = re.compile(r"(?:[ico]|ib|ibn|bn|b|o|n|[io]?b\d+|[io]?\d+)", re.IGNORECASE)
REPL_NUMBER_PATTERN = re.compile(r"[-+]?(?:\d+\.\d*|\.\d+|\d+)(?:[eE][-+]?\d+)?")
REPL_TOKEN_PATTERN = re.compile(
    r'(\s+|///.*|//.*|/\*.*?\*/|`"(?:[^"\n]|"")*"\'|"(?:[^"\n]|"")*"|`[^\'\n]+\'|\$[A-Za-z_][A-Za-z0-9_]*|>=|<=|==|!=|~=|:=|\|\||&&|[-+]?(?:\d+\.\d*|\.\d+|\d+)(?:[eE][-+]?\d+)?|\b[A-Za-z_][A-Za-z0-9_]*\b|[()\[\],:#=<>+\-*/.&|!^~])'
)
REPL_LOG_INFO_PATTERN = re.compile(
    r'^\s*(name:|log:|log type:|opened on:|closed on:)\s*',
    re.IGNORECASE,
)
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


def _emit_json(result: ExecutionResult) -> int:
    sys.stdout.write(result.model_dump_json(indent=2))
    sys.stdout.write("\n")
    return 0 if result.status == "success" else 1


def _render_error(message: str, session_id: str | None = None) -> ExecutionResult:
    return ExecutionResult(
        status="error",
        output="",
        session_id=session_id,
        log_file=None,
        graphs=[],
        error=message,
    )


def _is_test_mode() -> bool:
    return os.getenv(TEST_MODE_ENV, "").strip().lower() in {"1", "true", "yes", "on"}


def _mock_result_from_args(args: argparse.Namespace) -> ExecutionResult | dict[str, Any]:
    session_id = getattr(args, "session_id", None) or SessionManager.DEFAULT_SESSION_ID
    working_dir = getattr(args, "working_dir", None) or ""

    if args.command == "run":
        timeout = getattr(args, "timeout", None)
        return ExecutionResult(
            status="success",
            output=f"mock-run code={args.code} working_dir={working_dir} timeout={timeout}",
            session_id=session_id,
            log_file=None,
            graphs=[],
            error=None,
        )

    if args.command == "file":
        file_name = os.path.basename(args.file_path)
        temp_dir = tempfile.gettempdir()
        return ExecutionResult(
            status="success",
            output=f"mock-file file={file_name} working_dir={working_dir} timeout={args.timeout}",
            session_id=session_id,
            log_file=os.path.join(temp_dir, f"{os.path.splitext(file_name)[0]}.log"),
            graphs=[
                GraphArtifact(
                    name="mock_graph",
                    path=os.path.join(temp_dir, f"{os.path.splitext(file_name)[0]}.png"),
                    format="png",
                )
            ],
            error=None,
        )

    if args.command == "init":
        return init_workspace_command(args.target_dir)

    if args.command == "data":
        if args.data_command == "view":
            return {
                "status": "success",
                "columns": ["x", "y"],
                "dtypes": {"x": "float64", "y": "float64"},
                "rows": 2,
                "total_rows": 2,
                "displayed_rows": 2,
                "max_rows": args.max_rows,
                "index": [0, 1],
                "data": [[1, 2], [3, 4]],
                "source_dta": os.path.abspath(args.input_dta) if args.input_dta else None,
            }
        if args.data_command == "export-csv":
            output_path = os.path.abspath(args.output)
            Path(output_path).parent.mkdir(parents=True, exist_ok=True)
            Path(output_path).write_text("x,y\n1,2\n3,4\n", encoding="utf-8")
            return {
                "status": "success",
                "output": f"mock-export-csv output={output_path}",
                "output_csv": output_path,
                "session_id": session_id,
            }

    return _render_error(f"Unsupported mock command: {args.command}", session_id=session_id)


def _emit_json_payload(payload: object) -> int:
    if isinstance(payload, ExecutionResult):
        return _emit_json(payload)
    sys.stdout.write(f"{json.dumps(payload, indent=2)}\n")
    status = payload.get("status") if isinstance(payload, dict) else None
    return 0 if status in {"success", "running", "idle", "stop_sent", "stop_requested", "not_running"} else 1


def _print_human_payload(payload: object) -> None:
    if isinstance(payload, ExecutionResult):
        _print_human_result(payload)
        return
    print(json.dumps(payload, indent=2))


def _session_error(message: str) -> dict:
    return {"status": "error", "message": message}


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
    if (
        token.startswith("///")
        or token.startswith("//")
        or (token.startswith("/*") and token.endswith("*/"))
    ):
        return "class:comment"
    if (
        token.startswith('"') and token.endswith('"')
        or token.startswith('`"') and token.endswith('"\'')
    ):
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
        if stripped.startswith(". quietly set seed "):
            pending_separator = False
            continue
        if stripped.startswith(". capture log close"):
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


def _print_repl_result(result: ExecutionResult) -> None:
    text = _sanitize_repl_output(result.output or result.error or "")
    if text:
        try:
            print_formatted_text(FormattedText(_format_repl_output(text)), style=REPL_STYLE)
        except Exception:
            print(text, end="" if text.endswith("\n") else "\n")


def init_workspace_command(target_dir: str) -> dict:
    root = Path(target_dir).expanduser().resolve()
    planned_dirs = [root / relative for relative in INIT_DIRS]
    planned_files = [root / relative for relative in INIT_FILES]
    conflicts = [str(path) for path in planned_files if path.exists()]
    if conflicts:
        return {
            "status": "error",
            "message": "Refusing to overwrite existing scaffold files.",
            "target_dir": str(root),
            "conflicts": conflicts,
        }

    root.mkdir(parents=True, exist_ok=True)
    created: list[str] = []

    for directory in planned_dirs:
        directory.mkdir(parents=True, exist_ok=True)
        created.append(str(directory))

    for relative_path, content in INIT_FILES.items():
        path = root / relative_path
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
        created.append(str(path))

    return {
        "status": "success",
        "target_dir": str(root),
        "created": created,
        "message": f"Initialized AI-ready Stata workspace at {root}",
    }


def data_view_command(
    session_id: str | None,
    if_condition: str | None,
    max_rows: int,
    input_dta: str | None,
) -> dict[str, Any]:
    max_rows = max(1, int(max_rows))
    if input_dta:
        input_path = os.path.abspath(os.path.expanduser(input_dta))
        if not os.path.exists(input_path):
            return _session_error(f"Input DTA file not found: {input_path}")
        load_code = f'use "{input_path.replace(chr(92), "/")}", clear'
        if legacy.multi_session_enabled and legacy.session_manager is not None:
            load_result = legacy.session_manager.execute(
                stata_mcp._build_selection_for_working_dir(load_code, None),
                session_id=session_id,
            )
            if load_result.get("status") != "success":
                return _session_error(load_result.get("error", f"Failed to load DTA file: {input_path}"))
        else:
            load_output = legacy.run_stata_selection(load_code, None, False)
            filtered = legacy.process_mcp_output(
                load_output.replace("\\n", "\n"),
                for_mcp=True,
                filter_command_echo=False,
            )
            if filtered.lower().startswith("error:"):
                return _session_error(filtered)

    if legacy.multi_session_enabled and legacy.session_manager is not None:
        result = legacy.session_manager.get_data(
            session_id=session_id,
            if_condition=if_condition,
            max_rows=max_rows,
        )
        if result.get("status") == "error":
            return _session_error(result.get("error", "Failed to get data"))
        result["status"] = "success"
        result["source_dta"] = input_dta
        return cast(dict[str, Any], result)

    response = asyncio.run(
        legacy.view_data_endpoint(
            if_condition=if_condition,
            session_id=session_id,
            max_rows=max_rows,
        )
    )
    payload = cast(dict[str, Any], json.loads(response.body.decode("utf-8")))
    if payload.get("status") == "error":
        return _session_error(payload.get("message", "Failed to get data"))
    payload["source_dta"] = input_dta
    return payload


def data_export_csv_command(
    output: str,
    input_dta: str | None,
    session_id: str | None,
    working_dir: str | None,
    replace: bool,
) -> dict[str, Any]:
    output_path = os.path.abspath(os.path.expanduser(output))
    output_dir = os.path.dirname(output_path)
    if output_dir:
        os.makedirs(output_dir, exist_ok=True)
    if os.path.exists(output_path) and not replace:
        return _session_error(f"Output file already exists: {output_path}. Use --replace to overwrite it.")

    commands: list[str] = []
    if input_dta:
        input_path = os.path.abspath(os.path.expanduser(input_dta))
        if not os.path.exists(input_path):
            return _session_error(f"Input DTA file not found: {input_path}")
        commands.append(f'use "{input_path.replace(chr(92), "/")}", clear')
    commands.append(f'export delimited using "{output_path.replace(chr(92), "/")}", replace')
    code = "\n".join(commands)

    if legacy.multi_session_enabled and legacy.session_manager is not None:
        result = legacy.session_manager.execute(
            stata_mcp._build_selection_for_working_dir(code, working_dir),
            session_id=session_id,
        )
        output = result.get("output", "").replace("\\n", "\n")
        filtered = legacy.process_mcp_output(output, for_mcp=True, filter_command_echo=False)
        status = result.get("status", "error")
        return {
            "status": status,
            "output": filtered,
            "output_csv": output_path,
            "session_id": result.get("session_id", session_id),
            "error": result.get("error") or None,
        }

    output_text = legacy.run_stata_selection(code, working_dir, False)
    filtered = legacy.process_mcp_output(
        output_text.replace("\\n", "\n"),
        for_mcp=True,
        filter_command_echo=False,
    )
    status = "success" if not filtered.lower().startswith("error:") else "error"
    return {
        "status": status,
        "output": filtered,
        "output_csv": output_path,
        "session_id": session_id or SessionManager.DEFAULT_SESSION_ID,
        "error": filtered if status == "error" else None,
    }


def _graphs_from_extra(extra: dict | None) -> list[GraphArtifact]:
    graphs: list[GraphArtifact] = []
    if extra:
        for graph in extra.get("graphs", []) or []:
            graphs.append(GraphArtifact(**graph))
    return graphs


def _maybe_detect_single_session_graphs() -> list[GraphArtifact]:
    try:
        return [
            GraphArtifact(**graph)
            for graph in legacy.display_graphs_interactive(
                graph_format="png",
                width=800,
                height=600,
            )
        ]
    except Exception:
        return []


def run_selection_command(
    selection: str,
    session_id: str | None,
    working_dir: str | None,
    timeout: int | None = None,
) -> ExecutionResult:
    if legacy.multi_session_enabled and legacy.session_manager is not None:
        code = stata_mcp._build_selection_for_working_dir(selection, working_dir)
        result = legacy.session_manager.execute(
            code,
            session_id=session_id,
            timeout=float(timeout) if timeout else None,
        )
        output = result.get("output", "").replace("\\n", "\n")
        filtered = legacy.process_mcp_output(output, for_mcp=True, filter_command_echo=False)
        return ExecutionResult(
            status=result.get("status", "error"),
            output=filtered,
            session_id=result.get("session_id", session_id),
            log_file=result.get("log_file") or None,
            graphs=_graphs_from_extra(result.get("extra")),
            error=result.get("error") or None,
        )

    output = legacy.run_stata_selection(selection, working_dir, False)
    filtered = legacy.process_mcp_output(output.replace("\\n", "\n"), for_mcp=True, filter_command_echo=False)
    status = "success" if not filtered.lower().startswith("error:") else "error"
    return ExecutionResult(
        status=status,
        output=filtered,
        session_id=session_id or SessionManager.DEFAULT_SESSION_ID,
        log_file=None,
        graphs=_maybe_detect_single_session_graphs(),
        error=filtered if status == "error" else None,
    )


def run_file_command(
    file_path: str,
    timeout: int,
    session_id: str | None,
    working_dir: str | None,
) -> ExecutionResult:
    timeout = 600 if timeout <= 0 else int(timeout)
    resolved_path, tried_paths = legacy.resolve_do_file_path(file_path)
    effective_path = resolved_path or os.path.abspath(file_path)
    if not resolved_path:
        tried_display = ", ".join(tried_paths) if tried_paths else effective_path
        return _render_error(
            f"File not found: {file_path}. Tried these paths: {tried_display}",
            session_id=session_id,
        )

    base_name = os.path.splitext(os.path.basename(effective_path))[0]
    log_file = legacy.get_log_file_path(effective_path, base_name, session_id)
    os.makedirs(os.path.dirname(log_file), exist_ok=True)

    if legacy.multi_session_enabled and legacy.session_manager is not None:
        result = legacy.session_manager.execute_file(
            effective_path,
            session_id=session_id,
            timeout=float(timeout),
            log_file=log_file,
            working_dir=working_dir,
        )
        output = result.get("output", "").replace("\\n", "\n")
        filtered = legacy.process_mcp_output(output, for_mcp=True, filter_command_echo=True)
        return ExecutionResult(
            status=result.get("status", "error"),
            output=filtered,
            session_id=result.get("session_id", session_id),
            log_file=result.get("log_file") or log_file,
            graphs=_graphs_from_extra(result.get("extra")),
            error=result.get("error") or None,
        )

    output = legacy.run_stata_file(
        effective_path,
        timeout,
        False,
        working_dir,
    )
    filtered = legacy.process_mcp_output(output.replace("\\n", "\n"), for_mcp=True, filter_command_echo=True)
    status = "success" if not filtered.lower().startswith("error:") else "error"
    return ExecutionResult(
        status=status,
        output=filtered,
        session_id=session_id or SessionManager.DEFAULT_SESSION_ID,
        log_file=log_file,
        graphs=_maybe_detect_single_session_graphs(),
        error=filtered if status == "error" else None,
    )


def _print_human_result(result: ExecutionResult) -> None:
    if result.output:
        print(result.output)
    if result.graphs:
        print("\nGraphs:")
        for graph in result.graphs:
            print(f"- {graph.path}")
    if result.log_file:
        print(f"\nLog file: {result.log_file}")
    if result.error and not result.output:
        print(result.error, file=sys.stderr)


def repl_command(session_id: str | None, working_dir: str | None) -> int:
    session = _create_repl_session()

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
        _print_repl_result(result)


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Local Python backend for stata-cli")
    parser.add_argument("--stata-path")
    parser.add_argument("--stata-edition", choices=["mp", "se", "be"])
    parser.add_argument("--log-level", choices=["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"])
    parser.add_argument("--log-file")
    parser.add_argument("--result-display-mode", choices=["compact", "full"])
    parser.add_argument("--max-output-tokens", type=int)
    parser.add_argument("--multi-session", dest="multi_session", action="store_true")
    parser.add_argument("--no-multi-session", dest="multi_session", action="store_false")
    parser.add_argument("--max-sessions", type=int)
    parser.add_argument("--session-timeout", type=int)
    parser.add_argument("--json", action="store_true")
    parser.set_defaults(multi_session=None)

    subparsers = parser.add_subparsers(dest="command", required=True)

    run_parser = subparsers.add_parser("run", help="Execute a snippet of Stata code")
    run_parser.add_argument("--code", required=True)
    run_parser.add_argument("--session-id")
    run_parser.add_argument("--working-dir")
    run_parser.add_argument("--timeout", type=int)

    file_parser = subparsers.add_parser("file", help="Execute a .do file")
    file_parser.add_argument("file_path")
    file_parser.add_argument("--timeout", type=int, default=600)
    file_parser.add_argument("--session-id")
    file_parser.add_argument("--working-dir")

    repl_parser = subparsers.add_parser("repl", help="Start a minimal interactive shell")
    repl_parser.add_argument("--session-id")
    repl_parser.add_argument("--working-dir")

    init_parser = subparsers.add_parser("init", help="Create an AI-ready Stata workspace scaffold")
    init_parser.add_argument("target_dir")

    data_parser = subparsers.add_parser("data", help="Inspect the current dataset or export it")
    data_subparsers = data_parser.add_subparsers(dest="data_command", required=True)

    view_parser = data_subparsers.add_parser("view", help="View current data as structured rows")
    view_parser.add_argument("--session-id")
    view_parser.add_argument("--if-condition")
    view_parser.add_argument("--max-rows", type=int, default=DEFAULT_DATA_VIEW_MAX_ROWS)
    view_parser.add_argument("--input-dta")

    export_parser = data_subparsers.add_parser("export-csv", help="Export the current dataset or a .dta file to CSV")
    export_parser.add_argument("--output", required=True)
    export_parser.add_argument("--input-dta")
    export_parser.add_argument("--session-id")
    export_parser.add_argument("--working-dir")
    export_parser.add_argument("--replace", action="store_true")

    return parser


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    if _is_test_mode():
        if args.command == "repl":
            return 0
        result = _mock_result_from_args(args)
        if args.json:
            return _emit_json_payload(result)
        _print_human_payload(result)
        if isinstance(result, ExecutionResult):
            return 0 if result.status == "success" else 1
        return 0 if result.get("status") != "error" else 1

    config_args: list[str] = []
    for name in (
        "stata_path",
        "stata_edition",
        "log_level",
        "log_file",
        "result_display_mode",
        "max_output_tokens",
        "max_sessions",
        "session_timeout",
    ):
        value = getattr(args, name)
        if value is not None:
            config_args.extend([f"--{name.replace('_', '-')}", str(value)])
    if args.multi_session is True:
        config_args.append("--multi-session")
    elif args.multi_session is False:
        config_args.append("--no-multi-session")

    config = stata_mcp.parse_runtime_config(config_args)

    try:
        if args.command == "init":
            result = init_workspace_command(args.target_dir)
            if args.json:
                return _emit_json_payload(result)
            _print_human_payload(result)
            return 0 if result.get("status") != "error" else 1

        stata_mcp.initialize_runtime(config)

        if args.command == "run":
            result = run_selection_command(args.code, args.session_id, args.working_dir, args.timeout)
            if args.json:
                return _emit_json(result)
            _print_human_result(result)
            return 0 if result.status == "success" else 1

        if args.command == "file":
            result = run_file_command(args.file_path, args.timeout, args.session_id, args.working_dir)
            if args.json:
                return _emit_json(result)
            _print_human_result(result)
            return 0 if result.status == "success" else 1

        if args.command == "repl":
            return repl_command(args.session_id, args.working_dir)

        if args.command == "data":
            if args.data_command == "view":
                result = data_view_command(args.session_id, args.if_condition, args.max_rows, args.input_dta)
            elif args.data_command == "export-csv":
                result = data_export_csv_command(
                    args.output,
                    args.input_dta,
                    args.session_id,
                    args.working_dir,
                    args.replace,
                )
            else:
                result = _session_error(f"Unknown data command: {args.data_command}")

            if args.json:
                return _emit_json_payload(result)
            _print_human_payload(result)
            return 0 if result.get("status") != "error" else 1

        return _emit_json(_render_error(f"Unknown command: {args.command}"))
    except Exception as exc:
        error_result = _render_error(str(exc))
        if args.json:
            return _emit_json(error_result)
        print(str(exc), file=sys.stderr)
        return 1
    finally:
        stata_mcp._shutdown_runtime()


if __name__ == "__main__":
    raise SystemExit(main())
