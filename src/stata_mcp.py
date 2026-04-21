#!/usr/bin/env python3
"""
Native stdio MCP entrypoint for stata-mcp.

This module keeps the proven Stata execution/runtime logic from
``stata_mcp_server.py`` but exposes it through a native stdio MCP server so
Codex and other MCP clients can launch it directly without a VS Code host or
an HTTP-to-MCP proxy.
"""

from __future__ import annotations

import argparse
import asyncio
import json
import logging
import os
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    import tomllib
except ModuleNotFoundError:  # pragma: no cover - Python 3.10 fallback
    import tomli as tomllib

from mcp.server.fastmcp import Context, FastMCP

import stata_mcp_server as legacy
from api_models import (
    ExecutionResult,
    GraphArtifact,
    SessionDetailsResult,
    SessionListResult,
)
from session_manager import SessionManager
from utils import default_stata_install_dir

DEFAULT_CONFIG_FILES = (
    ".stata-mcp.toml",
    ".stata-mcp.json",
)


@dataclass
class RuntimeConfig:
    stata_path: str | None
    stata_edition: str
    log_level: str
    log_file: str
    result_display_mode: str
    max_output_tokens: int
    multi_session: bool
    max_sessions: int
    session_timeout: int


def _parse_bool(value: Any, default: bool) -> bool:
    if value is None:
        return default
    if isinstance(value, bool):
        return value
    text = str(value).strip().lower()
    if text in {"1", "true", "yes", "on"}:
        return True
    if text in {"0", "false", "no", "off"}:
        return False
    return default


def _load_config_file(path: str | None) -> dict[str, Any]:
    if not path:
        return {}
    config_path = Path(path).expanduser()
    if not config_path.exists():
        raise FileNotFoundError(f"Config file not found: {config_path}")

    raw = config_path.read_text(encoding="utf-8")
    if config_path.suffix == ".json":
        data = json.loads(raw)
    else:
        data = tomllib.loads(raw)

    if not isinstance(data, dict):
        raise ValueError(f"Config file must contain a top-level object: {config_path}")
    return data


def _discover_config_file(explicit_path: str | None) -> str | None:
    if explicit_path:
        return explicit_path
    for candidate in DEFAULT_CONFIG_FILES:
        if Path(candidate).exists():
            return candidate
    return None


def _detect_default_stata_path() -> str:
    if sys.platform == "darwin":
        return default_stata_install_dir("Darwin")
    if sys.platform.startswith("win"):
        return default_stata_install_dir("Windows")
    return default_stata_install_dir("Linux")


def parse_runtime_config(argv: list[str] | None = None) -> RuntimeConfig:
    parser = argparse.ArgumentParser(description="Run the standalone stata-mcp stdio server")
    parser.add_argument("--config", help="Optional TOML or JSON config file")
    parser.add_argument("--stata-path", help="Path to the Stata installation directory")
    parser.add_argument("--stata-edition", choices=["mp", "se", "be"], help="Stata edition")
    parser.add_argument("--log-level", choices=["DEBUG", "INFO", "WARNING", "ERROR", "CRITICAL"], help="Logging level")
    parser.add_argument("--log-file", help="Path to the server log file")
    parser.add_argument("--result-display-mode", choices=["compact", "full"], help="Output filtering mode")
    parser.add_argument("--max-output-tokens", type=int, help="Maximum output tokens (0 disables truncation)")
    parser.add_argument("--multi-session", dest="multi_session", action="store_true", help="Enable multi-session mode")
    parser.add_argument("--no-multi-session", dest="multi_session", action="store_false", help="Disable multi-session mode")
    parser.add_argument("--max-sessions", type=int, help="Maximum number of concurrent sessions")
    parser.add_argument("--session-timeout", type=int, help="Idle timeout for sessions in seconds")
    parser.set_defaults(multi_session=None)
    args = parser.parse_args(argv)

    config_file = _discover_config_file(args.config)
    file_config = _load_config_file(config_file)

    env = os.environ

    def pick(name: str, cli_value: Any, env_name: str, default: Any) -> Any:
        if cli_value is not None:
            return cli_value
        if env_name in env and env[env_name] != "":
            return env[env_name]
        return file_config.get(name, default)

    log_file_default = os.path.join(tempfile.gettempdir(), "stata_mcp_stdio.log")
    return RuntimeConfig(
        stata_path=pick("stata_path", args.stata_path, "STATA_PATH", _detect_default_stata_path()),
        stata_edition=str(pick("stata_edition", args.stata_edition, "STATA_EDITION", "mp")).lower(),
        log_level=str(pick("log_level", args.log_level, "STATA_MCP_LOG_LEVEL", "INFO")).upper(),
        log_file=os.path.abspath(os.path.expanduser(str(pick("log_file", args.log_file, "STATA_MCP_LOG_FILE", log_file_default)))),
        result_display_mode=str(pick("result_display_mode", args.result_display_mode, "STATA_MCP_RESULT_DISPLAY_MODE", "compact")),
        max_output_tokens=int(pick("max_output_tokens", args.max_output_tokens, "STATA_MCP_MAX_OUTPUT_TOKENS", 10000)),
        multi_session=_parse_bool(pick("multi_session", args.multi_session, "STATA_MCP_MULTI_SESSION", True), True),
        max_sessions=int(pick("max_sessions", args.max_sessions, "STATA_MCP_MAX_SESSIONS", 100)),
        session_timeout=int(pick("session_timeout", args.session_timeout, "STATA_MCP_SESSION_TIMEOUT", 3600)),
    )


def configure_runtime_logging(config: RuntimeConfig) -> None:
    log_dir = os.path.dirname(config.log_file)
    if log_dir:
        os.makedirs(log_dir, exist_ok=True)

    root = logging.getLogger()
    for handler in list(root.handlers):
        root.removeHandler(handler)

    root.setLevel(getattr(logging, config.log_level))
    formatter = logging.Formatter("%(asctime)s - %(name)s - %(levelname)s - %(message)s")

    file_handler = logging.FileHandler(config.log_file, mode="a", encoding="utf-8")
    file_handler.setFormatter(formatter)
    root.addHandler(file_handler)

    stderr_handler = logging.StreamHandler(sys.stderr)
    stderr_handler.setLevel(max(getattr(logging, config.log_level), logging.WARNING))
    stderr_handler.setFormatter(logging.Formatter("%(levelname)s: %(message)s"))
    root.addHandler(stderr_handler)

    legacy.console_handler = stderr_handler
    logging.getLogger("uvicorn.access").setLevel(logging.WARNING)
    logging.getLogger("uvicorn.error").setLevel(logging.WARNING)


def initialize_runtime(config: RuntimeConfig) -> None:
    configure_runtime_logging(config)

    legacy.result_display_mode = config.result_display_mode
    legacy.max_output_tokens = config.max_output_tokens
    legacy.stata_edition = config.stata_edition
    legacy.log_file_location = "extension"
    legacy.custom_log_directory = ""
    legacy.workspace_root = os.getcwd()
    legacy.extension_path = os.getcwd()
    legacy.multi_session_enabled = config.multi_session
    legacy.multi_session_max_sessions = config.max_sessions
    legacy.multi_session_timeout = config.session_timeout

    stata_path = os.path.abspath(os.path.expanduser(config.stata_path or _detect_default_stata_path()))
    legacy.STATA_PATH = stata_path

    if not os.path.exists(stata_path):
        raise FileNotFoundError(f"Stata path does not exist: {stata_path}")

    if legacy.multi_session_enabled:
        graphs_dir = os.path.join(os.getcwd(), ".stata-mcp", "graphs")
        os.makedirs(graphs_dir, exist_ok=True)
        legacy.session_manager = SessionManager(
            stata_path=stata_path,
            stata_edition=config.stata_edition,
            max_sessions=config.max_sessions,
            session_timeout=config.session_timeout,
            enabled=True,
            graphs_dir=graphs_dir,
        )
        if not legacy.session_manager.start():
            raise RuntimeError("Failed to start session manager")
        legacy.stata_available = True
        legacy.has_stata = True
    else:
        if not legacy.try_init_stata(stata_path):
            raise RuntimeError(f"Failed to initialize Stata from {stata_path}")


def _graphs_from_extra(extra: dict[str, Any] | None) -> list[GraphArtifact]:
    graphs = []
    if extra:
        for graph in extra.get("graphs", []) or []:
            graphs.append(GraphArtifact(**graph))
    return graphs


def _structured_result(
    *,
    status: str,
    output: str = "",
    session_id: str | None = None,
    log_file: str | None = None,
    error: str | None = None,
    graphs: list[GraphArtifact] | None = None,
) -> ExecutionResult:
    return ExecutionResult(
        status=status,
        output=output,
        session_id=session_id,
        log_file=log_file,
        error=error,
        graphs=graphs or [],
    )


def _build_selection_for_working_dir(selection: str, working_dir: str | None) -> str:
    processed = legacy.join_stata_line_continuations(selection)
    if working_dir and os.path.isdir(working_dir):
        wd = os.path.normpath(working_dir).replace("\\", "/")
        return f'cd "{wd}"\n{processed}'
    return processed


async def _notify_progress(ctx: Context | None, *, progress: float, total: float | None, message: str) -> None:
    if not ctx:
        return
    ctx.info(message)
    if total is not None:
        ctx.report_progress(progress, total=total, message=message)


async def _run_file_with_progress(
    *,
    file_path: str,
    timeout: int,
    session_id: str | None,
    working_dir: str | None,
    ctx: Context | None,
) -> ExecutionResult:
    resolved_path, tried_paths = legacy.resolve_do_file_path(file_path)
    effective_path = resolved_path or os.path.abspath(file_path)
    if not resolved_path:
        tried_display = ", ".join(tried_paths) if tried_paths else effective_path
        return _structured_result(
            status="error",
            error=f"File not found: {file_path}. Tried these paths: {tried_display}",
        )

    base_name = os.path.splitext(os.path.basename(effective_path))[0]
    log_file = legacy.get_log_file_path(effective_path, base_name, session_id)

    os.makedirs(os.path.dirname(log_file), exist_ok=True)
    with open(log_file, "w", encoding="utf-8"):
        pass

    async def invoke() -> tuple[Any, list[GraphArtifact]]:
        if legacy.multi_session_enabled and legacy.session_manager is not None:
            result_dict = await asyncio.to_thread(
                legacy.session_manager.execute_file,
                effective_path,
                session_id=session_id,
                timeout=float(timeout),
                log_file=log_file,
                working_dir=working_dir,
            )
            return result_dict, _graphs_from_extra(result_dict.get("extra"))

        output = await asyncio.to_thread(
            legacy.run_stata_file,
            effective_path,
            timeout,
            False,
            working_dir,
        )
        graphs = []
        try:
            graphs = [GraphArtifact(**graph) for graph in legacy.display_graphs_interactive(graph_format="png", width=800, height=600)]
        except Exception:
            graphs = []
        return output, graphs

    task = asyncio.create_task(invoke())
    start = asyncio.get_running_loop().time()
    last_offset = 0
    last_emit = 0.0
    interval = 5.0

    await _notify_progress(ctx, progress=0.0, total=float(timeout), message=f"Starting {os.path.basename(effective_path)}")

    while not task.done():
        await asyncio.sleep(1.0)
        now = asyncio.get_running_loop().time()
        elapsed = now - start

        if now - last_emit < interval:
            continue

        message = f"{os.path.basename(effective_path)} running for {elapsed:.0f}s"
        if os.path.exists(log_file):
            try:
                with open(log_file, encoding="utf-8", errors="replace") as handle:
                    handle.seek(last_offset)
                    new_content = handle.read()
                    last_offset = handle.tell()
                lines = [line for line in new_content.splitlines() if line.strip()]
                if lines:
                    message += "\n" + "\n".join(lines[-3:])
            except Exception as exc:  # pragma: no cover - defensive
                logging.debug("Unable to read progress log %s: %s", log_file, exc)

        await _notify_progress(ctx, progress=min(elapsed, float(timeout)), total=float(timeout), message=message)
        last_emit = now

    raw_result, graphs = await task

    if isinstance(raw_result, dict):
        status = raw_result.get("status", "error")
        output = raw_result.get("output", "")
        error = raw_result.get("error") or None
        session = raw_result.get("session_id", session_id)
        result_log = raw_result.get("log_file") or log_file
        if output:
            output = legacy.process_mcp_output(output.replace("\\n", "\n"), for_mcp=True, filter_command_echo=True)
        return _structured_result(
            status=status,
            output=output,
            session_id=session,
            log_file=result_log,
            error=error,
            graphs=graphs,
        )

    output = str(raw_result or "")
    filtered = legacy.process_mcp_output(output.replace("\\n", "\n"), for_mcp=True, filter_command_echo=True)
    return _structured_result(
        status="success" if not filtered.lower().startswith("error:") else "error",
        output=filtered,
        session_id=session_id or SessionManager.DEFAULT_SESSION_ID,
        log_file=log_file,
        error=filtered if filtered.lower().startswith("error:") else None,
        graphs=graphs,
    )


def build_mcp_server() -> FastMCP:
    server = FastMCP(
        name="stata-mcp",
        instructions=(
            "Run Stata code and .do files through a native MCP server. "
            "Use explicit session_id values for parallel work when needed."
        ),
        log_level="INFO",
    )

    @server.tool(name="stata_run_selection", description="Execute Stata code and return structured output.", structured_output=True)
    async def stata_run_selection(
        selection: str,
        session_id: str | None = None,
        working_dir: str | None = None,
        ctx: Context | None = None,
    ) -> ExecutionResult:
        await _notify_progress(ctx, progress=0.0, total=None, message="Running Stata selection")

        if legacy.multi_session_enabled and legacy.session_manager is not None:
            code = _build_selection_for_working_dir(selection, working_dir)
            result = await asyncio.to_thread(
                legacy.session_manager.execute,
                code,
                session_id=session_id,
            )
            output = result.get("output", "").replace("\\n", "\n")
            filtered = legacy.process_mcp_output(output, for_mcp=True, filter_command_echo=False)
            return _structured_result(
                status=result.get("status", "error"),
                output=filtered,
                session_id=result.get("session_id", session_id),
                log_file=result.get("log_file") or None,
                error=result.get("error") or None,
                graphs=_graphs_from_extra(result.get("extra")),
            )

        output = await asyncio.to_thread(legacy.run_stata_selection, selection, working_dir, False)
        filtered = legacy.process_mcp_output(output.replace("\\n", "\n"), for_mcp=True, filter_command_echo=False)
        status = "success" if not filtered.lower().startswith("error:") else "error"
        return _structured_result(
            status=status,
            output=filtered,
            session_id=session_id or SessionManager.DEFAULT_SESSION_ID,
            error=filtered if status == "error" else None,
        )

    @server.tool(name="stata_run_file", description="Execute a Stata .do file and return structured output.", structured_output=True)
    async def stata_run_file(
        file_path: str,
        timeout: int = 600,
        session_id: str | None = None,
        working_dir: str | None = None,
        ctx: Context | None = None,
    ) -> ExecutionResult:
        timeout = 600 if timeout <= 0 else int(timeout)
        return await _run_file_with_progress(
            file_path=file_path,
            timeout=timeout,
            session_id=session_id,
            working_dir=working_dir,
            ctx=ctx,
        )

    @server.tool(name="stata_list_sessions", description="List all active Stata sessions.", structured_output=True)
    async def stata_list_sessions() -> SessionListResult:
        if not legacy.multi_session_enabled or legacy.session_manager is None:
            return SessionListResult(status="error", sessions=[], error="Multi-session mode is not enabled")
        stats = legacy.session_manager.get_stats()
        return SessionListResult(
            status="success",
            sessions=legacy.session_manager.list_sessions(),
            max_sessions=stats.get("max_sessions"),
            available_slots=stats.get("available_slots"),
        )

    @server.tool(name="stata_create_session", description="Create a new session for parallel execution.", structured_output=True)
    async def stata_create_session(session_id: str | None = None) -> dict[str, Any]:
        if not legacy.multi_session_enabled or legacy.session_manager is None:
            return {"status": "error", "message": "Multi-session mode is not enabled"}
        return legacy.session_manager.create_session(session_id)

    @server.tool(name="stata_get_session", description="Inspect a specific Stata session.", structured_output=True)
    async def stata_get_session(session_id: str) -> SessionDetailsResult:
        if not legacy.multi_session_enabled or legacy.session_manager is None:
            return SessionDetailsResult(status="error", error="Multi-session mode is not enabled")
        session = legacy.session_manager.get_session(session_id)
        if not session:
            return SessionDetailsResult(status="error", error=f"Session not found: {session_id}")
        return SessionDetailsResult(status="success", session=session.to_dict())

    @server.tool(name="stata_destroy_session", description="Destroy a non-default Stata session.", structured_output=True)
    async def stata_destroy_session(session_id: str) -> dict[str, Any]:
        if not legacy.multi_session_enabled or legacy.session_manager is None:
            return {"status": "error", "message": "Multi-session mode is not enabled"}
        success, error = legacy.session_manager.destroy_session(session_id)
        return {"status": "success" if success else "error", "message": error or f"Session {session_id} destroyed"}

    @server.tool(name="stata_stop_execution", description="Stop the currently running Stata execution.", structured_output=True)
    async def stata_stop_execution(session_id: str | None = None) -> dict[str, Any]:
        return await legacy.stop_execution(session_id=session_id)

    @server.tool(name="stata_execution_status", description="Get the current execution status.", structured_output=True)
    async def stata_execution_status() -> dict[str, Any]:
        return await legacy.get_execution_status()

    @server.tool(name="stata_restart_session", description="Restart the default Stata session.", structured_output=True)
    async def stata_restart_session() -> dict[str, Any]:
        return await legacy.restart_session()

    return server


def _shutdown_runtime() -> None:
    if legacy.session_manager is not None:
        try:
            legacy.session_manager.stop()
        except Exception as exc:  # pragma: no cover - shutdown safety
            logging.debug("Error while stopping session manager: %s", exc)


def main(argv: list[str] | None = None) -> None:
    config = parse_runtime_config(argv)
    initialize_runtime(config)
    server = build_mcp_server()
    try:
        server.run(transport="stdio")
    finally:
        _shutdown_runtime()


if __name__ == "__main__":
    main()
