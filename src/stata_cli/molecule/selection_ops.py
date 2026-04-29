#!/usr/bin/env python3
"""Selection execution helpers."""

from __future__ import annotations

from ..atom.contracts import ExecutionResult
from ..atom.output_filter import process_output
from ..atom.pathing import build_selection_for_working_dir
from ..atom.runtime_state import get_runtime_state
from ..atom.session_identity import command_session_id, presented_session_id
from ..atom.session_manager import SessionManager, SessionState


def _wait_for_booting_session(
    manager: SessionManager,
    runtime_session_id: str | None,
) -> str | None:
    get_session = getattr(manager, "get_session", None)
    wait_for_ready = getattr(manager, "wait_for_ready", None)
    if not callable(get_session) or not callable(wait_for_ready):
        return None

    session = get_session(runtime_session_id)
    if session is None or session.state != SessionState.CREATING:
        return None

    worker_start_timeout = getattr(manager, "worker_start_timeout", 30)
    if wait_for_ready(session, timeout=float(worker_start_timeout)):
        return None

    error_message = session.error_message
    if isinstance(error_message, str) and error_message:
        return error_message
    return f"Session failed to become ready: {session.state.value}"


def run_selection_command(
    selection: str,
    session_id: str | None,
    working_dir: str | None,
) -> ExecutionResult:
    state = get_runtime_state()
    config = state.active_config()
    manager = state.active_session_manager()

    runtime_session_id = command_session_id(session_id, config)
    boot_error = _wait_for_booting_session(manager, runtime_session_id)
    if boot_error is not None:
        return render_error(boot_error, session_id=presented_session_id(session_id, runtime_session_id, config))
    code = build_selection_for_working_dir(selection, working_dir)
    result = manager.execute(
        code,
        session_id=runtime_session_id,
    )
    output = (result.get("output") or "").replace("\\n", "\n")
    filtered = output if config.raw_output else process_output(
        output,
        result_display_mode=config.result_display_mode,
        max_output_tokens=config.max_output_tokens,
        filter_command_echo=False,
    )
    status = result.get("status", "error")
    error = result.get("error") or None
    if status == "error" and not error:
        error = filtered

    return ExecutionResult(
        status=status,
        output=filtered,
        session_id=presented_session_id(session_id, result.get("session_id"), config),
        log_file=result.get("log_file") or None,
        graphs=[],
        error=error,
    )


def render_error(message: str, session_id: str | None = None) -> ExecutionResult:
    return ExecutionResult(
        status="error",
        output="",
        session_id=session_id,
        log_file=None,
        graphs=[],
        error=message,
    )


def default_presented_session(session_id: str | None) -> str:
    return session_id or SessionManager.DEFAULT_SESSION_ID
