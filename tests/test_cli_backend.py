#!/usr/bin/env python3
"""
Unit tests for the local Python backend used by the Rust CLI.
"""

import tempfile
from pathlib import Path

import stata_cli_backend as backend
from api_models import GraphArtifact


def test_run_selection_command_single_session(monkeypatch):
    graph_path = str(Path(tempfile.gettempdir()) / "g1.png")
    monkeypatch.setattr(backend.legacy, "multi_session_enabled", False)
    monkeypatch.setattr(backend.legacy, "session_manager", None)
    monkeypatch.setattr(backend.legacy, "run_stata_selection", lambda selection, working_dir, auto_detect: "result line")
    monkeypatch.setattr(backend.legacy, "process_mcp_output", lambda output, **kwargs: output)
    monkeypatch.setattr(
        backend,
        "_maybe_detect_single_session_graphs",
        lambda: [GraphArtifact(name="g1", path=graph_path, format=None)],
    )

    result = backend.run_selection_command("display 1+1", None, None)

    assert result.status == "success"
    assert result.output == "result line"
    assert result.session_id == "default"
    assert result.graphs[0].path == graph_path


def test_run_file_command_multi_session(monkeypatch):
    temp_dir = tempfile.gettempdir()
    log_path = str(Path(temp_dir) / "test.log")
    graph_path = str(Path(temp_dir) / "g1.png")
    do_path = str(Path(temp_dir) / "test.do")

    class DummyManager:
        def execute_file(self, *args, **kwargs):
            return {
                "status": "success",
                "output": "file output",
                "session_id": "abc",
                "log_file": log_path,
                "extra": {"graphs": [{"name": "g1", "path": graph_path, "format": None}]},
                "error": "",
            }

    monkeypatch.setattr(backend.legacy, "multi_session_enabled", True)
    monkeypatch.setattr(backend.legacy, "session_manager", DummyManager())
    monkeypatch.setattr(backend.legacy, "resolve_do_file_path", lambda file_path: (file_path, []))
    monkeypatch.setattr(backend.legacy, "get_log_file_path", lambda *args: log_path)
    monkeypatch.setattr(backend.legacy, "process_mcp_output", lambda output, **kwargs: output)

    result = backend.run_file_command(do_path, 30, "abc", None)

    assert result.status == "success"
    assert result.output == "file output"
    assert result.log_file == log_path
    assert result.graphs[0].path == graph_path


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


def test_build_parser_sets_agent_friendly_data_view_default():
    parser = backend.build_parser()
    args = parser.parse_args(["data", "view"])

    assert args.command == "data"
    assert args.data_command == "view"
    assert args.max_rows == 50


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


def test_init_workspace_command_creates_expected_scaffold(tmp_path):
    target = tmp_path / "analysis"

    result = backend.init_workspace_command(str(target))

    assert result["status"] == "success"
    assert result["target_dir"] == str(target.resolve())
    assert (target / "AGENTS.md").exists()
    assert (target / "data").is_dir()
    assert (target / "do" / "analysis.do").exists()
    assert (target / "outputs").is_dir()
    assert (target / "scripts" / "plot.py").exists()
    assert (target / "stata-packages.md").exists()


def test_init_workspace_command_writes_required_template_content(tmp_path):
    target = tmp_path / "analysis"
    backend.init_workspace_command(str(target))

    agents_text = (target / "AGENTS.md").read_text(encoding="utf-8")
    assert "stata-cli file do/analysis.do --json" in agents_text
    assert "outputs/result.txt" in agents_text
    assert "which <command>" in agents_text

    analysis_text = (target / "do" / "analysis.do").read_text(encoding="utf-8")
    assert "capture log close" in analysis_text
    assert "set more off" in analysis_text
    assert 'log using "outputs/result.txt", text replace' in analysis_text
    assert "display \"Run started:" in analysis_text
    assert "display \"Working directory:" in analysis_text
    assert "describe" in analysis_text
    assert "summarize" in analysis_text
    assert "log close" in analysis_text

    plot_text = (target / "scripts" / "plot.py").read_text(encoding="utf-8")
    assert "import matplotlib.pyplot as plt" in plot_text
    assert "import pandas as pd" in plot_text
    assert "import seaborn as sns" in plot_text
    assert 'OUTPUTS_DIR / "plot.png"' in plot_text

    packages_text = (target / "stata-packages.md").read_text(encoding="utf-8")
    assert "which <command>" in packages_text
    assert "`estout` / `esttab`" in packages_text
    assert "`reghdfe`" in packages_text
    assert "ask the user before installing anything" in packages_text


def test_init_workspace_command_errors_on_existing_scaffold_file(tmp_path):
    target = tmp_path / "analysis"
    target.mkdir()
    existing = target / "AGENTS.md"
    existing.write_text("existing\n", encoding="utf-8")

    result = backend.init_workspace_command(str(target))

    assert result["status"] == "error"
    assert "Refusing to overwrite" in result["message"]
    assert result["conflicts"] == [str(existing.resolve())]
    assert existing.read_text(encoding="utf-8") == "existing\n"


def test_lex_stata_line_highlights_basic_tokens():
    fragments = backend._lex_stata_line('regress y x1 if x1 >= 1 // note')

    assert ("class:command", "regress") in fragments
    assert ("class:keyword", "if") in fragments
    assert ("class:operator", ">=") in fragments
    assert ("class:number", "1") in fragments
    assert ("class:comment", "// note") in fragments


def test_lex_stata_line_highlights_extended_stata_categories():
    fragments = backend._lex_stata_line(
        "reghdfe wage i.industry##c.age if missing(wage) | _rc > 0 local cutoff = c(level)"
    )

    assert ("class:addon-command", "reghdfe") in fragments
    assert ("class:factor", "i") in fragments
    assert ("class:factor", "c") in fragments
    assert ("class:function", "missing") in fragments
    assert ("class:builtin-variable", "_rc") in fragments
    assert ("class:macro-command", "local") in fragments
    assert ("class:result-class", "c") in fragments


def test_format_repl_output_classifies_echo_numbers_notes_and_errors():
    fragments = backend._format_repl_output(
        ". display 2+3\n"
        "5\n"
        "note: dataset has changed since last save\n"
        "warning: file will be replaced\n"
        "invalid syntax\n"
        "r(198);\n"
    )

    assert ("class:echo-prompt", ". ") in fragments
    assert ("class:command", "display") in fragments
    assert ("class:result-number", "5") in fragments
    assert ("class:note", "note: dataset has changed since last save") in fragments
    assert ("class:warning", "warning: file will be replaced") in fragments
    assert ("class:error", "invalid syntax") in fragments
    assert ("class:return-code", "r(198);") in fragments


def test_print_repl_result_omits_graph_and_log_metadata(capsys):
    result = backend.ExecutionResult(
        status="success",
        output="regression output",
        session_id="default",
        log_file="/tmp/example.log",
        graphs=[backend.GraphArtifact(name="g1", path="/tmp/g1.png", format="png")],
        error=None,
    )

    backend._print_repl_result(result)

    captured = capsys.readouterr()
    assert "regression output" in captured.out
    assert captured.err == ""


def test_sanitize_repl_output_removes_internal_wrapper_noise():
    raw_output = """-------------------------------------------------------------------------------
      name:  <unnamed>
       log:  /tmp/stata.log
> _1776689348098.log
  log type:  text
 opened on:  20 Apr 2026, 20:40:50

. quietly set seed 917286971

. display 2+3
5

. capture log close _all
"""

    cleaned = backend._sanitize_repl_output(raw_output)

    assert "quietly set seed" not in cleaned
    assert "capture log close _all" not in cleaned
    assert "opened on:" not in cleaned
    assert "> _1776689348098.log" not in cleaned
    assert ". display 2+3" in cleaned
    assert "5" in cleaned


def test_print_repl_result_applies_repl_sanitizer(capsys):
    result = backend.ExecutionResult(
        status="success",
        output=". quietly set seed 1\n. display 2+3\n5\n. capture log close _all\n",
        session_id="default",
        log_file=None,
        graphs=[],
        error=None,
    )

    backend._print_repl_result(result)

    captured = capsys.readouterr()
    assert "quietly set seed" not in captured.out
    assert "capture log close _all" not in captured.out
    assert ". display 2+3" in captured.out
    assert "5" in captured.out
