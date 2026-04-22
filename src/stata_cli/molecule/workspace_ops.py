#!/usr/bin/env python3
"""Workspace scaffolding compatibility helpers."""

from __future__ import annotations

import shutil
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
BOILERPLATE_DIR = REPO_ROOT / "boilerplate"


def init_workspace_command(target_dir: str) -> dict:
    root = Path(target_dir).expanduser().resolve()
    if not BOILERPLATE_DIR.is_dir():
        return {
            "status": "error",
            "message": f"Boilerplate directory not found: {BOILERPLATE_DIR}",
            "target_dir": str(root),
        }

    root.mkdir(parents=True, exist_ok=True)
    created: list[str] = []
    for source in sorted(BOILERPLATE_DIR.rglob("*")):
        relative = source.relative_to(BOILERPLATE_DIR)
        destination = root / relative
        if source.is_dir():
            destination.mkdir(parents=True, exist_ok=True)
            created.append(str(destination))
            continue
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, destination)
        created.append(str(destination))

    return {
        "status": "success",
        "target_dir": str(root),
        "source_dir": str(BOILERPLATE_DIR),
        "created": created,
        "message": f"Copied boilerplate from {BOILERPLATE_DIR} into {root}",
    }
