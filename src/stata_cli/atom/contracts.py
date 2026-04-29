#!/usr/bin/env python3
"""Stable JSON contracts for the local CLI backend."""

from typing import Any

from pydantic import BaseModel, Field


class GraphArtifact(BaseModel):
    """Structured metadata for an exported graph artifact."""

    name: str = Field(..., description="Graph name inside Stata")
    path: str = Field(..., description="Exported graph file path")
    format: str | None = Field(None, description="Export format, when known")


class PartialFailure(BaseModel):
    """Structured metadata for a non-fatal Stata command failure."""

    line: int | None = Field(None, description="Approximate command echo line in the do-file log")
    command: str | None = Field(None, description="Command associated with the Stata return code")
    return_code: str | None = Field(None, description="Stata return code, for example r(199)")
    message: str = Field("", description="Error message preceding the return code")


class ExecutionResult(BaseModel):
    """Structured execution response for CLI commands."""

    status: str = Field(..., description="Execution status")
    output: str = Field("", description="Human-readable output")
    session_id: str | None = Field(None, description="Effective session identifier")
    log_file: str | None = Field(None, description="Path to the execution log, if available")
    graphs: list[GraphArtifact] = Field(default_factory=list, description="Exported graphs")
    partial_failures: list[PartialFailure] = Field(
        default_factory=list,
        description="Recoverable Stata failures detected inside an otherwise completed run",
    )
    partial_failure_count: int = Field(default=0, description="Number of detected recoverable Stata failures")
    error: str | None = Field(None, description="Structured error message")


class CompletionContextResult(BaseModel):
    """Structured completion snapshot for the Rust REPL."""

    status: str = Field(..., description="Completion query status")
    variables: list[str] = Field(default_factory=list, description="Visible variable names")
    macros: list[str] = Field(default_factory=list, description="Known macro names")
    error: str | None = Field(None, description="Structured error message")


class SessionDetailsResult(BaseModel):
    """Structured session lookup result."""

    status: str = Field(..., description="Lookup status")
    session: dict[str, Any] | None = Field(None, description="Session metadata")
    error: str | None = Field(None, description="Error message, if any")


class SessionListResult(BaseModel):
    """Structured list response for session management."""

    status: str = Field("success", description="Operation status")
    sessions: list[dict[str, Any]] = Field(default_factory=list, description="Known sessions")
    max_sessions: int | None = Field(None, description="Configured session limit")
    available_slots: int | None = Field(None, description="Remaining session capacity")
    error: str | None = Field(None, description="Error message, if any")
