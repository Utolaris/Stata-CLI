"""Track files created or modified by do-file execution."""

from __future__ import annotations

import os
from dataclasses import dataclass

from .contracts import ExecutionArtifact


@dataclass(frozen=True)
class FileSnapshot:
    """Minimal file metadata used to detect generated artifacts."""

    size_bytes: int
    mtime_ns: int


def artifact_scan_roots(do_file_path: str, working_dir: str | None) -> list[str]:
    """Return directories where do-file side effects are expected."""
    if working_dir:
        return [os.path.abspath(os.path.expanduser(working_dir))]

    do_dir = os.path.dirname(os.path.abspath(do_file_path))
    roots = [do_dir]
    parent = os.path.dirname(do_dir)
    if os.path.basename(do_dir).lower() == "do" and parent and parent != do_dir:
        roots.append(parent)
    return roots


def snapshot_files(roots: list[str]) -> dict[str, FileSnapshot]:
    """Snapshot files under root using metadata cheap enough for CLI use."""
    snapshot: dict[str, FileSnapshot] = {}
    for root in roots:
        if not os.path.isdir(root):
            continue
        for dirpath, dirnames, filenames in os.walk(root):
            dirnames[:] = [name for name in dirnames if name not in {".git", ".venv", "__pycache__"}]
            for filename in filenames:
                path = os.path.abspath(os.path.join(dirpath, filename))
                try:
                    stat = os.stat(path)
                except OSError:
                    continue
                snapshot[path] = FileSnapshot(size_bytes=stat.st_size, mtime_ns=stat.st_mtime_ns)
    return snapshot


def diff_artifacts(
    before: dict[str, FileSnapshot],
    after: dict[str, FileSnapshot],
    roots: list[str],
    exclude_paths: set[str],
) -> list[ExecutionArtifact]:
    """Return files that were created or changed during execution."""
    normalized_excludes = {os.path.abspath(path) for path in exclude_paths if path}
    artifacts: list[ExecutionArtifact] = []
    for path, metadata in sorted(after.items()):
        if path in normalized_excludes:
            continue
        previous = before.get(path)
        if previous == metadata:
            continue
        relative_path = _relative_to_nearest_root(path, roots)
        artifacts.append(
            ExecutionArtifact(
                path=path,
                relative_path=relative_path,
                size_bytes=metadata.size_bytes,
            )
        )
    return artifacts


def _relative_to_nearest_root(path: str, roots: list[str]) -> str | None:
    candidates: list[tuple[int, str]] = []
    for root in roots:
        try:
            relative = os.path.relpath(path, root)
        except ValueError:
            continue
        if relative == os.pardir or relative.startswith(f"{os.pardir}{os.sep}"):
            continue
        candidates.append((len(relative), relative.replace(os.sep, "/")))
    if not candidates:
        return None
    return sorted(candidates)[0][1]
