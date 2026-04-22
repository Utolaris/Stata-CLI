#!/usr/bin/env python3
"""Thin entrypoint for the packaged Python backend."""

from stata_cli.coordinator.command_commander import main

if __name__ == "__main__":
    raise SystemExit(main())
