#!/usr/bin/env python3
"""Path and file-resolution helpers for the local CLI backend."""

from __future__ import annotations

import logging
import os
import platform

from .session_manager import join_stata_line_continuations


def build_selection_for_working_dir(selection: str, working_dir: str | None) -> str:
    """Prepend a Stata `cd` command when a valid working directory is supplied."""
    processed = join_stata_line_continuations(selection)
    if working_dir and os.path.isdir(working_dir):
        wd = os.path.normpath(working_dir).replace("\\", "/")
        return f'cd "{wd}"\n{processed}'
    return processed


def get_log_file_path(do_file_path: str, do_file_base: str, session_id: str | None = None) -> str:
    """Return the execution log path for a `.do` file run."""
    do_file_dir = os.path.dirname(os.path.abspath(do_file_path))
    session_suffix = f"_{session_id}" if session_id else ""
    log_path = os.path.join(do_file_dir, f"{do_file_base}{session_suffix}_cli.log")
    return os.path.abspath(log_path)


def resolve_do_file_path(file_path: str) -> tuple[str | None, list[str]]:
    """Resolve a `.do` file path to an absolute location."""
    original_path = file_path
    normalized_path = os.path.normpath(file_path)

    if platform.system() == "Windows" and "/" in normalized_path:
        normalized_path = normalized_path.replace("/", "\\")
        logging.info("Converted path for Windows: %s", normalized_path)

    candidates: list[str] = []
    tried_paths: list[str] = []

    if not os.path.isabs(normalized_path):
        cwd = os.getcwd()
        candidates.extend(
            [
                normalized_path,
                os.path.join(cwd, normalized_path),
                os.path.join(cwd, os.path.basename(normalized_path)),
            ]
        )

        if platform.system() == "Windows":
            if "/" in original_path:
                win_path = original_path.replace("/", "\\")
                candidates.append(win_path)
                candidates.append(os.path.join(cwd, win_path))
            elif "\\" in original_path:
                unix_path = original_path.replace("\\", "/")
                candidates.append(unix_path)
                candidates.append(os.path.join(cwd, unix_path))

        for root, dirs, files in os.walk(cwd, topdown=True, followlinks=False):
            if os.path.basename(normalized_path) in files and root != cwd:
                candidates.append(os.path.join(root, os.path.basename(normalized_path)))
            if root.replace(cwd, "").count(os.sep) >= 2:
                dirs[:] = []
    else:
        candidates.append(normalized_path)

    seen: set[str] = set()
    unique_candidates: list[str] = []
    for candidate in candidates:
        normalized_candidate = os.path.normpath(candidate)
        if normalized_candidate not in seen:
            seen.add(normalized_candidate)
            unique_candidates.append(normalized_candidate)

    for candidate in unique_candidates:
        tried_paths.append(candidate)
        if os.path.isfile(candidate) and candidate.lower().endswith(".do"):
            return os.path.abspath(candidate), tried_paths

    return None, tried_paths

