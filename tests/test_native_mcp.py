#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Tests for the native stdio MCP entrypoint.
"""

from __future__ import annotations

import sys
from pathlib import Path

import anyio
import pytest
from mcp import ClientSession
from mcp.client.stdio import StdioServerParameters, stdio_client

from stata_mcp import parse_runtime_config


PROJECT_ROOT = Path(__file__).resolve().parents[1]


def test_parse_runtime_config_defaults(monkeypatch, tmp_path):
    monkeypatch.delenv("STATA_PATH", raising=False)
    monkeypatch.delenv("STATA_EDITION", raising=False)
    monkeypatch.delenv("STATA_MCP_LOG_LEVEL", raising=False)
    monkeypatch.delenv("STATA_MCP_MULTI_SESSION", raising=False)

    config = parse_runtime_config(
        [
            "--log-file",
            str(tmp_path / "server.log"),
            "--stata-path",
            "/Applications/Stata",
        ]
    )

    assert config.stata_path == "/Applications/Stata"
    assert config.stata_edition == "mp"
    assert config.log_level == "INFO"
    assert config.multi_session is True
    assert config.max_sessions == 100
    assert config.session_timeout == 3600


def test_parse_runtime_config_file_and_cli_precedence(tmp_path):
    config_file = tmp_path / "stata-mcp.toml"
    config_file.write_text(
        """
stata_path = "/tmp/from-file"
stata_edition = "se"
log_level = "ERROR"
multi_session = false
max_sessions = 3
session_timeout = 12
""".strip(),
        encoding="utf-8",
    )

    config = parse_runtime_config(
        [
            "--config",
            str(config_file),
            "--stata-path",
            "/tmp/from-cli",
            "--multi-session",
            "--max-sessions",
            "8",
        ]
    )

    assert config.stata_path == "/tmp/from-cli"
    assert config.stata_edition == "se"
    assert config.log_level == "ERROR"
    assert config.multi_session is True
    assert config.max_sessions == 8
    assert config.session_timeout == 12


@pytest.mark.integration
@pytest.mark.asyncio
async def test_native_mcp_stdio_smoke(requires_stata, test_data_dir):
    server = StdioServerParameters(
        command="uv",
        args=[
            "run",
            "--directory",
            str(PROJECT_ROOT),
            "stata-mcp",
            "--log-level",
            "WARNING",
        ],
        env=None,
    )

    async with stdio_client(server, errlog=sys.stderr) as (read, write):
        async with ClientSession(read, write) as session:
            await session.initialize()

            tools = await session.list_tools()
            tool_names = {tool.name for tool in tools.tools}
            assert "stata_run_selection" in tool_names
            assert "stata_run_file" in tool_names
            assert "stata_list_sessions" in tool_names

            selection = await session.call_tool("stata_run_selection", {"selection": "display 1+1"})
            assert selection.structuredContent["status"] == "success"
            assert "2" in selection.structuredContent["output"]

            file_path = str(Path(test_data_dir) / "test_stata.do")
            run_file = await session.call_tool(
                "stata_run_file",
                {"file_path": file_path, "timeout": 60},
            )
            assert run_file.structuredContent["status"] == "success"
            assert run_file.structuredContent["graphs"]
