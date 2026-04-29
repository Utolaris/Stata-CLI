#!/usr/bin/env python3
"""File execution helpers."""

from __future__ import annotations

import os

from ..atom.contracts import ExecutionResult, GraphArtifact
from ..atom.output_filter import process_output
from ..atom.partial_failure_parser import parse_partial_failures
from ..atom.pathing import get_log_file_path, resolve_do_file_path
from ..atom.runtime_state import get_runtime_state
from ..atom.session_identity import command_session_id, presented_session_id


def _graphs_from_result(result: dict) -> list[GraphArtifact]:
    graphs: list[GraphArtifact] = []
    extra = result.get("extra", {}) or {}
    for graph in extra.get("graphs", []) or []:
        graphs.append(GraphArtifact(**graph))
    return graphs


def run_file_command(
    file_path: str,
    session_id: str | None,
    working_dir: str | None,
) -> ExecutionResult:
    state = get_runtime_state()
    config = state.active_config()
    manager = state.active_session_manager()

    resolved_path, tried_paths = resolve_do_file_path(file_path)
    effective_path = resolved_path or os.path.abspath(file_path)
    if not resolved_path:
        tried_display = ", ".join(tried_paths) if tried_paths else effective_path
        return ExecutionResult(
            status="error",
            output="",
            session_id=presented_session_id(session_id, None, config),
            log_file=None,
            graphs=[],
            error=f"File not found: {file_path}. Tried these paths: {tried_display}",
        )

    base_name = os.path.splitext(os.path.basename(effective_path))[0]
    log_file = get_log_file_path(effective_path, base_name, session_id)
    os.makedirs(os.path.dirname(log_file), exist_ok=True)

    result = manager.execute_file(
        effective_path,
        session_id=command_session_id(session_id, config),
        log_file=log_file,
        working_dir=working_dir,
    )
    output = (result.get("output") or "").replace("\\n", "\n")
    partial_failures = parse_partial_failures(output)
    result_log_file = result.get("log_file") or log_file
    filtered = output if config.raw_output else process_output(
        output,
        result_display_mode=config.result_display_mode,
        max_output_tokens=config.max_output_tokens,
        log_path=log_file,
        filter_command_echo=True,
    )
    status = result.get("status", "error")
    error = result.get("error") or None
    if status == "error" and not error:
        error = filtered

    return ExecutionResult(
        status=status,
        output=filtered,
        session_id=presented_session_id(session_id, result.get("session_id"), config),
        log_file=result_log_file,
        graphs=_graphs_from_result(result),
        partial_failures=partial_failures,
        partial_failure_count=len(partial_failures),
        error=error,
    )
