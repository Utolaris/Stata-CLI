#!/usr/bin/env python3
"""Central command dispatch for the packaged Python backend."""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
import time
from pathlib import Path
from typing import Any

from ..atom.contracts import ExecutionResult, PartialFailure
from ..atom.pathing import resolve_path_for_working_dir
from ..coordinator.bridge_commander import bridge_command, mock_bridge_command
from ..coordinator.runtime_commander import (
    build_runtime_config,
    initialize_runtime,
    shutdown_runtime,
)
from ..molecule.data_ops import data_export_csv_command, data_view_command
from ..molecule.file_ops import run_file_command
from ..molecule.selection_ops import default_presented_session, render_error, run_selection_command

TEST_MODE_ENV = "STATA_CLI_BACKEND_TEST_MODE"
DEFAULT_DATA_VIEW_MAX_ROWS = 50


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Local Python backend for stata-cli")
    parser.add_argument("--stata-path")
    parser.add_argument("--stata-edition", choices=["mp", "se", "be"])
    parser.add_argument("--log-level", choices=["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"])
    parser.add_argument("--result-display-mode", choices=["compact", "full"])
    parser.add_argument("--max-output-tokens", type=int)
    parser.add_argument("--multi-session", dest="multi_session", action="store_true", help=argparse.SUPPRESS)
    parser.add_argument("--no-multi-session", dest="multi_session", action="store_false", help=argparse.SUPPRESS)
    parser.add_argument("--max-sessions", type=int, help=argparse.SUPPRESS)
    parser.add_argument("--session-timeout", type=int, help=argparse.SUPPRESS)
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--raw-output", action="store_true", help=argparse.SUPPRESS)
    parser.set_defaults(multi_session=None)

    subparsers = parser.add_subparsers(dest="command", required=True)

    run_parser = subparsers.add_parser("run", help="Execute a snippet of Stata code")
    run_parser.add_argument("--code", required=True)
    run_parser.add_argument("--session-id", help=argparse.SUPPRESS)
    run_parser.add_argument("--working-dir")

    file_parser = subparsers.add_parser("file", help="Execute a .do file")
    file_parser.add_argument("file_path")
    file_parser.add_argument("--session-id", help=argparse.SUPPRESS)
    file_parser.add_argument("--working-dir")

    bridge_parser = subparsers.add_parser("bridge", help=argparse.SUPPRESS)
    bridge_parser.add_argument("--session-id", help=argparse.SUPPRESS)
    bridge_parser.add_argument("--working-dir")

    data_parser = subparsers.add_parser("data", help="Inspect or export an explicit .dta file")
    data_subparsers = data_parser.add_subparsers(dest="data_command", required=True)

    view_parser = data_subparsers.add_parser("view", help="View an explicit .dta file as structured rows")
    view_parser.add_argument("--session-id", help=argparse.SUPPRESS)
    view_parser.add_argument("--if-condition")
    view_parser.add_argument("--max-rows", type=int, default=DEFAULT_DATA_VIEW_MAX_ROWS)
    view_parser.add_argument("--input-dta", required=True)

    export_parser = data_subparsers.add_parser("export-csv", help="Export an explicit .dta file to CSV")
    export_parser.add_argument("--output", required=True)
    export_parser.add_argument("--input-dta", required=True)
    export_parser.add_argument("--session-id", help=argparse.SUPPRESS)
    export_parser.add_argument("--working-dir")
    export_parser.add_argument("--replace", action="store_true")

    return parser


def _emit_json(result: ExecutionResult) -> int:
    sys.stdout.write(result.model_dump_json(indent=2, ensure_ascii=True))
    sys.stdout.write("\n")
    return 0 if result.status == "success" else 1


def emit_json_payload(payload: object) -> int:
    if isinstance(payload, ExecutionResult):
        return _emit_json(payload)
    sys.stdout.write(f"{json.dumps(payload, indent=2)}\n")
    status = payload.get("status") if isinstance(payload, dict) else None
    return 0 if status in {"success", "running", "idle", "stop_sent", "stop_requested", "not_running"} else 1


def print_human_payload(payload: object) -> None:
    if isinstance(payload, ExecutionResult):
        print(payload.model_dump_json(indent=2, ensure_ascii=True))
        return
    print(json.dumps(payload, indent=2))


def payload_exit_code(payload: object) -> int:
    if isinstance(payload, ExecutionResult):
        return 0 if payload.status == "success" else 1
    if isinstance(payload, dict):
        return 0 if payload.get("status") in {"success", "running", "idle", "stop_sent", "stop_requested", "not_running"} else 1
    return 1


def is_test_mode() -> bool:
    return os.getenv(TEST_MODE_ENV, "").strip().lower() in {"1", "true", "yes", "on"}


def _contract_echo_from_args(args: argparse.Namespace) -> dict[str, Any]:
    return {
        "command": args.command,
        "data_command": getattr(args, "data_command", None),
        "file_path": getattr(args, "file_path", None),
        "code": getattr(args, "code", None),
        "session_id": getattr(args, "session_id", None),
        "working_dir": getattr(args, "working_dir", None),
        "stata_path": args.stata_path,
        "stata_edition": args.stata_edition,
        "log_level": args.log_level,
        "result_display_mode": args.result_display_mode,
        "max_output_tokens": args.max_output_tokens,
        "multi_session": args.multi_session,
        "max_sessions": args.max_sessions,
        "session_timeout": args.session_timeout,
        "json": args.json,
        "raw_output": args.raw_output,
        "if_condition": getattr(args, "if_condition", None),
        "max_rows": getattr(args, "max_rows", None),
        "input_dta": getattr(args, "input_dta", None),
        "output": getattr(args, "output", None),
        "replace": getattr(args, "replace", None),
    }


def _should_echo_contract_args() -> bool:
    return os.getenv("STATA_CLI_BACKEND_TEST_ECHO_ARGS", "").strip().lower() in {
        "1",
        "true",
        "yes",
        "on",
    }


def mock_result_from_args(args: argparse.Namespace) -> ExecutionResult | dict[str, Any]:
    sleep_ms = int(os.getenv("STATA_CLI_BACKEND_TEST_SLEEP_MS", "0") or "0")
    if sleep_ms > 0:
        time.sleep(sleep_ms / 1000.0)

    session_id = getattr(args, "session_id", None)
    presented_session_id = default_presented_session(session_id)
    working_dir = getattr(args, "working_dir", None) or ""
    if _should_echo_contract_args():
        echoed = json.dumps(_contract_echo_from_args(args), sort_keys=True)
        if args.command in {"run", "file"}:
            return ExecutionResult(
                status="success",
                output=echoed,
                session_id=presented_session_id,
                log_file=None,
                graphs=[],
                error=None,
            )
        return {"status": "success", "contract": json.loads(echoed)}

    if args.command == "run":
        return ExecutionResult(
            status="success",
            output=f"mock-run code={args.code} working_dir={working_dir}",
            session_id=presented_session_id,
            log_file=None,
            graphs=[],
            error=None,
        )

    if args.command == "file":
        file_name = os.path.basename(args.file_path)
        temp_dir = tempfile.gettempdir()
        partial_failures = []
        if os.getenv("STATA_CLI_BACKEND_TEST_PARTIAL_FAILURE", "").strip():
            partial_failures.append(
                PartialFailure(
                    line=2,
                    command="capture noisily esttab ols using st_reg.rtf, replace",
                    return_code="r(199)",
                    message="command esttab is unrecognized",
                )
            )
        return ExecutionResult(
            status="success",
            output=f"mock-file file={file_name} working_dir={working_dir}",
            session_id=presented_session_id,
            log_file=os.path.join(temp_dir, f"{os.path.splitext(file_name)[0]}.log"),
            graphs=[],
            partial_failures=partial_failures,
            partial_failure_count=len(partial_failures),
            error=None,
        )

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
            output_path = resolve_path_for_working_dir(args.output, getattr(args, "working_dir", None))
            Path(output_path).parent.mkdir(parents=True, exist_ok=True)
            Path(output_path).write_text("x,y\n1,2\n3,4\n", encoding="utf-8")
            return {
                "status": "success",
                "output": f"mock-export-csv output={output_path}",
                "output_csv": output_path,
                "session_id": presented_session_id,
            }

    return render_error(f"Unsupported mock command: {args.command}", session_id=presented_session_id)


def main(argv: list[str] | None = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    if is_test_mode():
        if args.command == "bridge":
            return mock_bridge_command(getattr(args, "session_id", None), getattr(args, "working_dir", None))
        mock_payload = mock_result_from_args(args)
        if args.json:
            return emit_json_payload(mock_payload)
        print_human_payload(mock_payload)
        return payload_exit_code(mock_payload)

    runtime_config = build_runtime_config(args)
    try:
        payload: object
        initialize_runtime(runtime_config, lazy_default_session=args.command == "bridge")

        if args.command == "run":
            payload = run_selection_command(args.code, args.session_id, args.working_dir)
        elif args.command == "file":
            payload = run_file_command(args.file_path, args.session_id, args.working_dir)
        elif args.command == "bridge":
            return bridge_command(args.session_id, args.working_dir)
        elif args.command == "data":
            if args.data_command == "view":
                payload = data_view_command(args.session_id, args.if_condition, args.max_rows, args.input_dta)
            elif args.data_command == "export-csv":
                payload = data_export_csv_command(
                    args.output,
                    args.input_dta,
                    args.session_id,
                    args.working_dir,
                    args.replace,
                )
            else:
                payload = {"status": "error", "message": f"Unknown data command: {args.data_command}"}
        else:
            payload = render_error(f"Unknown command: {args.command}")

        if args.json:
            return emit_json_payload(payload)
        print_human_payload(payload)
        return payload_exit_code(payload)
    except RuntimeError as exc:
        error_result = render_error(str(exc))
        if args.json:
            return _emit_json(error_result)
        print(str(exc), file=sys.stderr)
        return 1
    finally:
        shutdown_runtime()
