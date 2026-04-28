#!/usr/bin/env python3
"""Manual timeout diagnostic against the packaged session manager."""

from __future__ import annotations

import os
import sys
import time
from pathlib import Path

# Add the src directory to Python path for direct execution.
TESTS_DIR = Path(__file__).resolve().parent
REPO_ROOT = TESTS_DIR.parent
sys.path.insert(0, str(REPO_ROOT / "src"))

from stata_cli.atom.session_manager import SessionManager  # noqa: E402

TEST_FILE = TESTS_DIR / "fixtures" / "test_timeout.do"
STATA_PATH = os.environ.get("STATA_PATH", "/Applications/StataNow")
STATA_EDITION = os.environ.get("STATA_EDITION", "mp")


def run_timeout_test(timeout_seconds: int, test_name: str) -> None:
    """Run a timeout diagnostic with the packaged backend session manager."""
    print(f"\n{'=' * 70}")
    print(f"TEST: {test_name}")
    print(f"Timeout set to: {timeout_seconds} seconds ({timeout_seconds / 60:.2f} minutes)")
    print(f"{'=' * 70}\n")

    manager = SessionManager(
        stata_path=STATA_PATH,
        stata_edition=STATA_EDITION,
        max_sessions=1,
        enabled=True,
    )

    if not manager.start():
        raise RuntimeError("Failed to start SessionManager for timeout diagnostic")

    try:
        start_time = time.time()
        result = manager.execute_file(str(TEST_FILE), timeout=float(timeout_seconds))
        elapsed_time = time.time() - start_time
    finally:
        manager.stop()

    output = result.get("output", "")
    error = result.get("error", "")
    combined = "\n".join(part for part in [output, error] if part)
    timeout_triggered = "timeout" in combined.lower() or result.get("status") == "error"

    print(f"\n{'=' * 70}")
    print(f"RESULTS for {test_name}:")
    print(f"Elapsed time: {elapsed_time:.2f} seconds ({elapsed_time / 60:.2f} minutes)")
    print(f"Expected timeout: {timeout_seconds} seconds")
    print(f"Timeout triggered: {timeout_triggered}")
    print(f"Status: {result.get('status')}")
    print(f"{'=' * 70}\n")

    print("Last 500 characters of output/error:")
    print(combined[-500:] if combined else "<no output>")
    print(f"\n{'=' * 70}\n")


if __name__ == "__main__":
    if not TEST_FILE.exists():
        raise FileNotFoundError(f"Timeout fixture is missing: {TEST_FILE}")

    run_timeout_test(12, "Test 1: 12 second timeout (0.2 minutes)")
    print("\nWaiting 5 seconds before next test...\n")
    time.sleep(5)
    run_timeout_test(30, "Test 2: 30 second timeout (0.5 minutes)")
