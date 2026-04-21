#!/usr/bin/env python3
"""
API Models for Stata MCP Server

This module contains Pydantic models and dataclasses used for API
request/response handling in the Stata MCP Server.
"""

from typing import Any

from pydantic import BaseModel, Field

# =============================================================================
# MCP Tool Parameter Models
# =============================================================================

class RunSelectionParams(BaseModel):
    """Parameters for running Stata code selection."""
    selection: str = Field(..., description="The Stata code to execute")
    session_id: str | None = Field(None, description="Optional session ID for multi-session mode")
    working_dir: str | None = Field(None, description="Optional working directory")


class RunFileParams(BaseModel):
    """Parameters for running a Stata .do file."""
    file_path: str = Field(..., description="The full path to the .do file")
    timeout: int = Field(600, description="Timeout in seconds (default: 600 seconds / 10 minutes)")
    session_id: str | None = Field(None, description="Optional session ID for multi-session mode")
    working_dir: str | None = Field(None, description="Optional working directory")


class GraphArtifact(BaseModel):
    """Structured graph export metadata."""
    name: str = Field(..., description="Graph name inside Stata")
    path: str = Field(..., description="Exported graph file path")
    format: str | None = Field(None, description="Export format, when known")


class ExecutionResult(BaseModel):
    """Structured execution response for native MCP tools."""
    status: str = Field(..., description="Execution status")
    output: str = Field("", description="Human-readable output")
    session_id: str | None = Field(None, description="Effective session identifier")
    log_file: str | None = Field(None, description="Path to the execution log, if available")
    graphs: list[GraphArtifact] = Field(default_factory=list, description="Exported graphs")
    error: str | None = Field(None, description="Structured error message")


class SessionCreateParams(BaseModel):
    """Parameters for creating a session."""
    session_id: str | None = Field(None, description="Optional custom session ID")


class SessionStopParams(BaseModel):
    """Parameters for stopping a running session."""
    session_id: str | None = Field(None, description="Session to stop, defaults to the default session")


class SessionDestroyParams(BaseModel):
    """Parameters for destroying a session."""
    session_id: str = Field(..., description="Session identifier to destroy")


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


# =============================================================================
# Legacy VS Code Extension Support Models
# =============================================================================

class ToolRequest(BaseModel):
    """Request format for legacy /v1/tools endpoint."""
    tool: str
    parameters: dict[str, Any]


class ToolResponse(BaseModel):
    """Response format for legacy /v1/tools endpoint."""
    status: str
    result: str | None = None
    message: str | None = None


# =============================================================================
# Session Management Models
# =============================================================================

class SessionInfo(BaseModel):
    """Information about a Stata session."""
    session_id: str = Field(..., description="Unique session identifier")
    state: str = Field(..., description="Current session state (idle, busy, error)")
    created_at: str | None = Field(None, description="Session creation timestamp")
    last_used: str | None = Field(None, description="Last activity timestamp")


class SessionListResponse(BaseModel):
    """Response for listing all sessions."""
    sessions: list[SessionInfo] = Field(default_factory=list)
    total: int = Field(0, description="Total number of active sessions")
    max_sessions: int = Field(0, description="Maximum allowed sessions")


class SessionCreateRequest(BaseModel):
    """Request to create a new session."""
    session_id: str | None = Field(None, description="Optional custom session ID")


class SessionActionRequest(BaseModel):
    """Request for session actions (stop, destroy)."""
    action: str = Field(..., description="Action: 'stop' or 'destroy'")


# =============================================================================
# Execution Status Models
# =============================================================================

class ExecutionStatus(BaseModel):
    """Current execution status."""
    is_executing: bool = Field(False, description="Whether code is currently executing")
    session_id: str | None = Field(None, description="Session ID if executing")
    command_id: str | None = Field(None, description="Current command ID")


class StopExecutionResponse(BaseModel):
    """Response from stop execution request."""
    status: str = Field(..., description="Result status")
    message: str | None = Field(None, description="Status message")


# =============================================================================
# Health and Status Models
# =============================================================================

class HealthResponse(BaseModel):
    """Server health check response."""
    status: str = Field("ok", description="Server status")
    stata_available: bool = Field(False, description="Whether Stata is available")
    multi_session_enabled: bool = Field(False, description="Whether multi-session mode is enabled")
    active_sessions: int = Field(0, description="Number of active sessions")
    version: str = Field("0.4.1", description="Server version")


# =============================================================================
# Error Response Models
# =============================================================================

class ErrorResponse(BaseModel):
    """Standard error response."""
    error: str = Field(..., description="Error message")
    details: str | None = Field(None, description="Additional error details")
    code: str | None = Field(None, description="Error code for programmatic handling")
