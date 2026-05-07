"""Tests for scripts/apply-github-settings.sh.

Strategy: prepend a tmp dir to PATH that contains a mock `gh` binary. The mock
records every invocation to a log file and returns scripted responses based on
a per-test response map. The real `gh` is not invoked; real `jq` is still on
PATH and used as-is by the script.
"""

from __future__ import annotations

import json
import os
import shutil
import stat
import subprocess
from pathlib import Path

import pytest

REPO_ROOT = Path(__file__).resolve().parent.parent.parent
SCRIPT = REPO_ROOT / "scripts" / "apply-github-settings.sh"
BRANCH_PROTECTION_JSON = REPO_ROOT / ".github" / "branch-protection.json"
BASH = shutil.which("bash") or "/bin/bash"


def _write_mock_gh(bin_dir: Path, log_path: Path, responses_dir: Path) -> None:
    """Write an executable mock `gh` into bin_dir that logs argv + stdin
    and prints responses from responses_dir keyed by a request signature."""
    mock = bin_dir / "gh"
    mock.write_text(
        f"""#!/usr/bin/env bash
# Mock gh: logs invocation and returns a canned response.
set -e
LOG={log_path!s}
RESP_DIR={responses_dir!s}
echo "gh $*" >> "$LOG"
# If --input is present, append the input file's path to the log for assertions.
for arg in "$@"; do
  case "$arg" in
    --input) NEXT_IS_INPUT=1 ;;
    *) if [[ "${{NEXT_IS_INPUT:-0}}" == "1" ]]; then
         echo "  input: $arg" >> "$LOG"
         NEXT_IS_INPUT=0
       fi ;;
  esac
done

# Routing:
#   gh auth status     → exit 0 silently
#   gh repo view --json ... → print owner/repo + default branch
#   gh api --method PATCH /repos/... → print '{{}}'
#   gh api --method PUT  /repos/.../branches/.../protection → print '{{}}'
#   gh api /repos/.../branches/.../protection → print readback from RESP_DIR/readback.json
case "$1" in
  auth) exit 0 ;;
  repo)
    # gh repo view --json nameWithOwner,defaultBranchRef --jq ...
    echo "pgoell/Klassenzeit master"
    ;;
  api)
    method=""
    url=""
    for a in "$@"; do
      case "$a" in
        --method) next=method ;;
        /repos/*) url="$a" ;;
        *) if [[ "${{next:-}}" == "method" ]]; then method="$a"; next=""; fi ;;
      esac
    done
    if [[ "$method" == "PATCH" || "$method" == "PUT" ]]; then
      echo '{{}}'
    else
      cat "$RESP_DIR/readback.json"
    fi
    ;;
  *)
    echo "mock gh: unexpected command: $*" >&2
    exit 99 ;;
esac
"""
    )
    mock.chmod(mock.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


@pytest.fixture
def mock_gh(tmp_path: Path) -> dict[str, Path]:
    bin_dir = tmp_path / "bin"
    bin_dir.mkdir()
    log_path = tmp_path / "gh.log"
    log_path.touch()
    responses_dir = tmp_path / "responses"
    responses_dir.mkdir()
    # Default readback = identical to branch-protection.json (modulo extras the
    # jq filter strips). Individual tests can overwrite this file.
    readback = json.loads(BRANCH_PROTECTION_JSON.read_text())
    readback["url"] = "https://api.github.com/mock/protection"
    # Simulate the GET-shape wrappers for boolean toggles:
    for key in (
        "required_linear_history",
        "allow_force_pushes",
        "allow_deletions",
        "block_creations",
        "required_conversation_resolution",
        "lock_branch",
        "allow_fork_syncing",
    ):
        if key in readback and isinstance(readback[key], bool):
            readback[key] = {"enabled": readback[key]}
    (responses_dir / "readback.json").write_text(json.dumps(readback, indent=2))
    _write_mock_gh(bin_dir, log_path, responses_dir)
    return {"bin_dir": bin_dir, "log": log_path, "responses": responses_dir}


def run_script(
    mock_gh: dict[str, Path],
    *args: str,
    expect_exit: int | None = None,
) -> subprocess.CompletedProcess[str]:
    env = os.environ.copy()
    env["PATH"] = f"{mock_gh['bin_dir']}:{env['PATH']}"
    result = subprocess.run(  # noqa: S603
        [BASH, str(SCRIPT), *args],
        cwd=str(REPO_ROOT),
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    if expect_exit is not None:
        assert result.returncode == expect_exit, (
            f"expected exit {expect_exit}, got {result.returncode}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result


def read_log(mock_gh: dict[str, Path]) -> list[str]:
    return mock_gh["log"].read_text().splitlines()


def test_help_flag_exits_zero_and_prints_usage(mock_gh):
    result = run_script(mock_gh, "--help", expect_exit=0)
    assert "Usage" in result.stdout or "usage" in result.stdout


def test_unknown_flag_exits_2(mock_gh):
    result = run_script(mock_gh, "--badflag", expect_exit=2)
    assert "unknown" in result.stderr.lower() or "usage" in result.stderr.lower()


def test_positional_args_rejected(mock_gh):
    result = run_script(mock_gh, "somearg", expect_exit=2)
    assert result.returncode == 2


def test_missing_gh_exits_2(tmp_path, monkeypatch):
    # Build an isolated PATH that contains jq but NOT gh.
    isolated = tmp_path / "bin"
    isolated.mkdir()
    real_jq = shutil.which("jq")
    assert real_jq, "jq must be installed for this test suite"
    (isolated / "jq").symlink_to(real_jq)
    env = os.environ.copy()
    env["PATH"] = str(isolated)
    result = subprocess.run(  # noqa: S603
        [BASH, str(SCRIPT)],
        cwd=str(REPO_ROOT),
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 2, result.stderr
    assert "gh" in result.stderr.lower()


def test_missing_jq_exits_2(tmp_path, monkeypatch):
    isolated = tmp_path / "bin"
    isolated.mkdir()
    # symlink target; content irrelevant for this test
    real_gh = shutil.which("gh") or "/usr/bin/true"
    (isolated / "gh").symlink_to(real_gh)
    env = os.environ.copy()
    env["PATH"] = str(isolated)
    result = subprocess.run(  # noqa: S603
        [BASH, str(SCRIPT)],
        cwd=str(REPO_ROOT),
        env=env,
        capture_output=True,
        text=True,
        check=False,
    )
    assert result.returncode == 2, result.stderr
    assert "jq" in result.stderr.lower()


def test_dry_run_prints_planned_commands_and_does_not_apply(mock_gh):
    result = run_script(mock_gh, "--dry-run", expect_exit=0)
    assert "would run" in result.stdout.lower()
    assert "PATCH" in result.stdout
    assert "PUT" in result.stdout
    assert "/repos/pgoell/Klassenzeit" in result.stdout
    # No actual apply calls were made
    log = read_log(mock_gh)
    assert not any("--method PATCH" in line for line in log), log
    assert not any("--method PUT" in line for line in log), log


def test_dry_run_calls_resolve_but_not_apply(mock_gh):
    run_script(mock_gh, "--dry-run", expect_exit=0)
    log = read_log(mock_gh)
    assert any("repo view" in line for line in log), log


def test_apply_order_is_repo_settings_then_protection(mock_gh):
    run_script(mock_gh, "--skip-verify", expect_exit=0)
    log = read_log(mock_gh)
    patch_idx = next((i for i, line in enumerate(log) if "--method PATCH" in line), None)
    put_idx = next((i for i, line in enumerate(log) if "--method PUT" in line), None)
    assert patch_idx is not None, log
    assert put_idx is not None, log
    assert patch_idx < put_idx, log


def test_apply_passes_correct_input_files(mock_gh):
    run_script(mock_gh, "--skip-verify", expect_exit=0)
    log_text = mock_gh["log"].read_text()
    assert ".github/repo-settings.json" in log_text
    assert ".github/branch-protection.json" in log_text


def test_apply_calls_correct_endpoints(mock_gh):
    run_script(mock_gh, "--skip-verify", expect_exit=0)
    log_text = mock_gh["log"].read_text()
    assert "/repos/pgoell/Klassenzeit" in log_text
    assert "/repos/pgoell/Klassenzeit/branches/master/protection" in log_text


def test_clean_readback_exits_zero(mock_gh):
    # Default fixture already makes readback match branch-protection.json.
    result = run_script(mock_gh, expect_exit=0)
    assert "matches" in result.stdout.lower() or "✔" in result.stdout


def test_drift_detection_exits_5(mock_gh):
    # Mutate readback: flip required_linear_history.
    rb_path = mock_gh["responses"] / "readback.json"
    readback = json.loads(rb_path.read_text())
    readback["required_linear_history"] = {"enabled": False}
    rb_path.write_text(json.dumps(readback))
    result = run_script(mock_gh, expect_exit=5)
    assert "drift" in result.stderr.lower() or "required_linear_history" in result.stderr


def test_skip_verify_skips_readback_get(mock_gh):
    run_script(mock_gh, "--skip-verify", expect_exit=0)
    log_text = mock_gh["log"].read_text()
    assert "branches/master/protection" in log_text  # the PUT line
    protection_lines = [
        line
        for line in log_text.splitlines()
        if "/branches/master/protection" in line and line.startswith("gh api")
    ]
    assert len(protection_lines) == 1, protection_lines
    assert "--method PUT" in protection_lines[0]


def test_check_skips_apply_calls(mock_gh):
    run_script(mock_gh, "--check", expect_exit=0)
    log = read_log(mock_gh)
    assert not any("--method PATCH" in line for line in log), log
    assert not any("--method PUT" in line for line in log), log


def test_check_clean_readback_exits_zero(mock_gh):
    result = run_script(mock_gh, "--check", expect_exit=0)
    assert "matches" in result.stdout.lower() or "✔" in result.stdout


def test_check_drift_exits_5(mock_gh):
    rb_path = mock_gh["responses"] / "readback.json"
    readback = json.loads(rb_path.read_text())
    readback["required_linear_history"] = {"enabled": False}
    rb_path.write_text(json.dumps(readback))
    result = run_script(mock_gh, "--check", expect_exit=5)
    assert "drift" in result.stderr.lower() or "required_linear_history" in result.stderr


def test_check_and_dry_run_are_mutually_exclusive(mock_gh):
    result = run_script(mock_gh, "--check", "--dry-run", expect_exit=2)
    assert "mutually exclusive" in result.stderr.lower() or "cannot" in result.stderr.lower()
