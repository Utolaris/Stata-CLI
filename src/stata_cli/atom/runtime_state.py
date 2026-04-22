#!/usr/bin/env python3
"""Shared runtime state for the packaged Python backend."""

from __future__ import annotations

from dataclasses import dataclass

from .session_manager import SessionManager


@dataclass(slots=True)
class RuntimeConfig:
    stata_path: str | None
    stata_edition: str
    log_level: str
    result_display_mode: str
    max_output_tokens: int
    multi_session: bool
    max_sessions: int
    session_timeout: int


@dataclass(slots=True)
class RuntimeState:
    config: RuntimeConfig | None = None
    session_manager: SessionManager | None = None

    def active_config(self) -> RuntimeConfig:
        if self.config is None:
            raise RuntimeError("Runtime has not been initialized")
        return self.config

    def active_session_manager(self) -> SessionManager:
        if self.session_manager is None:
            raise RuntimeError("Runtime session manager is not available")
        return self.session_manager


_STATE = RuntimeState()


def get_runtime_state() -> RuntimeState:
    return _STATE

