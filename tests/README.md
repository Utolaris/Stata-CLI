# Tests Overview

The `tests/` directory keeps a focused set of diagnostics and fixtures for the Rust CLI and local Python backend.

## Python Diagnostics
- `test_cli_backend.py` – Validates command parsing, JSON payloads, workspace scaffolding, and REPL formatting helpers.
- `test_platform_paths.py` – Covers cross-platform Stata install and executable path detection.
- `test_timeout_direct.py` – Calls the worker/session stack directly to ensure timeout enforcement works end-to-end.

## Stata `.do` Fixtures
- Streaming: `test_streaming.do`, `test_keepalive.do`
- Timeout: `test_timeout.do`
- Graph investigations: `test_gr_list_issue.do`, `test_graph_issue.do`, `test_graph_name_param.do`
- Log path validation: `test_log_location.do`
- General regression harnesses: `test_stata.do`, `test_stata2.do`, `test_understanding.do`
