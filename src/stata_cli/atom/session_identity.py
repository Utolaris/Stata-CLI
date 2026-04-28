#!/usr/bin/env python3
"""Session ID mapping helpers for backend command results."""

from __future__ import annotations

from typing import Protocol

DEFAULT_SESSION_ID = "default"


class SessionIdentityConfig(Protocol):
    multi_session: bool


def command_session_id(session_id: str | None, config: SessionIdentityConfig) -> str | None:
    """Map command-level session IDs to runtime session IDs."""
    if config.multi_session:
        return session_id
    return None


def presented_session_id(
    session_id: str | None,
    result_session_id: str | None,
    config: SessionIdentityConfig,
) -> str | None:
    """Return the session identifier that should be shown in command output."""
    if config.multi_session:
        return result_session_id or session_id
    return session_id or DEFAULT_SESSION_ID
