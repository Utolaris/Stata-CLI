#!/usr/bin/env python3
"""Runtime initialization and teardown for the packaged backend."""

from __future__ import annotations

import logging
import os
import sys

from ..atom.platform_stata import default_stata_install_dir
from ..atom.runtime_state import RuntimeConfig, get_runtime_state
from ..atom.session_manager import SessionManager


def build_runtime_config(args) -> RuntimeConfig:
    """Build runtime configuration from parsed CLI args."""
    stata_path = args.stata_path or os.environ.get("STATA_PATH") or default_stata_install_dir()
    max_sessions = int(args.max_sessions if args.max_sessions is not None else 100)
    multi_session = args.multi_session is not False
    if not multi_session:
        max_sessions = 1

    return RuntimeConfig(
        stata_path=stata_path,
        stata_edition=(args.stata_edition or "mp").lower(),
        log_level=(args.log_level or "WARNING").upper(),
        result_display_mode=args.result_display_mode or "compact",
        max_output_tokens=int(args.max_output_tokens if args.max_output_tokens is not None else 10000),
        raw_output=bool(getattr(args, "raw_output", False)),
        multi_session=multi_session,
        max_sessions=max_sessions,
        session_timeout=int(args.session_timeout if args.session_timeout is not None else 3600),
    )


def configure_runtime_logging(config: RuntimeConfig) -> None:
    """Configure logging for backend commands and REPL runs."""
    level = getattr(logging, config.log_level, logging.WARNING)
    root = logging.getLogger()
    for handler in list(root.handlers):
        root.removeHandler(handler)
    logging.basicConfig(
        level=level,
        format="%(asctime)s - %(name)s - %(levelname)s - %(message)s",
        stream=sys.stderr,
    )


def initialize_runtime(config: RuntimeConfig, *, lazy_default_session: bool = False) -> None:
    """Initialize the shared session manager for backend commands."""
    configure_runtime_logging(config)
    if config.stata_path and not os.path.exists(config.stata_path):
        raise FileNotFoundError(f"Stata path does not exist: {config.stata_path}")

    state = get_runtime_state()
    shutdown_runtime()

    manager = SessionManager(
        stata_path=os.path.abspath(config.stata_path or default_stata_install_dir()),
        stata_edition=config.stata_edition,
        max_sessions=config.max_sessions,
        session_timeout=config.session_timeout,
        enabled=True,
        graphs_dir=None,
    )
    if not manager.start(wait_for_default_ready=not lazy_default_session):
        raise RuntimeError("Failed to start the backend session manager")

    state.config = config
    state.session_manager = manager


def shutdown_runtime() -> None:
    """Stop and clear the shared backend session manager."""
    state = get_runtime_state()
    if state.session_manager is not None:
        state.session_manager.stop()
    state.session_manager = None
    state.config = None


def command_session_id(session_id: str | None, config: RuntimeConfig | None = None) -> str | None:
    """Map command-level session IDs to runtime session IDs."""
    active_config = config or get_runtime_state().active_config()
    if active_config.multi_session:
        return session_id
    return None


def presented_session_id(
    session_id: str | None,
    result_session_id: str | None,
    config: RuntimeConfig | None = None,
) -> str | None:
    """Return the session identifier that should be shown in command output."""
    active_config = config or get_runtime_state().active_config()
    if active_config.multi_session:
        return result_session_id or session_id
    return session_id or SessionManager.DEFAULT_SESSION_ID
