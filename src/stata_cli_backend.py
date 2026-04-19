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
import os
import sys
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


def _mock_result_from_args(args: argparse.Namespace) -> ExecutionResult:
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
        return ExecutionResult(
            status="success",
            output=f"mock-file file={file_name} working_dir={working_dir} timeout={args.timeout}",
            session_id=session_id,
            log_file=f"/tmp/{os.path.splitext(file_name)[0]}.log",
            graphs=[
                GraphArtifact(
                    name="mock_graph",
                    path=f"/tmp/{os.path.splitext(file_name)[0]}.png",
                    format="png",
                )
            ],
            error=None,
        )

    return _render_error(f"Unsupported mock command: {args.command}", session_id=session_id)


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
            return _emit_json(result)
        _print_human_result(result)
        return 0 if result.status == "success" else 1

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
