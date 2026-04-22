#!/usr/bin/env python3
"""
Cross-platform path detection tests.
"""

from __future__ import annotations

import os

from stata_cli.atom import worker_process
from stata_cli.coordinator import runtime_commander
from utils import default_stata_install_dir, find_stata_executable_path


def test_default_stata_install_dir_prefers_statanow_on_macos():
    existing = {"/Applications/StataNow"}

    detected = default_stata_install_dir("Darwin", path_exists=existing.__contains__)

    assert detected == "/Applications/StataNow"


def test_default_stata_install_dir_includes_windows_statanow():
    existing = {r"C:\Program Files\StataNow"}

    detected = default_stata_install_dir("Windows", path_exists=existing.__contains__)

    assert detected == r"C:\Program Files\StataNow"


def test_default_stata_install_dir_prefers_windows_statanow_over_legacy_install():
    existing = {
        r"C:\Program Files\Stata16",
        r"C:\Program Files\StataNow",
    }

    detected = default_stata_install_dir("Windows", path_exists=existing.__contains__)

    assert detected == r"C:\Program Files\StataNow"


def test_find_stata_executable_path_handles_macos_app_bundle_casing():
    app_path = "/Applications/StataMP.app"
    expected = os.path.join(app_path, "Contents", "MacOS", "StataMP")
    existing = {expected}

    executable = find_stata_executable_path(
        app_path,
        edition="mp",
        system_name="Darwin",
        path_exists=existing.__contains__,
    )

    assert executable == expected


def test_find_stata_executable_path_handles_nested_macos_app_bundle():
    base_path = "/Applications/Stata"
    expected = os.path.join(base_path, "StataMP.app", "Contents", "MacOS", "StataMP")
    existing = {expected}

    executable = find_stata_executable_path(
        base_path,
        edition="mp",
        system_name="Darwin",
        path_exists=existing.__contains__,
    )

    assert executable == expected


def test_find_stata_executable_path_handles_windows_binaries():
    base_path = r"C:\Program Files\Stata18"
    expected = os.path.join(base_path, "StataMP-64.exe")
    existing = {expected}

    executable = find_stata_executable_path(
        base_path,
        edition="mp",
        system_name="Windows",
        path_exists=existing.__contains__,
    )

    assert executable == expected


def test_worker_find_stata_executable_uses_shared_platform_logic(monkeypatch):
    monkeypatch.setattr(worker_process.platform, "system", lambda: "Darwin")
    monkeypatch.setattr(
        worker_process,
        "find_stata_executable_path",
        lambda stata_path, stata_edition, system_name=None: os.path.join(
            stata_path,
            "Contents",
            "MacOS",
            "StataMP",
        ),
    )

    executable = worker_process.find_stata_executable("/Applications/StataMP.app", "mp")

    assert executable == os.path.join("/Applications/StataMP.app", "Contents", "MacOS", "StataMP")


def test_runtime_commander_uses_shared_default_path_logic(monkeypatch):
    monkeypatch.setattr(
        runtime_commander,
        "default_stata_install_dir",
        lambda: "/Applications/StataNow",
    )

    args = type(
        "Args",
        (),
        {
            "stata_path": None,
            "stata_edition": None,
            "log_level": None,
            "result_display_mode": None,
            "max_output_tokens": None,
            "multi_session": None,
            "max_sessions": None,
            "session_timeout": None,
        },
    )()

    config = runtime_commander.build_runtime_config(args)

    assert config.stata_path == "/Applications/StataNow"
