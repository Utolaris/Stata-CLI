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


def test_data_view_command_multi_session(monkeypatch):
    class DummyManager:
        def execute(self, code, session_id=None):
            return {"status": "success", "session_id": session_id}

        def get_data(self, **kwargs):
            return {
                "status": "success",
                "data": [[1, 2]],
                "columns": ["x", "y"],
                "dtypes": {"x": "int64", "y": "int64"},
                "rows": 1,
                "total_rows": 1,
                "displayed_rows": 1,
                "max_rows": kwargs["max_rows"],
                "index": [0],
            }

    monkeypatch.setattr(backend.legacy, "multi_session_enabled", True)
    monkeypatch.setattr(backend.legacy, "session_manager", DummyManager())

    result = backend.data_view_command("abc", "x > 0", 250, None)

    assert result["status"] == "success"
    assert result["columns"] == ["x", "y"]
    assert result["max_rows"] == 250


def test_data_view_command_with_input_dta(monkeypatch, tmp_path):
    captured = {}

    class DummyManager:
        def execute(self, code, session_id=None):
            captured["code"] = code
            captured["session_id"] = session_id
            return {"status": "success", "session_id": session_id}

        def get_data(self, **kwargs):
            return {
                "status": "success",
                "data": [[1, 2]],
                "columns": ["x", "y"],
                "dtypes": {"x": "int64", "y": "int64"},
                "rows": 1,
                "total_rows": 1,
                "displayed_rows": 1,
                "max_rows": kwargs["max_rows"],
                "index": [0],
            }

    input_dta = tmp_path / "sample.dta"
    input_dta.write_text("placeholder", encoding="utf-8")

    monkeypatch.setattr(backend.legacy, "multi_session_enabled", True)
    monkeypatch.setattr(backend.legacy, "session_manager", DummyManager())

    result = backend.data_view_command("abc", "x > 0", 250, str(input_dta))

    assert result["status"] == "success"
    assert result["columns"] == ["x", "y"]
    assert result["max_rows"] == 250
    assert result["source_dta"] == str(input_dta)
    assert 'use "' in captured["code"]


def test_data_view_command_respects_small_max_rows(monkeypatch):
    class DummyManager:
        def get_data(self, **kwargs):
            return {
                "status": "success",
                "data": [],
                "columns": [],
                "dtypes": {},
                "rows": 0,
                "total_rows": 0,
                "displayed_rows": 0,
                "max_rows": kwargs["max_rows"],
                "index": [],
            }

    monkeypatch.setattr(backend.legacy, "multi_session_enabled", True)
    monkeypatch.setattr(backend.legacy, "session_manager", DummyManager())

    result = backend.data_view_command(None, None, 5, None)

    assert result["status"] == "success"
    assert result["max_rows"] == 5


def test_data_export_csv_command_single_session(monkeypatch, tmp_path):
    captured = {}

    def fake_run_stata_selection(selection, working_dir, auto_detect):
        captured["selection"] = selection
        captured["working_dir"] = working_dir
        return "ok"

    monkeypatch.setattr(backend.legacy, "multi_session_enabled", False)
    monkeypatch.setattr(backend.legacy, "session_manager", None)
    monkeypatch.setattr(backend.legacy, "run_stata_selection", fake_run_stata_selection)
    monkeypatch.setattr(backend.legacy, "process_mcp_output", lambda output, **kwargs: output)

    input_dta = tmp_path / "sample.dta"
    input_dta.write_text("placeholder", encoding="utf-8")
    output_csv = tmp_path / "sample.csv"

    result = backend.data_export_csv_command(
        str(output_csv),
        str(input_dta),
        None,
        str(tmp_path),
        True,
    )

    assert result["status"] == "success"
    assert result["output_csv"] == str(output_csv)
    assert 'use "' in captured["selection"]
    assert 'export delimited using "' in captured["selection"]
