#!/usr/bin/env python3
"""Compatibility shim for the packaged Python backend entrypoint."""

from stata_cli.atom.contracts import (  # noqa: F401
    ExecutionArtifact,
    ExecutionResult,
    GraphArtifact,
    PartialFailure,
)
from stata_cli.coordinator.command_commander import (  # noqa: F401
    DEFAULT_DATA_VIEW_MAX_ROWS,
    build_parser,
    emit_json_payload,
    is_test_mode,
    main,
    mock_result_from_args,
)
from stata_cli.coordinator.repl_commander import (  # noqa: F401
    REPL_STYLE,
    _delete_before_cursor_if_possible,
    _delete_under_cursor_if_possible,
    _format_repl_output,
    _lex_stata_line,
    _move_cursor_left_if_possible,
    _move_cursor_to_start,
    _sanitize_repl_output,
)
from stata_cli.coordinator.repl_commander import (
    print_repl_result as _print_repl_result,
)
from stata_cli.molecule.data_ops import (  # noqa: F401
    data_export_csv_command,
    data_view_command,
)
from stata_cli.molecule.file_ops import run_file_command  # noqa: F401
from stata_cli.molecule.selection_ops import render_error as _render_error  # noqa: F401
from stata_cli.molecule.selection_ops import run_selection_command  # noqa: F401
from stata_cli.molecule.workspace_ops import init_workspace_command  # noqa: F401

__all__ = [
    "DEFAULT_DATA_VIEW_MAX_ROWS",
    "REPL_STYLE",
    "_delete_before_cursor_if_possible",
    "_delete_under_cursor_if_possible",
    "_format_repl_output",
    "_lex_stata_line",
    "_move_cursor_left_if_possible",
    "_move_cursor_to_start",
    "_print_repl_result",
    "_render_error",
    "_sanitize_repl_output",
    "build_parser",
    "data_export_csv_command",
    "data_view_command",
    "emit_json_payload",
    "ExecutionArtifact",
    "ExecutionResult",
    "GraphArtifact",
    "PartialFailure",
    "init_workspace_command",
    "is_test_mode",
    "main",
    "mock_result_from_args",
    "run_file_command",
    "run_selection_command",
]


if __name__ == "__main__":
    raise SystemExit(main())
