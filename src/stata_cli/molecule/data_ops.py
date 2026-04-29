#!/usr/bin/env python3
"""Data preview and CSV export helpers."""

from __future__ import annotations

import os
from typing import Any, cast

from ..atom.output_filter import process_output
from ..atom.pathing import build_selection_for_working_dir, resolve_path_for_working_dir
from ..atom.runtime_state import get_runtime_state
from ..atom.session_identity import command_session_id


def _session_error(message: str) -> dict[str, Any]:
    return {"status": "error", "message": message}


def data_view_command(
    session_id: str | None,
    if_condition: str | None,
    max_rows: int,
    input_dta: str | None,
) -> dict[str, Any]:
    state = get_runtime_state()
    config = state.active_config()
    manager = state.active_session_manager()
    runtime_session_id = command_session_id(session_id, config)

    max_rows = max(1, int(max_rows))
    if input_dta:
        input_path = os.path.abspath(os.path.expanduser(input_dta))
        if not os.path.exists(input_path):
            return _session_error(f"Input DTA file not found: {input_path}")
        load_code = build_selection_for_working_dir(
            f'use "{input_path.replace(chr(92), "/")}", clear',
            None,
        )
        load_result = manager.execute(load_code, session_id=runtime_session_id)
        if load_result.get("status") != "success":
            return _session_error(load_result.get("error", f"Failed to load DTA file: {input_path}"))

    result = manager.get_data(
        session_id=runtime_session_id,
        if_condition=if_condition,
        max_rows=max_rows,
    )
    if result.get("status") == "error":
        return _session_error(result.get("error", "Failed to get data"))
    result["status"] = "success"
    result["source_dta"] = os.path.abspath(os.path.expanduser(input_dta)) if input_dta else None
    return cast(dict[str, Any], result)


def data_export_csv_command(
    output: str,
    input_dta: str | None,
    session_id: str | None,
    working_dir: str | None,
    replace: bool,
) -> dict[str, Any]:
    state = get_runtime_state()
    config = state.active_config()
    manager = state.active_session_manager()
    runtime_session_id = command_session_id(session_id, config)

    output_path = resolve_path_for_working_dir(output, working_dir)
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
    code = build_selection_for_working_dir("\n".join(commands), working_dir)

    result = manager.execute(code, session_id=runtime_session_id)
    output_text = (result.get("output") or "").replace("\\n", "\n")
    filtered = output_text if config.raw_output else process_output(
        output_text,
        result_display_mode=config.result_display_mode,
        max_output_tokens=config.max_output_tokens,
        filter_command_echo=False,
    )
    status = result.get("status", "error")
    return {
        "status": status,
        "output": filtered,
        "output_csv": output_path,
        "session_id": result.get("session_id", session_id),
        "error": result.get("error") or (filtered if status == "error" else None),
    }
