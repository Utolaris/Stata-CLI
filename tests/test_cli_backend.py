#!/usr/bin/env python3
"""Unit tests for the local Python backend used by the Rust CLI."""

import io
import json
import tempfile
from pathlib import Path

import stata_cli_backend as backend
from stata_cli.atom.runtime_state import RuntimeConfig
from stata_cli.atom.session_manager import SessionState
from stata_cli.coordinator import bridge_commander
from stata_cli.molecule import data_ops, file_ops, selection_ops


class DummyState:
    def __init__(self, manager, *, multi_session: bool = True, raw_output: bool = False):
        self._manager = manager
        self._config = RuntimeConfig(
            stata_path="/Applications/Stata",
            stata_edition="mp",
            log_level="WARNING",
            result_display_mode="full",
            max_output_tokens=10000,
            raw_output=raw_output,
            multi_session=multi_session,
            max_sessions=100 if multi_session else 1,
            session_timeout=3600,
        )

    def active_config(self):
        return self._config

    def active_session_manager(self):
        return self._manager


def test_run_selection_command_single_session(monkeypatch):
    class DummyManager:
        def execute(self, code, session_id=None, timeout=None):
            return {"status": "success", "output": "result line", "session_id": session_id or "default"}

    monkeypatch.setattr(selection_ops, "get_runtime_state", lambda: DummyState(DummyManager(), multi_session=False))

    result = backend.run_selection_command("display 1+1", None, None)

    assert result.status == "success"
    assert result.output == "result line"
    assert result.session_id == "default"
    assert result.graphs == []


def test_run_selection_command_waits_for_booting_default_session(monkeypatch):
    events = []

    class DummySession:
        def __init__(self):
            self.state = SessionState.CREATING
            self.error_message = ""

    class DummyManager:
        worker_start_timeout = 12

        def __init__(self):
            self.session = DummySession()

        def get_session(self, session_id=None):
            events.append(("get_session", session_id))
            return self.session

        def wait_for_ready(self, session, timeout=30.0):
            events.append(("wait_for_ready", timeout, session.state.value))
            session.state = SessionState.READY
            return True

        def execute(self, code, session_id=None, timeout=None):
            events.append(("execute", session_id, timeout, code))
            return {"status": "success", "output": "booted", "session_id": session_id or "default"}

    monkeypatch.setattr(selection_ops, "get_runtime_state", lambda: DummyState(DummyManager(), multi_session=False))

    result = backend.run_selection_command("display 1+1", None, None)

    assert result.status == "success"
    assert result.output == "booted"
    assert ("get_session", None) in events
    assert ("wait_for_ready", 12.0, "creating") in events
    assert any(event[0] == "execute" for event in events)


def test_run_selection_command_returns_boot_error_when_default_session_never_readies(monkeypatch):
    class DummySession:
        def __init__(self):
            self.state = SessionState.CREATING
            self.error_message = "Stata startup failed"

    class DummyManager:
        worker_start_timeout = 9

        def __init__(self):
            self.session = DummySession()
            self.executed = False

        def get_session(self, session_id=None):
            return self.session

        def wait_for_ready(self, session, timeout=30.0):
            session.state = SessionState.ERROR
            return False

        def execute(self, code, session_id=None, timeout=None):
            self.executed = True
            return {"status": "success", "output": "unexpected", "session_id": session_id or "default"}

    manager = DummyManager()
    monkeypatch.setattr(selection_ops, "get_runtime_state", lambda: DummyState(manager, multi_session=False))

    result = backend.run_selection_command("display 1+1", None, None)

    assert result.status == "error"
    assert result.error == "Stata startup failed"
    assert result.session_id == "default"
    assert manager.executed is False


def test_run_file_command_multi_session(monkeypatch):
    temp_dir = tempfile.gettempdir()
    log_path = str(Path(temp_dir) / "test.log")
    do_path = str(Path(temp_dir) / "test.do")

    class DummyManager:
        def execute_file(self, *args, **kwargs):
            return {
                "status": "success",
                "output": "file output",
                "session_id": "abc",
                "log_file": log_path,
                "extra": {"graphs": [{"name": "g1", "path": str(Path(temp_dir) / "g1.png"), "format": None}]},
                "error": "",
            }

    monkeypatch.setattr(file_ops, "get_runtime_state", lambda: DummyState(DummyManager(), multi_session=True))
    monkeypatch.setattr(file_ops, "resolve_do_file_path", lambda file_path: (file_path, []))
    monkeypatch.setattr(file_ops, "get_log_file_path", lambda *args: log_path)

    result = backend.run_file_command(do_path, 30, "abc", None)

    assert result.status == "success"
    assert result.output == "file output"
    assert result.log_file == log_path
    assert result.graphs[0].name == "g1"
    assert result.graphs[0].path.endswith("g1.png")


def test_run_file_command_reports_partial_failures_from_successful_log(monkeypatch):
    temp_dir = tempfile.gettempdir()
    log_path = str(Path(temp_dir) / "test.log")
    do_path = str(Path(temp_dir) / "test.do")
    raw_output = """
. regress y x

. capture noisily esttab ols using st_reg.rtf, replace
command esttab is unrecognized
r(199);

. display "analysis completed"
analysis completed
"""

    class DummyManager:
        def execute_file(self, *args, **kwargs):
            return {
                "status": "success",
                "output": raw_output,
                "session_id": "abc",
                "log_file": log_path,
                "extra": {"graphs": []},
                "error": "",
            }

    monkeypatch.setattr(file_ops, "get_runtime_state", lambda: DummyState(DummyManager(), multi_session=True))
    monkeypatch.setattr(file_ops, "resolve_do_file_path", lambda file_path: (file_path, []))
    monkeypatch.setattr(file_ops, "get_log_file_path", lambda *args: log_path)

    result = backend.run_file_command(do_path, 30, "abc", None)

    assert result.status == "success"
    assert len(result.partial_failures) == 1
    failure = result.partial_failures[0]
    assert failure.command == "capture noisily esttab ols using st_reg.rtf, replace"
    assert failure.return_code == "r(199)"
    assert failure.message == "command esttab is unrecognized"


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

    monkeypatch.setattr(data_ops, "get_runtime_state", lambda: DummyState(DummyManager(), multi_session=True))

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

    monkeypatch.setattr(data_ops, "get_runtime_state", lambda: DummyState(DummyManager(), multi_session=True))

    result = backend.data_view_command("abc", "x > 0", 250, str(input_dta))

    assert result["status"] == "success"
    assert result["columns"] == ["x", "y"]
    assert result["max_rows"] == 250
    assert result["source_dta"] == str(input_dta.resolve())
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

    monkeypatch.setattr(data_ops, "get_runtime_state", lambda: DummyState(DummyManager(), multi_session=True))

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

    class DummyManager:
        def execute(self, code, session_id=None, timeout=None):
            captured["selection"] = code
            return {"status": "success", "output": "ok", "session_id": session_id or "default"}

    monkeypatch.setattr(data_ops, "get_runtime_state", lambda: DummyState(DummyManager(), multi_session=False))

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


def test_run_selection_command_skips_python_filter_in_raw_mode(monkeypatch):
    class DummyManager:
        def execute(self, code, session_id=None, timeout=None):
            return {"status": "success", "output": ". display 1+1\n2\n", "session_id": session_id or "default"}

    monkeypatch.setattr(
        selection_ops,
        "get_runtime_state",
        lambda: DummyState(DummyManager(), multi_session=False, raw_output=True),
    )
    monkeypatch.setattr(
        selection_ops,
        "process_output",
        lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError("process_output should not run")),
    )

    result = backend.run_selection_command("display 1+1", None, None)

    assert result.status == "success"
    assert result.output == ". display 1+1\n2\n"


def test_run_file_command_skips_python_filter_in_raw_mode(monkeypatch, tmp_path):
    log_path = str(tmp_path / "test.log")
    do_path = str(tmp_path / "test.do")

    class DummyManager:
        def execute_file(self, *args, **kwargs):
            return {
                "status": "success",
                "output": ". do test.do\nresult\n",
                "session_id": "abc",
                "log_file": log_path,
                "extra": {"graphs": []},
                "error": "",
            }

    monkeypatch.setattr(
        file_ops,
        "get_runtime_state",
        lambda: DummyState(DummyManager(), multi_session=True, raw_output=True),
    )
    monkeypatch.setattr(file_ops, "resolve_do_file_path", lambda file_path: (file_path, []))
    monkeypatch.setattr(file_ops, "get_log_file_path", lambda *args: log_path)
    monkeypatch.setattr(
        file_ops,
        "process_output",
        lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError("process_output should not run")),
    )

    result = backend.run_file_command(do_path, 30, "abc", None)

    assert result.status == "success"
    assert result.output == ". do test.do\nresult\n"
    assert result.log_file == log_path


def test_data_export_csv_command_skips_python_filter_in_raw_mode(monkeypatch, tmp_path):
    captured = {}

    class DummyManager:
        def execute(self, code, session_id=None, timeout=None):
            captured["selection"] = code
            return {"status": "success", "output": "raw export output", "session_id": session_id or "default"}

    monkeypatch.setattr(
        data_ops,
        "get_runtime_state",
        lambda: DummyState(DummyManager(), multi_session=False, raw_output=True),
    )
    monkeypatch.setattr(
        data_ops,
        "process_output",
        lambda *_args, **_kwargs: (_ for _ in ()).throw(AssertionError("process_output should not run")),
    )

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
    assert result["output"] == "raw export output"
    assert 'export delimited using "' in captured["selection"]


def test_bridge_command_routes_requests_to_backend(monkeypatch):
    calls = []

    def fake_run_selection_command(code, session_id, working_dir, timeout=None):
        calls.append((code, session_id, working_dir, timeout))
        return backend.ExecutionResult(
            status="success",
            output="bridge-result",
            session_id=session_id,
            log_file=None,
            graphs=[],
            error=None,
        )

    monkeypatch.setattr(bridge_commander, "run_selection_command", fake_run_selection_command)
    stdin = io.StringIO(
        json.dumps({"command": "run", "code": "display 1+1", "working_dir": "/tmp/test", "timeout": 17})
        + "\n"
        + json.dumps({"command": "quit"})
        + "\n"
    )
    stdout = io.StringIO()
    monkeypatch.setattr(bridge_commander.sys, "stdin", stdin)
    monkeypatch.setattr(bridge_commander.sys, "stdout", stdout)

    exit_code = bridge_commander.bridge_command("bridge-session", "/tmp/fallback")

    assert exit_code == 0
    assert calls == [("display 1+1", "bridge-session", "/tmp/test", 17)]
    response = json.loads(stdout.getvalue().splitlines()[0])
    assert response["status"] == "success"
    assert response["output"] == "bridge-result"
    assert response["session_id"] == "bridge-session"


def test_bridge_command_returns_completion_snapshot(monkeypatch):
    class DummySession:
        state = SessionState.READY

    class DummyManager:
        def get_session(self, session_id=None):
            assert session_id == "bridge-session"
            return DummySession()

        def get_data(self, **kwargs):
            assert kwargs["session_id"] == "bridge-session"
            return {"status": "success", "columns": ["iq", "income"]}

        def execute(self, code, session_id=None, timeout=None):
            assert code == "macro dir"
            assert session_id == "bridge-session"
            return {
                "status": "success",
                "output": "global macros\n  sample_macro: value\n  stata_path: /Applications/Stata\n",
            }

    monkeypatch.setattr(bridge_commander, "get_runtime_state", lambda: DummyState(DummyManager()))
    stdin = io.StringIO(json.dumps({"command": "complete_context"}) + "\n" + json.dumps({"command": "quit"}) + "\n")
    stdout = io.StringIO()
    monkeypatch.setattr(bridge_commander.sys, "stdin", stdin)
    monkeypatch.setattr(bridge_commander.sys, "stdout", stdout)

    exit_code = bridge_commander.bridge_command("bridge-session", "/tmp/fallback")

    assert exit_code == 0
    response = json.loads(stdout.getvalue().splitlines()[0])
    assert response["status"] == "success"
    assert response["variables"] == ["iq", "income"]
    assert response["macros"] == ["sample_macro", "stata_path"]


def test_mock_bridge_command_returns_mocked_display_output(monkeypatch):
    stdin = io.StringIO(
        json.dumps({"command": "run", "code": "display 2+3", "working_dir": "/tmp/test"})
        + "\n"
        + json.dumps({"command": "quit"})
        + "\n"
    )
    stdout = io.StringIO()
    monkeypatch.setattr(bridge_commander.sys, "stdin", stdin)
    monkeypatch.setattr(bridge_commander.sys, "stdout", stdout)

    exit_code = bridge_commander.mock_bridge_command("bridge-session", "/tmp/fallback")

    assert exit_code == 0
    response = json.loads(stdout.getvalue().splitlines()[0])
    assert response["status"] == "success"
    assert response["output"] == ". display 2+3\n5\n"
    assert response["session_id"] == "bridge-session"


def test_mock_bridge_command_returns_mocked_completion_snapshot(monkeypatch):
    stdin = io.StringIO(json.dumps({"command": "complete_context"}) + "\n" + json.dumps({"command": "quit"}) + "\n")
    stdout = io.StringIO()
    monkeypatch.setattr(bridge_commander.sys, "stdin", stdin)
    monkeypatch.setattr(bridge_commander.sys, "stdout", stdout)

    exit_code = bridge_commander.mock_bridge_command("bridge-session", "/tmp/fallback")

    assert exit_code == 0
    response = json.loads(stdout.getvalue().splitlines()[0])
    assert response["status"] == "success"
    assert "iq" in response["variables"]
    assert "sample_macro" in response["macros"]
