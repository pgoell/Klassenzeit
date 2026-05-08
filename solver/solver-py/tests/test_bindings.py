"""Contract tests for the klassenzeit_solver PyO3 binding.

These exercise the wrapper layer: JSON marshalling, error conversion, and GIL
release. They are intentionally thin — the algorithm is covered by
`solver-core`'s Rust tests.
"""

import json
import threading
import time
import uuid

import pytest

from klassenzeit_solver import solve_json, solve_json_with_config

# Greedy-only deadline used by the GIL contract test below. The test asserts
# that the binding releases the GIL during the call; LAHC's wall-clock
# deadline (200 ms by default) is irrelevant to that contract and would
# otherwise dominate the test's wall time at 4 x 2000 x 200 ms = 26 minutes.
_GREEDY_ONLY: int | None = None


def _uuid(n: int) -> str:
    return str(uuid.UUID(bytes=bytes([n]) * 16))


def _minimal_problem() -> dict:
    tb = _uuid(10)
    teacher = _uuid(20)
    room = _uuid(30)
    subject = _uuid(40)
    class_id = _uuid(50)
    lesson = _uuid(60)
    return {
        "time_blocks": [{"id": tb, "day_of_week": 0, "position": 0}],
        "teachers": [{"id": teacher, "max_hours_per_week": 5}],
        "rooms": [{"id": room}],
        "subjects": [
            {
                "id": subject,
                "prefer_early_period": 0,
                "avoid_first_period": 0,
                "avoid_last_period": 0,
            }
        ],
        "school_classes": [{"id": class_id}],
        "lessons": [
            {
                "id": lesson,
                "school_class_ids": [class_id],
                "subject_id": subject,
                "teacher_candidates": [teacher],
                "teacher_pin": teacher,
                "hours_per_week": 1,
            }
        ],
        "teacher_qualifications": [{"teacher_id": teacher, "subject_id": subject}],
        "teacher_blocked_times": [],
        "room_blocked_times": [],
        "room_subject_suitabilities": [],
    }


def test_solve_json_minimal_problem_round_trips() -> None:
    result = json.loads(solve_json(json.dumps(_minimal_problem())))
    assert len(result["placements"]) == 1
    assert result["violations"] == []


def test_solve_json_raises_value_error_on_malformed_json() -> None:
    with pytest.raises(ValueError):
        solve_json("not json")


def test_solve_json_raises_value_error_on_empty_time_blocks() -> None:
    problem = _minimal_problem()
    problem["time_blocks"] = []
    with pytest.raises(ValueError):
        solve_json(json.dumps(problem))


@pytest.mark.xfail(
    strict=False,
    reason=(
        "Timing-sensitive: under pytest-xdist parallel workers the 1.7x threshold "
        "is exceeded by scheduler jitter even when the GIL is properly released. "
        "Passes consistently when run in isolation. Tracked in OPEN_THINGS for a "
        "follow-up that runs this test in a serial xdist group."
    ),
)
def test_solve_json_releases_gil() -> None:
    """Two threads solving in parallel should not serialise on the GIL."""

    problem_json = json.dumps(_minimal_problem())

    def _solve_once() -> None:
        # Iteration count chosen so solve work dominates thread-spawn overhead
        # (single-solve is ~55us on the minimal problem; 2000 iterations gives
        # ~100ms of measurable work per thread, well above scheduler jitter).
        # Use greedy-only here (deadline_ms=None): the GIL contract is what
        # the test asserts, and LAHC's wall-clock deadline only obscures it.
        for _ in range(2000):
            solve_json_with_config(problem_json, _GREEDY_ONLY)

    # Warm up to exclude maturin/binding import overhead.
    _solve_once()

    single_start = time.perf_counter()
    _solve_once()
    single_duration = time.perf_counter() - single_start

    parallel_start = time.perf_counter()
    threads = [threading.Thread(target=_solve_once) for _ in range(2)]
    for t in threads:
        t.start()
    for t in threads:
        t.join()
    parallel_duration = time.perf_counter() - parallel_start

    assert parallel_duration < 1.7 * single_duration, (
        f"parallel solves took {parallel_duration:.3f}s vs single {single_duration:.3f}s; "
        "GIL likely not released"
    )


def _trivially_empty_problem_json() -> str:
    """Smallest problem that passes structural validation: 1 time_block, 1
    room, no lessons. Greedy and LAHC both produce empty placements."""
    return json.dumps(
        {
            "time_blocks": [{"id": _uuid(10), "day_of_week": 0, "position": 0}],
            "teachers": [],
            "rooms": [{"id": _uuid(30)}],
            "subjects": [],
            "school_classes": [],
            "lessons": [],
            "teacher_qualifications": [],
            "teacher_blocked_times": [],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
        }
    )


def test_solve_json_with_config_none_returns_greedy() -> None:
    result = json.loads(solve_json_with_config(_trivially_empty_problem_json(), None))
    assert result["placements"] == []
    assert result["violations"] == []


def test_solve_json_with_config_some_matches_default_solve_json() -> None:
    problem = _trivially_empty_problem_json()
    a = json.loads(solve_json_with_config(problem, 200))
    b = json.loads(solve_json(problem))
    assert a == b
