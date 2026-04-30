#!/usr/bin/env python3
"""Selection execution helpers."""

from __future__ import annotations

from pathlib import Path

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
    blocked_gui_prefix = _blocked_gui_prefix(selection)
    if blocked_gui_prefix is not None:
        return render_error(
            "This command opens a Stata GUI dialog and is not suitable for CLI execution.",
            session_id=presented_session_id(session_id, runtime_session_id, config),
        )
    boot_error = _wait_for_booting_session(manager, runtime_session_id)
    if boot_error is not None:
        return render_error(boot_error, session_id=presented_session_id(session_id, runtime_session_id, config))
    help_guidance = _help_topic_guidance(selection)
    if help_guidance is not None:
        return ExecutionResult(
            status="success",
            output=help_guidance,
            session_id=presented_session_id(session_id, runtime_session_id, config),
            log_file=None,
            graphs=[],
            error=None,
        )
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


def _help_topic_guidance(selection: str) -> str | None:
    normalized = selection.strip()
    if not normalized:
        return None

    lines = [line.strip() for line in normalized.splitlines() if line.strip()]
    if len(lines) != 1:
        return None

    line = lines[0]
    if not line.lower().startswith("help"):
        return None

    parts = line.split(None, 1)
    topic = parts[1].strip() if len(parts) > 1 else ""
    if not topic:
        return None

    suggested_doc = _skill_doc_for_help_topic(topic)
    message = (
        f"`help {topic}` cannot be captured reliably from the local Stata terminal bridge. "
        "Read the local `skills/stata-cli/SKILL.md` reference library instead."
    )
    if suggested_doc is not None:
        message += f" Start with `{suggested_doc}`."
    return message


def _skill_doc_for_help_topic(topic: str) -> str | None:
    repo_root = Path(__file__).resolve().parents[3]
    normalized_topic = topic.strip().lower()
    package_aliases = {
        "esttab": "estout",
        "estout": "estout",
        "eststo": "estout",
        "estadd": "estout",
    }
    candidate_names = []
    if normalized_topic in package_aliases:
        candidate_names.append(("packages", package_aliases[normalized_topic]))
    candidate_names.append(("packages", normalized_topic))
    candidate_names.append(("references", normalized_topic))

    for folder, name in candidate_names:
        relative = Path("boilerplate") / "skills" / "stata-cli" / folder / f"{name}.md"
        if (repo_root / relative).exists():
            return (Path("skills") / "stata-cli" / folder / f"{name}.md").as_posix()
    return "skills/stata-cli/SKILL.md"


def _blocked_gui_prefix(selection: str) -> str | None:
    normalized = selection.strip()
    if not normalized:
        return None

    lines = [line.strip() for line in normalized.splitlines() if line.strip()]
    if len(lines) != 1:
        return None

    first_token = lines[0].split(None, 1)[0].lower()
    blocked = {"browse", "edit", "db", "dialog", "window", "shell", "winexec"}
    if first_token in blocked:
        return first_token
    return None
