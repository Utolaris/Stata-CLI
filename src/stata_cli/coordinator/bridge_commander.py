#!/usr/bin/env python3
"""Line-oriented bridge between the Rust REPL and the Python runtime."""

from __future__ import annotations

import json
import sys

from ..atom.contracts import ExecutionResult
from ..molecule.selection_ops import render_error, run_selection_command


def _emit(result: ExecutionResult) -> None:
    sys.stdout.write(result.model_dump_json())
    sys.stdout.write("\n")
    sys.stdout.flush()


def bridge_command(session_id: str | None, working_dir: str | None) -> int:
    for raw_line in sys.stdin:
        message = raw_line.strip()
        if not message:
            continue

        try:
            payload = json.loads(message)
        except json.JSONDecodeError as exc:
            _emit(render_error(f"Invalid bridge request: {exc}", session_id=session_id))
            continue

        command = payload.get("command")
        if command == "quit":
            return 0
        if command != "run":
            _emit(render_error(f"Unsupported bridge command: {command}", session_id=session_id))
            continue

        code = payload.get("code")
        if not isinstance(code, str) or not code:
            _emit(render_error("Bridge run command requires a non-empty `code` string.", session_id=session_id))
            continue

        request_working_dir = payload.get("working_dir")
        timeout = payload.get("timeout")
        _emit(
            run_selection_command(
                code,
                session_id,
                request_working_dir if isinstance(request_working_dir, str) else working_dir,
                timeout if isinstance(timeout, int) else None,
            )
        )

    return 0


def mock_bridge_command(session_id: str | None, working_dir: str | None) -> int:
    for raw_line in sys.stdin:
        message = raw_line.strip()
        if not message:
            continue

        try:
            payload = json.loads(message)
        except json.JSONDecodeError as exc:
            _emit(render_error(f"Invalid bridge request: {exc}", session_id=session_id))
            continue

        command = payload.get("command")
        if command == "quit":
            return 0
        if command != "run":
            _emit(render_error(f"Unsupported bridge command: {command}", session_id=session_id))
            continue

        code = payload.get("code", "")
        request_working_dir = payload.get("working_dir")
        effective_working_dir = request_working_dir if isinstance(request_working_dir, str) else working_dir or ""
        output = f"mock-repl code={code} working_dir={effective_working_dir}"
        if code.strip() == "display 2+3":
            output = ". display 2+3\n5\n"
        _emit(
            ExecutionResult(
                status="success",
                output=output,
                session_id=session_id or "default",
                log_file=None,
                graphs=[],
                error=None,
            )
        )

    return 0
