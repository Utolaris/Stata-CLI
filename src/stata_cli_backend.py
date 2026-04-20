#!/usr/bin/env python3
# -*- coding: utf-8 -*-
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
import os
import sys
import tempfile
from pathlib import Path
from typing import Optional

import stata_mcp
import stata_mcp_server as legacy
from api_models import ExecutionResult, GraphArtifact
from session_manager import SessionManager

TEST_MODE_ENV = "STATA_CLI_BACKEND_TEST_MODE"


def _emit_json(result: ExecutionResult) -> int:
    sys.stdout.write(result.model_dump_json(indent=2))
    sys.stdout.write("\n")
    return 0 if result.status == "success" else 1


def _render_error(message: str, session_id: Optional[str] = None) -> ExecutionResult:
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


def _mock_result_from_args(args: argparse.Namespace) -> object:
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


def data_view_command(
    session_id: Optional[str],
    if_condition: Optional[str],
    max_rows: int,
    input_dta: Optional[str],
) -> dict:
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
        return result

    response = asyncio.run(
        legacy.view_data_endpoint(
            if_condition=if_condition,
            session_id=session_id,
            max_rows=max_rows,
        )
    )
    payload = json.loads(response.body.decode("utf-8"))
    if payload.get("status") == "error":
        return _session_error(payload.get("message", "Failed to get data"))
    payload["source_dta"] = input_dta
    return payload


def data_export_csv_command(
    output: str,
    input_dta: Optional[str],
    session_id: Optional[str],
    working_dir: Optional[str],
    replace: bool,
) -> dict:
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


def _graphs_from_extra(extra: Optional[dict]) -> list[GraphArtifact]:
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
    session_id: Optional[str],
    working_dir: Optional[str],
    timeout: Optional[int] = None,
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
    session_id: Optional[str],
    working_dir: Optional[str],
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


def repl_command(session_id: Optional[str], working_dir: Optional[str]) -> int:
    print("stata-cli repl")
    print("Type Stata code. Use :exit or :quit to leave.")

    while True:
        try:
            line = input("stata> ")
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

        result = run_selection_command(stripped, session_id, working_dir)
        _print_human_result(result)


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

    data_parser = subparsers.add_parser("data", help="Inspect the current dataset or export it")
    data_subparsers = data_parser.add_subparsers(dest="data_command", required=True)

    view_parser = data_subparsers.add_parser("view", help="View current data as structured rows")
    view_parser.add_argument("--session-id")
    view_parser.add_argument("--if-condition")
    view_parser.add_argument("--max-rows", type=int, default=1000)
    view_parser.add_argument("--input-dta")

    export_parser = data_subparsers.add_parser("export-csv", help="Export the current dataset or a .dta file to CSV")
    export_parser.add_argument("--output", required=True)
    export_parser.add_argument("--input-dta")
    export_parser.add_argument("--session-id")
    export_parser.add_argument("--working-dir")
    export_parser.add_argument("--replace", action="store_true")

    return parser


def main(argv: Optional[list[str]] = None) -> int:
    parser = build_parser()
    args = parser.parse_args(argv)

    if _is_test_mode():
        if args.command == "repl":
            print("stata-cli repl")
            print("Type Stata code. Use :exit or :quit to leave.")
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
