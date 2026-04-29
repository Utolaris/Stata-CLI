#!/usr/bin/env python3
"""Compatibility shim for the packaged Python backend entrypoint."""

from stata_cli.atom.contracts import (  # noqa: F401
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
from stata_cli.molecule.data_ops import (  # noqa: F401
    data_export_csv_command,
    data_view_command,
)
from stata_cli.molecule.file_ops import run_file_command  # noqa: F401
from stata_cli.molecule.selection_ops import render_error as _render_error  # noqa: F401
from stata_cli.molecule.selection_ops import run_selection_command  # noqa: F401

__all__ = [
    "DEFAULT_DATA_VIEW_MAX_ROWS",
    "_render_error",
    "build_parser",
    "data_export_csv_command",
    "data_view_command",
    "emit_json_payload",
    "ExecutionResult",
    "GraphArtifact",
    "PartialFailure",
    "is_test_mode",
    "main",
    "mock_result_from_args",
    "run_file_command",
    "run_selection_command",
]


if __name__ == "__main__":
    raise SystemExit(main())
