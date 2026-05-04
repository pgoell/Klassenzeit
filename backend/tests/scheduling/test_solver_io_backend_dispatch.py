"""Backend dispatch on solver_backend Settings field."""

import json
from typing import Literal
from uuid import uuid4

import pytest

from klassenzeit_backend.scheduling import solver_io


def _minimal_runnable_problem_json() -> str:
    """Smallest problem the Rust validator accepts; CP-SAT also handles it."""
    teacher = str(uuid4())
    tb = str(uuid4())
    subject = str(uuid4())
    room = str(uuid4())
    klass = str(uuid4())
    lesson = str(uuid4())
    return json.dumps(
        {
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
            "school_classes": [{"id": klass}],
            "lessons": [
                {
                    "id": lesson,
                    "school_class_ids": [klass],
                    "subject_id": subject,
                    "teacher_id": teacher,
                    "hours_per_week": 1,
                    "preferred_block_size": 1,
                }
            ],
            "teacher_qualifications": [{"teacher_id": teacher, "subject_id": subject}],
            "teacher_blocked_times": [],
            "room_blocked_times": [],
            "room_subject_suitabilities": [],
            "pinned_placements": [],
        }
    )


@pytest.mark.parametrize(
    "backend",
    ["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"],
)
async def test_run_solve_dispatches_each_backend(
    backend: Literal["lahc", "lahc_rr", "lahc_rr_kempe", "cpsat"],
) -> None:
    """All four KZ_SOLVER_BACKEND values dispatch and return a valid Solution dict."""
    out = await solver_io.run_solve(
        _minimal_runnable_problem_json(),
        scope_id=None,
        input_counts={},
        deadline_ms=None,
        solver_backend=backend,
    )
    # One lesson, one feasible slot: every backend must place it without violations.
    assert len(out["placements"]) == 1
    assert out["violations"] == []


async def test_run_solve_lahc_dispatch_calls_solve_json_with_config_no_period_kwargs(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    captured: dict[str, object] = {}

    def fake_lahc_solve(problem_json: str, deadline_ms: int | None, **kwargs: object) -> str:
        captured["kwargs"] = kwargs
        return '{"placements":[],"violations":[],"soft_score":0}'

    monkeypatch.setattr(solver_io, "_solve_json_with_config", fake_lahc_solve)
    await solver_io.run_solve(
        _minimal_runnable_problem_json(),
        scope_id=None,
        input_counts={},
        deadline_ms=None,
        solver_backend="lahc",
    )
    assert captured["kwargs"] == {}


async def test_run_solve_lahc_rr_dispatch_passes_period_kwarg(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    captured: dict[str, object] = {}

    def fake_lahc_rr_solve(problem_json: str, deadline_ms: int | None, **kwargs: object) -> str:
        captured["kwargs"] = kwargs
        return '{"placements":[],"violations":[],"soft_score":0}'

    monkeypatch.setattr(solver_io, "_solve_json_with_config", fake_lahc_rr_solve)
    await solver_io.run_solve(
        _minimal_runnable_problem_json(),
        scope_id=None,
        input_counts={},
        deadline_ms=None,
        solver_backend="lahc_rr",
    )
    assert captured["kwargs"] == {"lahc_rr_period": 25}


async def test_run_solve_lahc_rr_kempe_dispatch_passes_both_period_kwargs(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    captured: dict[str, object] = {}

    def fake_kempe_solve(problem_json: str, deadline_ms: int | None, **kwargs: object) -> str:
        captured["kwargs"] = kwargs
        return '{"placements":[],"violations":[],"soft_score":0}'

    monkeypatch.setattr(solver_io, "_solve_json_with_config", fake_kempe_solve)
    await solver_io.run_solve(
        _minimal_runnable_problem_json(),
        scope_id=None,
        input_counts={},
        deadline_ms=None,
        solver_backend="lahc_rr_kempe",
    )
    assert captured["kwargs"] == {"lahc_rr_period": 25, "lahc_kempe_period": 23}


async def test_run_solve_cpsat_dispatch_calls_solve_cpsat_json(
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    called: dict[str, str] = {}

    def fake_cpsat(problem_json: str, deadline_ms: int | None) -> str:
        called["fn"] = "cpsat"
        return '{"placements":[],"violations":[],"soft_score":0}'

    monkeypatch.setattr(solver_io, "_solve_cpsat_json", fake_cpsat)
    await solver_io.run_solve(
        _minimal_runnable_problem_json(),
        scope_id=None,
        input_counts={},
        deadline_ms=None,
        solver_backend="cpsat",
    )
    assert called.get("fn") == "cpsat"
