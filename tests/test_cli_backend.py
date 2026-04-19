#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""
Unit tests for the local Python backend used by the Rust CLI.
"""

from api_models import GraphArtifact
import stata_cli_backend as backend


def test_run_selection_command_single_session(monkeypatch):
    monkeypatch.setattr(backend.legacy, "multi_session_enabled", False)
    monkeypatch.setattr(backend.legacy, "session_manager", None)
    monkeypatch.setattr(backend.legacy, "run_stata_selection", lambda selection, working_dir, auto_detect: "result line")
    monkeypatch.setattr(backend.legacy, "process_mcp_output", lambda output, **kwargs: output)
    monkeypatch.setattr(
        backend,
        "_maybe_detect_single_session_graphs",
        lambda: [GraphArtifact(name="g1", path="/tmp/g1.png", format=None)],
    )

    result = backend.run_selection_command("display 1+1", None, None)

    assert result.status == "success"
    assert result.output == "result line"
    assert result.session_id == "default"
    assert result.graphs[0].path == "/tmp/g1.png"


def test_run_file_command_multi_session(monkeypatch):
    class DummyManager:
        def execute_file(self, *args, **kwargs):
            return {
                "status": "success",
                "output": "file output",
                "session_id": "abc",
                "log_file": "/tmp/test.log",
                "extra": {"graphs": [{"name": "g1", "path": "/tmp/g1.png", "format": None}]},
                "error": "",
            }

    monkeypatch.setattr(backend.legacy, "multi_session_enabled", True)
    monkeypatch.setattr(backend.legacy, "session_manager", DummyManager())
    monkeypatch.setattr(backend.legacy, "resolve_do_file_path", lambda file_path: (file_path, []))
    monkeypatch.setattr(backend.legacy, "get_log_file_path", lambda *args: "/tmp/test.log")
    monkeypatch.setattr(backend.legacy, "process_mcp_output", lambda output, **kwargs: output)

    result = backend.run_file_command("/tmp/test.do", 30, "abc", None)

    assert result.status == "success"
    assert result.output == "file output"
    assert result.log_file == "/tmp/test.log"
    assert result.graphs[0].path == "/tmp/g1.png"
