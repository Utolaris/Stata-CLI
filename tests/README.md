# Tests Overview

The `tests/` directory keeps Stata `.do` fixtures for the native Rust CLI.
The Rust test suites live in `rust-cli/src` (unit tests) and
`rust-cli/tests` (integration tests). Tests that need a real Stata
installation are skipped when `SKIP_STATA_TESTS` is set.

## Stata `.do` Fixtures
- Streaming: `test_streaming.do`, `test_keepalive.do`
- Timeout: `test_timeout.do`
- Graph investigations: `test_gr_list_issue.do`, `test_graph_issue.do`, `test_graph_name_param.do`
- Log path validation: `test_log_location.do`
- General regression harnesses: `test_stata.do`, `test_understanding.do`
