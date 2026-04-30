"""Parse non-fatal Stata command failures from execution logs."""

from __future__ import annotations

import re

from .contracts import PartialFailure

COMMAND_ECHO_PATTERN = re.compile(r"^\.\s+(?P<command>.+?)\s*$")
CONTINUATION_ECHO_PATTERN = re.compile(r"^>\s?(?P<command>.+?)\s*$")
RETURN_CODE_PATTERN = re.compile(r"\br\((?P<code>\d+)\);")
ERROR_MESSAGE_PATTERN = re.compile(
    r"(?i)^\s*(?:"
    r"command .+ is unrecognized|"
    r"variable .+ not found|"
    r"file .+ not found|"
    r"invalid syntax|"
    r"type mismatch|"
    r"no observations|"
    r"insufficient observations|"
    r"conformability error|"
    r"option .+ not allowed"
    r")\s*$"
)


def _is_wrapper_command(command: str) -> bool:
    lowered = command.strip().lower()
    return lowered.startswith(("capture log close", "log using ", "set seed ", "cd "))


def _clean_message(lines: list[str]) -> str:
    cleaned = [line.strip() for line in lines if line.strip()]
    return "\n".join(cleaned)


def parse_partial_failures(output: str) -> list[PartialFailure]:
    """Extract recoverable Stata failures from a raw log-like output string."""
    if not output:
        return []

    failures: list[PartialFailure] = []
    seen: set[tuple[int | None, str | None, str | None, str]] = set()
    current_command: str | None = None
    current_line: int | None = None
    current_error_index: int | None = None
    message_lines: list[str] = []
    command_line = 0

    for raw_line in output.replace("\r\n", "\n").replace("\r", "\n").split("\n"):
        command_match = COMMAND_ECHO_PATTERN.match(raw_line)
        if command_match:
            command = command_match.group("command").strip()
            if _is_wrapper_command(command):
                current_command = None
                current_line = None
            else:
                command_line += 1
                current_command = command
                current_line = command_line
            current_error_index = None
            message_lines = []
            continue

        continuation_match = CONTINUATION_ECHO_PATTERN.match(raw_line)
        if continuation_match and current_command:
            current_command = f"{current_command}\n{continuation_match.group('command').strip()}"
            continue

        return_match = RETURN_CODE_PATTERN.search(raw_line)
        if return_match and current_command:
            return_code = f"r({return_match.group('code')})"
            message = _clean_message(message_lines)
            if current_error_index is not None and current_error_index < len(failures):
                failures[current_error_index].return_code = return_code
            else:
                return_signature: tuple[int | None, str | None, str | None, str] = (
                    current_line,
                    current_command,
                    return_code,
                    message,
                )
                if return_signature in seen:
                    message_lines = []
                    continue
                failures.append(
                    PartialFailure(
                        line=current_line,
                        command=current_command,
                        return_code=return_code,
                        message=message or raw_line.strip(),
                    )
                )
                seen.add(return_signature)
                current_error_index = len(failures) - 1
            message_lines = []
            continue

        if current_command and raw_line.strip():
            if ERROR_MESSAGE_PATTERN.match(raw_line):
                message = raw_line.strip()
                error_signature: tuple[int | None, str | None, str | None, str] = (
                    current_line,
                    current_command,
                    None,
                    message,
                )
                if error_signature not in seen:
                    failures.append(
                        PartialFailure(
                            line=current_line,
                            command=current_command,
                            return_code=None,
                            message=message,
                        )
                    )
                    seen.add(error_signature)
                    current_error_index = len(failures) - 1
            message_lines.append(raw_line)

    return failures
