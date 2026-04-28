#!/usr/bin/env python3
"""Line-oriented bridge between the Rust REPL and the Python runtime."""

from __future__ import annotations

import json
import os
import re
import sys
import time

from ..atom.contracts import CompletionContextResult, ExecutionResult
from ..atom.runtime_state import get_runtime_state
from ..atom.session_manager import SessionState
from ..molecule.selection_ops import render_error, run_selection_command


def _emit(result: ExecutionResult) -> None:
    sys.stdout.write(result.model_dump_json())
    sys.stdout.write("\n")
    sys.stdout.flush()


def _emit_completion(result: CompletionContextResult) -> None:
    sys.stdout.write(result.model_dump_json())
    sys.stdout.write("\n")
    sys.stdout.flush()


def _wait_for_bridge_session(session_id: str | None) -> bool:
    state = get_runtime_state()
    manager = state.active_session_manager()
    session = manager.get_session(session_id)
    if session is None:
        return False
    if session.state == SessionState.READY:
        return True
    if session.state != SessionState.CREATING:
        return False
    return bool(manager.wait_for_ready(session, timeout=1.0))


def _list_variables(session_id: str | None) -> list[str]:
    try:
        state = get_runtime_state()
        manager = state.active_session_manager()
        if not _wait_for_bridge_session(session_id):
            return []
        result = manager.get_data(session_id=session_id, max_rows=1, timeout=5.0)
    except Exception:
        return []
    if result.get("status") != "success":
        return []
    columns = result.get("columns") or []
    return [column for column in columns if isinstance(column, str) and column]


_MACRO_NAME_PATTERN = re.compile(r"^\s*([A-Za-z_][A-Za-z0-9_]*)\s*:")
_MACRO_SECTION_PATTERN = re.compile(r"^\s*(global|local)\s+macros", re.IGNORECASE)


def _parse_macro_names(output: str) -> list[str]:
    names: list[str] = []
    for raw_line in output.replace("\r\n", "\n").replace("\r", "\n").split("\n"):
        line = raw_line.strip()
        if not line or line.startswith(". ") or line.startswith("> "):
            continue
        if _MACRO_SECTION_PATTERN.match(line):
            continue
        match = _MACRO_NAME_PATTERN.match(raw_line)
        if match:
            names.append(match.group(1))
    return sorted(set(names))


def _list_macros(session_id: str | None) -> list[str]:
    try:
        state = get_runtime_state()
        manager = state.active_session_manager()
        if not _wait_for_bridge_session(session_id):
            return []
        result = manager.execute("macro dir", session_id=session_id, timeout=5.0)
    except Exception:
        return []
    if result.get("status") != "success":
        return []
    output = (result.get("output") or "").replace("\\n", "\n")
    return _parse_macro_names(output)


def _completion_snapshot(session_id: str | None) -> CompletionContextResult:
    return CompletionContextResult(
        status="success",
        variables=_list_variables(session_id),
        macros=_list_macros(session_id),
        error=None,
    )


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
        if command == "complete_context":
            _emit_completion(_completion_snapshot(session_id))
            continue
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
        if command == "complete_context":
            _emit_completion(
                CompletionContextResult(
                    status="success",
                    variables=["iq", "income", "kww"],
                    macros=["sample_macro", "stata_path"],
                    error=None,
                )
            )
            continue
        if command != "run":
            _emit(render_error(f"Unsupported bridge command: {command}", session_id=session_id))
            continue

        code = payload.get("code", "")
        request_working_dir = payload.get("working_dir")
        effective_working_dir = request_working_dir if isinstance(request_working_dir, str) else working_dir or ""
        sleep_ms = int(os.getenv("STATA_CLI_BRIDGE_TEST_SLEEP_MS", "0") or "0")
        if sleep_ms > 0:
            time.sleep(sleep_ms / 1000.0)
        output = f"mock-repl code={code} working_dir={effective_working_dir}"
        if code.strip() == "display 2+3":
            output = ". display 2+3\n5\n"
        status = "success"
        error = None
        if code.strip() == "force error":
            status = "error"
            output = ""
            error = "forced mock error"
        _emit(
            ExecutionResult(
                status=status,
                output=output,
                session_id=session_id or "default",
                log_file=None,
                graphs=[],
                error=error,
            )
        )

    return 0
