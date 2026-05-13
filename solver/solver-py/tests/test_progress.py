"""Binding contract tests for the ProgressHandle / solve_json_with_progress surface."""

from __future__ import annotations

import json
import threading
import time
from pathlib import Path

import klassenzeit_solver as kz

FIXTURE_PATH = Path(__file__).parent / "fixtures" / "grundschule_problem.json"


def _problem_json() -> str:
    """Load the grundschule fixture JSON shipped with the test suite."""
    return FIXTURE_PATH.read_text()


def test_progress_handle_snapshot_shape() -> None:
    """Snapshot dict carries the agreed five keys."""
    handle = kz.ProgressHandle()
    snap = handle.snapshot()
    assert set(snap.keys()) == {
        "iter",
        "placement_count",
        "best_score",
        "is_feasible",
        "cancel_requested",
    }
    assert snap["iter"] == 0
    assert snap["cancel_requested"] is False


def test_snapshot_advances_during_solve() -> None:
    """A live solve writes monotonic increases to iter."""
    handle = kz.ProgressHandle()
    problem_json = _problem_json()

    seen: list[int] = []

    def run_advancing_solve() -> None:
        kz.solve_json_with_progress(problem_json, 500, handle)

    thread = threading.Thread(target=run_advancing_solve)
    thread.start()
    for _ in range(20):
        time.sleep(0.025)
        seen.append(handle.snapshot()["iter"])
        if seen[-1] > 0:
            break
    thread.join()

    assert any(v > 0 for v in seen), f"iter never advanced: {seen}"


def test_cancel_returns_was_cancelled_solution() -> None:
    """Cancel mid-solve; solution JSON has was_cancelled=true."""
    handle = kz.ProgressHandle()
    problem_json = _problem_json()
    result_holder: dict[str, str] = {}

    def run_cancellable_solve() -> None:
        result_holder["json"] = kz.solve_json_with_progress(problem_json, 10_000, handle)

    thread = threading.Thread(target=run_cancellable_solve)
    thread.start()
    time.sleep(0.05)
    handle.cancel()
    thread.join(timeout=2.0)
    assert not thread.is_alive(), "cancel did not unblock the solver"

    payload = json.loads(result_holder["json"])
    assert payload["was_cancelled"] is True
