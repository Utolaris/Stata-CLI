#!/usr/bin/env python3
"""Small doctor/probe helpers for the packaged backend."""

from __future__ import annotations

from .selection_ops import run_selection_command


def backend_probe() -> dict[str, str]:
    result = run_selection_command("display 1+1", None, None, timeout=30)
    if result.status == "success":
        return {"status": "success", "message": "Backend successfully executed `display 1+1`."}
    return {"status": "error", "message": result.error or "Backend probe failed."}
